use anyhow::Result;
use nostr_sdk::prelude::*;
use std::collections::{HashMap, HashSet};
use std::sync::Arc;
use tokio::sync::RwLock;

use crate::runtime::NostrClient;

/// Loads replaceable/addressable events by kind:pubkey:identifier
/// Automatically deduplicates requests and caches results
#[derive(Clone)]
pub struct AddressLoader {
    client: Arc<NostrClient>,
    /// Track requested addresses to avoid duplicates
    requested: Arc<RwLock<HashSet<String>>>,
    /// Cache of loaded events by address
    cache: Arc<RwLock<HashMap<String, Event>>>,
}

impl AddressLoader {
    pub fn new(client: Arc<NostrClient>) -> Self {
        Self {
            client,
            requested: Arc::new(RwLock::new(HashSet::new())),
            cache: Arc::new(RwLock::new(HashMap::new())),
        }
    }

    /// Load a single addressable event
    pub async fn load(
        &self,
        kind: u16,
        pubkey: PublicKey,
        identifier: Option<String>,
    ) -> Result<Option<Event>> {
        let addr = format!(
            "{}:{}:{}",
            kind,
            pubkey.to_hex(),
            identifier.as_deref().unwrap_or("")
        );

        // Check cache first
        if let Some(event) = self.cache.read().await.get(&addr) {
            return Ok(Some(event.clone()));
        }

        // Deduplicate concurrent requests
        if !self.requested.write().await.insert(addr.clone()) {
            // Already being fetched, wait a bit and check cache
            tokio::time::sleep(tokio::time::Duration::from_millis(100)).await;
            return Ok(self.cache.read().await.get(&addr).cloned());
        }

        // Build filter
        let mut filter = Filter::new().kind(Kind::from(kind)).author(pubkey);

        if let Some(d) = identifier {
            filter = filter.custom_tag(
                SingleLetterTag::lowercase(Alphabet::D),
                d,
            );
        }

        // Fetch from relays
        let events = self.client.fetch_events(filter, None).await?;

        // For replaceable events, keep the newest
        if let Some(event) = events.into_iter().max_by_key(|e| e.created_at) {
            self.cache.write().await.insert(addr, event.clone());
            Ok(Some(event))
        } else {
            Ok(None)
        }
    }

    /// Batch load profiles for multiple pubkeys
    /// Automatically deduplicates - won't re-fetch already-loaded profiles
    pub async fn load_profiles(&self, pubkeys: Vec<PublicKey>) -> Result<Vec<Event>> {
        // Filter out already-cached profiles
        let to_fetch: Vec<PublicKey> = {
            let cache = self.cache.read().await;
            pubkeys
                .into_iter()
                .filter(|pk| {
                    let addr = format!("0:{}:", pk.to_hex());
                    !cache.contains_key(&addr)
                })
                .collect()
        };

        if to_fetch.is_empty() {
            return Ok(vec![]);
        }

        println!("  👤 Fetching {} new profiles...", to_fetch.len());

        // Mark as requested
        {
            let mut requested = self.requested.write().await;
            for pk in &to_fetch {
                let addr = format!("0:{}:", pk.to_hex());
                requested.insert(addr);
            }
        }

        // Batch request for all profiles
        let filter = Filter::new().kind(Kind::Metadata).authors(to_fetch);

        let events = self.client.fetch_events(filter, Some(10)).await?;

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

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_address_format() {
        let pk = PublicKey::from_hex(
            "3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d",
        )
        .unwrap();
        let addr = format!("0:{}:", pk.to_hex());
        assert!(addr.starts_with("0:3bf0c63fcb93463407af97a5e5ee64fa883d107ef9e558472c4eb9aaaefa459d:"));
    }
}
