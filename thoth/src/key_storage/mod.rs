//! Simple key storage - separate from memvid event log
//!
//! Stores encrypted keys and metadata in ~/.thoth/keys/
//! Keeps credentials separate from event-sourced world state

use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};
use std::fs;

/// Key metadata (unencrypted)
#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyMetadata {
    pub created_at: String,
    pub device_name: String,
    pub pubkey: String, // Nostr public key (hex or bech32)
    pub key_type: KeyType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KeyType {
    MasterSeed,    // BIP39 mnemonic
    DeviceKey,     // Nostr nsec
}

/// Key storage manager
pub struct KeyStorage {
    base_path: PathBuf,
}

impl KeyStorage {
    pub fn new() -> Result<Self> {
        let base_path = directories::BaseDirs::new()
            .context("no home directory")?
            .home_dir()
            .join(".thoth")
            .join("keys");
        
        fs::create_dir_all(&base_path)
            .context("failed to create key storage directory")?;
        
        Ok(Self { base_path })
    }
    
    /// Check if keys exist
    pub fn has_keys(&self) -> bool {
        self.base_path.join("master_seed.enc").exists()
    }
    
    /// Save encrypted master seed
    pub fn save_master_seed(&self, encrypted_seed: &[u8]) -> Result<()> {
        let path = self.base_path.join("master_seed.enc");
        fs::write(&path, encrypted_seed)
            .context("failed to write master seed")
    }
    
    /// Load encrypted master seed
    pub fn load_master_seed(&self) -> Result<Vec<u8>> {
        let path = self.base_path.join("master_seed.enc");
        fs::read(&path).context("failed to read master seed")
    }
    
    /// Save encrypted device key
    pub fn save_device_key(&self, encrypted_key: &[u8]) -> Result<()> {
        let path = self.base_path.join("device_key.enc");
        fs::write(&path, encrypted_key)
            .context("failed to write device key")
    }
    
    /// Load encrypted device key
    pub fn load_device_key(&self) -> Result<Vec<u8>> {
        let path = self.base_path.join("device_key.enc");
        fs::read(&path).context("failed to read device key")
    }
    
    /// Save key metadata (unencrypted)
    pub fn save_metadata(&self, pubkey: &str, metadata: &KeyMetadata) -> Result<()> {
        let path = self.base_path.join("keyring.json");
        let mut keyring: Vec<(String, KeyMetadata)> = if path.exists() {
            let data = fs::read_to_string(&path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        
        // Upsert
        keyring.retain(|(k, _)| k != pubkey);
        keyring.push((pubkey.to_string(), metadata.clone()));
        
        let data = serde_json::to_string_pretty(&keyring)?;
        fs::write(&path, data).context("failed to write keyring")
    }
    
    /// Get key directory path (for backup)
    pub fn key_dir(&self) -> &Path {
        &self.base_path
    }
}

/// Placeholder: encrypt data with user password
/// TODO: Use proper encryption (AES-GCM with PBKDF2)
pub fn encrypt_with_password(data: &[u8], _password: &str) -> Result<Vec<u8>> {
    // For now, just return the data (placeholder for real encryption)
    Ok(data.to_vec())
}

/// Placeholder: decrypt data with user password
pub fn decrypt_with_password(encrypted: &[u8], _password: &str) -> Result<Vec<u8>> {
    // For now, just return the data (placeholder for real encryption)
    Ok(encrypted.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;
    
    #[test]
    fn test_key_storage_creation() {
        let storage = KeyStorage::new().unwrap();
        assert!(storage.base_path.exists());
    }
    
    #[test]
    fn test_has_keys() {
        let storage = KeyStorage::new().unwrap();
        // Should be false initially
        assert!(!storage.has_keys());
    }
}
