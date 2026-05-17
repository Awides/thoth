#[cfg(not(target_arch = "wasm32"))]
pub mod key_manager;
pub mod config;
pub mod shell;

#[cfg(not(target_arch = "wasm32"))]
pub mod dialog;
#[cfg(not(target_arch = "wasm32"))]
pub mod commands;
#[cfg(not(target_arch = "wasm32"))]
pub mod features;
#[cfg(not(target_arch = "wasm32"))]
pub mod model;
pub mod identity;

pub use config::{AppConfig, needs_onboarding, complete_onboarding, complete_onboarding_full, check_and_handle_onboarding_auto};
#[cfg(not(target_arch = "wasm32"))]
pub use key_manager::{KeyManager, KeyStorage, DeviceKey};
