//! Distributed runtime for message-native applications
//!
//! This module implements the core messaging layer with:
//! - Nostr relay integration (publish/subscribe)
//! - MLS group management (end-to-end encryption)
//! - Message threading and prompt registration
//! - Background runtime (tokio tasks)
//! - Rhai scripting integration

pub mod message;
pub mod nostr_client;
pub mod mls_group;
pub mod relay_inference;
pub mod runtime;
pub mod rhai_integration;

pub use message::{Message, MessageData, Tag, MessageType};
pub use nostr_client::NostrClient;
pub use mls_group::MlsGroupManager;
pub use runtime::{NetRuntime, NetEvent};
pub use rhai_integration::{RhaiEngine, load_prebaked_scripts};
pub use relay_inference::{
    InferenceRelay, InferenceRequest, InferenceResponse,
    DeviceCaps, RelayEvent, GroupMessage,
    KIND_DEVICE_CAPS, KIND_MLS_INVITE,
};
