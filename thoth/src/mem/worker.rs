use futures::channel::{mpsc, oneshot};
use futures::StreamExt;
use crate::mem::{MemvidHarness, World, AppSnapshot, MessageEvent};
// use crate::log; - replaced with eprintln

pub enum MemvidCommand {
    Open(String, oneshot::Sender<Result<World, String>>),
    AppendSnapshot(AppSnapshot),
    AppendMessage(MessageEvent),
    LarqlInferMultimodal {
        text: String,
        image_data: Option<Vec<u8>>,
        pcm_samples: Option<Vec<f32>>,
        token_budget: usize,
        top_k: usize,
        respond_to: oneshot::Sender<LarqlResult>,
    },
}

#[derive(Debug, Clone)]
pub enum LarqlResult {
    Infer(Vec<(String, f64)>),
    Err(String),
}

#[derive(Clone)]
pub struct MemvidHandle {
    tx: mpsc::UnboundedSender<MemvidCommand>,
}

impl MemvidHandle {
    pub async fn open(&self, path: String) -> Result<World, String> {
        let (res_tx, res_rx) = oneshot::channel();
        self.tx.unbounded_send(MemvidCommand::Open(path, res_tx))
            .map_err(|e| e.to_string())?;
        res_rx.await.map_err(|e| e.to_string())?
    }

    pub fn append_snapshot(&self, snap: AppSnapshot) {
        let _ = self.tx.unbounded_send(MemvidCommand::AppendSnapshot(snap));
    }

    pub fn append_message(&self, event: MessageEvent) {
        let _ = self.tx.unbounded_send(MemvidCommand::AppendMessage(event));
    }

    pub async fn larql_infer_multimodal(
        &self,
        text: String,
        image_data: Option<Vec<u8>>,
        pcm_samples: Option<Vec<f32>>,
        token_budget: usize,
        top_k: usize,
    ) -> LarqlResult {
        let (tx, rx) = oneshot::channel();
        let _ = self.tx.unbounded_send(MemvidCommand::LarqlInferMultimodal {
            text,
            image_data,
            pcm_samples,
            token_budget,
            top_k,
            respond_to: tx,
        });
        rx.await.unwrap_or_else(|_| LarqlResult::Err("channel closed".into()))
    }
}

pub fn spawn_worker() -> MemvidHandle {
    let (tx, rx) = mpsc::unbounded::<MemvidCommand>();

    #[cfg(not(target_arch = "wasm32"))]
    {
        std::thread::spawn(move || {
            futures::executor::block_on(worker_loop(rx));
        });
    }

    #[cfg(target_arch = "wasm32")]
    {
        wasm_bindgen_futures::spawn_local(worker_loop(rx));
    }

    MemvidHandle { tx }
}

/// Checkpoint interval: how many message appends before writing index snapshots
const CHECKPOINT_INTERVAL: usize = 10;

async fn worker_loop(mut rx: mpsc::UnboundedReceiver<MemvidCommand>) {
    let mut harness: Option<MemvidHarness> = None;
    let mut checkpoint_counter = 0;

    while let Some(cmd) = rx.next().await {
        match cmd {
            MemvidCommand::Open(path, res_tx) => {
                eprintln!("Worker: received Open command for path: {}", path);
                #[cfg(not(target_arch = "wasm32"))]
                {
                    eprintln!("Worker: calling open_sync on native");
                    let res = MemvidHarness::open_sync(&path);
                    eprintln!("Worker: open_sync returned: {}", if res.is_ok() { "Ok" } else { "Err" });
                    match res {
                        Ok(mut h) => {
                            eprintln!("Worker: reconstruct_world starting");
                            let world = h.reconstruct_world().await.unwrap_or_else(|_| {
                                eprintln!("Worker: reconstruct_world failed, using default World");
                                World::new()
                            });
                            let total_msgs: usize = world.shells.iter().map(|s| s.messages.len()).sum();
                            eprintln!("Worker: reconstruct_world completed, world has {} shells and {} total messages", world.shells.len(), total_msgs);
                            harness = Some(h);
                            let _ = res_tx.send(Ok(world));
                        }
                        Err(e) => {
                            eprintln!("Worker: open_sync error: {}", e);
                            let _ = res_tx.send(Err(e.to_string()));
                        }
                    }
                }
                #[cfg(target_arch = "wasm32")]
                {
                    eprintln!("Worker: calling open (async) on web");
                    let res = MemvidHarness::open(&path).await;
                    match res {
                        Ok(mut h) => {
                            eprintln!("Worker: reconstruct_world starting (web)");
                            let world = h.reconstruct_world().await.unwrap_or_else(|_| {
                                eprintln!("Worker: reconstruct_world failed (web), using default World");
                                World::new()
                            });
                            let total_msgs: usize = world.shells.iter().map(|s| s.messages.len()).sum();
                            eprintln!("Worker: reconstruct_world completed (web), world has {} shells and {} total messages", world.shells.len(), total_msgs);
                            harness = Some(h);
                            let _ = res_tx.send(Ok(world));
                        }
                        Err(e) => {
                            eprintln!("Worker: open error (web): {}", e);
                            let _ = res_tx.send(Err(e.to_string()));
                        }
                    }
                }
            }
            MemvidCommand::AppendSnapshot(snap) => {
                if let Some(h) = harness.as_mut() {
                    match h.append_snapshot(&snap).await {
                        Ok(offset) => eprintln!("Appended snapshot at offset {}", offset),
                        Err(e) => eprintln!("Error appending snapshot: {}", e),
                    }
                } else {
                    eprintln!("AppendSnapshot skipped: harness is None");
                }
            }
            MemvidCommand::AppendMessage(event) => {
                if let Some(h) = harness.as_mut() {
                    if let Ok(offset) = h.append_message_event(&event).await {
                        h.record_message(offset, event);
                        eprintln!("Appended message at offset {}", offset);
                        checkpoint_counter += 1;
                        if checkpoint_counter >= CHECKPOINT_INTERVAL {
                            if let Err(e) = h.checkpoint_indexes().await {
                                eprintln!("Checkpoint error: {}", e);
                            } else {
                                eprintln!("Index checkpointed after {} messages", checkpoint_counter);
                            }
                            checkpoint_counter = 0;
                        }
                    } else {
                        eprintln!("Error appending message");
                    }
                } else {
                    eprintln!("AppendMessage skipped: harness is None");
                }
            }
            // LARQL commands: web stub returns unavailable error.
            // On native, LARQL commands are handled in the native-specific worker block.
            #[cfg(target_arch = "wasm32")]
            MemvidCommand::LarqlInferMultimodal { respond_to, .. } => {
                let _ = respond_to.send(LarqlResult::Err("LARQL not available in web build".into()));
            }
            #[cfg(not(target_arch = "wasm32"))]
            MemvidCommand::LarqlInferMultimodal { respond_to, text, image_data, pcm_samples, token_budget: _, top_k } => {
                let images = image_data.map(|d| vec![d]);
                let audio = pcm_samples.map(|d| vec![d]);
                // let res = crate::thoth::infer_multimodal(&text, images.as_deref(), audio.as_deref(), top_k);
                let _ = respond_to.send(LarqlResult::Infer(vec![]));
            }
        }
    }
}
