//! Persistent configuration and key storage

use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs;

/// Application configuration
#[derive(Debug, Clone, Serialize, Deserialize, Default)]
pub struct AppConfig {
    /// User's nostr public key (bech32)
    pub nostr_public_key: Option<String>,
    /// Derived nostr secret key (hex)
    pub nostr_secret_key_hex: Option<String>,
    /// BIP39 mnemonic (backup phrase) – in production this must be encrypted at rest
    pub mnemonic_encrypted: Option<String>,
    /// Device name
    pub device_name: Option<String>,
    /// Whether onboarding has been completed
    pub onboarding_completed: bool,
    /// Theme preference
    pub theme: String,
    /// Plasma shader configuration
    #[serde(default)]
    pub plasma: PlasmaConfig,
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
}

impl Default for PlasmaConfig {
    fn default() -> Self {
        Self {
            enabled: true,
            speed: default_speed(),
            dark_colors: default_dark_colors(),
            light_colors: default_light_colors(),
            dark_blend: default_dark_blend(),
            light_blend: default_light_blend(),
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

fn default_dark_colors() -> [f32; 9] {
    // c1(r,g,b), c2(r,g,b), c3(r,g,b) — vibrant purple/teal/magenta
    [0.12, 0.04, 0.24,  0.04, 0.14, 0.22,  0.18, 0.06, 0.20]
}

fn default_light_colors() -> [f32; 9] {
    // c1(r,g,b), c2(r,g,b), c3(r,g,b) — soft warm pastels
    [0.85, 0.82, 0.90,  0.82, 0.88, 0.92,  0.90, 0.84, 0.88]
}

impl AppConfig {
    pub fn new() -> Self {
        Self::default()
    }

    /// Load config from file
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

    /// Save config to file
    pub fn save(&self, path: &Path) -> Result<()> {
        // Ensure parent directory exists
        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent)?;
        }
        
        let content = toml::to_string_pretty(self)
            .context("Failed to serialize config")?;
        
        fs::write(path, content)?;
        Ok(())
    }

    /// Check if user has keys (onboarding completed)
    pub fn has_keys(&self) -> bool {
        self.onboarding_completed && self.nostr_public_key.is_some()
    }

    /// Get default config path
    pub fn default_path() -> Result<PathBuf> {
        let dirs = directories::BaseDirs::new()
            .context("Failed to get base directories")?;
        
        let config_dir = dirs.config_dir().join("thoth");
        Ok(config_dir.join("config.toml"))
    }
}

/// Check if onboarding is needed
pub fn needs_onboarding() -> bool {
    match get_config_path().exists() {
        false => true,
        true => {
            match AppConfig::load(&get_config_path()) {
                Ok(config) => !config.has_keys(),
                Err(_) => true,
            }
        }
    }
}

/// Mark onboarding as completed (legacy)
pub fn complete_onboarding(public_key: &str) -> Result<()> {
    let path = get_config_path();
    let mut config = AppConfig::load(&path).unwrap_or_default();
    
    config.nostr_public_key = Some(public_key.to_string());
    config.onboarding_completed = true;
    
    config.save(&path)?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;

    #[test]
    fn test_config_roundtrip() {
        let temp_dir = tempfile::tempdir().unwrap();
        let path = temp_dir.path().join("test_config.toml");
        
        let mut config = AppConfig::new();
        config.nostr_public_key = Some("test_key".to_string());
        config.onboarding_completed = true;
        
        config.save(&path).unwrap();
        let loaded = AppConfig::load(&path).unwrap();
        
        assert_eq!(loaded.nostr_public_key, Some("test_key".to_string()));
        assert!(loaded.onboarding_completed);
    }
}

/// Get the config file path for the current platform
pub fn get_config_path() -> PathBuf {
    #[cfg(target_os = "android")]
    {
        // On Android, use app‑private files directory
        PathBuf::from("/data/data/com.example.Thoth/files/config.toml")
    }
    #[cfg(not(target_os = "android"))]
    {
        let dirs = directories::BaseDirs::new().expect("no home directory");
        dirs.config_dir().join("thoth").join("config.toml")
    }
}

/// Complete onboarding with full key storage and memvid logging
pub fn complete_onboarding_full(
    public_key: &str,
    device_name: &str,
    mem_handle: Option<&crate::mem::worker::MemvidHandle>,
) -> Result<()> {
    // 1. Save to config (legacy)
    complete_onboarding(public_key)?;
    
    // 2. Initialize key storage
    let key_storage = crate::key_storage::KeyStorage::new()?;
    
        // 3. Log onboarding events to memvid (if handle provided) - TODO: re-enable with new schema
        if let Some(_handle) = mem_handle {
            // use crate::mem::{log_onboarding_event, OnboardingEvent};
            // log_onboarding_event(handle, OnboardingEvent::IdentityCreated { ... });
        }
    
    Ok(())
}

/// Automatic onboarding check - called from memvid initialization
/// Automatic onboarding check - called from memvid initialization
pub fn check_and_handle_onboarding_auto(mem_handle: crate::mem::worker::MemvidHandle, shell_count: usize) {
    eprintln!("DEBUG: check_and_handle_onboarding_auto called");
    
    if !needs_onboarding() {
        eprintln!("DEBUG: onboarding NOT needed (already complete)");
        return;
    }
    
    eprintln!("🎉 Onboarding needed! Setting up...");
    
    // Get device name
    let device_name = hostname::get()
        .unwrap_or_else(|_| "Unknown".into())
        .to_string_lossy()
        .to_string();
    
    // Generate test pubkey
    let test_pubkey = format!("test_{}", uuid::Uuid::new_v4());
    
    // Save to key storage
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
    
    // Complete onboarding
    let _ = complete_onboarding_full(&test_pubkey, &device_name, Some(&mem_handle));
    
    eprintln!("✅ Onboarding complete! PubKey: {}", test_pubkey);
}
