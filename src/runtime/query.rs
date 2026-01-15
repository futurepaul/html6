use anyhow::Result;
use nostr_sdk::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::runtime::{AddressLoader, NostrClient, RuntimeContext};
use crate::parser::ast;
use crate::runtime::filters::compile_filter;
use crate::runtime::jaq::JaqEvaluator;

/// Query runtime for managing Nostr subscriptions and event streams
pub struct QueryRuntime {
    client: Arc<NostrClient>,
    /// Map of query IDs to their event collections
    queries: Arc<RwLock<HashMap<String, Vec<Event>>>>,
    /// Broadcast channels for each query (send updates when new events arrive)
    broadcasters: HashMap<String, broadcast::Sender<Vec<Event>>>,
    /// Version counter that increments on each update (for detecting changes)
    version: Arc<RwLock<u64>>,
    /// Loader for addressable events (profiles, etc.)
    pub address_loader: AddressLoader,
}

impl QueryRuntime {
    /// Create a new QueryRuntime with a new Nostr client
    pub async fn new() -> Result<Self> {
        let client = NostrClient::new(vec![]).await?;
        let client_arc = Arc::new(client);

        Ok(Self {
            client: Arc::clone(&client_arc),
            queries: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: HashMap::new(),
            version: Arc::new(RwLock::new(0)),
            address_loader: AddressLoader::new(Arc::clone(&client_arc)),
        })
    }

    /// Add a relay to the client
    pub async fn add_relay(&self, url: &str) -> Result<()> {
        self.client.client().add_relay(url).await?;
        self.client.client().connect().await;
        Ok(())
    }

    /// Subscribe to an AST filter (compiles it first)
    pub async fn subscribe_ast_filter(
        &mut self,
        id: &str,
        ast_filter: &ast::Filter,
        ctx: &RuntimeContext,
    ) -> Result<broadcast::Receiver<Vec<Event>>> {
        // Compile the AST filter
        let mut evaluator = JaqEvaluator::new();
        let filter = compile_filter(ast_filter, ctx, &mut evaluator)?;

        // Subscribe to the compiled filter
        self.subscribe_filter(id.to_string(), filter).await
    }

    /// Subscribe to a filter and start collecting events
    pub async fn subscribe_filter(
        &mut self,
        id: String,
        filter: Filter,
    ) -> Result<broadcast::Receiver<Vec<Event>>> {
        // Create broadcast channel for this query
        let (tx, rx) = broadcast::channel(100);

        // Subscribe to the filter and get subscription ID
        let output = self.client.subscribe(filter.clone()).await?;
        let sub_id = output.val;

        // Clone references for the background task
        let client = Arc::clone(&self.client);
        let queries = Arc::clone(&self.queries);
        let version = Arc::clone(&self.version);
        let address_loader = self.address_loader.clone();
        let query_id = id.clone();
        let tx_clone = tx.clone();

        // Spawn background task to listen for events from the subscription
        tokio::spawn(async move {
            println!("  👂 Listening for events on subscription '{}'...", query_id);

            // Handle notifications for this subscription
            let mut notifications = client.client().notifications();

            let mut collected_events = Vec::new();
            let mut last_update = std::time::Instant::now();

            while let Ok(notification) = notifications.recv().await {
                use nostr_sdk::RelayPoolNotification;

                match notification {
                    RelayPoolNotification::Event { subscription_id, event, .. } => {
                        // Only handle events for our subscription
                        if subscription_id == sub_id {
                            collected_events.push(*event);

                            // Update every 500ms or when we hit the limit
                            if last_update.elapsed().as_millis() > 500 || collected_events.len() >= filter.limit.unwrap_or(100) as usize {
                                println!("  📥 Received {} events for query '{}'", collected_events.len(), query_id);

                                // Sort by created_at (newest first)
                                collected_events.sort_by(|a, b| b.created_at.cmp(&a.created_at));

                                // Apply limit (keep only the most recent N events)
                                if let Some(limit) = filter.limit {
                                    collected_events.truncate(limit as usize);
                                }

                                // Store in queries map and increment version
                                {
                                    let mut queries_lock = queries.write().await;
                                    queries_lock.insert(query_id.clone(), collected_events.clone());

                                    let mut version_lock = version.write().await;
                                    *version_lock += 1;

                                    println!("  ✓ Stored {} events in queries map (version: {})", collected_events.len(), *version_lock);
                                }

                                // Broadcast updated events
                                let _ = tx_clone.send(collected_events.clone());

                                // If this is the feed query, fetch profiles for new authors
                                if query_id == "feed" {
                                    let address_loader_clone = address_loader.clone();
                                    let queries_clone = Arc::clone(&queries);
                                    let version_clone = Arc::clone(&version);

                                    tokio::spawn(async move {
                                        // Extract unique pubkeys from feed
                                        let pubkeys: Vec<PublicKey> = {
                                            let queries_lock = queries_clone.read().await;
                                            if let Some(feed_events) = queries_lock.get("feed") {
                                                feed_events
                                                    .iter()
                                                    .map(|e| e.pubkey)
                                                    .collect::<std::collections::HashSet<_>>()
                                                    .into_iter()
                                                    .collect()
                                            } else {
                                                vec![]
                                            }
                                        };

                                        if !pubkeys.is_empty() {
                                            // Load profiles (AddressLoader handles deduplication)
                                            if let Ok(new_profiles) = address_loader_clone.load_profiles(pubkeys).await {
                                                if !new_profiles.is_empty() {
                                                    // Merge with existing profiles
                                                    let mut queries_lock = queries_clone.write().await;
                                                    let existing = queries_lock.get("profiles").cloned().unwrap_or_default();

                                                    let mut all_profiles_map: HashMap<String, Event> = HashMap::new();
                                                    for event in existing {
                                                        all_profiles_map.insert(event.pubkey.to_hex(), event);
                                                    }
                                                    for event in new_profiles {
                                                        all_profiles_map.insert(event.pubkey.to_hex(), event);
                                                    }

                                                    let all_profiles: Vec<Event> = all_profiles_map.into_values().collect();
                                                    queries_lock.insert("profiles".to_string(), all_profiles);

                                                    // Increment version
                                                    let mut version_lock = version_clone.write().await;
                                                    *version_lock += 1;
                                                    println!("  ✓ Profiles updated (version: {})", *version_lock);
                                                }
                                            }
                                        }
                                    });
                                }

                                last_update = std::time::Instant::now();

                                // Continue listening for new events (don't break!)
                                // The limit just controls how many we keep, not when to stop listening
                            }
                        }
                    }
                    RelayPoolNotification::Message { .. } => {
                        // Ignore other messages
                    }
                    _ => {}
                }
            }
        });

        // Store broadcaster
        self.broadcasters.insert(id, tx);

        Ok(rx)
    }

    /// Get events for a specific query
    pub async fn get_query_events(&self, id: &str) -> Option<Vec<Event>> {
        let queries = self.queries.read().await;
        queries.get(id).cloned()
    }

    /// Get a receiver for a query (to subscribe to updates)
    pub fn get_receiver(&self, id: &str) -> Option<broadcast::Receiver<Vec<Event>>> {
        self.broadcasters.get(id).map(|tx| tx.subscribe())
    }

    /// Get the current version (increments on each update)
    pub async fn get_version(&self) -> u64 {
        *self.version.read().await
    }

    /// Convert query events to JSON for use in RuntimeContext
    pub async fn to_json(&self) -> Value {
        let queries = self.queries.read().await;
        let mut json_queries = serde_json::Map::new();

        for (id, events) in queries.iter() {
            let events_json: Vec<Value> = events.iter().map(event_to_json).collect();
            json_queries.insert(id.clone(), json!(events_json));
        }

        // Add enrichedFeed by joining feed with profiles
        if let (Some(feed), Some(profiles)) = (queries.get("feed"), queries.get("profiles")) {
            let enriched: Vec<Value> = feed.iter().map(|event| {
                let mut event_json = event_to_json(event);

                // Find matching profile
                if let Some(profile_event) = profiles.iter().find(|p| p.pubkey == event.pubkey) {
                    // Parse profile content as JSON
                    if let Ok(profile_data) = serde_json::from_str::<Value>(&profile_event.content) {
                        if let Some(obj) = event_json.as_object_mut() {
                            obj.insert("profile".to_string(), profile_data);
                        }
                    }
                }

                event_json
            }).collect();

            json_queries.insert("enrichedFeed".to_string(), json!(enriched));
        }

        json!(json_queries)
    }

    /// Update the RuntimeContext with current query data
    pub async fn populate_context(&self, ctx: &mut crate::runtime::RuntimeContext) {
        ctx.queries = self.to_json().await;
    }

    /// Fetch profiles for authors in the feed
    /// Automatically deduplicates - won't re-fetch already-loaded profiles
    pub async fn fetch_profiles_for_feed(&self) -> Result<()> {
        // Get current feed events
        let feed_events = {
            let queries = self.queries.read().await;
            queries.get("feed").cloned().unwrap_or_default()
        };

        if feed_events.is_empty() {
            return Ok(());
        }

        // Extract unique pubkeys
        let pubkeys: Vec<PublicKey> = feed_events
            .iter()
            .map(|e| e.pubkey)
            .collect::<std::collections::HashSet<_>>()
            .into_iter()
            .collect();

        // Load profiles (deduplicates automatically via AddressLoader)
        let new_profiles = self.address_loader.load_profiles(pubkeys).await?;

        // Store profiles in queries if we got any new ones
        if !new_profiles.is_empty() {
            let mut queries = self.queries.write().await;

            // Get existing profiles
            let existing = queries.get("profiles").cloned().unwrap_or_default();

            // Merge profiles (deduplicate by pubkey)
            let mut all_profiles_map: HashMap<String, Event> = HashMap::new();
            for event in existing {
                all_profiles_map.insert(event.pubkey.to_hex(), event);
            }
            for event in new_profiles {
                all_profiles_map.insert(event.pubkey.to_hex(), event);
            }

            let all_profiles: Vec<Event> = all_profiles_map.into_values().collect();
            queries.insert("profiles".to_string(), all_profiles);

            // Increment version to trigger UI update
            let mut version = self.version.write().await;
            *version += 1;
            println!("  ✓ Profiles updated (version: {})", *version);
        }

        Ok(())
    }
}

/// Convert a Nostr Event to JSON
fn event_to_json(event: &Event) -> Value {
    json!({
        "id": event.id.to_hex(),
        "pubkey": event.pubkey.to_hex(),
        "created_at": event.created_at.as_u64(),
        "kind": event.kind.as_u16(), // In nostr-sdk 0.43, Kind uses u16
        "content": event.content,
        "tags": event.tags.iter().map(|t| {
            // Convert Tag to array of strings using to_vec()
            // Clone is needed because to_vec() consumes self
            json!(t.clone().to_vec())
        }).collect::<Vec<_>>(),
        "sig": event.sig.to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_query_runtime_creation() {
        let runtime = QueryRuntime::new().await.unwrap();
        assert!(runtime.queries.read().await.is_empty());
    }

    #[tokio::test]
    async fn test_subscribe_filter() {
        let mut runtime = QueryRuntime::new().await.unwrap();

        // Add a relay
        runtime.add_relay("wss://relay.damus.io").await.unwrap();

        // Create a basic filter
        let filter = Filter::new().kind(Kind::from(1)).limit(10);

        // Subscribe to the filter
        let result = runtime.subscribe_filter("test_feed".to_string(), filter).await;

        // Should succeed (even if no events are returned)
        assert!(result.is_ok());
    }

    #[test]
    fn test_event_to_json() {
        use nostr_sdk::prelude::*;

        // Create a test event
        let keys = Keys::generate();
        let event = EventBuilder::text_note("Hello, Nostr!")
            .sign_with_keys(&keys)
            .unwrap();

        let json = event_to_json(&event);

        assert_eq!(json["content"], "Hello, Nostr!");
        assert_eq!(json["kind"], 1);
        assert!(json["id"].is_string());
        assert!(json["pubkey"].is_string());
        assert!(json["created_at"].is_number());
    }
}
