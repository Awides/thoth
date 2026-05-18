//! Distributed runtime for message-native applications
//!
//! This module implements the core messaging layer with:
//! - Nostr relay integration (publish/subscribe)
//! - MLS group management (end-to-end encryption)
//! - Multi-identity pool (concurrent user + agent identities)
//! - Message threading and prompt registration
//! - Background runtime (tokio tasks)
//! - Rhai scripting integration

pub mod message;
pub mod nostr_client;
pub mod nostr_utils;
pub mod mls_group;
pub mod group_registry;
pub mod relay_inference;
pub mod runtime;
pub mod rhai_integration;
pub mod identity_pool;

pub use message::{Message, MessageData, Tag, MessageType};
pub use nostr_client::NostrClient;
pub use mls_group::MlsGroupManager;
pub use group_registry::{GroupRegistry, GroupInfo, GroupType, PendingInvite};
pub use runtime::{NetRuntime, NetEvent};
pub use rhai_integration::{RhaiEngine, load_prebaked_scripts};
pub use relay_inference::{
    InferenceRelay, InferenceRequest, InferenceResponse,
    DeviceCaps, RelayEvent, GroupMessage,
};
pub use identity_pool::{IdentityPool, IdentitySlot, IdentityType, SlotState, IdentityInfo};
