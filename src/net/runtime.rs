//! Background runtime for network operations
//!
//! Handles:
//! - Nostr client connection and subscription
//! - MLS group management
//! - Inference relay (route prompts to strongest device)
//! - Rhai scripting
//! - Message routing

use anyhow::Result;
use std::sync::Arc;
use tokio::sync::{mpsc, Mutex};
use tracing::{info, warn, error};
use nostr_sdk::prelude::*;

use crate::net::{NostrClient, MlsGroupManager, RhaiEngine};
use crate::net::relay_inference::{InferenceRelay, InferenceRequest, InferenceResponse, DeviceCaps, KIND_DEVICE_CAPS, KIND_MLS_INVITE};

#[derive(Debug, Clone)]
pub enum NetEvent {
    InferenceResponse(InferenceResponse),
    DeviceDiscovered(DeviceCaps),
    NostrMessage { sender: String, content: String },
    InferenceRequest { sender: String, request: InferenceRequest },
    InferenceResponseArrived { sender: String },
    MlsInvite { sender: String, group_id: String, welcome_bytes: Vec<u8> },
    GroupText { sender: String, content: String },
}

type EventCallback = Arc<std::sync::Mutex<dyn FnMut(NetEvent) + Send>>;

pub struct NetRuntime {
    nostr: Arc<Mutex<Option<NostrClient>>>,
    mls: Arc<Mutex<MlsGroupManager>>,
    rhai: Arc<Mutex<RhaiEngine>>,
    relay: Arc<InferenceRelay>,
    event_tx: mpsc::UnboundedSender<NetEvent>,
    on_event: Arc<std::sync::Mutex<Option<EventCallback>>>,
    running: Arc<Mutex<bool>>,
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
        Self {
            nostr: Arc::new(Mutex::new(None)),
            mls: Arc::new(Mutex::new(MlsGroupManager::new())),
            rhai: Arc::new(Mutex::new(RhaiEngine::new())),
            relay: Arc::new(InferenceRelay::new()),
            event_tx,
            on_event: Arc::new(std::sync::Mutex::new(None)),
            running: Arc::new(Mutex::new(false)),
        }
    }

    pub fn set_event_callback(&self, cb: impl FnMut(NetEvent) + Send + 'static) {
        if let Ok(mut slot) = self.on_event.lock() {
            *slot = Some(Arc::new(std::sync::Mutex::new(cb)));
        }
    }

    pub async fn start(&self) -> Result<()> {
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

    pub async fn public_key(&self) -> Option<String> {
        let nostr = self.nostr.lock().await;
        nostr.as_ref().map(|c| c.public_key())
    }

    pub async fn send_inference_request(&self, req: InferenceRequest) -> Result<tokio::sync::oneshot::Receiver<InferenceResponse>> {
        let best = self.relay.best_device().await;
        match best {
            Some(device) => {
                info!("Routing inference request to {} (score={:.0})", device.device_name, device.score());
                self.relay.send_inference_request(&device.pubkey, req).await
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

    pub async fn send_nostr_message(&self, content: String) -> Result<()> {
        let nostr = self.nostr.lock().await;
        if let Some(client) = nostr.as_ref() {
            client.publish(content, vec![]).await?;
        }
        Ok(())
    }

    async fn start_nostr_listener(&self) -> Result<()> {
        let on_event = self.on_event.clone();
        let event_tx = self.event_tx.clone();
        let relay = self.relay.clone();

        let client_guard = self.nostr.lock().await;
        let client_opt = client_guard.as_ref().map(|c| c.client().clone());
        drop(client_guard);

        let Some(client) = client_opt else {
            return Ok(());
        };

        let filter = Filter::new()
            .kinds(vec![Kind::TextNote, Kind::GiftWrap, KIND_DEVICE_CAPS, KIND_MLS_INVITE])
            .limit(0);

        client.subscribe(vec![filter], None).await?;

        #[cfg(not(target_arch = "wasm32"))]
        {
            let client = client.clone();
            tokio::spawn(async move {
                info!("Nostr listener task started");
                let mut notifications = client.notifications();
                while let Ok(notification) = notifications.recv().await {
                    match notification {
                        RelayPoolNotification::Event { event, .. } => {
                            info!("Received event kind={}", event.kind.as_u16());
                            if event.kind == Kind::TextNote || event.kind == Kind::GiftWrap {
                                dispatch_event(
                                    NetEvent::NostrMessage { sender: event.pubkey.to_hex(), content: event.content.clone() },
                                    &event_tx, &on_event,
                                );
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
            wasm_bindgen_futures::spawn_local(async move {
                info!("Nostr listener task started (wasm)");
                let mut notifications = client.notifications();
                loop {
                    match notifications.recv().await {
                        Ok(notification) => {
                            match notification {
                                RelayPoolNotification::Event { event, .. } => {
                                    info!("Received event kind={}", event.kind.as_u16());
                                    if event.kind == Kind::TextNote || event.kind == Kind::GiftWrap {
                                        dispatch_event(
                                            NetEvent::NostrMessage { sender: event.pubkey.to_hex(), content: event.content.clone() },
                                            &event_tx, &on_event,
                                        );
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
        crate::net::relay_inference::RelayEvent::GroupText { sender, content } => {
            Some(NetEvent::GroupText { sender, content })
        }
    }
}
