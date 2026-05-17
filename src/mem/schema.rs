use serde::{Serialize, Deserialize};
use crate::shared::{self, MessageRole, MessageKind};

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ChatMessage {
    pub id: u64,
    pub role: String,
    pub content: String,
    pub thinking: String,
    pub timestamp: u64,
    #[serde(default)]
    pub sender: String,
}

impl ChatMessage {
    pub fn from_shared(m: &shared::Message) -> Self {
        Self {
            id: m.id,
            role: match m.role {
                MessageRole::User => "user",
                MessageRole::Assistant => "assistant",
                MessageRole::System => "system",
                MessageRole::Peer => "peer",
            }.to_string(),
            content: m.content.clone(),
            thinking: m.thinking.clone(),
            timestamp: m.timestamp,
            sender: m.sender.clone(),
        }
    }

    pub fn to_shared(&self) -> shared::Message {
        shared::Message {
            id: self.id,
            role: match self.role.as_str() {
                "user" => MessageRole::User,
                "assistant" => MessageRole::Assistant,
                "peer" => MessageRole::Peer,
                _ => MessageRole::System,
            },
            content: self.content.clone(),
            thinking: self.thinking.clone(),
            kind: shared::MessageKind::Text,
            timestamp: self.timestamp,
            sender: self.sender.clone(),
        }
    }
}

#[derive(Serialize, Deserialize, Debug, Clone, PartialEq)]
pub struct MemoryFact {
    pub key: String,
    pub value: String,
}

#[derive(Serialize, Deserialize, Debug, Clone)]
pub struct ConversationSnapshot {
    pub next_id: u64,
    pub messages: Vec<ChatMessage>,
    #[serde(default)]
    pub facts: Vec<MemoryFact>,
}
