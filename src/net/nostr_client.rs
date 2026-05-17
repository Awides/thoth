use anyhow::Result;
use nostr_sdk::prelude::*;
use tracing::info;

pub struct NostrClient {
    client: Client,
    _keys: Keys,
}

impl NostrClient {
    pub async fn new() -> Result<Self> {
        let keys = Self::load_or_generate_keys();
        let client = Client::builder()
            .signer(keys.clone())
            .build();

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

    pub fn load_or_generate_keys() -> Keys {
        #[cfg(not(target_arch = "wasm32"))]
        {
            if let Ok(nsec) = std::env::var("NOSTR_NSEC") {
                if let Ok(keys) = Keys::parse(&nsec) {
                    return keys;
                }
            }
            let key_path = std::path::PathBuf::from("nsec.key");
            if key_path.exists() {
                if let Ok(nsec) = std::fs::read_to_string(&key_path) {
                    if let Ok(keys) = Keys::parse(&nsec.trim()) {
                        return keys;
                    }
                }
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(Some(nsec)) = storage.get_item("thoth_nsec") {
                        if let Ok(keys) = Keys::parse(&nsec) {
                            return keys;
                        }
                    }
                }
            }
        }

        let keys = Keys::generate();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let secret_key = keys.secret_key();
            if let Ok(nsec) = secret_key.to_bech32() {
                let _ = std::fs::write("nsec.key", format!("{}\n", nsec));
                info!("Generated new Nostr keys, saved to nsec.key");
            }
        }

        #[cfg(target_arch = "wasm32")]
        {
            if let Some(window) = web_sys::window() {
                if let Ok(Some(storage)) = window.local_storage() {
                    if let Ok(nsec) = keys.secret_key().to_bech32() {
                        let _ = storage.set_item("thoth_nsec", &nsec);
                    }
                }
            }
        }

        keys
    }

    pub fn public_key(&self) -> String {
        self._keys.public_key().to_bech32().unwrap_or_default()
    }

    pub fn client(&self) -> &Client {
        &self.client
    }

    pub async fn publish(&self, content: String, _tags: Vec<String>) -> Result<()> {
        let builder = EventBuilder::text_note(&content);
        let output = self.client.send_event_builder(builder).await?;
        info!("Published event to {} relays", output.success.len());
        Ok(())
    }

    pub async fn subscribe(&self, filter: Filter) -> Result<()> {
        let _subscription_id = self.client.subscribe(vec![filter], None).await?;
        Ok(())
    }
}

#[cfg(not(target_arch = "wasm32"))]
#[cfg(test)]
mod tests {
    use super::*;

    #[tokio::test]
    async fn test_nostr_client_creation() {
        let client = NostrClient::new().await;
        assert!(client.is_ok());
    }
}
