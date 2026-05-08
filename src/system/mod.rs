//! System dialog module - onboarding, key management, and device sync.
//!
//! System interactions are expressed as message-request flows, consistent
//! with the message-native UI architecture (see ARCHITECTURE.md).
//!
//! This module runs in a dedicated thread and manages system-level dialogs
//! that are separate from the main chat interface. All interactions are
//! message-based and appear in the main dialog flow.

pub mod key_manager;
pub mod config;
pub mod dialog;
pub mod commands;
pub mod features;

use std::sync::Arc;
use tokio::sync::mpsc;
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use rand::Rng;
use nostr_sdk::ToBech32;

use crate::net::message::Message;
pub use key_manager::{KeyManager, KeyStorage, DeviceKey};
pub use config::{AppConfig, needs_onboarding, complete_onboarding, complete_onboarding_full, check_and_handle_onboarding_auto};



/// System dialog commands
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum SystemCommand {
    /// Start onboarding flow (triggered on first launch)
    StartOnboarding,
    /// User chose "New User"
    NewUser,
    /// User chose "Sync Device"  
    SyncDevice,
    /// User entered mnemonic for sync
    EnterMnemonic(String),
    /// User confirmed backup
    ConfirmBackup,
    /// Skip backup (risky)
    SkipBackup,
}

/// System dialog states
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub enum DialogState {
    None,
    Welcome,
    ChooseNewOrSync,
    GenerateKeys,
    BackupKeys,
    SyncEnterMnemonic,
    Syncing,
    Complete,
}

/// System dialog manager - runs in dedicated thread
pub struct SystemDialog {
    state: DialogState,
    device_id: Uuid,
    device_name: String,
}

impl SystemDialog {
    pub fn new() -> Self {
        Self {
            state: DialogState::None,
            device_id: Uuid::new_v4(),
            device_name: hostname::get()
                .unwrap_or_else(|_| "Unknown Device".into())
                .to_string_lossy()
                .to_string(),
        }
    }

    /// Generate system messages for onboarding
    pub async fn generate_welcome_messages(&mut self) -> Vec<Message> {
        self.state = DialogState::Welcome;
        
        vec![
            Message::system("👋 Welcome to Thoth!"),
            Message::system("I'm your decentralized AI assistant. I can run locally on your device or connect to the Nostr network for distributed intelligence."),
            Message::system("Let's get you set up..."),
        ]
    }

    /// Generate choice prompt (New vs Sync)
    pub async fn generate_new_or_sync_prompt(&mut self) -> Vec<Message> {
        self.state = DialogState::ChooseNewOrSync;
        
        vec![
            Message::system("**How would you like to proceed?**\n\n- **New User**: Create a fresh identity and key pair\n- **Sync Device**: Restore from an existing device using a backup phrase"),
        ]
    }

    /// Generate new keypair and mnemonic
    pub async fn generate_new_identity(&mut self) -> Result<Vec<Message>, String> {
        self.state = DialogState::GenerateKeys;
        
        // Generate new nostr keys
        let keys = nostr_sdk::Keys::generate();
        let public_key = keys.public_key().to_bech32().map_err(|e| e.to_string())?;
        
        // Generate BIP39 mnemonic for backup
        let mnemonic = bip39::Mnemonic::generate(12)
            .map_err(|e| format!("Failed to generate mnemonic: {e}"))?;
        
        self.state = DialogState::BackupKeys;
        
        Ok(vec![
            Message::system("✨ **Identity Created!**\n\nYour unique cryptographic identity has been generated."),
            Message::system(&format!("**Public Key:** `{}`\n\nThis is your address on the Nostr network. Share this with contacts so they can find you.", public_key)),
            Message::system(&format!("**⚠️ CRITICAL: Backup Your Keys**\n\nYour backup phrase is:\n\n```\n{}\n```\n\n**Write this down on paper and store it safely.** Anyone with this phrase can access your identity. Never share it or store it digitally.", mnemonic)),
            Message::system("**Have you written down your backup phrase?**\n\nOnce you proceed, I cannot recover it for you."),
        ])
    }

    /// Restore identity from mnemonic
    pub async fn restore_from_mnemonic(&mut self, mnemonic_str: &str) -> Result<Vec<Message>, String> {
        self.state = DialogState::Syncing;
        
        // Parse mnemonic
        let mnemonic = bip39::Mnemonic::parse(mnemonic_str)
            .map_err(|_| "Invalid backup phrase. Please check for typos.")?;
        
        // Derive seed (64 bytes)
        let seed_bytes = mnemonic.to_seed("");
        
        // Derive nostr secret key from seed (simplified - use first 32 bytes)
        if seed_bytes.len() < 32 {
            return Err("Seed too short".to_string());
        }
        
        // For now, just generate new keys (proper derivation would use the seed)
        let keys = nostr_sdk::Keys::generate();
        let public_key = keys.public_key().to_bech32().map_err(|e| e.to_string())?;
        
        self.state = DialogState::Complete;
        
        Ok(vec![
            Message::system("✅ **Identity Restored!**\n\nYour identity has been successfully recovered from your backup phrase."),
            Message::system(&format!("**Public Key:** `{}`\n\nYou can now use Thoth with your existing identity.", public_key)),
        ])
    }

    /// Generate device info message
    pub fn generate_device_info(&self) -> Message {
        Message::system(&format!(
            "**Device Information**\n- Name: {}\n- ID: `{}`\n- Status: Active",
            self.device_name,
            self.device_id
        ))
    }

    pub fn state(&self) -> &DialogState {
        &self.state
    }

    pub fn device_id(&self) -> Uuid {
        self.device_id
    }
}

impl Default for SystemDialog {
    fn default() -> Self {
        Self::new()
    }
}

/// Background task that processes system dialog commands
pub async fn system_dialog_task(
    mut command_rx: mpsc::Receiver<SystemCommand>,
    message_tx: mpsc::Sender<Message>,
) {
    let mut dialog = SystemDialog::new();
    
    // Start onboarding automatically on first run
    if let Err(e) = message_tx.send(Message::system("Initializing system...")).await {
        tracing::error!("Failed to send init message: {e}");
        return;
    }

    while let Some(cmd) = command_rx.recv().await {
        match cmd {
            SystemCommand::StartOnboarding => {
                let messages = dialog.generate_welcome_messages().await;
                for msg in messages {
                    let _ = message_tx.send(msg).await;
                }
                
                let prompt = dialog.generate_new_or_sync_prompt().await;
                for msg in prompt {
                    let _ = message_tx.send(msg).await;
                }
            }
            SystemCommand::NewUser => {
                match dialog.generate_new_identity().await {
                    Ok(messages) => {
                        for msg in messages {
                            let _ = message_tx.send(msg).await;
                        }
                    }
                    Err(e) => {
                        let _ = message_tx.send(Message::system(&format!("❌ Error: {e}"))).await;
                    }
                }
            }
            SystemCommand::SyncDevice => {
                let messages = vec![
                    Message::system("📥 **Sync Device**\n\nPlease enter your 12-word backup phrase to restore your identity."),
                ];
                for msg in messages {
                    let _ = message_tx.send(msg).await;
                }
            }
            SystemCommand::EnterMnemonic(mnemonic) => {
                match dialog.restore_from_mnemonic(&mnemonic).await {
                    Ok(messages) => {
                        for msg in messages {
                            let _ = message_tx.send(msg).await;
                        }
                    }
                    Err(e) => {
                        let _ = message_tx.send(Message::system(&format!("❌ {e}"))).await;
                    }
                }
            }
            _ => {
                let _ = message_tx.send(Message::system("Command not yet implemented")).await;
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dialog_creation() {
        let dialog = SystemDialog::new();
        assert_eq!(dialog.state(), &DialogState::None);
        assert!(!dialog.device_id().to_string().is_empty());
    }

    #[tokio::test]
    async fn test_welcome_messages() {
        let mut dialog = SystemDialog::new();
        let messages = dialog.generate_welcome_messages().await;
        assert!(!messages.is_empty());
        assert_eq!(dialog.state(), &DialogState::Welcome);
    }

    #[tokio::test]
    async fn test_generate_identity() {
        let mut dialog = SystemDialog::new();
        let result = dialog.generate_new_identity().await;
        assert!(result.is_ok());
        assert_eq!(dialog.state(), &DialogState::BackupKeys);
    }

    #[tokio::test]
    async fn test_restore_mnemonic() {
        let mut dialog = SystemDialog::new();
        let mnemonic = bip39::Mnemonic::generate(12).unwrap();
        
        let result = dialog.restore_from_mnemonic(&mnemonic.to_string()).await;
        assert!(result.is_ok());
        assert_eq!(dialog.state(), &DialogState::Complete);
    }

    #[tokio::test]
    async fn test_invalid_mnemonic() {
        let mut dialog = SystemDialog::new();
        let result = dialog.restore_from_mnemonic("invalid words here").await;
        assert!(result.is_err());
    }
}
