//! Message schema: the core of Thoth's message-native UI.
//!
//! All UI is expressed as typed request messages; replies carry user input.
//! The message log is the authoritative source of truth for state and UI.
//!
//! See ARCHITECTURE.md for the design rationale.
//!
//! Supports the cute syntax: `?text #firstname @thoth remember her name`
//! Where:
//! - `?text` = input type (text, number, date, etc.)
//! - `#tag` = metadata tags
//! - `@target` = message recipient/handler
//! - remainder = prompt/payload

use serde::{Deserialize, Serialize};
use uuid::Uuid;
use chrono::{DateTime, Utc};

/// Input field types for UI generation
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type", content = "value")]
pub enum InputType {
    Text { default: Option<String> },
    Number { default: Option<f64>, min: Option<f64>, max: Option<f64> },
    Date { default: Option<String> },
    DateTime { default: Option<String> },
    Boolean { default: bool },
    Select { options: Vec<String>, default: Option<String> },
    MultiSelect { options: Vec<String>, defaults: Vec<String> },
    File { mime_types: Vec<String> },
    Json { schema: Option<serde_json::Value> },
}

/// Message tags for routing and metadata
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct Tag {
    pub name: String,
    pub value: String,
}

/// Message type - request, reply, or system
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "snake_case")]
pub enum MessageType {
    /// Request for input (from human/LLM/script)
    Request,
    /// Reply to a request
    Reply { parent_id: Uuid },
    /// System/notification message
    System,
    /// Prompt registration (attach handler to future reply)
    RegisterPrompt { parent_id: Uuid, handler: String },
}

/// Core message data structure
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MessageData {
    /// Unique message ID
    pub id: Uuid,
    /// Thread root (for reply chains)
    pub thread_id: Uuid,
    /// Message type and payload
    pub message_type: MessageType,
    /// Input type specification (for requests)
    pub input_type: Option<InputType>,
    /// Tags for routing/metadata
    pub tags: Vec<Tag>,
    /// Target handler/user/device
    pub target: Option<String>,
    /// Human-readable content (markdown)
    pub content: String,
    /// Structured payload (for scripts/LLMs)
    pub payload: Option<serde_json::Value>,
    /// Timestamp
    pub timestamp: DateTime<Utc>,
    /// Sender's public key (Nostr format)
    pub sender: String,
}

/// High-level message wrapper
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Message {
    pub data: MessageData,
    /// Signature (Nostr signature or MLS MAC)
    pub signature: String,
    /// Encrypted flag (true if MLS encrypted)
    pub encrypted: bool,
}

impl Message {
    /// Create a new request message
    pub fn new_request(
        content: String,
        input_type: Option<InputType>,
        target: Option<String>,
        tags: Vec<Tag>,
        sender: String,
    ) -> Self {
        let id = Uuid::new_v4();
        Self {
            data: MessageData {
                id,
                thread_id: id,
                message_type: MessageType::Request,
                input_type,
                tags,
                target,
                content,
                payload: None,
                timestamp: Utc::now(),
                sender,
            },
            signature: String::new(), // Signed by sender
            encrypted: false,
        }
    }

    /// Create a reply message
    pub fn new_reply(
        parent_id: Uuid,
        content: String,
        payload: Option<serde_json::Value>,
        sender: String,
    ) -> Self {
        let id = Uuid::new_v4();
        Self {
            data: MessageData {
                id,
                thread_id: parent_id,
                message_type: MessageType::Reply { parent_id },
                input_type: None,
                tags: vec![],
                target: None,
                content,
                payload,
                timestamp: Utc::now(),
                sender,
            },
            signature: String::new(),
            encrypted: false,
        }
    }

    /// Create a system message
    pub fn system(content: &str) -> Self {
        let id = Uuid::new_v4();
        Self {
            data: MessageData {
                id,
                thread_id: id,
                message_type: MessageType::System,
                input_type: None,
                tags: vec![],
                target: None,
                content: content.to_string(),
                payload: None,
                timestamp: Utc::now(),
                sender: "system".to_string(),
            },
            signature: String::new(),
            encrypted: false,
        }
    }

    /// Parse cute syntax: `?text #firstname @thoth remember her name`
    pub fn parse_cute_syntax(input: &str, sender: String) -> anyhow::Result<Self> {
        let mut input_type: Option<InputType> = None;
        let mut tags = Vec::new();
        let mut target: Option<String> = None;
        let mut content_parts = Vec::new();

        for token in input.split_whitespace() {
            if token.starts_with('?') {
                // Input type specification
                input_type = Some(parse_input_type(&token[1..])?);
            } else if token.starts_with('#') {
                // Tag
                let tag_parts: Vec<&str> = token[1..].split(':').collect();
                let name = tag_parts[0].to_string();
                let value = tag_parts.get(1).map(|s| s.to_string()).unwrap_or_default();
                tags.push(Tag { name, value });
            } else if token.starts_with('@') {
                // Target
                target = Some(token[1..].to_string());
            } else {
                // Content
                content_parts.push(token);
            }
        }

        let content = content_parts.join(" ");
        Ok(Self::new_request(content, input_type, target, tags, sender))
    }
}

/// Parse input type from string (e.g., "text", "number:0:100", "select:a,b,c")
fn parse_input_type(s: &str) -> anyhow::Result<InputType> {
    match s {
        "text" => Ok(InputType::Text { default: None }),
        "number" => Ok(InputType::Number { default: None, min: None, max: None }),
        "date" => Ok(InputType::Date { default: None }),
        "boolean" => Ok(InputType::Boolean { default: false }),
        s if s.starts_with("select:") => {
            let options: Vec<String> = s[7..].split(',').map(|s| s.to_string()).collect();
            Ok(InputType::Select { options, default: None })
        }
        _ => Ok(InputType::Text { default: None }),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_cute_syntax() {
        let msg = Message::parse_cute_syntax(
            "?text #firstname @thoth remember her name",
            "sender_pubkey".to_string(),
        ).unwrap();

        assert_eq!(msg.data.target, Some("thoth".to_string()));
        assert_eq!(msg.data.tags.len(), 1);
        assert_eq!(msg.data.tags[0].name, "firstname");
    }
}
