//! Agent types — per-agent configuration for identity, system prompt, and tools.
//!
//! An "agent" is a named configuration that defines:
//! - Agent identity (name, personality)
//! - System prompt
//! - Available tools (subset of all loaded tools)
//! - Model parameters (temperature, etc.)
//!
//! Users can switch between agents with `/agent <name>` to change the agent's
//! behavior without restarting. Agents are persisted in the app config.

use serde::{Serialize, Deserialize};
use std::collections::HashMap;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
pub struct AgentConfig {
    pub name: String,
    pub agent_name: String,
    pub system_prompt: String,
    pub tools: Vec<String>,
    pub temperature: f32,
    pub top_p: f32,
    pub description: String,
}

impl Default for AgentConfig {
    fn default() -> Self {
        Self::tot()
    }
}

impl AgentConfig {
    pub fn tot() -> Self {
        Self {
            name: "tot".into(),
            agent_name: "Tot".into(),
            system_prompt: "You are Tot, a helpful and concise AI assistant. You respond in the user's language. Be friendly but brief. You can use the provided tools when helpful.".into(),
            tools: vec!["current_time".into(), "calculator".into()],
            temperature: 0.5,
            top_p: 0.85,
            description: "Default assistant — friendly, concise, bilingual".into(),
        }
    }

    pub fn coder() -> Self {
        Self {
            name: "coder".into(),
            agent_name: "Coder".into(),
            system_prompt: "You are Coder, a precise and technical AI assistant focused on programming. Provide exact code, concise explanations, and prefer working solutions over discussion. Use tools when they help with calculations or data.".into(),
            tools: vec!["calculator".into()],
            temperature: 0.3,
            top_p: 0.9,
            description: "Code-focused — precise, technical, low temperature".into(),
        }
    }

    pub fn creative() -> Self {
        Self {
            name: "creative".into(),
            agent_name: "Muse".into(),
            system_prompt: "You are Muse, a creative and imaginative AI assistant. Think laterally, suggest unusual ideas, and explore possibilities. Be playful and expressive. You can use tools for calculations when needed.".into(),
            tools: vec!["calculator".into(), "current_time".into()],
            temperature: 0.8,
            top_p: 0.95,
            description: "Creative — high temperature, lateral thinking".into(),
        }
    }

    pub fn analyst() -> Self {
        Self {
            name: "analyst".into(),
            agent_name: "Analyst".into(),
            system_prompt: "You are Analyst, a methodical and thorough AI assistant. Break down problems step by step, show your reasoning, and verify conclusions. Use tools for precision. Be structured and detailed.".into(),
            tools: vec!["calculator".into(), "current_time".into()],
            temperature: 0.2,
            top_p: 0.8,
            description: "Analytical — methodical, structured, very low temperature".into(),
        }
    }

    pub fn minimal() -> Self {
        Self {
            name: "minimal".into(),
            agent_name: "Bot".into(),
            system_prompt: "You are Bot. Respond as briefly as possible. One sentence maximum unless asked for more.".into(),
            tools: vec![],
            temperature: 0.4,
            top_p: 0.8,
            description: "Minimal — ultra-brief responses".into(),
        }
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AgentManager {
    pub agents: HashMap<String, AgentConfig>,
    pub active: String,
}

impl AgentManager {
    pub fn new() -> Self {
        let mut mgr = Self::default();
        if mgr.agents.is_empty() {
            mgr.add_builtins();
        }
        mgr.active = "tot".to_string();
        mgr
    }

    fn add_builtins(&mut self) {
        for agent in &[
            AgentConfig::tot(),
            AgentConfig::coder(),
            AgentConfig::creative(),
            AgentConfig::analyst(),
            AgentConfig::minimal(),
        ] {
            self.agents.insert(agent.name.clone(), agent.clone());
        }
    }

    pub fn active(&self) -> &AgentConfig {
        self.agents.get(&self.active).unwrap_or_else(|| {
            static DEFAULT: std::sync::OnceLock<AgentConfig> = std::sync::OnceLock::new();
            DEFAULT.get_or_init(AgentConfig::tot)
        })
    }

    pub fn switch(&mut self, name: &str) -> Result<AgentConfig, String> {
        if let Some(agent) = self.agents.get(name).cloned() {
            self.active = name.to_string();
            Ok(agent)
        } else {
            Err(format!("Unknown agent: `{}`. Available: {}", name, self.list_names().join(" · ")))
        }
    }

    pub fn add_agent(&mut self, agent: AgentConfig) {
        self.agents.insert(agent.name.clone(), agent);
    }

    pub fn remove_agent(&mut self, name: &str) -> Result<(), String> {
        if name == "tot" {
            return Err("Cannot remove the default agent.".into());
        }
        if self.agents.remove(name).is_none() {
            return Err(format!("Agent `{}` not found.", name));
        }
        if self.active == name {
            self.active = "tot".to_string();
        }
        Ok(())
    }

    pub fn list_names(&self) -> Vec<String> {
        let mut names: Vec<String> = self.agents.keys().cloned().collect();
        names.sort();
        names
    }

    pub fn list_display(&self) -> Vec<(String, String, bool)> {
        let mut entries: Vec<_> = self.agents.values().map(|a| {
            (a.name.clone(), a.description.clone(), a.name == self.active)
        }).collect();
        entries.sort_by(|a, b| a.0.cmp(&b.0));
        entries
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_default_agent_is_tot() {
        let mgr = AgentManager::new();
        assert_eq!(mgr.active().name, "tot");
        assert_eq!(mgr.active().agent_name, "Tot");
    }

    #[test]
    fn test_switch_agent() {
        let mut mgr = AgentManager::new();
        let agent = mgr.switch("coder").unwrap();
        assert_eq!(agent.agent_name, "Coder");
        assert_eq!(mgr.active, "coder");
    }

    #[test]
    fn test_switch_unknown() {
        let mut mgr = AgentManager::new();
        assert!(mgr.switch("nonexistent").is_err());
    }

    #[test]
    fn test_cannot_remove_tot() {
        let mut mgr = AgentManager::new();
        assert!(mgr.remove_agent("tot").is_err());
    }

    #[test]
    fn test_add_custom_agent() {
        let mut mgr = AgentManager::new();
        let custom = AgentConfig {
            name: "custom".into(),
            agent_name: "CustomBot".into(),
            system_prompt: "You are CustomBot.".into(),
            tools: vec![],
            temperature: 0.5,
            top_p: 0.85,
            description: "A custom agent".into(),
        };
        mgr.add_agent(custom);
        assert!(mgr.switch("custom").is_ok());
    }

    #[test]
    fn test_builtin_agents_count() {
        let mgr = AgentManager::new();
        assert!(mgr.agents.len() >= 5);
    }
}
