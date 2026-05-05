//! Background runtime for network operations
//! 
//! Handles:
//! - Tokio task for Nostr listener
//! - MLS group management
//! - Message routing
//! - Push notifications
//! - QUIC upgrade (when available)

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn, error};

use crate::net::{NostrClient, MlsGroupManager, Message, RhaiEngine};

/// Network runtime state
pub struct NetRuntime {
    /// Nostr client
    pub nostr: Arc<Mutex<Option<NostrClient>>>,
    /// MLS group manager
    pub mls: Arc<Mutex<MlsGroupManager>>,
    /// Rhai engine
    pub rhai: Arc<Mutex<RhaiEngine>>,
    /// Message channel (to UI)
    pub message_tx: mpsc::UnboundedSender<Message>,
    /// Runtime control
    pub running: Arc<Mutex<bool>>,
}

impl NetRuntime {
    pub fn new(message_tx: mpsc::UnboundedSender<Message>) -> Self {
        Self {
            nostr: Arc::new(Mutex::new(None)),
            mls: Arc::new(Mutex::new(MlsGroupManager::new())),
            rhai: Arc::new(Mutex::new(RhaiEngine::new())),
            message_tx,
            running: Arc::new(Mutex::new(false)),
        }
    }

    /// Start the background runtime
    pub async fn start(&self) -> Result<()> {
        info!("Starting network runtime...");

        // Initialize Nostr client
        let nostr_client = NostrClient::new().await?;
        {
            let mut nostr = self.nostr.lock().await;
            *nostr = Some(nostr_client);
        }

        // Initialize prebaked Rhai scripts
        {
            let mut rhai = self.rhai.lock().await;
            crate::net::load_prebaked_scripts(&mut rhai)?;
        }

        // Start Nostr listener task
        self.start_nostr_listener().await?;

        // Mark as running
        {
            let mut running = self.running.lock().await;
            *running = true;
        }

        info!("Network runtime started");
        Ok(())
    }

    /// Stop the runtime
    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
        info!("Network runtime stopped");
    }

    /// Start Nostr message listener
    async fn start_nostr_listener(&self) -> Result<()> {
        let nostr = self.nostr.clone();
        let message_tx = self.message_tx.clone();
        let running = self.running.clone();

        tokio::spawn(async move {
            info!("Nostr listener task started");

            // TODO: Implement actual Nostr subscription and message routing
            // For now, this is a placeholder

            while *running.lock().await {
                tokio::time::sleep(tokio::time::Duration::from_secs(1)).await;
                // Placeholder: poll for messages
            }
        });

        Ok(())
    }

    /// Send a message
    pub async fn send_message(&self, message: Message) -> Result<()> {
        let nostr = self.nostr.lock().await;
        if let Some(client) = nostr.as_ref() {
            // TODO: Serialize and publish message
            info!("Sending message via Nostr: {:?}", message.data.id);
            let _ = client.publish(message.data.content, vec![]).await;
        } else {
            warn!("Nostr client not initialized");
        }
        Ok(())
    }

    /// Create device group (user's own devices)
    pub async fn create_device_group(&self, device_ids: Vec<String>) -> Result<()> {
        let mut mls = self.mls.lock().await;
        crate::net::mls_group::create_device_group(&mut mls, device_ids)
    }

    /// Create user group
    pub async fn create_user_group(&self, group_id: String, creator: String) -> Result<()> {
        let mut mls = self.mls.lock().await;
        crate::net::mls_group::create_user_group(&mut mls, group_id, creator)
    }

    /// Register a message handler (for reply prompts)
    pub async fn register_handler(
        &self,
        trigger_type: String,
        trigger_pattern: String,
        handler_script: String,
    ) -> Result<()> {
        let mut rhai = self.rhai.lock().await;
        rhai.register_message_handler(trigger_type, trigger_pattern, handler_script)
    }
}

/// Create runtime with message channel
pub fn create_runtime(
    message_tx: mpsc::UnboundedSender<Message>,
) -> NetRuntime {
    NetRuntime::new(message_tx)
}
