//! MLS group registry — tracks group metadata, membership, and associated shells.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum GroupType {
    Device,
    Chat,
    Shell { shell_name: String },
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupInfo {
    pub group_id: String,
    pub name: String,
    pub group_type: GroupType,
    pub members: Vec<String>,
    pub creator: String,
    pub created_at: u64,
}

impl Default for GroupInfo {
    fn default() -> Self {
        Self {
            group_id: String::new(),
            name: String::new(),
            group_type: GroupType::Device,
            members: Vec::new(),
            creator: String::new(),
            created_at: 0,
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct GroupRegistry {
    groups: HashMap<String, GroupInfo>,
    pending_invites: Vec<PendingInvite>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PendingInvite {
    pub group_id: String,
    pub sender: String,
    pub welcome_bytes: Vec<u8>,
    pub received_at: u64,
}

impl GroupRegistry {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn register(&mut self, info: GroupInfo) {
        self.groups.insert(info.group_id.clone(), info);
    }

    pub fn get(&self, group_id: &str) -> Option<&GroupInfo> {
        self.groups.get(group_id)
    }

    pub fn get_mut(&mut self, group_id: &str) -> Option<&mut GroupInfo> {
        self.groups.get_mut(group_id)
    }

    pub fn list(&self) -> Vec<&GroupInfo> {
        let mut entries: Vec<_> = self.groups.values().collect();
        entries.sort_by(|a, b| a.created_at.cmp(&b.created_at));
        entries
    }

    pub fn update_members(&mut self, group_id: &str, members: Vec<String>) {
        if let Some(info) = self.groups.get_mut(group_id) {
            info.members = members;
        }
    }

    pub fn add_pending_invite(&mut self, invite: PendingInvite) {
        self.pending_invites.push(invite);
    }

    pub fn take_pending_invite(&mut self, group_id: &str) -> Option<PendingInvite> {
        let idx = self.pending_invites.iter().position(|i| i.group_id == group_id)?;
        Some(self.pending_invites.remove(idx))
    }

    pub fn pending_invites(&self) -> &[PendingInvite] {
        &self.pending_invites
    }

    pub fn has_group(&self, group_id: &str) -> bool {
        self.groups.contains_key(group_id)
    }

    pub fn group_for_shell(&self, shell_name: &str) -> Option<&GroupInfo> {
        self.groups.values().find(|g| {
            matches!(&g.group_type, GroupType::Shell { shell_name: sn } if sn == shell_name)
        })
    }

    pub fn dm_group(&self) -> Option<&GroupInfo> {
        self.group_for_shell("dm")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_register_and_get() {
        let mut reg = GroupRegistry::new();
        let info = GroupInfo {
            group_id: "test".into(),
            name: "Test Group".into(),
            group_type: GroupType::Chat,
            members: vec!["alice".into()],
            creator: "alice".into(),
            created_at: 100,
        };
        reg.register(info);
        assert!(reg.has_group("test"));
        assert_eq!(reg.get("test").unwrap().name, "Test Group");
    }

    #[test]
    fn test_pending_invite() {
        let mut reg = GroupRegistry::new();
        let invite = PendingInvite {
            group_id: "g1".into(),
            sender: "alice".into(),
            welcome_bytes: vec![1, 2, 3],
            received_at: 100,
        };
        reg.add_pending_invite(invite);
        assert_eq!(reg.pending_invites().len(), 1);
        let taken = reg.take_pending_invite("g1").unwrap();
        assert_eq!(taken.sender, "alice");
        assert!(reg.pending_invites().is_empty());
    }

    #[test]
    fn test_shell_group_lookup() {
        let mut reg = GroupRegistry::new();
        reg.register(GroupInfo {
            group_id: "dm-group".into(),
            name: "DM Group".into(),
            group_type: GroupType::Shell { shell_name: "dm".into() },
            members: vec![],
            creator: "alice".into(),
            created_at: 0,
        });
        assert!(reg.dm_group().is_some());
        assert!(reg.group_for_shell("social").is_none());
    }
}
