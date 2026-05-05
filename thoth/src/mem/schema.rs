//! Memvid schema - serialization types for the append-only log

use serde::{Serialize, Deserialize};

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct AppSnapshot {
    pub current_shell_idx: usize,
    pub dark_mode: bool,
    pub shells: Vec<ShellMetadata>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct ShellMetadata {
    pub id: u32,
    pub title: String,
    pub last_message_id: u32,
    pub messages: Vec<Message>,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MessageEvent {
    pub shell_id: u32,
    pub message: Message,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum MessageType {
    Display,
    Request,
    Commit,
    Reject,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Message {
    pub id: u32,
    pub msg_type: MessageType,
    pub content: String,
    pub element_kind: Option<String>,
    pub value: Option<String>,
    pub sender_id: String,
    pub sender_name: String,
    pub timestamp: String,
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct Shell {
    pub id: u32,
    pub title: String,
    pub messages: Vec<Message>,
}

/// Onboarding events for audit trail
#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub enum OnboardingEvent {
    IdentityCreated {
        timestamp: String,
        device_name: String,
    },
    DeviceRegistered {
        pubkey: String,
        timestamp: String,
    },
    BackupViewed {
        timestamp: String,
    },
    OnboardingCompleted {
        timestamp: String,
    },
}
