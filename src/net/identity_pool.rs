//! Multi-identity management for concurrent Nostr identities.
//!
//! Each identity (user or agent) gets its own:
//! - Nostr client connection
//! - MDK instance (MLS groups, key packages)
//! - Event listener loop
//! - Group registry
//!
//! The IdentityPool manages all slots:
//! - **Foreground**: the active identity shown in the UI
//! - **Background**: still connected, processing events, agent work
//! - **Dormant**: disconnected, not processing (future: push wake)
//!
//! This enables:
//! - Single-process E2E testing (two identities in one Thoth)
//! - Agent identities that run alongside the user's identity
//! - Push notifications waking a different identity

use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::net::{NetRuntime, NetEvent};

#[derive(Debug, Clone, PartialEq)]
pub enum IdentityType {
    User,
    Agent,
}

#[derive(Debug, Clone, PartialEq)]
pub enum SlotState {
    Foreground,
    Background,
    Dormant,
}

#[derive(Clone)]
pub struct IdentitySlot {
    pub pubkey_hex: String,
    pub label: String,
    pub identity_type: IdentityType,
    pub state: SlotState,
    pub runtime: Arc<NetRuntime>,
}

impl IdentitySlot {
    pub fn new(
        pubkey_hex: String,
        label: String,
        identity_type: IdentityType,
        event_tx: tokio::sync::mpsc::UnboundedSender<NetEvent>,
    ) -> Self {
        Self {
            pubkey_hex,
            label,
            identity_type,
            state: SlotState::Dormant,
            runtime: Arc::new(NetRuntime::new(event_tx)),
        }
    }
}

pub struct IdentityPool {
    slots: HashMap<String, IdentitySlot>,
    foreground: Option<String>,
    event_tx: tokio::sync::mpsc::UnboundedSender<NetEvent>,
}

impl IdentityPool {
    pub fn new(event_tx: tokio::sync::mpsc::UnboundedSender<NetEvent>) -> Self {
        Self {
            slots: HashMap::new(),
            foreground: None,
            event_tx,
        }
    }

    pub fn add(&mut self, pubkey_hex: String, label: String, identity_type: IdentityType) -> Result<(), String> {
        if self.slots.contains_key(&pubkey_hex) {
            return Err(format!("Identity {} already exists", &pubkey_hex[..8.min(pubkey_hex.len())]));
        }
        let mut slot = IdentitySlot::new(pubkey_hex.clone(), label, identity_type, self.event_tx.clone());
        if self.foreground.is_none() {
            slot.state = SlotState::Foreground;
            self.foreground = Some(pubkey_hex.clone());
        } else {
            slot.state = SlotState::Background;
        }
        info!("Added identity '{}' ({}) as {:?}", slot.label, &pubkey_hex[..8.min(pubkey_hex.len())], slot.state);
        self.slots.insert(pubkey_hex, slot);
        Ok(())
    }

    pub fn remove(&mut self, pubkey_hex: &str) -> Result<(), String> {
        if !self.slots.contains_key(pubkey_hex) {
            return Err(format!("Identity {} not found", &pubkey_hex[..8.min(pubkey_hex.len())]));
        }
        self.slots.remove(pubkey_hex);
        if self.foreground.as_deref() == Some(pubkey_hex) {
            self.foreground = self.slots.keys().next().cloned();
            if let Some(ref fg) = self.foreground {
                if let Some(slot) = self.slots.get_mut(fg) {
                    slot.state = SlotState::Foreground;
                    info!("Foreground moved to '{}' ({})", slot.label, &slot.pubkey_hex[..8.min(slot.pubkey_hex.len())]);
                }
            }
        }
        Ok(())
    }

    pub fn switch_foreground(&mut self, pubkey_hex: &str) -> Result<(), String> {
        if !self.slots.contains_key(pubkey_hex) {
            return Err(format!("Identity {} not found", &pubkey_hex[..8.min(pubkey_hex.len())]));
        }
        if let Some(ref old_fg) = self.foreground {
            if let Some(slot) = self.slots.get_mut(old_fg) {
                slot.state = SlotState::Background;
            }
        }
        if let Some(slot) = self.slots.get_mut(pubkey_hex) {
            slot.state = SlotState::Foreground;
            info!("Foreground switched to '{}' ({})", slot.label, &slot.pubkey_hex[..8.min(slot.pubkey_hex.len())]);
        }
        self.foreground = Some(pubkey_hex.to_string());
        Ok(())
    }

    pub fn foreground(&self) -> Option<&IdentitySlot> {
        self.foreground.as_ref().and_then(|pk| self.slots.get(pk))
    }

    pub fn foreground_mut(&mut self) -> Option<&mut IdentitySlot> {
        self.foreground.as_deref().and_then(|pk| self.slots.get_mut(pk))
    }

    pub fn get(&self, pubkey_hex: &str) -> Option<&IdentitySlot> {
        self.slots.get(pubkey_hex)
    }

    pub fn get_mut(&mut self, pubkey_hex: &str) -> Option<&mut IdentitySlot> {
        self.slots.get_mut(pubkey_hex)
    }

    pub fn list(&self) -> Vec<IdentityInfo> {
        self.slots.values().map(|s| IdentityInfo {
            pubkey_hex: s.pubkey_hex.clone(),
            label: s.label.clone(),
            identity_type: s.identity_type.clone(),
            state: s.state.clone(),
            is_foreground: self.foreground.as_deref() == Some(&s.pubkey_hex),
        }).collect()
    }

    pub fn len(&self) -> usize {
        self.slots.len()
    }

    pub fn is_empty(&self) -> bool {
        self.slots.is_empty()
    }

    pub fn all_runtimes(&self) -> Vec<Arc<NetRuntime>> {
        self.slots.values().map(|s| s.runtime.clone()).collect()
    }
}

#[derive(Debug, Clone)]
pub struct IdentityInfo {
    pub pubkey_hex: String,
    pub label: String,
    pub identity_type: IdentityType,
    pub state: SlotState,
    pub is_foreground: bool,
}

#[cfg(test)]
mod tests {
    use super::*;

    fn make_pool() -> IdentityPool {
        let (tx, _rx) = tokio::sync::mpsc::unbounded_channel();
        IdentityPool::new(tx)
    }

    #[test]
    fn test_add_first_becomes_foreground() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        assert_eq!(pool.len(), 1);
        let fg = pool.foreground().unwrap();
        assert_eq!(fg.pubkey_hex, "aa11");
        assert_eq!(fg.state, SlotState::Foreground);
    }

    #[test]
    fn test_add_second_is_background() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        pool.add("bb22".to_string(), "bob".to_string(), IdentityType::User).unwrap();
        assert_eq!(pool.len(), 2);
        let bob = pool.get("bb22").unwrap();
        assert_eq!(bob.state, SlotState::Background);
    }

    #[test]
    fn test_switch_foreground() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        pool.add("bb22".to_string(), "bob".to_string(), IdentityType::User).unwrap();
        pool.switch_foreground("bb22").unwrap();
        assert_eq!(pool.foreground().unwrap().pubkey_hex, "bb22");
        assert_eq!(pool.get("aa11").unwrap().state, SlotState::Background);
        assert_eq!(pool.get("bb22").unwrap().state, SlotState::Foreground);
    }

    #[test]
    fn test_add_duplicate_fails() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        assert!(pool.add("aa11".to_string(), "alice2".to_string(), IdentityType::User).is_err());
    }

    #[test]
    fn test_remove_foreground_promotes_next() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        pool.add("bb22".to_string(), "bob".to_string(), IdentityType::User).unwrap();
        pool.remove("aa11").unwrap();
        assert_eq!(pool.len(), 1);
        assert_eq!(pool.foreground().unwrap().pubkey_hex, "bb22");
    }

    #[test]
    fn test_switch_nonexistent_fails() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        assert!(pool.switch_foreground("zz99").is_err());
    }

    #[test]
    fn test_list_includes_foreground_flag() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        pool.add("bb22".to_string(), "bob".to_string(), IdentityType::Agent).unwrap();
        let infos = pool.list();
        assert_eq!(infos.len(), 2);
        let alice_info = infos.iter().find(|i| i.pubkey_hex == "aa11").unwrap();
        assert!(alice_info.is_foreground);
        assert_eq!(alice_info.identity_type, IdentityType::User);
        let bob_info = infos.iter().find(|i| i.pubkey_hex == "bb22").unwrap();
        assert!(!bob_info.is_foreground);
        assert_eq!(bob_info.identity_type, IdentityType::Agent);
    }

    #[test]
    fn test_all_runtimes() {
        let mut pool = make_pool();
        pool.add("aa11".to_string(), "alice".to_string(), IdentityType::User).unwrap();
        pool.add("bb22".to_string(), "bob".to_string(), IdentityType::Agent).unwrap();
        assert_eq!(pool.all_runtimes().len(), 2);
    }
}
