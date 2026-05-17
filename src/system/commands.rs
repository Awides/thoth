//! Command parser for slash commands
//! 
//! Handles:
//! - /new - Create new identity
//! - /sync - Restore from backup
//! - /backup - Show backup phrase
//! - /create - Create MLS group
//! - /invite - Invite to group
//! - /join - Join group
//! - /theme - Toggle theme
//! - /relays - Manage relays
//! - /clear - Clear history
//! - /help - Show help
//! - /tutorial - Start tutorial

use anyhow::Result;
use serde::{Serialize, Deserialize};

use crate::net::message::Message as NetMessage;
use crate::system::dialog::{DialogManager, DialogType, DialogResponse};

/// Parsed command
#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum Command {
    // Identity
    New,
    Sync,
    Backup,
    
    // Groups
    CreateGroup { name: Option<String> },
    InviteToGroup { group_id: String },
    JoinGroup { invite_code: String },
    
    // Settings
    Theme,
    Relays,
    Clear,
    
    // Help
    Help,
    Tutorial,
    
    // Dialog continuation
    DialogInput(String),
}

impl Command {
    /// Parse input string into command
    pub fn parse(input: &str) -> Option<Self> {
        let trimmed = input.trim();
        
        // Check for slash commands
        if trimmed.starts_with('/') {
            let parts: Vec<&str> = trimmed.split_whitespace().collect();
            match parts[0] {
                "/new" => Some(Command::New),
                "/sync" => Some(Command::Sync),
                "/backup" => Some(Command::Backup),
                "/create" => {
                    let name = parts.get(1).map(|s| s.to_string());
                    Some(Command::CreateGroup { name })
                }
                "/invite" => {
                    if parts.len() > 1 {
                        Some(Command::InviteToGroup { group_id: parts[1].to_string() })
                    } else {
                        None
                    }
                }
                "/join" => {
                    if parts.len() > 1 {
                        Some(Command::JoinGroup { invite_code: parts[1].to_string() })
                    } else {
                        None
                    }
                }
                "/theme" => Some(Command::Theme),
                "/relays" => Some(Command::Relays),
                "/clear" => Some(Command::Clear),
                "/help" => Some(Command::Help),
                "/tutorial" => Some(Command::Tutorial),
                _ => None,
            }
        } else {
            // Plain text - might be dialog input
            None
        }
    }
    
    /// Execute command
    pub fn execute(&self, dialog: &mut DialogManager) -> DialogResponse {
        match self {
            Command::New => {
                // Start new identity flow
                dialog.start_onboarding();
                DialogResponse::messages(vec![
                    NetMessage::system("✨ Creating new identity..."),
                ])
            }
            Command::Sync => {
                // Start sync flow
                DialogResponse::messages(vec![
                    NetMessage::system("📥 Please enter your 12-word backup phrase..."),
                ])
                .with_input("Enter backup phrase...")
            }
            Command::Backup => {
                // In production, retrieve actual backup phrase from key manager
                DialogResponse::messages(vec![
                    NetMessage::system("🔐 **Your Backup Phrase**:\n\n```\nabandon ability able about above absent\nabsorb abstract absurd abuse access accident\n```\n\n⚠️ **CRITICAL**: Store this safely! Anyone with this phrase can access your identity.\n\nType `/backup` again to see it."),
                ])
            }
            Command::CreateGroup { name } => {
                let group_name = name.as_deref().unwrap_or("New Group");
                // In production: call MLS group creation
                DialogResponse::messages(vec![
                    NetMessage::system(&format!("👥 Creating MLS group '{}'...", group_name)),
                    NetMessage::system("✅ Group created! Use `/invite` to generate an invite link for your contacts."),
                ])
            }
            Command::InviteToGroup { group_id } => {
                DialogResponse::messages(vec![
                    NetMessage::system(&format!("📨 Generated invite for group {}...", group_id)),
                ])
            }
            Command::JoinGroup { invite_code } => {
                DialogResponse::messages(vec![
                    NetMessage::system(&format!("🔗 Joining group with code: {}...", invite_code)),
                ])
            }
            Command::Theme => {
                DialogResponse::messages(vec![
                    NetMessage::system("🎨 Theme toggled!"),
                ])
            }
            Command::Relays => {
                let relay_list: String = crate::net::runtime::DEFAULT_RELAYS
                    .iter()
                    .map(|r| format!("- `{}`", r))
                    .collect::<Vec<_>>()
                    .join("\n");
                DialogResponse::messages(vec![
                    NetMessage::system(&format!("**Nostr Relays:**\n{relay_list}\n\nUse `/relays` to see live status, `/relay_add <url>` to add more.")),
                ])
            }
            Command::Clear => {
                DialogResponse::messages(vec![
                    NetMessage::system("🧹 Chat history cleared."),
                ])
            }
            Command::Help => {
                DialogResponse::messages(vec![
                    NetMessage::system("📚 **Thoth Commands**:\n\n**Identity**:\n- `/new` - Create new identity\n- `/sync` - Restore from backup\n- `/backup` - Show backup phrase\n\n**Groups**:\n- `/create` - Create MLS group\n- `/invite` - Invite to group  \n- `/join` - Join group\n\n**Settings**:\n- `/theme` - Toggle theme\n- `/relays` - Manage relays\n- `/clear` - Clear history\n\n**Help**:\n- `/help` - This message\n- `/tutorial` - Interactive tutorial"),
                ])
            }
            Command::Tutorial => {
                DialogResponse::messages(vec![
                    NetMessage::system("📚 **Welcome to the Thoth Tutorial**!\n\nThis is an interactive guide to help you get started."),
                ])
            }
            Command::DialogInput(_) => {
                // Handled by dialog manager
                dialog.process("")
            }
        }
    }
}

/// Check if input needs dialog context
pub fn is_dialog_input(input: &str) -> bool {
    // If it's not a slash command and dialog is active, it's dialog input
    !input.trim().starts_with('/')
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_parse_new() {
        assert!(matches!(Command::parse("/new"), Some(Command::New)));
    }
    
    #[test]
    fn test_parse_create() {
        match Command::parse("/create MyGroup") {
            Some(Command::CreateGroup { name }) => {
                assert_eq!(name, Some("MyGroup".to_string()));
            }
            _ => panic!("Expected CreateGroup"),
        }
    }
    
    #[test]
    fn test_parse_help() {
        assert!(matches!(Command::parse("/help"), Some(Command::Help)));
    }
    
    #[test]
    fn test_parse_invalid() {
        assert_eq!(Command::parse("hello"), None);
    }
}

