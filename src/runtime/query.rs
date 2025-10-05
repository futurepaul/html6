use anyhow::Result;
use nostr_sdk::prelude::*;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::{broadcast, RwLock};

use crate::runtime::{NostrClient, RuntimeContext};
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
}

impl QueryRuntime {
    /// Create a new QueryRuntime with a new Nostr client
    pub async fn new() -> Result<Self> {
        let client = NostrClient::new(vec![]).await?;
        Ok(Self {
            client: Arc::new(client),
            queries: Arc::new(RwLock::new(HashMap::new())),
            broadcasters: HashMap::new(),
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

                                // Apply limit
                                if let Some(limit) = filter.limit {
                                    collected_events.truncate(limit as usize);
                                }

                                // Store in queries map
                                {
                                    let mut queries_lock = queries.write().await;
                                    queries_lock.insert(query_id.clone(), collected_events.clone());
                                    println!("  ✓ Stored {} events in queries map", collected_events.len());
                                }

                                // Broadcast updated events
                                let _ = tx_clone.send(collected_events.clone());

                                last_update = std::time::Instant::now();

                                // If we've hit the limit, we can stop
                                if collected_events.len() >= filter.limit.unwrap_or(100) as usize {
                                    break;
                                }
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

    /// Convert query events to JSON for use in RuntimeContext
    pub async fn to_json(&self) -> Value {
        let queries = self.queries.read().await;
        let mut json_queries = serde_json::Map::new();

        for (id, events) in queries.iter() {
            let events_json: Vec<Value> = events.iter().map(event_to_json).collect();
            json_queries.insert(id.clone(), json!(events_json));
        }

        json!(json_queries)
    }

    /// Update the RuntimeContext with current query data
    pub async fn populate_context(&self, ctx: &mut crate::runtime::RuntimeContext) {
        ctx.queries = self.to_json().await;
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
