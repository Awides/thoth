//! App shell types — switchable application contexts.
//!
//! An "app shell" defines what happens when the user types a message:
//! - Which Rhai script handles the input
//! - Whether messages are published (Public), sent via MLS (Private), or kept local (Local)
//! - Which MLS channels the app participates in
//! - Which agent personality the app uses
//!
//! Built-in shells: social (microblog), dm (private chat), agent (local inference)
//! Users can create custom shells backed by Rhai scripts.

use serde::{Serialize, Deserialize};

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub enum PublishMode {
    Public,
    Private,
    Local,
}

impl Default for PublishMode {
    fn default() -> Self { PublishMode::Local }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AppShell {
    pub name: String,
    pub description: String,
    pub publish_mode: PublishMode,
    pub channels: Vec<String>,
    pub agent: String,
    pub rhai_script: Option<String>,
    pub icon: String,
}

impl AppShell {
    pub fn social() -> Self {
        Self {
            name: "social".into(),
            description: "Microblog — publish to Nostr, view public feed".into(),
            publish_mode: PublishMode::Public,
            channels: vec![],
            agent: "tot".into(),
            rhai_script: Some("social".into()),
            icon: "🌐".into(),
        }
    }

    pub fn dm() -> Self {
        Self {
            name: "dm".into(),
            description: "Private chat — MLS-encrypted direct messages".into(),
            publish_mode: PublishMode::Private,
            channels: vec!["dm".into()],
            agent: "tot".into(),
            rhai_script: Some("dm".into()),
            icon: "🔒".into(),
        }
    }

    pub fn agent() -> Self {
        Self {
            name: "agent".into(),
            description: "Local AI — chat with @Tot, commands stay private".into(),
            publish_mode: PublishMode::Local,
            channels: vec![],
            agent: "tot".into(),
            rhai_script: None,
            icon: "🤖".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct ShellManager {
    pub shells: Vec<AppShell>,
    pub active: String,
}

impl ShellManager {
    pub fn new() -> Self {
        let mut mgr = Self::default();
        if mgr.shells.is_empty() {
            mgr.add_builtins();
        }
        mgr.active = "agent".to_string();
        mgr
    }

    fn add_builtins(&mut self) {
        self.shells.push(AppShell::social());
        self.shells.push(AppShell::dm());
        self.shells.push(AppShell::agent());
    }

    pub fn active(&self) -> &AppShell {
        self.shells.iter().find(|s| s.name == self.active).unwrap_or_else(|| {
            static DEFAULT: std::sync::OnceLock<AppShell> = std::sync::OnceLock::new();
            DEFAULT.get_or_init(AppShell::agent)
        })
    }

    pub fn switch(&mut self, name: &str) -> Result<AppShell, String> {
        if let Some(shell) = self.shells.iter().find(|s| s.name == name).cloned() {
            self.active = name.to_string();
            Ok(shell)
        } else {
            let available: Vec<&str> = self.shells.iter().map(|s| s.name.as_str()).collect();
            Err(format!("Unknown shell: `{}`. Available: {}", name, available.join(" · ")))
        }
    }

    pub fn add_shell(&mut self, shell: AppShell) {
        self.shells.push(shell);
    }

    pub fn remove_shell(&mut self, name: &str) -> Result<(), String> {
        if name == "agent" {
            return Err("Cannot remove the default shell.".into());
        }
        let before = self.shells.len();
        self.shells.retain(|s| s.name != name);
        if self.shells.len() == before {
            return Err(format!("Shell `{}` not found.", name));
        }
        if self.active == name {
            self.active = "agent".to_string();
        }
        Ok(())
    }

    pub fn list_display(&self) -> Vec<(String, String, String, bool)> {
        self.shells.iter().map(|s| {
            (s.name.clone(), s.description.clone(), s.icon.clone(), s.name == self.active)
        }).collect()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_shell_is_agent() {
        let mgr = ShellManager::new();
        assert_eq!(mgr.active().name, "agent");
    }

    #[test]
    fn test_switch_shell() {
        let mut mgr = ShellManager::new();
        let shell = mgr.switch("social").unwrap();
        assert_eq!(shell.publish_mode, PublishMode::Public);
        assert_eq!(mgr.active, "social");
    }

    #[test]
    fn test_switch_unknown() {
        let mut mgr = ShellManager::new();
        assert!(mgr.switch("nonexistent").is_err());
    }

    #[test]
    fn test_cannot_remove_agent() {
        let mut mgr = ShellManager::new();
        assert!(mgr.remove_shell("agent").is_err());
    }

    #[test]
    fn test_builtin_shells_count() {
        let mgr = ShellManager::new();
        assert!(mgr.shells.len() >= 3);
    }
}
