//! Memvid persistence layer - append-only log with Roaring bitmap indexes
//!
//! Provides fast reconstruction of app state from disk using:
//! - Frame-based serialization (snapshots + messages)
//! - Roaring bitmap indexes for fast categorical queries
//! - Zero-copy reads via mmap (native) or OPFS (web)

use serde_json;
use chrono;

pub mod schema;
pub mod index;

#[cfg(not(target_arch = "wasm32"))]
pub mod native;
#[cfg(target_arch = "wasm32")]
pub mod web;
pub mod worker;

#[cfg(not(target_arch = "wasm32"))]
pub use native::*;
#[cfg(target_arch = "wasm32")]
pub use web::*;
pub use worker::*;
pub use schema::{AppSnapshot, MessageEvent, Shell, Message, MessageType, ShellMetadata, OnboardingEvent};

/// Runtime world state
pub struct World {
    pub current_shell_idx: usize,
    pub dark_mode: bool,
    pub shells: Vec<Shell>,
}

impl World {
    pub fn new() -> Self {
        Self {
            current_shell_idx: 0,
            dark_mode: true,
            shells: vec![
                Shell {
                    id: 1,
                    title: "🗝️".to_string(),
                    messages: vec![
                        Message {
                            id: 1,
                            msg_type: MessageType::Display,
                            content: "2026-03-31 14:00".to_string(),
                            element_kind: Some("p".to_string()),
                            value: None,
                            sender_id: "0".to_string(),
                            sender_name: "HADES".to_string(),
                            timestamp: "2026-03-31 14:00".to_string(),
                        },
                        Message {
                            id: 2,
                            msg_type: MessageType::Display,
                            content: "Welcome to Thoth".to_string(),
                            element_kind: Some("h1".to_string()),
                            value: None,
                            sender_id: "1".to_string(),
                            sender_name: "MERCURY".to_string(),
                            timestamp: "2026-03-31 14:00".to_string(),
                        },
                        Message {
                            id: 3,
                            msg_type: MessageType::Display,
                            content: "Type a message:".to_string(),
                            element_kind: Some("p".to_string()),
                            value: None,
                            sender_id: "1".to_string(),
                            sender_name: "MERCURY".to_string(),
                            timestamp: "2026-03-31 14:00".to_string(),
                        },
                        Message {
                            id: 4,
                            msg_type: MessageType::Request,
                            content: "".to_string(),
                            element_kind: Some("nip-XX:text".to_string()),
                            value: None,
                            sender_id: "1".to_string(),
                            sender_name: "MERCURY".to_string(),
                            timestamp: "2026-03-31 14:00".to_string(),
                        },
                    ],
                },
            ],
        }
    }

    pub fn from_snapshot(snap: AppSnapshot) -> Self {
        let shells = snap.shells.into_iter()
            .map(|meta| Shell {
                id: meta.id,
                title: meta.title,
                messages: meta.messages,
            })
            .collect();
        Self {
            current_shell_idx: snap.current_shell_idx,
            dark_mode: snap.dark_mode,
            shells,
        }
    }

    pub fn add_message(&mut self, msg_event: MessageEvent) {
        let shell_id = msg_event.shell_id;
        let msg = msg_event.message;
        if let Some(shell) = self.shells.iter_mut().find(|s| s.id == shell_id) {
            shell.messages.push(msg);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[tokio::test]
    async fn test_persistence() {
        let _ = std::fs::remove_file("test.mv2");
        let _ = std::fs::remove_file("test.mv2.idx");
        
        let handle = worker::spawn_worker();
        assert!(handle.open("test.mv2".to_string()).await.is_ok());
        
        let event = schema::MessageEvent {
            shell_id: 1,
            message: schema::Message {
                id: 1,
                msg_type: schema::MessageType::Display,
                content: "Test!".to_string(),
                element_kind: None,
                value: None,
                sender_id: "test".into(),
                sender_name: "Test".into(),
                timestamp: String::new(),
            },
        };
        
        handle.append_message(event);
        tokio::time::sleep(std::time::Duration::from_millis(200)).await;
        
        if let Ok(m) = std::fs::metadata("test.mv2") {
            assert!(m.len() > 0, "File should not be empty");
            println!("✓ Persistence works! File size: {} bytes", m.len());
        } else {
            panic!("Cannot read file");
        }
        
        let _ = std::fs::remove_file("test.mv2");
        let _ = std::fs::remove_file("test.mv2.idx");
    }
}

/// Log an onboarding event to memvid
pub fn log_onboarding_event(handle: &worker::MemvidHandle, event: OnboardingEvent) {
    use schema::{MessageEvent, Message, MessageType};
    
    let event_json = serde_json::to_string(&event).unwrap_or_default();
    let timestamp = chrono::Utc::now().to_rfc3339();
    
    let mem_event = MessageEvent {
        shell_id: 0, // System shell
        message: Message {
            id: 0,
            msg_type: MessageType::Commit, // Use Commit for system events
            content: format!("ONBOARDING_EVENT:{}", event_json),
            element_kind: Some("system:onboarding".to_string()),
            value: None,
            sender_id: "system".to_string(),
            sender_name: "Onboarding".to_string(),
            timestamp,
        },
    };
    
    handle.append_message(mem_event);
}
