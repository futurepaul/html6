use anyhow::Result;
use nostr_sdk::prelude::*;

/// Nostr client wrapper for HNMD applications
pub struct NostrClient {
    client: Client,
}

impl NostrClient {
    /// Create a new Nostr client and connect to the specified relays
    pub async fn new(relays: Vec<String>) -> Result<Self> {
        // Generate keys for now (TODO: support loading from config/NIP-07)
        let keys = Keys::generate();
        let client = Client::new(keys);

        // Add relays
        for relay_url in relays {
            client.add_relay(&relay_url).await?;
        }

        // Connect to all relays
        client.connect().await;

        Ok(Self { client })
    }

    /// Subscribe to a single Nostr filter
    pub async fn subscribe(&self, filter: Filter) -> Result<Output<SubscriptionId>> {
        // In nostr-sdk 0.43, subscribe takes a single Filter and returns Output<SubscriptionId>
        let output = self.client.subscribe(filter, None).await?;
        Ok(output)
    }

    /// Subscribe to multiple Nostr filters
    /// (Combines them with OR logic into a single subscription)
    pub async fn subscribe_with_filters(&self, filters: Vec<Filter>) -> Result<Vec<Output<SubscriptionId>>> {
        // Subscribe to each filter separately
        let mut outputs = Vec::new();
        for filter in filters {
            let output = self.subscribe(filter).await?;
            outputs.push(output);
        }
        Ok(outputs)
    }

    /// Get the underlying client for advanced operations
    pub fn client(&self) -> &Client {
        &self.client
    }

    /// Publish an event
    pub async fn publish(&self, event: Event) -> Result<Output<EventId>> {
        // In nostr-sdk 0.43, send_event expects a reference and returns Output<EventId>
        let output = self.client.send_event(&event).await?;
        Ok(output)
    }

    /// Get events matching a filter
    pub async fn get_events(&self, filter: Filter) -> Result<Vec<Event>> {
        // Get events from the database
        // In nostr-sdk 0.43, query takes a single Filter not a Vec
        let events = self.client.database().query(filter).await?;
        Ok(events.into_iter().collect())
    }

    /// Fetch events once (one-time subscription: subscribe, wait for EOSE, unsubscribe)
    /// This is for loading specific events/profiles without keeping a long-lived subscription
    pub async fn fetch_events(
        &self,
        filter: Filter,
        timeout_secs: Option<u64>,
    ) -> Result<Vec<Event>> {
        // Subscribe
        let output = self.subscribe(filter).await?;
        let sub_id = output.val;

        // Get notifications channel
        let mut notifications = self.client.notifications();

        // Collect events until EOSE
        let mut events = Vec::new();
        let timeout_duration = std::time::Duration::from_secs(timeout_secs.unwrap_or(5));
        let timeout = tokio::time::sleep(timeout_duration);
        tokio::pin!(timeout);

        loop {
            tokio::select! {
                Ok(notification) = notifications.recv() => {
                    match notification {
                        RelayPoolNotification::Event { subscription_id, event, .. } => {
                            if subscription_id == sub_id {
                                events.push(*event);
                            }
                        }
                        RelayPoolNotification::Message { message, .. } => {
                            // Check for EOSE
                            if let RelayMessage::EndOfStoredEvents(id) = message {
                                if id.as_ref() == &sub_id {
                                    // Got EOSE, we're done
                                    break;
                                }
                            }
                        }
                        _ => {}
                    }
                }
                _ = &mut timeout => {
                    // Timeout reached
                    break;
                }
            }
        }

        // Unsubscribe
        self.client.unsubscribe(&sub_id).await;

        Ok(events)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_client_creation() {
        let relays = vec!["wss://relay.damus.io".to_string()];
        let client = NostrClient::new(relays).await;
        assert!(client.is_ok());
    }
}
