//! Device group messaging — all private communication flows through MLS groups.
//!
//! Architecture:
//! - Device discovery: KIND_DEVICE_CAPS (public, unencrypted) — device advertises capabilities
//! - Group bootstrap: KIND_MLS_INVITE (GiftWrap) — sends MLS Welcome to new device
//! - Inference & all group messages: MLS-encrypted ciphertext sent via GiftWrap DM
//! The Nostr DM is just the transport envelope; the actual content is always
//! MLS group ciphertext. No side channels — everything is first-class group messages.
//!
//! Custom Nostr kinds:
//! - 21104: Device capability advertisement (public)
//! - 21105: MLS group invite (GiftWrap, contains Welcome message)
//!
//! Group message types (all MLS-encrypted, transported inside GiftWrap DMs):
//! - InferenceRequest: prompt segments + params, routed to strongest device
//! - InferenceResponse: generated text + metadata, sent back to requester
//! - TextMessage: arbitrary group chat
//! - CapsUpdate: device capability change within the group

use anyhow::{Result, anyhow};
use nostr_sdk::prelude::*;
use serde::{Serialize, Deserialize};
use std::collections::HashMap;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{info, warn};

use crate::net::MlsGroupManager;
use crate::net::group_registry::{GroupRegistry, GroupInfo, GroupType, PendingInvite};

pub const KIND_DEVICE_CAPS: Kind = Kind::Custom(21104);
pub const KIND_MLS_INVITE: Kind = Kind::Custom(21105);

const DEVICE_GROUP_ID: &str = "devices";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceRequest {
    pub request_id: String,
    pub prompt_segments: Vec<String>,
    pub system_prompt: String,
    pub model_hint: Option<String>,
    pub max_tokens: Option<u32>,
    pub temperature: Option<f32>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct InferenceResponse {
    pub request_id: String,
    pub content: String,
    pub thinking: String,
    pub model_used: String,
    pub tokens_generated: u32,
    pub elapsed_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DeviceCaps {
    pub device_name: String,
    pub pubkey: String,
    pub gpu_layers: u32,
    pub ram_mb: u64,
    pub cpu_cores: u32,
    pub model_loaded: Option<String>,
    pub is_desktop: bool,
    pub supports_inference: bool,
    pub timestamp: u64,
}

impl DeviceCaps {
    pub fn score(&self) -> f64 {
        let gpu_score = self.gpu_layers as f64 * 10.0;
        let ram_score = (self.ram_mb as f64 / 1024.0).min(32.0) * 5.0;
        let cpu_score = self.cpu_cores as f64 * 2.0;
        let model_score = if self.model_loaded.is_some() { 50.0 } else { 0.0 };
        let desktop_bonus = if self.is_desktop { 20.0 } else { 0.0 };
        let inference_bonus = if self.supports_inference { 30.0 } else { -100.0 };
        gpu_score + ram_score + cpu_score + model_score + desktop_bonus + inference_bonus
    }
}

/// All messages sent within an MLS group are typed via this enum.
/// The ciphertext stored in Nostr events is the MLS-encrypted serialization of this.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GroupMessage {
    InferenceRequest(InferenceRequest),
    InferenceResponse(InferenceResponse),
    TextMessage { content: String },
    CapsUpdate(DeviceCaps),
}

pub struct InferenceRelay {
    client: Arc<Mutex<Option<Client>>>,
    pending_requests: Arc<Mutex<HashMap<String, tokio::sync::oneshot::Sender<InferenceResponse>>>>,
    known_devices: Arc<Mutex<HashMap<String, DeviceCaps>>>,
    own_pubkey: Arc<Mutex<String>>,
    mls: Arc<Mutex<MlsGroupManager>>,
    registry: Arc<Mutex<GroupRegistry>>,
}

impl InferenceRelay {
    pub fn new(mls: Arc<Mutex<MlsGroupManager>>, registry: Arc<Mutex<GroupRegistry>>) -> Self {
        Self {
            client: Arc::new(Mutex::new(None)),
            pending_requests: Arc::new(Mutex::new(HashMap::new())),
            known_devices: Arc::new(Mutex::new(HashMap::new())),
            own_pubkey: Arc::new(Mutex::new(String::new())),
            mls,
            registry,
        }
    }

    pub async fn set_client(&self, client: Client, pubkey: String) {
        *self.client.lock().await = Some(client);
        *self.own_pubkey.lock().await = pubkey.clone();
        let mut mls = self.mls.lock().await;
        if !mls.has_group(DEVICE_GROUP_ID) {
            let _ = mls.create_group(DEVICE_GROUP_ID.to_string(), pubkey.clone());
            info!("Created device group '{}' for {}", DEVICE_GROUP_ID, pubkey);
            let members = mls.get_members(DEVICE_GROUP_ID).unwrap_or_default();
            drop(mls);
            let mut reg = self.registry.lock().await;
            reg.register(GroupInfo {
                group_id: DEVICE_GROUP_ID.to_string(),
                name: "Device Sync".to_string(),
                group_type: GroupType::Device,
                members,
                creator: pubkey,
                created_at: crate::shared::now_secs(),
            });
        }
    }

    pub async fn own_pubkey(&self) -> Option<String> {
        let pk = self.own_pubkey.lock().await;
        if pk.is_empty() { None } else { Some(pk.clone()) }
    }

    pub async fn best_device(&self) -> Option<DeviceCaps> {
        let devices = self.known_devices.lock().await;
        let own = self.own_pubkey.lock().await;
        devices
            .values()
            .filter(|d| d.pubkey != *own && d.supports_inference)
            .max_by(|a, b| a.score().partial_cmp(&b.score()).unwrap_or(std::cmp::Ordering::Equal))
            .cloned()
    }

    async fn send_group_message(&self, target_pubkey: &str, group_id: &str, msg: &GroupMessage) -> Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow!("Nostr client not initialized"))?;

        let plaintext = serde_json::to_vec(msg)?;
        let ciphertext = {
            let mut mls = self.mls.lock().await;
            mls.encrypt(group_id, &plaintext)?
        };
        let payload = hex::encode(&ciphertext);

        let target_pk = PublicKey::from_hex(target_pubkey)
            .map_err(|e| anyhow!("Invalid target pubkey: {:?}", e))?;

        let rumor = EventBuilder::new(Kind::GiftWrap, &payload);
        client.gift_wrap(&target_pk, rumor, []).await?;

        Ok(())
    }

    async fn decrypt_group_message(&self, group_id: &str, ciphertext: &[u8]) -> Result<GroupMessage> {
        let mut mls = self.mls.lock().await;
        let plaintext = mls.decrypt(group_id, ciphertext)?;
        let msg: GroupMessage = serde_json::from_slice(&plaintext)?;
        Ok(msg)
    }

    pub async fn send_inference_request(&self, target_pubkey: &str, req: InferenceRequest) -> Result<tokio::sync::oneshot::Receiver<InferenceResponse>> {
        let (tx, rx) = tokio::sync::oneshot::channel();
        {
            let mut pending = self.pending_requests.lock().await;
            pending.insert(req.request_id.clone(), tx);
        }

        self.send_group_message(target_pubkey, DEVICE_GROUP_ID, &GroupMessage::InferenceRequest(req.clone())).await?;

        info!("Sent inference request {} to {} (MLS-encrypted)", req.request_id, target_pubkey);
        Ok(rx)
    }

    pub async fn send_inference_response(&self, target_pubkey: &str, resp: InferenceResponse) -> Result<()> {
        self.send_group_message(target_pubkey, DEVICE_GROUP_ID, &GroupMessage::InferenceResponse(resp.clone())).await?;
        info!("Sent inference response {} to {} (MLS-encrypted)", resp.request_id, target_pubkey);
        Ok(())
    }

    pub async fn send_text(&self, target_pubkey: &str, group_id: &str, content: String) -> Result<()> {
        self.send_group_message(target_pubkey, group_id, &GroupMessage::TextMessage { content }).await
    }

    pub async fn advertise_caps(&self, caps: DeviceCaps) -> Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow!("Nostr client not initialized"))?;

        let payload = serde_json::to_string(&caps)?;
        let builder = EventBuilder::new(KIND_DEVICE_CAPS, &payload);
        client.send_event_builder(builder).await?;

        {
            let mut devices = self.known_devices.lock().await;
            devices.insert(caps.pubkey.clone(), caps.clone());
        }

        info!("Advertised device capabilities (score={:.0})", caps.score());
        Ok(())
    }

    pub async fn send_mls_invite(&self, target_pubkey: &str, group_id: &str, welcome_bytes: &[u8]) -> Result<()> {
        let client_guard = self.client.lock().await;
        let client = client_guard.as_ref().ok_or_else(|| anyhow!("Nostr client not initialized"))?;

        let invite = serde_json::json!({
            "type": "mls_invite",
            "group_id": group_id,
            "welcome": hex::encode(welcome_bytes),
        });
        let payload = serde_json::to_string(&invite)?;
        let target_pk = PublicKey::from_hex(target_pubkey)
            .map_err(|e| anyhow!("Invalid target pubkey: {:?}", e))?;

        let rumor = EventBuilder::new(KIND_MLS_INVITE, &payload);
        client.gift_wrap(&target_pk, rumor, []).await?;

        info!("Sent MLS invite for group '{}' to {}", group_id, target_pubkey);
        Ok(())
    }

    pub async fn create_chat_group(&self, group_id: String, creator: String) -> Result<()> {
        let mut mls = self.mls.lock().await;
        mls.create_group(group_id.clone(), creator.clone())?;
        let members = mls.get_members(&group_id).unwrap_or_default();
        drop(mls);
        let mut reg = self.registry.lock().await;
        reg.register(GroupInfo {
            group_id: group_id.clone(),
            name: group_id.clone(),
            group_type: GroupType::Chat,
            members,
            creator,
            created_at: crate::shared::now_secs(),
        });
        Ok(())
    }

    pub async fn create_shell_group(&self, shell_name: String, creator: String) -> Result<String> {
        let group_id = format!("shell-{}", shell_name);
        let mut mls = self.mls.lock().await;
        mls.create_group(group_id.clone(), creator.clone())?;
        let members = mls.get_members(&group_id).unwrap_or_default();
        drop(mls);
        let mut reg = self.registry.lock().await;
        reg.register(GroupInfo {
            group_id: group_id.clone(),
            name: format!("{} shell", shell_name),
            group_type: GroupType::Shell { shell_name: shell_name.clone() },
            members,
            creator,
            created_at: crate::shared::now_secs(),
        });
        Ok(group_id)
    }

    pub async fn invite_to_group(&self, group_id: &str, member_pubkey: &str) -> Result<()> {
        let mut mls = self.mls.lock().await;
        let key_package = mls.generate_key_package(member_pubkey.to_string())?;
        let (_commit, welcome) = mls.add_member(group_id, &key_package, member_pubkey.to_string())?;
        let members = mls.get_members(group_id).unwrap_or_default();
        drop(mls);

        {
            let mut reg = self.registry.lock().await;
            reg.update_members(group_id, members);
        }

        self.send_mls_invite(member_pubkey, group_id, &welcome).await?;
        info!("Invited {} to group '{}'", member_pubkey, group_id);
        Ok(())
    }

    pub async fn process_invite(&self, welcome_bytes: &[u8], identity: String) -> Result<String> {
        let group_id;
        let members;
        {
            let mut mls = self.mls.lock().await;
            group_id = mls.process_welcome(welcome_bytes, identity)?;
            members = mls.get_members(&group_id).unwrap_or_default();
        }

        {
            let mut reg = self.registry.lock().await;
            if !reg.has_group(&group_id) {
                reg.register(GroupInfo {
                    group_id: group_id.clone(),
                    name: group_id.clone(),
                    group_type: GroupType::Chat,
                    members,
                    creator: String::new(),
                    created_at: crate::shared::now_secs(),
                });
            } else if let Some(info) = reg.get_mut(&group_id) {
                info.members = members;
            }
        }

        info!("Joined MLS group '{}' via welcome", group_id);
        Ok(group_id)
    }

    pub async fn get_group_members(&self, group_id: &str) -> Result<Vec<String>> {
        let mls = self.mls.lock().await;
        mls.get_members(group_id)
    }

    pub async fn list_groups(&self) -> Vec<GroupInfo> {
        let reg = self.registry.lock().await;
        reg.list().into_iter().cloned().collect()
    }

    pub async fn pending_invites(&self) -> Vec<PendingInvite> {
        let reg = self.registry.lock().await;
        reg.pending_invites().to_vec()
    }

    pub async fn handle_event(&self, event: &nostr_sdk::Event) -> Result<RelayEvent> {
        match event.kind {
            k if k == KIND_DEVICE_CAPS => {
                let caps: DeviceCaps = serde_json::from_str(&event.content)?;
                info!("Device caps from {} (score={:.0}): gpu={} ram={}MB cores={} model={:?}", 
                    caps.device_name, caps.score(), caps.gpu_layers, caps.ram_mb, caps.cpu_cores, caps.model_loaded);
                {
                    let mut devices = self.known_devices.lock().await;
                    devices.insert(caps.pubkey.clone(), caps.clone());
                }
                Ok(RelayEvent::DeviceCaps(caps))
            }
            k if k == KIND_MLS_INVITE => {
                let invite: serde_json::Value = serde_json::from_str(&event.content)?;
                let group_id = invite["group_id"].as_str().unwrap_or("unknown").to_string();
                let welcome_hex = invite["welcome"].as_str().unwrap_or("");
                let welcome_bytes = hex::decode(welcome_hex).unwrap_or_default();
                info!("Received MLS invite for group '{}' from {}", group_id, event.pubkey);

                {
                    let mut reg = self.registry.lock().await;
                    reg.add_pending_invite(PendingInvite {
                        group_id: group_id.clone(),
                        sender: event.pubkey.to_hex(),
                        welcome_bytes: welcome_bytes.clone(),
                        received_at: crate::shared::now_secs(),
                    });
                }

                Ok(RelayEvent::MlsInvite {
                    sender: event.pubkey.to_hex(),
                    group_id,
                    welcome_bytes,
                })
            }
            Kind::GiftWrap => {
                let ciphertext = hex::decode(&event.content).unwrap_or_default();
                if ciphertext.is_empty() {
                    return Err(anyhow!("Empty or non-hex GiftWrap content — not a group message"));
                }

                let group_ids: Vec<String> = {
                    let mls = self.mls.lock().await;
                    mls.group_ids()
                };

                for gid in &group_ids {
                    match self.decrypt_group_message(gid, &ciphertext).await {
                        Ok(group_msg) => {
                            let sender = event.pubkey.to_hex();
                            return match group_msg {
                                GroupMessage::InferenceRequest(req) => {
                                    info!("Received inference request {} from {} (MLS-decrypted)", req.request_id, sender);
                                    Ok(RelayEvent::InferenceRequest { sender, request: req })
                                }
                                GroupMessage::InferenceResponse(resp) => {
                                    info!("Received inference response {} from {} (MLS-decrypted)", resp.request_id, sender);
                                    let mut pending = self.pending_requests.lock().await;
                                    if let Some(tx) = pending.remove(&resp.request_id) {
                                        let _ = tx.send(resp);
                                    }
                                    Ok(RelayEvent::InferenceResponse { sender })
                                }
                                GroupMessage::TextMessage { content } => {
                                    info!("Received group text from {} in {} (MLS-decrypted)", sender, gid);
                                    Ok(RelayEvent::GroupText { sender, content, group_id: gid.clone() })
                                }
                                GroupMessage::CapsUpdate(caps) => {
                                    info!("Received caps update from {} (MLS-decrypted)", caps.device_name);
                                    let mut devices = self.known_devices.lock().await;
                                    devices.insert(caps.pubkey.clone(), caps.clone());
                                    Ok(RelayEvent::DeviceCaps(caps))
                                }
                            };
                        }
                        Err(_) => continue,
                    }
                }

                warn!("GiftWrap from {} could not be decrypted in any known group", event.pubkey);
                Err(anyhow!("Group message decryption failed for all groups"))
            }
            _ => Err(anyhow!("Unhandled relay event kind: {}", event.kind.as_u16())),
        }
    }

    pub async fn get_known_devices(&self) -> Vec<DeviceCaps> {
        self.known_devices.lock().await.values().cloned().collect()
    }
}

#[derive(Debug, Clone)]
pub enum RelayEvent {
    InferenceRequest {
        sender: String,
        request: InferenceRequest,
    },
    InferenceResponse {
        sender: String,
    },
    DeviceCaps(DeviceCaps),
    MlsInvite {
        sender: String,
        group_id: String,
        welcome_bytes: Vec<u8>,
    },
    GroupText {
        sender: String,
        content: String,
        group_id: String,
    },
}

pub fn local_device_caps(device_name: &str, pubkey: &str, gpu_layers: u32, model_loaded: Option<String>) -> DeviceCaps {
    DeviceCaps {
        device_name: device_name.to_string(),
        pubkey: pubkey.to_string(),
        gpu_layers,
        ram_mb: sys_info::mem_info()
            .map(|m| m.total / 1024)
            .unwrap_or(8192),
        cpu_cores: std::thread::available_parallelism()
            .map(|n| n.get() as u32)
            .unwrap_or(4),
        model_loaded,
        is_desktop: cfg!(not(target_os = "android")) && cfg!(not(target_arch = "wasm32")),
        supports_inference: gpu_layers > 0,
        timestamp: crate::shared::now_secs(),
    }
}

mod sys_info {
    pub fn mem_info() -> Result<MemInfo, ()> {
        #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
        {
            let content = std::fs::read_to_string("/proc/meminfo").map_err(|_| ())?;
            let total = content.lines()
                .find(|l| l.starts_with("MemTotal:"))
                .and_then(|l| l.split_whitespace().nth(1))
                .and_then(|v| v.parse::<u64>().ok())
                .unwrap_or(8388608);
            Ok(MemInfo { total })
        }
        #[cfg(any(target_arch = "wasm32", target_os = "android"))]
        {
            Ok(MemInfo { total: 8388608 })
        }
    }

    pub struct MemInfo {
        pub total: u64,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_device_caps_score() {
        let desktop = DeviceCaps { device_name: "desktop".into(), pubkey: "pk1".into(), gpu_layers: 99, ram_mb: 32768, cpu_cores: 16, model_loaded: Some("Bonsai-1.7B".into()), is_desktop: true, supports_inference: true, timestamp: 0 };
        let phone = DeviceCaps { device_name: "phone".into(), pubkey: "pk2".into(), gpu_layers: 0, ram_mb: 8192, cpu_cores: 4, model_loaded: Some("Bonsai-1.7B".into()), is_desktop: false, supports_inference: true, timestamp: 0 };
        assert!(desktop.score() > phone.score());
    }

    #[test]
    fn test_group_message_serde() {
        let req = InferenceRequest { request_id: "test-123".into(), prompt_segments: vec!["hello".into()], system_prompt: "You are Tot.".into(), model_hint: Some("Bonsai".into()), max_tokens: Some(512), temperature: Some(0.5) };
        let msg = GroupMessage::InferenceRequest(req);
        let json = serde_json::to_vec(&msg).unwrap();
        let de: GroupMessage = serde_json::from_slice(&json).unwrap();
        assert!(matches!(de, GroupMessage::InferenceRequest(_)));
    }

    #[test]
    fn test_inference_request_serde() {
        let req = InferenceRequest { request_id: "test-123".into(), prompt_segments: vec!["hello".into()], system_prompt: "You are Tot.".into(), model_hint: Some("Bonsai".into()), max_tokens: Some(512), temperature: Some(0.5) };
        let json = serde_json::to_string(&req).unwrap();
        let de: InferenceRequest = serde_json::from_str(&json).unwrap();
        assert_eq!(de.request_id, "test-123");
    }

    #[test]
    fn test_inference_response_serde() {
        let resp = InferenceResponse { request_id: "test-456".into(), content: "Hello!".into(), thinking: "Hmm...".into(), model_used: "Bonsai-1.7B".into(), tokens_generated: 10, elapsed_ms: 500 };
        let json = serde_json::to_string(&resp).unwrap();
        let de: InferenceResponse = serde_json::from_str(&json).unwrap();
        assert_eq!(de.content, "Hello!");
    }

    #[test]
    fn test_encrypt_decrypt_roundtrip() {
        let mut alice = crate::net::MlsGroupManager::new();
        let mut bob = crate::net::MlsGroupManager::new();

        alice.create_group(DEVICE_GROUP_ID.to_string(), "alice".to_string()).unwrap();

        let bob_kp = bob.generate_key_package("bob".to_string()).unwrap();
        let (_commit, welcome) = alice.add_member(DEVICE_GROUP_ID, &bob_kp, "bob".to_string()).unwrap();
        bob.process_welcome(&welcome, "bob".to_string()).unwrap();

        let msg = GroupMessage::InferenceRequest(InferenceRequest {
            request_id: "rt-1".into(),
            prompt_segments: vec!["test prompt".into()],
            system_prompt: "You are Tot.".into(),
            model_hint: None,
            max_tokens: Some(256),
            temperature: Some(0.7),
        });
        let plaintext = serde_json::to_vec(&msg).unwrap();
        let ciphertext = alice.encrypt(DEVICE_GROUP_ID, &plaintext).unwrap();
        let decrypted = bob.decrypt(DEVICE_GROUP_ID, &ciphertext).unwrap();
        assert_eq!(plaintext, decrypted);
    }
}
