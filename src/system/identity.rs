use super::config::AppConfig;
use anyhow::Result;
use bip39::Mnemonic;
use nostr_sdk::{Keys, ToBech32};
use std::path::Path;
use hostname;
use crate::shared::{Message, MessageRole, MessageKind};

/// Ensure an identity exists; if missing, generate a new one.
pub fn ensure_identity(cfg: &mut AppConfig, path: &Path) -> Result<()> {
    if cfg.mnemonic_encrypted.is_some() && cfg.nostr_secret_key_hex.is_some() {
        return Ok(());
    }
    let mnemonic = Mnemonic::generate(12)?;
    let keys = Keys::generate();
    let secret_hex = keys.secret_key().to_secret_hex();
    let public_bech32 = keys.public_key().to_bech32()?;
    let device_name = hostname::get()
        .unwrap_or_else(|_| "unknown".into())
        .to_string_lossy()
        .into_owned();
    cfg.mnemonic_encrypted = Some(mnemonic.to_string());
    cfg.nostr_secret_key_hex = Some(secret_hex);
    cfg.nostr_public_key = Some(public_bech32);
    cfg.device_name = Some(device_name);
    cfg.onboarding_completed = true;
    cfg.save(path)?;
    Ok(())
}
