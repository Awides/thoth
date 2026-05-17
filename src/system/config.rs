use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
#[cfg(not(target_arch = "wasm32"))]
use std::path::{Path, PathBuf};
#[cfg(not(target_arch = "wasm32"))]
use std::fs;

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    pub nostr_public_key: Option<String>,
    pub nostr_secret_key_hex: Option<String>,
    pub mnemonic_encrypted: Option<String>,
    pub device_name: Option<String>,
    pub onboarding_completed: bool,
    pub theme: String,
    #[serde(default)]
    pub plasma: PlasmaConfig,
    #[serde(default = "default_agents")]
    pub agents: Vec<crate::system::agent::AgentConfig>,
    #[serde(default = "default_active_agent")]
    pub active_agent: String,
    #[serde(default = "default_shells")]
    pub shells: Vec<crate::system::app_shell::AppShell>,
    #[serde(default = "default_active_shell")]
    pub active_shell: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PlasmaConfig {
    #[serde(default = "default_true")]
    pub enabled: bool,
    #[serde(default = "default_speed")]
    pub speed: f32,
    #[serde(default = "default_dark_colors")]
    pub dark_colors: [f32; 9],
    #[serde(default = "default_light_colors")]
    pub light_colors: [f32; 9],
    #[serde(default = "default_dark_blend")]
    pub dark_blend: String,
    #[serde(default = "default_light_blend")]
    pub light_blend: String,
    #[serde(default = "default_pattern")]
    pub pattern: String,
}

pub const PLASMA_PATTERNS: &[&str] = &[
    "plasma", "aurora", "warp", "cells", "ocean", "fire", "nebula",
];

impl Default for PlasmaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: default_speed(),
            dark_colors: default_dark_colors(),
            light_colors: default_light_colors(),
            dark_blend: default_dark_blend(),
            light_blend: default_light_blend(),
            pattern: default_pattern(),
        }
    }
}

pub const BLEND_MODES: &[&str] = &[
    "normal", "multiply", "screen", "overlay", "darken", "lighten",
    "color-dodge", "color-burn", "hard-light", "soft-light",
    "difference", "exclusion", "hue", "saturation", "color", "luminosity",
];

pub fn default_dark_blend() -> String { "screen".to_string() }
pub fn default_light_blend() -> String { "multiply".to_string() }
fn default_true() -> bool { true }
fn default_speed() -> f32 { 1.0 }
fn default_pattern() -> String { "plasma".to_string() }
fn default_agents() -> Vec<crate::system::agent::AgentConfig> { Vec::new() }
fn default_active_agent() -> String { "tot".to_string() }
fn default_shells() -> Vec<crate::system::app_shell::AppShell> { Vec::new() }
fn default_active_shell() -> String { "agent".to_string() }

fn default_dark_colors() -> [f32; 9] {
    [0.12, 0.04, 0.24, 0.04, 0.14, 0.22, 0.18, 0.06, 0.20]
}

fn default_light_colors() -> [f32; 9] {
    [0.85, 0.82, 0.90, 0.82, 0.88, 0.92, 0.90, 0.84, 0.88]
}

impl AppConfig {
    pub fn new() -> Self {
        Self::default()
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load(path: &Path) -> Result<Self> {
        if !path.exists() {
            return Ok(Self::new());
        }
        let content = fs::read_to_string(path)
            .context("Failed to read config file")?;
        let config: Self = toml::from_str(&content)
            .context("Failed to parse config file")?;
        Ok(config)
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load(_path: &std::path::Path) -> Result<Self> {
        Self::load_from_local_storage().unwrap_or_else(|| Ok(Self::new()))
    }

    #[cfg(target_arch = "wasm32")]
    fn load_from_local_storage() -> Option<Result<Self>> {
        let window = web_sys::window()?;
        let storage = window.local_storage().ok()??;
        let val = storage.get_item("thoth_config").ok()??;
        Some(serde_json::from_str(&val).map_err(|e| anyhow::anyhow!("localStorage parse: {}", e)))
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save(&self, path: &Path) -> Result<()> {
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        fs::write(path, content)?;
        Ok(())
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save(&self, _path: &std::path::Path) -> Result<()> {
        let window = web_sys::window().context("no window")?;
        let storage = window.local_storage().map_err(|_| anyhow::anyhow!("no localStorage"))?
            .context("localStorage unavailable")?;
        let json = serde_json::to_string(self).context("serialize config")?;
        storage.set_item("thoth_config", &json)
            .map_err(|_| anyhow::anyhow!("localStorage write failed"))?;
        Ok(())
    }

    pub fn has_keys(&self) -> bool {
        self.onboarding_completed && self.nostr_public_key.is_some()
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn needs_onboarding() -> bool {
    let path = get_config_path();
    if !path.exists() { return true; }
    match AppConfig::load(&path) {
        Ok(config) => !config.has_keys(),
        Err(_) => true,
    }
}

#[cfg(target_arch = "wasm32")]
pub fn needs_onboarding() -> bool {
    match AppConfig::load(&get_config_path()) {
        Ok(config) => !config.has_keys(),
        Err(_) => true,
    }
}

#[cfg(not(target_arch = "wasm32"))]
pub fn complete_onboarding(public_key: &str) -> Result<()> {
    let path = get_config_path();
    let mut config = AppConfig::load(&path).unwrap_or_default();
    config.nostr_public_key = Some(public_key.to_string());
    config.onboarding_completed = true;
    config.save(&path)?;
    Ok(())
}

#[cfg(target_arch = "wasm32")]
pub fn complete_onboarding(_public_key: &str) -> Result<()> {
    Ok(())
}

pub fn complete_onboarding_full(
    _public_key: &str,
    _device_name: &str,
    _mem_handle: Option<&crate::mem::MemvidHandle>,
) -> Result<()> {
    #[cfg(not(target_arch = "wasm32"))]
    {
        complete_onboarding(_public_key)?;
        if let Ok(key_storage) = crate::key_storage::KeyStorage::new() {
            let _ = key_storage.save_master_seed(b"test_seed");
            let _ = key_storage.save_device_key(b"test_key");
        }
    }
    Ok(())
}

pub fn check_and_handle_onboarding_auto(_mem_handle: crate::mem::MemvidHandle, _app_count: usize) {
    #[cfg(not(target_arch = "wasm32"))]
    {
        if !needs_onboarding() { return; }
        let device_name = crate::shared::get_hostname();
        let test_pubkey = format!("test_{}", uuid::Uuid::new_v4());
        if let Ok(key_storage) = crate::key_storage::KeyStorage::new() {
            let _ = key_storage.save_master_seed(b"test_seed");
            let _ = key_storage.save_device_key(b"test_key");
            use crate::key_storage::KeyMetadata;
            let metadata = KeyMetadata {
                created_at: chrono::Utc::now().to_rfc3339(),
                device_name: device_name.clone(),
                pubkey: test_pubkey.clone(),
                key_type: crate::key_storage::KeyType::DeviceKey,
            };
            let _ = key_storage.save_metadata(&test_pubkey, &metadata);
        }
        let _ = complete_onboarding_full(&test_pubkey, &device_name, Some(&_mem_handle));
    }
}

#[cfg(target_os = "android")]
pub fn get_config_path() -> std::path::PathBuf {
    std::path::PathBuf::from("/data/data/com.example.Thoth/files/config.toml")
}

#[cfg(all(not(target_os = "android"), not(target_arch = "wasm32")))]
pub fn get_config_path() -> std::path::PathBuf {
    let dirs = directories::BaseDirs::new().expect("no home directory");
    dirs.config_dir().join("thoth").join("config.toml")
}

#[cfg(target_arch = "wasm32")]
pub fn get_config_path() -> std::path::PathBuf {
    std::path::PathBuf::from("/tmp/thoth/config.toml")
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_config_default() {
        let config = AppConfig::new();
        assert!(!config.onboarding_completed);
    }
}
