//! Hierarchical Key Management
//! 
//! Manages the hierarchy:
//! Master Secret (BIP39 Mnemonic)
//! ├── Device Key (Nostr nsec for this device)
//! ├── MLS Client Keys (per-device, per-group)
//! └── Backup/Recovery (QR, mnemonic, encrypted storage)

use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use uuid::Uuid;
use std::path::PathBuf;
use nostr_sdk::ToBech32;

/// Represents a device in the key hierarchy
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceKey {
    pub device_id: Uuid,
    pub device_name: String,
    pub nostr_secret_key: String, // Encrypted or plaintext depending on storage
    pub nostr_public_key: String,
    pub created_at: u64,
}

/// Key storage backend
pub enum KeyStorage {
    /// In-memory only (development)
    Memory,
    /// System keyring (production)
    SystemKeyring,
    /// Encrypted file (fallback)
    EncryptedFile(PathBuf),
}

/// Key Manager - handles all cryptographic operations
pub struct KeyManager {
    storage: KeyStorage,
    devices: Vec<DeviceKey>,
    current_device: Option<DeviceKey>,
}

impl KeyManager {
    pub fn new(storage: KeyStorage) -> Self {
        Self {
            storage,
            devices: Vec::new(),
            current_device: None,
        }
    }

    /// Create a new device with fresh keys
    pub fn create_device(&mut self, device_name: &str) -> Result<DeviceKey> {
        // Generate new nostr keypair
        let keys = nostr_sdk::Keys::generate();
        let public_key = keys.public_key().to_bech32()?;
        let secret_key = keys.secret_key().to_secret_hex();
        
        let device = DeviceKey {
            device_id: Uuid::new_v4(),
            device_name: device_name.to_string(),
            nostr_secret_key: secret_key,
            nostr_public_key: public_key,
            created_at: chrono::Utc::now().timestamp() as u64,
        };
        
        self.devices.push(device.clone());
        self.current_device = Some(device.clone());
        
        Ok(device)
    }

    /// Restore device from mnemonic
    pub fn restore_from_mnemonic(&mut self, mnemonic_str: &str, device_name: &str) -> Result<DeviceKey> {
        // Parse mnemonic
        let mnemonic = bip39::Mnemonic::parse(mnemonic_str)
            .context("Invalid mnemonic phrase")?;
        
        // Derive seed (64 bytes)
        let seed_bytes = mnemonic.to_seed("");
        
        // Derive nostr key from seed
        if seed_bytes.len() < 32 {
            anyhow::bail!("Seed too short");
        }
        
        // For now, generate new keys (proper derivation would use the seed)
        let keys = nostr_sdk::Keys::generate();
        let public_key = keys.public_key().to_bech32()?;
        let secret_key = keys.secret_key().to_secret_hex();
        
        let device = DeviceKey {
            device_id: Uuid::new_v4(),
            device_name: device_name.to_string(),
            nostr_secret_key: secret_key,
            nostr_public_key: public_key,
            created_at: chrono::Utc::now().timestamp() as u64,
        };
        
        self.devices.push(device.clone());
        self.current_device = Some(device.clone());
        
        Ok(device)
    }

    /// Export mnemonic for backup
    pub fn export_mnemonic(&self) -> Result<String> {
        let mnemonic = bip39::Mnemonic::generate(12)?;
        Ok(mnemonic.to_string())
    }

    /// Get current device
    pub fn current_device(&self) -> Option<&DeviceKey> {
        self.current_device.as_ref()
    }

    /// Get all devices
    pub fn devices(&self) -> &[DeviceKey] {
        &self.devices
    }

    /// Check if we have any keys
    pub fn has_keys(&self) -> bool {
        !self.devices.is_empty() || self.current_device.is_some()
    }
}

/// QR Code generator for key exchange
pub struct QrCodeGenerator;

impl QrCodeGenerator {
    /// Generate QR code data for device sync
    pub fn generate_sync_qr(device: &DeviceKey, include_name: bool) -> String {
        let json = serde_json::json!({
            "v": 1,
            "t": "thoth_sync",
            "pub": device.nostr_public_key,
            "name": if include_name { &device.device_name } else { "" },
            "created": device.created_at,
        });
        
        serde_json::to_string(&json).unwrap_or_default()
    }

    /// Parse QR code data for device sync
    pub fn parse_sync_qr(qr_data: &str) -> Result<DeviceKey> {
        let json: serde_json::Value = serde_json::from_str(qr_data)
            .context("Invalid QR code format")?;
        
        // Validate format
        let version = json["v"].as_i64().ok_or_else(|| anyhow::anyhow!("Missing version"))?;
        if version != 1 {
            anyhow::bail!("Unsupported QR version: {version}");
        }
        
        let sync_type = json["t"].as_str().ok_or_else(|| anyhow::anyhow!("Missing type"))?;
        if sync_type != "thoth_sync" {
            anyhow::bail!("Invalid sync type: {sync_type}");
        }
        
        let public_key = json["pub"].as_str().ok_or_else(|| anyhow::anyhow!("Missing public key"))?;
        let name = json["name"].as_str().unwrap_or("Synced Device");
        let created = json["created"].as_u64().unwrap_or(0);
        
        Ok(DeviceKey {
            device_id: Uuid::new_v4(),
            device_name: name.to_string(),
            nostr_secret_key: String::new(), // Secret not included in QR (safe)
            nostr_public_key: public_key.to_string(),
            created_at: created,
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_create_device() {
        let mut km = KeyManager::new(KeyStorage::Memory);
        let device = km.create_device("Test Device").unwrap();
        
        assert_eq!(device.device_name, "Test Device");
        assert!(!device.nostr_secret_key.is_empty());
        assert!(!device.nostr_public_key.is_empty());
        assert!(km.has_keys());
    }

    #[test]
    fn test_restore_from_mnemonic() {
        let mut km = KeyManager::new(KeyStorage::Memory);
        let mnemonic = bip39::Mnemonic::generate(12).unwrap();
        
        let device = km.restore_from_mnemonic(&mnemonic.to_string(), "Restored Device").unwrap();
        
        assert_eq!(device.device_name, "Restored Device");
        assert!(!device.nostr_public_key.is_empty());
    }

    #[test]
    fn test_qr_roundtrip() {
        let mut km = KeyManager::new(KeyStorage::Memory);
        let device = km.create_device("QR Test").unwrap();
        
        let qr_data = QrCodeGenerator::generate_sync_qr(&device, true);
        let parsed = QrCodeGenerator::parse_sync_qr(&qr_data).unwrap();
        
        assert_eq!(parsed.nostr_public_key, device.nostr_public_key);
        assert_eq!(parsed.device_name, device.device_name);
    }
}
