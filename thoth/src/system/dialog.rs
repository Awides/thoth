//! Dialog state machine for multi-step flows
//! 
//! Manages complex interactions like:
//! - Onboarding (new user, sync, relay selection)
//! - Group creation (MLS group setup)
//! - Contact management
//! - Settings

use serde::{Serialize, Deserialize};
use uuid::Uuid;

use crate::net::message::Message as NetMessage;

/// Dialog types
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum DialogType {
    /// First-time onboarding
    Onboarding(OnboardingState),
    /// Creating MLS group
    CreateGroup,
    /// Adding contact
    AddContact,
    /// Settings
    Settings,
    /// Help/tutorial
    Help,
}

/// Onboarding sub-states
#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum OnboardingState {
    Welcome,
    ChooseNewOrSync,
    GeneratingKeys,
    BackupKeys,
    ConfirmBackup,
    SelectRelays,
    ImportContacts,
    Complete,
}

/// Dialog response
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DialogResponse {
    pub messages: Vec<NetMessage>,
    pub next_state: Option<DialogType>,
    pub requires_input: bool,
    pub input_placeholder: Option<String>,
}

impl DialogResponse {
    pub fn messages(msgs: Vec<NetMessage>) -> Self {
        Self {
            messages: msgs,
            next_state: None,
            requires_input: false,
            input_placeholder: None,
        }
    }
    
    pub fn with_input(mut self, placeholder: &str) -> Self {
        self.requires_input = true;
        self.input_placeholder = Some(placeholder.to_string());
        self
    }
    
    pub fn with_next(mut self, next: DialogType) -> Self {
        self.next_state = Some(next);
        self
    }
}

/// Dialog manager
pub struct DialogManager {
    current: Option<DialogType>,
    history: Vec<DialogType>,
}

impl DialogManager {
    pub fn new() -> Self {
        Self {
            current: None,
            history: Vec::new(),
        }
    }
    
    /// Start onboarding dialog
    pub fn start_onboarding(&mut self) -> DialogResponse {
        let state = OnboardingState::Welcome;
        self.current = Some(DialogType::Onboarding(state.clone()));
        
        DialogResponse::messages(vec![
            NetMessage::system("👋 Welcome to Thoth!"),
        ])
        .with_input("Type 'new' to create identity or 'sync' to restore...")
    }
    
    /// Process user input in current dialog
    pub fn process(&mut self, input: &str) -> DialogResponse {
        let current = self.current.clone();
        match current {
            Some(DialogType::Onboarding(state)) => {
                self.handle_onboarding(&state, input)
            }
            Some(DialogType::CreateGroup) => {
                self.handle_create_group(input)
            }
            Some(DialogType::AddContact) => {
                self.handle_add_contact(input)
            }
            Some(DialogType::Settings) => {
                self.handle_settings(input)
            }
            Some(DialogType::Help) => {
                self.handle_help(input)
            }
            None => {
                // No active dialog
                DialogResponse::messages(vec![
                    NetMessage::system("No active dialog. Type `/help` for commands.")
                ])
            }
        }
    }
    
    fn handle_onboarding(&mut self, state: &OnboardingState, input: &str) -> DialogResponse {
        match state {
            OnboardingState::Welcome => {
                match input.to_lowercase().trim() {
                    "new" => {
                        self.current = Some(DialogType::Onboarding(OnboardingState::ChooseNewOrSync));
                        DialogResponse::messages(vec![
                            NetMessage::system("✨ Great! Let's create your decentralized identity..."),
                        ])
                    }
                    "sync" => {
                        self.current = Some(DialogType::Onboarding(OnboardingState::ChooseNewOrSync));
                        DialogResponse::messages(vec![
                            NetMessage::system("📥 Ready to sync. Please enter your 12-word backup phrase..."),
                        ])
                    }
                    _ => {
                        DialogResponse::messages(vec![
                            NetMessage::system("Please type `new` or `sync`"),
                        ])
                        .with_input("new or sync")
                    }
                }
            }
            OnboardingState::ChooseNewOrSync => {
                // Handled by button clicks in UI
                DialogResponse::messages(vec![])
            }
            _ => {
                DialogResponse::messages(vec![
                    NetMessage::system("Onboarding in progress...")
                ])
            }
        }
    }
    
    fn handle_create_group(&mut self, input: &str) -> DialogResponse {
        DialogResponse::messages(vec![
            NetMessage::system("👥 Creating MLS group..."),
            NetMessage::system("Group created! Invite your contacts to start secure messaging."),
        ])
    }
    
    fn handle_settings(&mut self, input: &str) -> DialogResponse {
        DialogResponse::messages(vec![
            NetMessage::system("⚙️ Settings:\n- `/theme` - Toggle dark/light\n- `/relays` - Manage Nostr relays\n- `/backup` - Export backup\n- `/clear` - Clear chat history"),
        ])
    }
    
    fn handle_add_contact(&mut self, input: &str) -> DialogResponse {
        DialogResponse::messages(vec![
            NetMessage::system("👤 Adding contact...")
        ])
    }
    
    fn handle_help(&mut self, input: &str) -> DialogResponse {
        DialogResponse::messages(vec![
            NetMessage::system("📚 **Thoth Commands**:\n\n**Identity**:\n- `/new` - Create new identity\n- `/sync` - Restore from backup\n- `/backup` - Show backup phrase\n\n**Groups**:\n- `/create` - Create MLS group\n- `/invite` - Invite to group\n- `/join` - Join group\n\n**Settings**:\n- `/theme` - Toggle theme\n- `/relays` - Manage relays\n- `/clear` - Clear history\n\n**Help**:\n- `/help` - This message\n- `/tutorial` - Interactive tutorial"),
        ])
    }
}

impl Default for DialogManager {
    fn default() -> Self {
        Self::new()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_dialog_creation() {
        let mut dm = DialogManager::new();
        let resp = dm.start_onboarding();
        assert!(!resp.messages.is_empty());
        assert!(resp.requires_input);
    }
    
    #[test]
    fn test_onboarding_flow() {
        let mut dm = DialogManager::new();
        dm.start_onboarding();
        
        let resp = dm.process("new");
        assert!(!resp.messages.is_empty());
    }
}
