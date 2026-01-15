# Feed with Profiles Implementation Plan

## Goal
Enrich Nostr feed events with author profile data (name, picture) using EventLoader/AddressLoader pattern for efficient deduplication.

## Target Syntax

```yaml
---
filters:
  feed:
    kinds: [1]
    limit: 50

pipes:
  enrichedFeed: |
    .feed | map(. as $event |
      $event + {
        profile: (.profiles[] | select(.pubkey == $event.pubkey) | .content | fromjson)
      }
    )
---

<each from={queries.enrichedFeed} as="note">
<hstack>

<image src={note.profile.picture} />

<vstack>

**{note.profile.name}**

{note.content}

</vstack>

</hstack>
</each>
```

## Architecture: Loader Pattern (Applesauce-Style)

### Core Concepts

- **Subscriptions** = ongoing streams (feed of notes) - keep open forever
- **Loaders** = one-time fetches with deduplication (profiles, specific events) - close after EOSE

### Data Flow

```
feed subscription (ongoing)
  → new events arrive
  → extract unique pubkeys
  → AddressLoader.load_profiles(pubkeys)  [one-time fetch]
  → profiles stored in queries.profiles
  → pipe executes to create enrichedFeed
  → UI updates
```

## Implementation Steps

### Step 1: Implement NostrClient.fetch_events()

Add one-time fetch helper (subscribe, wait for EOSE, unsubscribe):

```rust
// src/runtime/client.rs

impl NostrClient {
    /// Fetch events once (subscribe, wait for EOSE, unsubscribe)
    pub async fn fetch_events(&self, filter: Filter, relays: Vec<String>) -> Result<Vec<Event>> {
        let sub_id = SubscriptionId::generate();

        // Subscribe to specified relays
        for relay_url in &relays {
            self.client.send_msg_to(relay_url, ClientMessage::req(sub_id.clone(), vec![filter.clone()])).await?;
        }

        // Collect events until EOSE from all relays
        let mut events = Vec::new();
        let mut eose_count = 0;
        let mut notifications = self.client.notifications();

        // Timeout after 5 seconds
        let timeout = tokio::time::sleep(Duration::from_secs(5));
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Ok(notification) = notifications.recv() => {
                    match notification {
                        RelayPoolNotification::Event { subscription_id, event, .. } if subscription_id == sub_id => {
                            events.push(*event);
                        }
                        RelayPoolNotification::Message { message, .. } => {
                            if matches!(message, RelayMessage::EndOfStoredEvents(id) if id == sub_id) {
                                eose_count += 1;
                                if eose_count >= relays.len() {
                                    break; // Got EOSE from all relays
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ = &mut timeout => {
                    break; // Timeout
                }
            }
        }

        // Unsubscribe
        self.client.unsubscribe(sub_id).await;

        Ok(events)
    }
}
```

**Tasks:**
- [ ] Implement `fetch_events()` with EOSE detection
- [ ] Add timeout handling
- [ ] Test with simple filter

---

### Step 2: Implement AddressLoader

Generic loader for replaceable/addressable events (profiles, etc.):

```rust
// src/runtime/loaders.rs

use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

/// Loads replaceable/addressable events by kind:pubkey:identifier
pub struct AddressLoader {
    client: Arc<NostrClient>,
    /// Track requested addresses to avoid duplicates
    requested: Arc<RwLock<HashSet<String>>>,
    /// Cache of loaded events by address
    cache: Arc<RwLock<HashMap<String, Event>>>,
    /// Default relays to query
    default_relays: Vec<String>,
}

impl AddressLoader {
    pub fn new(client: Arc<NostrClient>, default_relays: Vec<String>) -> Self {
        Self {
            client,
            requested: Arc::new(RwLock::new(HashSet::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
            default_relays,
        }
    }

    /// Load a single addressable event
    pub async fn load(
        &self,
        kind: u16,
        pubkey: PublicKey,
        identifier: Option<String>,
        relays: Option<Vec<String>>,
    ) -> Result<Option<Event>> {
        let addr = format!("{}:{}:{}", kind, pubkey.to_hex(), identifier.as_deref().unwrap_or(""));

        // Check cache first
        if let Some(event) = self.cache.read().await.get(&addr) {
            return Ok(Some(event.clone()));
        }

        // Deduplicate concurrent requests
        if !self.requested.write().await.insert(addr.clone()) {
            // Already being fetched, wait and check cache
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            return Ok(self.cache.read().await.get(&addr).cloned());
        }

        // Build filter
        let mut filter = Filter::new()
            .kind(Kind::from(kind))
            .author(pubkey);

        if let Some(d) = identifier {
            filter = filter.custom_tag(SingleLetterTag::lowercase(Alphabet::D), vec![d]);
        }

        // Fetch from relays
        let relay_list = relays.unwrap_or(self.default_relays.clone());
        let events = self.client.fetch_events(filter, relay_list).await?;

        // For replaceable events, keep the newest
        if let Some(event) = events.into_iter().max_by_key(|e| e.created_at) {
            self.cache.write().await.insert(addr, event.clone());
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// Batch load profiles for multiple pubkeys
    pub async fn load_profiles(
        &self,
        pubkeys: Vec<PublicKey>,
        relays: Option<Vec<String>>,
    ) -> Result<Vec<Event>> {
        // Filter out already-cached profiles
        let to_fetch: Vec<PublicKey> = {
            let cache = self.cache.read().await;
            pubkeys.into_iter()
                .filter(|pk| {
                    let addr = format!("0:{}:", pk.to_hex());
                    !cache.contains_key(&addr)
                })
                .collect()
        };

        if to_fetch.is_empty() {
            return Ok(vec![]);
        }

        println!("  👤 Fetching {} profiles...", to_fetch.len());

        // Mark as requested
        {
            let mut requested = self.requested.write().await;
            for pk in &to_fetch {
                let addr = format!("0:{}:", pk.to_hex());
                requested.insert(addr);
            }
        }

        // Batch request for all profiles
        let filter = Filter::new()
            .kind(Kind::Metadata)
            .authors(to_fetch);

        let relay_list = relays.unwrap_or(self.default_relays.clone());
        let events = self.client.fetch_events(filter, relay_list).await?;

        // Store in cache
        {
            let mut cache = self.cache.write().await;
            for event in &events {
                let addr = format!("0:{}:", event.pubkey.to_hex());
                cache.insert(addr, event.clone());
            }
        }

        println!("  ✓ Loaded {} profiles", events.len());

        Ok(events)
    }

    /// Get all cached profiles as a Vec (for queries.profiles)
    pub async fn get_all_profiles(&self) -> Vec<Event> {
        self.cache.read().await.values().cloned().collect()
    }
}
```

**Tasks:**
- [ ] Create `AddressLoader` struct
- [ ] Implement `load()` for single addresses
- [ ] Implement `load_profiles()` for batch profile fetching
- [ ] Add deduplication via HashSet
- [ ] Add caching via HashMap
- [ ] Test with sample pubkeys

---

### Step 3: Integrate AddressLoader into QueryRuntime

```rust
// src/runtime/query.rs

pub struct QueryRuntime {
    client: Arc<NostrClient>,
    queries: Arc<RwLock<HashMap<String, Vec<Event>>>>,
    version: Arc<RwLock<u64>>,
    broadcasters: HashMap<String, broadcast::Sender<Vec<Event>>>,

    // Add loader
    address_loader: AddressLoader,
}

impl QueryRuntime {
    pub async fn new() -> Result<Self> {
        let client = NostrClient::new(vec![]).await?;
        let client_arc = Arc::new(client);

        let default_relays = vec![
            "wss://relay.damus.io".to_string(),
            "wss://nos.lol".to_string(),
        ];

        Ok(Self {
            client: Arc::clone(&client_arc),
            queries: Arc::new(RwLock::new(HashMap::new())),
            version: Arc::new(RwLock::new(0)),
            broadcasters: HashMap::new(),
            address_loader: AddressLoader::new(client_arc, default_relays),
        })
    }

    /// Fetch profiles for new authors in feed
    pub async fn fetch_profiles_for_feed(&self) -> Result<()> {
        // Get current feed events
        let feed_events = {
            let queries = self.queries.read().await;
            queries.get("feed").cloned().unwrap_or_default()
        };

        // Extract unique pubkeys
        let pubkeys: Vec<PublicKey> = feed_events.iter()
            .map(|e| e.pubkey)
            .collect::<HashSet<_>>()
            .into_iter()
            .collect();

        if pubkeys.is_empty() {
            return Ok(());
        }

        // Load profiles (deduplicates automatically)
        let profiles = self.address_loader.load_profiles(pubkeys, None).await?;

        // Store profiles in queries
        if !profiles.is_empty() {
            let mut queries = self.queries.write().await;

            // Get existing profiles or create new vec
            let existing = queries.get("profiles").cloned().unwrap_or_default();

            // Merge with new profiles (deduplicate by pubkey)
            let mut all_profiles_map: HashMap<String, Event> = HashMap::new();
            for event in existing {
                all_profiles_map.insert(event.pubkey.to_hex(), event);
            }
            for event in profiles {
                all_profiles_map.insert(event.pubkey.to_hex(), event);
            }

            let all_profiles: Vec<Event> = all_profiles_map.into_values().collect();
            queries.insert("profiles".to_string(), all_profiles);

            // Increment version
            let mut version = self.version.write().await;
            *version += 1;
        }

        Ok(())
    }
}
```

**Tasks:**
- [ ] Add `address_loader` field to QueryRuntime
- [ ] Implement `fetch_profiles_for_feed()`
- [ ] Handle profile merging/deduplication
- [ ] Test profile fetching

---

### Step 4: Trigger Profile Fetching on Feed Updates

Hook into the feed event handler to automatically fetch profiles:

```rust
// In src/runtime/query.rs, in the subscribe_ast_filter background task

// After storing feed events (around line 113):
{
    let mut queries_lock = queries.write().await;
    queries_lock.insert(query_id.clone(), collected_events.clone());

    let mut version_lock = version.write().await;
    *version_lock += 1;

    println!("  ✓ Stored {} events in queries map (version: {})", collected_events.len(), *version_lock);
}

// NEW: Trigger profile loading for feed
if query_id == "feed" {
    // Need a way to call back to QueryRuntime
    // Option 1: Pass a callback channel
    // Option 2: Store reference to QueryRuntime in the task

    // For now, use a channel to signal profile fetch needed
    let _ = profile_fetch_tx.send(collected_events.clone());
}
```

**Challenge:** The background task doesn't have access to QueryRuntime (it's spawned in `subscribe_ast_filter`).

**Solution:** Add a callback channel:

```rust
pub async fn subscribe_ast_filter(
    &mut self,
    id: &str,
    ast_filter: &ast::Filter,
    ctx: &RuntimeContext,
    on_update: Option<broadcast::Sender<String>>,  // Signal which query updated
) -> Result<broadcast::Receiver<Vec<Event>>> {
    // ... existing code ...

    // In the event handler loop:
    {
        // Store events...

        // Notify that this query updated
        if let Some(tx) = &on_update {
            let _ = tx.send(query_id.clone());
        }
    }
}

// In main.rs:
let (update_tx, mut update_rx) = broadcast::channel(10);

// Subscribe to feed with callback
query_runtime.subscribe_ast_filter("feed", &filter, &ctx, Some(update_tx)).await?;

// Spawn task to listen for updates and fetch profiles
tokio::spawn(async move {
    while let Ok(query_id) = update_rx.recv().await {
        if query_id == "feed" {
            let _ = query_runtime.fetch_profiles_for_feed().await;
        }
    }
});
```

**Tasks:**
- [ ] Add callback mechanism to `subscribe_ast_filter()`
- [ ] Listen for feed updates in background task
- [ ] Trigger profile fetching
- [ ] Test profile loading happens automatically

---

### Step 5: Implement Pipe Executor

Execute jq pipes to enrich data:

```rust
// src/runtime/pipes.rs

use crate::runtime::jaq::JaqEvaluator;
use serde_json::Value;

pub struct PipeExecutor {
    evaluator: JaqEvaluator,
}

impl PipeExecutor {
    pub fn new() -> Self {
        Self {
            evaluator: JaqEvaluator::new(),
        }
    }

    pub fn execute(&mut self, pipe_expr: &str, context: &Value) -> Result<Value> {
        // Pipe expressions already start with "."
        self.evaluator.eval(pipe_expr, context)
    }
}

/// Execute all pipes and add results to queries
pub fn execute_all_pipes(
    pipes: &HashMap<String, Pipe>,
    queries_json: &mut Value,
) -> Result<()> {
    let mut executor = PipeExecutor::new();

    for (pipe_id, pipe_def) in pipes {
        let result = executor.execute(&pipe_def.expression, queries_json)?;

        // Add pipe result to queries
        if let Some(obj) = queries_json.as_object_mut() {
            obj.insert(pipe_id.clone(), result);
        }
    }

    Ok(())
}
```

**Tasks:**
- [ ] Create `PipeExecutor` wrapper around JaqEvaluator
- [ ] Implement `execute_all_pipes()`
- [ ] Test enrichment pipe with sample data

---

### Step 6: Execute Pipes in Reload Handler

```rust
// In main.rs, in the ReloadAction handler

fn handle_reload(&mut self, ctx: &mut DriverCtx) {
    // Get queries from query runtime
    let queries_json = self.query_runtime.to_json().await;

    // Execute pipes (enrichment happens here)
    let mut enriched_queries = queries_json;
    if let Err(e) = execute_all_pipes(&self.doc.frontmatter.pipes, &mut enriched_queries) {
        eprintln!("Pipe execution error: {}", e);
    }

    // Update runtime context with enriched queries
    self.runtime_ctx.queries = enriched_queries;

    // Rebuild widgets with new context
    // ... existing reconciliation code
}
```

**Tasks:**
- [ ] Call pipe executor in reload handler
- [ ] Update RuntimeContext with enriched queries
- [ ] Test that enrichedFeed is accessible in UI

---

### Step 7: Update feed.hnmd UI

```yaml
---
filters:
  feed:
    kinds: [1]
    limit: 50

pipes:
  enrichedFeed: |
    .feed | map(. as $event |
      $event + {
        profile: (
          .profiles[]
          | select(.pubkey == $event.pubkey)
          | .content
          | fromjson
          // {name: "Anonymous", picture: ""}
        )
      }
    )

state:
  title: "Nostr Feed (Live)"
---

# {state.title}

<each from={queries.enrichedFeed} as="note">
<hstack>

<image src={note.profile.picture} alt={note.profile.name} />

<vstack>

**{note.profile.name}** · {note.created_at}

{note.content}

---

</vstack>

</hstack>
</each>
```

**Tasks:**
- [ ] Add enrichment pipe to frontmatter
- [ ] Update UI to use `queries.enrichedFeed`
- [ ] Add profile picture and name rendering
- [ ] Add fallback for missing profiles
- [ ] Test end-to-end

---

## Additional Improvements

### EventLoader (for future use)

```rust
// src/runtime/loaders.rs

/// Loads individual events by ID
pub struct EventLoader {
    client: Arc<NostrClient>,
    requested: Arc<RwLock<HashSet<EventId>>>,
    cache: Arc<RwLock<HashMap<EventId, Event>>>,
    default_relays: Vec<String>,
}

impl EventLoader {
    pub async fn load(&self, id: EventId, relays: Option<Vec<String>>) -> Result<Option<Event>> {
        // Similar pattern to AddressLoader
        // Check cache → deduplicate → fetch → cache → return
    }

    pub async fn load_many(&self, ids: Vec<EventId>, relays: Option<Vec<String>>) -> Result<Vec<Event>> {
        // Batch load multiple events
    }
}
```

**Use cases:**
- Loading quoted notes by ID
- Loading thread parents
- Loading specific reactions

---

## Testing Strategy

### Unit Tests
- [ ] Test AddressLoader deduplication
- [ ] Test profile caching
- [ ] Test pipe execution with enrichment
- [ ] Test fallback for missing profiles

### Integration Tests
- [ ] Feed loads → profiles fetch → enrichment pipe → UI updates
- [ ] New feed event with new author → profile fetches → UI updates
- [ ] Same author appears twice → only one profile fetch

### Manual Tests
- [ ] Run feed.hnmd, verify profiles load
- [ ] Verify profile pictures display
- [ ] Verify names display
- [ ] Verify graceful fallback for missing/invalid profiles
- [ ] Verify live updates work with profiles

---

## Benefits of This Approach

1. ✅ **No hot loop resubscriptions** - profiles loaded once per pubkey
2. ✅ **Automatic deduplication** - HashSet prevents duplicate fetches
3. ✅ **Generic loaders** - EventLoader and AddressLoader work for any event type
4. ✅ **Efficient batching** - multiple profiles in one filter
5. ✅ **Caching** - loaded profiles persist in memory
6. ✅ **Idiomatic Nostr** - matches how Damus, Amethyst, etc. work
7. ✅ **Clean separation** - subscriptions for streams, loaders for one-time fetches

---

## Implementation Checklist

**Phase 1: Core Loaders**
- [ ] Implement `NostrClient.fetch_events()` (one-time subscription)
- [ ] Implement `AddressLoader` with caching and deduplication
- [ ] Add `AddressLoader` to QueryRuntime

**Phase 2: Profile Fetching**
- [ ] Add callback mechanism for query updates
- [ ] Implement `fetch_profiles_for_feed()`
- [ ] Hook up profile fetching to feed updates

**Phase 3: Pipe Execution**
- [ ] Create `PipeExecutor` struct
- [ ] Implement `execute_all_pipes()`
- [ ] Integrate pipe execution into reload handler

**Phase 4: UI Integration**
- [ ] Update feed.hnmd with enrichment pipe
- [ ] Update UI template with profile rendering
- [ ] Test end-to-end with live Nostr data
- [ ] Add fallback handling for missing data
