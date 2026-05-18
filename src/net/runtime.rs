//! Background runtime for network operations
//!
//! Handles:
//! - Nostr client connection and subscription
//! - MLS group management (shared single instance)
//! - Group registry (metadata, membership, pending invites)
//! - Inference relay (route prompts to strongest device)
//! - Rhai scripting
//! - Message routing

use std::sync::atomic::{AtomicBool, Ordering};

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn, error, trace};
use nostr_sdk::prelude::*;

use crate::net::{NostrClient, MlsGroupManager, RhaiEngine};
use crate::net::relay_inference::{InferenceRelay, InferenceRequest, InferenceResponse, DeviceCaps};
use crate::net::group_registry::{GroupRegistry, GroupInfo, PendingInvite};

pub static NET_CALLBACK_SET: AtomicBool = AtomicBool::new(false);

pub fn is_net_initialized() -> bool {
    NET_CALLBACK_SET.load(Ordering::SeqCst)
}

#[derive(Debug, Clone)]
pub enum NetEvent {
    InferenceResponse(InferenceResponse),
    DeviceDiscovered(DeviceCaps),
    NostrMessage { sender: String, content: String },
    InferenceRequest { sender: String, request: InferenceRequest },
    InferenceResponseArrived { sender: String },
    MlsInvite { sender: String, group_id: String, welcome_bytes: Vec<u8> },
    GroupText { sender: String, content: String, group_id: String },
    RelayStatusChanged { url: String, status: String },
    GroupJoined { group_id: String },
}

type EventCallback = Arc<std::sync::Mutex<dyn FnMut(NetEvent) + Send>>;

pub const DEFAULT_RELAYS: &[&str] = &[
    "wss://relay.nostr.io",
    "wss://nos.lol",
    "wss://relay.damus.io",
    "wss://nostr-pub.wellorder.net",
];

pub struct NetRuntime {
    nostr: Arc<Mutex<Option<NostrClient>>>,
    mls: Arc<Mutex<MlsGroupManager>>,
    registry: Arc<Mutex<GroupRegistry>>,
    rhai: Arc<Mutex<RhaiEngine>>,
    relay: Arc<InferenceRelay>,
    event_tx: mpsc::UnboundedSender<NetEvent>,
    on_event: Arc<std::sync::Mutex<Option<EventCallback>>>,
    running: Arc<Mutex<bool>>,
    started: Arc<std::sync::atomic::AtomicBool>,
}

fn dispatch_event(event: NetEvent, event_tx: &mpsc::UnboundedSender<NetEvent>, on_event: &Arc<std::sync::Mutex<Option<EventCallback>>>) {
    let _ = event_tx.send(event.clone());
    if let Ok(mut slot) = on_event.lock() {
        if let Some(cb) = slot.as_ref() {
            if let Ok(mut cb_guard) = cb.lock() {
                cb_guard(event);
            }
        }
    }
}

impl NetRuntime {
    pub fn new(event_tx: mpsc::UnboundedSender<NetEvent>) -> Self {
        let mls = Arc::new(Mutex::new(MlsGroupManager::new()));
        let registry = Arc::new(Mutex::new(GroupRegistry::new()));
        let relay = Arc::new(InferenceRelay::new(mls.clone(), registry.clone()));
        Self {
            nostr: Arc::new(Mutex::new(None)),
            mls,
            registry,
            rhai: Arc::new(Mutex::new(RhaiEngine::new())),
            relay,
            event_tx,
            on_event: Arc::new(std::sync::Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
            started: Arc::new(std::sync::atomic::AtomicBool::new(false)),
        }
    }

    pub fn set_event_callback(&self, cb: impl FnMut(NetEvent) + Send + 'static) {
        if let Ok(mut slot) = self.on_event.lock() {
            *slot = Some(Arc::new(std::sync::Mutex::new(cb)));
        }
    }

    pub async fn start(&self) -> Result<()> {
        if self.started.swap(true, Ordering::SeqCst) {
            info!("Network runtime already started, skipping.");
            return Ok(());
        }
        info!("Starting network runtime...");

        let nostr_client = NostrClient::new().await?;
        let pubkey = nostr_client.public_key();
        {
            let mut nostr = self.nostr.lock().await;
            *nostr = Some(nostr_client);
        }

        {
            let mut rhai = self.rhai.lock().await;
            crate::net::load_prebaked_scripts(&mut rhai)?;
        }

        {
            let client_guard = self.nostr.lock().await;
            if let Some(client) = client_guard.as_ref() {
                self.relay.set_client(client.client().clone(), pubkey.clone()).await;
            }
        }

        self.start_nostr_listener().await?;

        if let Err(e) = self.relay.publish_key_package(&pubkey).await {
            warn!("Failed to publish key package: {}", e);
        }

        {
            let mut running = self.running.lock().await;
            *running = true;
        }

        info!("Network runtime started as {}", pubkey);
        Ok(())
    }

    pub async fn stop(&self) {
        let mut running = self.running.lock().await;
        *running = false;
        info!("Network runtime stopped");
    }

    pub async fn is_running(&self) -> bool {
        *self.running.lock().await
    }

    pub fn is_started(&self) -> bool {
        self.started.load(Ordering::SeqCst)
    }

    pub async fn public_key(&self) -> Option<String> {
        let nostr = self.nostr.lock().await;
        nostr.as_ref().map(|c| c.public_key())
    }

    pub async fn public_key_hex(&self) -> Option<String> {
        let nostr = self.nostr.lock().await;
        nostr.as_ref().map(|c| c.public_key_hex())
    }

    pub async fn send_group_text(&self, content: String) -> Result<()> {
        let registry = self.registry.lock().await;
        let group_id = registry.dm_group()
            .map(|g| g.group_id.clone())
            .unwrap_or_else(|| "devices".to_string());
        drop(registry);
        self.relay.send_text(&group_id, content).await
    }

    pub async fn send_shell_text(&self, shell_name: &str, content: String) -> Result<()> {
        let group_id = {
            let registry = self.registry.lock().await;
            registry.group_for_shell(shell_name)
                .map(|g| g.group_id.clone())
                .unwrap_or_else(|| "devices".to_string())
        };
        self.relay.send_text(&group_id, content).await
    }

    pub async fn own_pubkey(&self) -> Option<String> {
        self.relay.own_pubkey().await
    }

    pub async fn send_inference_request(&self, req: InferenceRequest) -> Result<tokio::sync::oneshot::Receiver<InferenceResponse>> {
        let best = self.relay.best_device().await;
        match best {
            Some(device) => {
                info!("Routing inference request to {} (score={:.0})", device.device_name, device.score());
                self.relay.send_inference_request(req).await
            }
            None => {
                warn!("No inference-capable device found");
                Err(anyhow::anyhow!("No inference-capable device online. Start the desktop app with a model loaded."))
            }
        }
    }

    pub async fn advertise_caps(&self, caps: DeviceCaps) -> Result<()> {
        self.relay.advertise_caps(caps).await
    }

    pub async fn get_known_devices(&self) -> Vec<DeviceCaps> {
        self.relay.get_known_devices().await
    }

    pub async fn send_nostr_message(&self, content: String) -> Result<()> {
        let nostr = self.nostr.lock().await;
        if let Some(client) = nostr.as_ref() {
            client.publish(content, vec![]).await?;
        }
        Ok(())
    }

    pub async fn publish_text_note(&self, content: String) -> Result<()> {
        let nostr = self.nostr.lock().await;
        if let Some(client) = nostr.as_ref() {
            client.publish(content, vec![]).await?;
        }
        Ok(())
    }

    pub async fn create_chat_group(&self, group_id: String, creator: String) -> Result<()> {
        self.relay.create_chat_group(group_id, creator).await
    }

    pub async fn create_shell_group(&self, shell_name: String, creator: String) -> Result<String> {
        self.relay.create_shell_group(shell_name, creator).await
    }

    pub async fn invite_to_group(&self, group_id: &str, member_pubkey: &str) -> Result<()> {
        self.relay.invite_to_group(group_id, member_pubkey).await
    }

    pub async fn process_invite(&self, welcome_bytes: &[u8], identity: String) -> Result<String> {
        self.relay.process_invite(welcome_bytes, identity).await
    }

    pub async fn join_pending_invite(&self, group_id: &str) -> Result<String> {
        let invite = {
            let mut reg = self.registry.lock().await;
            reg.take_pending_invite(group_id)
        };
        match invite {
            Some(inv) => {
                let result = self.relay.process_invite(&inv.welcome_bytes, inv.sender.clone()).await?;
                dispatch_event(NetEvent::GroupJoined { group_id: result.clone() }, &self.event_tx, &self.on_event);
                Ok(result)
            }
            None => Err(anyhow::anyhow!("No pending invite for group '{}'", group_id)),
        }
    }

    pub async fn get_group_members(&self, group_id: &str) -> Result<Vec<String>> {
        self.relay.get_group_members(group_id).await
    }

    pub async fn list_groups(&self) -> Vec<GroupInfo> {
        self.relay.list_groups().await
    }

    pub async fn pending_invites(&self) -> Vec<PendingInvite> {
        self.relay.pending_invites().await
    }

    async fn start_nostr_listener(&self) -> Result<()> {
        let on_event = self.on_event.clone();
        let event_tx = self.event_tx.clone();
        let relay = self.relay.clone();

        let client_guard = self.nostr.lock().await;
        let client_opt = client_guard.as_ref().map(|c| c.client().clone());
        let own_pk_hex = client_guard.as_ref().map(|c| c.public_key_hex());
        drop(client_guard);

        let Some(client) = client_opt else {
            return Ok(());
        };

        let own_pk_for_filter = own_pk_hex.clone().and_then(|hex| PublicKey::from_hex(&hex).ok());

        let mut filter = Filter::new()
            .kinds(vec![Kind::TextNote, Kind::Custom(445), Kind::Custom(30443), Kind::GiftWrap])
            .limit(0);

        if let Some(pk) = &own_pk_for_filter {
            filter = filter.pubkey(*pk);
        }

        client.subscribe(filter, None).await?;

        let own_pk_hex = own_pk_hex.unwrap_or_default();

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = client.clone();
            let own_pk_hex = own_pk_hex.clone();
            tokio::spawn(async move {
                info!("Nostr listener task started");
                let mut notifications = client.notifications();
                while let Ok(notification) = notifications.recv().await {
                    match notification {
                        RelayPoolNotification::Event { event, .. } => {
                            trace!("Received event kind={}", event.kind.as_u16());
                    if event.kind == Kind::TextNote {
                        if event.pubkey.to_hex() == own_pk_hex {
                            continue;
                        }
                        let mentions_us = event.tags.iter().any(|t| t.kind() == TagKind::p() && t.content() == Some(&own_pk_hex));
                        let is_tot_query = event.content.to_lowercase().starts_with("@tot ");
                        if mentions_us || is_tot_query {
                            dispatch_event(
                                NetEvent::NostrMessage { sender: event.pubkey.to_hex(), content: event.content.clone() },
                                &event_tx, &on_event,
                            );
                        }
                    } else if event.kind == Kind::Custom(30443) {
                        if event.pubkey.to_hex() != own_pk_hex {
                            let _ = relay.handle_key_package_event(&*event).await;
                        }
                    } else if event.kind == Kind::GiftWrap {
                        let _ = relay.handle_gift_wrap(&*event).await;
                    } else if let Ok(relay_event) = relay.handle_event(&*event).await {
                                if let Some(ne) = relay_event_into_net_event(relay_event) {
                                    dispatch_event(ne, &event_tx, &on_event);
                                }
                            }
                        }
                        RelayPoolNotification::Shutdown => break,
                        _ => {}
                    }
                }
                info!("Nostr listener task ended");
            });
        }

        #[cfg(target_arch = "wasm32")]
        {
            let client = client.clone();
            let own_pk_hex = own_pk_hex.clone();
            wasm_bindgen_futures::spawn_local(async move {
                info!("Nostr listener task started (wasm)");
                let mut notifications = client.notifications();
                loop {
                    match notifications.recv().await {
                        Ok(notification) => {
                            match notification {
                                RelayPoolNotification::Event { event, .. } => {
                                    trace!("Received event kind={}", event.kind.as_u16());
                        if event.kind == Kind::TextNote {
                            if event.pubkey.to_hex() == own_pk_hex {
                                continue;
                            }
                            let mentions_us = event.tags.iter().any(|t| t.kind() == TagKind::p() && t.content() == Some(&own_pk_hex));
                            let is_tot_query = event.content.to_lowercase().starts_with("@tot ");
                            if mentions_us || is_tot_query {
                                dispatch_event(
                                    NetEvent::NostrMessage { sender: event.pubkey.to_hex(), content: event.content.clone() },
                                    &event_tx, &on_event,
                                );
                            }
                        } else if event.kind == Kind::Custom(30443) {
                            if event.pubkey.to_hex() != own_pk_hex {
                                let _ = relay.handle_key_package_event(&*event).await;
                            }
                        } else if event.kind == Kind::GiftWrap {
                            let _ = relay.handle_gift_wrap(&*event).await;
                        } else if let Ok(relay_event) = relay.handle_event(&*event).await {
                                        if let Some(ne) = relay_event_into_net_event(relay_event) {
                                            dispatch_event(ne, &event_tx, &on_event);
                                        }
                                    }
                                }
                                RelayPoolNotification::Shutdown => break,
                                _ => {}
                            }
                        }
                        Err(e) => {
                            warn!("Notification error: {}", e);
                            crate::shared::sleep_ms(1000).await;
                        }
                    }
                }
                info!("Nostr listener task ended (wasm)");
            });
        }

        Ok(())
    }

    pub async fn create_device_group(&self, device_ids: Vec<String>) -> Result<()> {
        let mut mls = self.mls.lock().await;
        crate::net::mls_group::create_device_group(&mut mls, device_ids)
    }

    pub async fn create_user_group(&self, group_id: String, creator: String) -> Result<()> {
        let mut mls = self.mls.lock().await;
        crate::net::mls_group::create_user_group(&mut mls, group_id, creator)
    }

    pub async fn register_handler(
        &self,
        trigger_type: String,
        trigger_pattern: String,
        handler_script: String,
    ) -> Result<()> {
        let mut rhai = self.rhai.lock().await;
        rhai.register_message_handler(trigger_type, trigger_pattern, handler_script)
    }

    pub async fn relay_statuses(&self) -> Vec<(String, String)> {
        let nostr = self.nostr.lock().await;
        match nostr.as_ref() {
            Some(client) => client.relay_statuses().await,
            None => DEFAULT_RELAYS.iter().map(|r| (r.to_string(), "not connected".to_string())).collect(),
        }
    }

    pub async fn connected_relay_count(&self) -> usize {
        let nostr = self.nostr.lock().await;
        match nostr.as_ref() {
            Some(client) => client.connected_count().await,
            None => 0,
        }
    }

    pub async fn add_relay(&self, url: &str) -> Result<()> {
        let nostr = self.nostr.lock().await;
        match nostr.as_ref() {
            Some(client) => client.add_relay(url).await,
            None => Err(anyhow::anyhow!("Nostr client not initialized")),
        }
    }

    pub async fn remove_relay(&self, url: &str) -> Result<()> {
        let nostr = self.nostr.lock().await;
        match nostr.as_ref() {
            Some(client) => client.remove_relay(url).await,
            None => Err(anyhow::anyhow!("Nostr client not initialized")),
        }
    }
}

fn relay_event_into_net_event(re: crate::net::relay_inference::RelayEvent) -> Option<NetEvent> {
    match re {
        crate::net::relay_inference::RelayEvent::InferenceRequest { sender, request } => {
            Some(NetEvent::InferenceRequest { sender, request })
        }
        crate::net::relay_inference::RelayEvent::InferenceResponse { sender } => {
            Some(NetEvent::InferenceResponseArrived { sender })
        }
        crate::net::relay_inference::RelayEvent::DeviceCaps(caps) => {
            Some(NetEvent::DeviceDiscovered(caps))
        }
        crate::net::relay_inference::RelayEvent::MlsInvite { sender, group_id, welcome_bytes } => {
            Some(NetEvent::MlsInvite { sender, group_id, welcome_bytes })
        }
        crate::net::relay_inference::RelayEvent::GroupText { sender, content, group_id } => {
            Some(NetEvent::GroupText { sender, content, group_id })
        }
    }
}
