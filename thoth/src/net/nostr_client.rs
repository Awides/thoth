//! Nostr SDK client for relay connections
//! 
//! Handles:
//! - Connection to relays (pub/sub)
//! - Publishing signed events
//! - Subscribing to filters
//! - Key management (nsec file/localstorage)

use anyhow::Result;
use nostr_sdk::prelude::*;
use tokio::sync::mpsc;
use tracing::info;

/// Nostr client wrapper
pub struct NostrClient {
    client: Client,
    _keys: Keys, // Keep keys alive
}

impl NostrClient {
    /// Create new Nostr client with generated or loaded keys
    pub async fn new() -> Result<Self> {
        let keys = Self::load_or_generate_keys();
        let client = Client::builder()
            .signer(keys.clone())
            .build();

        // Connect to default relays
        let relays = vec![
            "wss://relay.nostr.io",
            "wss://nos.lol",
            "wss://relay.damus.io",
        ];

        for relay in &relays {
            client.add_relay(*relay).await?;
        }

        client.connect().await;

        info!("Connected to Nostr relays as {}", keys.public_key().to_bech32()?);

        Ok(Self { client, _keys: keys })
    }

    /// Load keys from nsec string or generate new
    fn load_or_generate_keys() -> Keys {
        // Try to load from environment
        if let Ok(nsec) = std::env::var("NOSTR_NSEC") {
            if let Ok(keys) = Keys::parse(&nsec) {
                return keys;
            }
        }

        // Try to load from file
        let key_path = std::path::PathBuf::from("nsec.key");
        if key_path.exists() {
            if let Ok(nsec) = std::fs::read_to_string(&key_path) {
                if let Ok(keys) = Keys::parse(&nsec.trim()) {
                    return keys;
                }
            }
        }

        // Generate new keys
        let keys = Keys::generate();
        
        // Save to file
        let secret_key = keys.secret_key();
        if let Ok(nsec) = secret_key.to_bech32() {
            let _ = std::fs::write(&key_path, format!("{}\n", nsec));
            info!("Generated new Nostr keys, saved to {:?}", key_path);
        }

        keys
    }

    /// Get public key
    pub fn public_key(&self) -> String {
        self._keys.public_key().to_bech32().unwrap_or_default()
    }

    /// Publish a message to relays
    pub async fn publish(&self, content: String, _tags: Vec<String>) -> Result<()> {
        let builder = EventBuilder::text_note(&content);
        let output = self.client.send_event_builder(builder).await?;
        info!("Published event to {} relays", output.success.len());
        Ok(())
    }

    /// Subscribe to messages with a filter
    pub async fn subscribe(&self, filter: Filter) -> Result<()> {
        let _subscription_id = self.client.subscribe(vec![filter], None).await?;
        Ok(())
    }

    /// Listen for incoming messages (streaming)
    pub async fn listen_messages(
        &self,
        filter: Filter,
        mut tx: mpsc::UnboundedSender<String>,
    ) -> Result<()> {
        let mut receiver = self.client.notifications();

        while let Ok(notification) = receiver.recv().await {
            match notification {
                RelayPoolNotification::Event { event, .. } => {
                    info!("Received event from {}", event.pubkey);
                    let _ = tx.send(event.content.clone());
                }
                _ => {}
            }
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nostr_client_creation() {
        let client = NostrClient::new().await;
        assert!(client.is_ok());
    }
}
