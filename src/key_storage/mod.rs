use anyhow::{Result, Context};
use serde::{Serialize, Deserialize};
use std::path::{Path, PathBuf};

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct KeyMetadata {
    pub created_at: String,
    pub device_name: String,
    pub pubkey: String,
    pub key_type: KeyType,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub enum KeyType {
    MasterSeed,
    DeviceKey,
}

pub struct KeyStorage {
    #[cfg(not(target_arch = "wasm32"))]
    base_path: PathBuf,
}

impl KeyStorage {
    #[cfg(not(target_arch = "wasm32"))]
    pub fn new() -> Result<Self> {
        let base_path = directories::BaseDirs::new()
            .context("no home directory")?
            .home_dir()
            .join(".thoth")
            .join("keys");

        std::fs::create_dir_all(&base_path)
            .context("failed to create key storage directory")?;

        Ok(Self { base_path })
    }

    #[cfg(target_arch = "wasm32")]
    pub fn new() -> Result<Self> {
        Ok(Self {})
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn has_keys(&self) -> bool {
        self.base_path.join("master_seed.enc").exists()
    }

    #[cfg(target_arch = "wasm32")]
    pub fn has_keys(&self) -> bool {
        let window = match web_sys::window() {
            Some(w) => w,
            None => return false,
        };
        window.local_storage().ok().flatten()
            .map(|s| s.get_item("thoth_master_seed").ok().flatten().is_some())
            .unwrap_or(false)
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_master_seed(&self, encrypted_seed: &[u8]) -> Result<()> {
        let path = self.base_path.join("master_seed.enc");
        std::fs::write(&path, encrypted_seed)
            .context("failed to write master seed")
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save_master_seed(&self, encrypted_seed: &[u8]) -> Result<()> {
        let window = web_sys::window().context("no window")?;
        let storage = window.local_storage().map_err(|_| anyhow::anyhow!("no localStorage"))?
            .context("localStorage unavailable")?;
        let encoded = hex::encode(encrypted_seed);
        storage.set_item("thoth_master_seed", &encoded)
            .map_err(|_| anyhow::anyhow!("localStorage write failed"))?;
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_master_seed(&self) -> Result<Vec<u8>> {
        let path = self.base_path.join("master_seed.enc");
        std::fs::read(&path).context("failed to read master seed")
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_master_seed(&self) -> Result<Vec<u8>> {
        let window = web_sys::window().context("no window")?;
        let storage = window.local_storage().map_err(|_| anyhow::anyhow!("no localStorage"))?
            .context("localStorage unavailable")?;
        let encoded = storage.get_item("thoth_master_seed")
            .map_err(|_| anyhow::anyhow!("localStorage read failed"))?
            .context("no master seed stored")?;
        hex::decode(&encoded).context("failed to decode master seed")
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_device_key(&self, encrypted_key: &[u8]) -> Result<()> {
        let path = self.base_path.join("device_key.enc");
        std::fs::write(&path, encrypted_key)
            .context("failed to write device key")
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save_device_key(&self, encrypted_key: &[u8]) -> Result<()> {
        let window = web_sys::window().context("no window")?;
        let storage = window.local_storage().map_err(|_| anyhow::anyhow!("no localStorage"))?
            .context("localStorage unavailable")?;
        let encoded = hex::encode(encrypted_key);
        storage.set_item("thoth_device_key", &encoded)
            .map_err(|_| anyhow::anyhow!("localStorage write failed"))?;
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn load_device_key(&self) -> Result<Vec<u8>> {
        let path = self.base_path.join("device_key.enc");
        std::fs::read(&path).context("failed to read device key")
    }

    #[cfg(target_arch = "wasm32")]
    pub fn load_device_key(&self) -> Result<Vec<u8>> {
        let window = web_sys::window().context("no window")?;
        let storage = window.local_storage().map_err(|_| anyhow::anyhow!("no localStorage"))?
            .context("localStorage unavailable")?;
        let encoded = storage.get_item("thoth_device_key")
            .map_err(|_| anyhow::anyhow!("localStorage read failed"))?
            .context("no device key stored")?;
        hex::decode(&encoded).context("failed to decode device key")
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn save_metadata(&self, pubkey: &str, metadata: &KeyMetadata) -> Result<()> {
        let path = self.base_path.join("keyring.json");
        let mut keyring: Vec<(String, KeyMetadata)> = if path.exists() {
            let data = std::fs::read_to_string(&path)?;
            serde_json::from_str(&data).unwrap_or_default()
        } else {
            Vec::new()
        };
        keyring.retain(|(k, _)| k != pubkey);
        keyring.push((pubkey.to_string(), metadata.clone()));
        let data = serde_json::to_string_pretty(&keyring)?;
        std::fs::write(&path, data).context("failed to write keyring")
    }

    #[cfg(target_arch = "wasm32")]
    pub fn save_metadata(&self, pubkey: &str, metadata: &KeyMetadata) -> Result<()> {
        let window = web_sys::window().context("no window")?;
        let storage = window.local_storage().map_err(|_| anyhow::anyhow!("no localStorage"))?
            .context("localStorage unavailable")?;
        let data = serde_json::to_string(&metadata)?;
        storage.set_item(&format!("thoth_key_meta_{}", pubkey), &data)
            .map_err(|_| anyhow::anyhow!("localStorage write failed"))?;
        Ok(())
    }

    #[cfg(not(target_arch = "wasm32"))]
    pub fn key_dir(&self) -> &Path {
        &self.base_path
    }

    #[cfg(target_arch = "wasm32")]
    pub fn key_dir(&self) -> &Path {
        Path::new("/tmp/thoth/keys")
    }
}

pub fn encrypt_with_password(data: &[u8], _password: &str) -> Result<Vec<u8>> {
    Ok(data.to_vec())
}

pub fn decrypt_with_password(encrypted: &[u8], _password: &str) -> Result<Vec<u8>> {
    Ok(encrypted.to_vec())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_key_storage_creation() {
        let storage = KeyStorage::new().unwrap();
        assert!(storage.base_path.exists());
    }

    #[test]
    #[cfg(not(target_arch = "wasm32"))]
    fn test_has_keys() {
        let storage = KeyStorage::new().unwrap();
        assert!(!storage.has_keys());
    }
}
