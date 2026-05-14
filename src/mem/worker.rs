use std::sync::mpsc::{channel, Sender, Receiver};
use crate::mem::native::MemvidHarness;
use crate::mem::schema::{ChatMessage, ConversationSnapshot};

pub enum MemvidCommand {
    Open(String, Sender<Result<ConversationSnapshot, String>>),
    AppendMessage(ChatMessage),
    AppendSnapshot(ConversationSnapshot),
    Compact(Sender<Result<(), String>>),
}

#[derive(Clone)]
pub struct MemvidHandle {
    tx: Sender<MemvidCommand>,
}

impl MemvidHandle {
    pub fn open(&self, path: String) -> Result<ConversationSnapshot, String> {
        let (res_tx, res_rx) = channel();
        self.tx.send(MemvidCommand::Open(path, res_tx)).map_err(|e| e.to_string())?;
        res_rx.recv().map_err(|e| e.to_string())?
    }

    pub fn append_message(&self, msg: ChatMessage) {
        let _ = self.tx.send(MemvidCommand::AppendMessage(msg));
    }

    pub fn append_snapshot(&self, snap: ConversationSnapshot) {
        let _ = self.tx.send(MemvidCommand::AppendSnapshot(snap));
    }

    pub fn compact(&self) -> Result<(), String> {
        let (res_tx, res_rx) = channel();
        self.tx.send(MemvidCommand::Compact(res_tx)).map_err(|e| e.to_string())?;
        res_rx.recv().map_err(|e| e.to_string())?
    }
}

pub fn spawn_worker() -> MemvidHandle {
    let (tx, rx) = channel();
    std::thread::spawn(move || worker_loop(rx));
    MemvidHandle { tx }
}

const SNAPSHOT_INTERVAL: usize = 20;

fn worker_loop(rx: Receiver<MemvidCommand>) {
    let mut harness: Option<MemvidHarness> = None;
    let mut append_count: usize = 0;

    while let Ok(cmd) = rx.recv() {
        match cmd {
            MemvidCommand::Open(path, res_tx) => {
                match MemvidHarness::open_sync(&path) {
                    Ok(mut h) => {
                        match futures::executor::block_on(h.load()) {
                            Ok(snap) => {
                                harness = Some(h);
                                let _ = res_tx.send(Ok(snap));
                            }
                            Err(e) => {
                                let _ = res_tx.send(Err(e.to_string()));
                            }
                        }
                    }
                    Err(e) => {
                        let _ = res_tx.send(Err(e.to_string()));
                    }
                }
            }
            MemvidCommand::AppendMessage(msg) => {
                if let Some(h) = harness.as_mut() {
                    let _ = futures::executor::block_on(h.append_message(&msg));
                    append_count += 1;
                    if append_count >= SNAPSHOT_INTERVAL {
                        if let Some(h) = harness.as_mut() {
                            if let Ok(snap) = futures::executor::block_on(h.load()) {
                                let _ = futures::executor::block_on(h.append_snapshot(&snap));
                            }
                        }
                        append_count = 0;
                    }
                }
            }
            MemvidCommand::AppendSnapshot(snap) => {
                if let Some(h) = harness.as_mut() {
                    let _ = futures::executor::block_on(h.append_snapshot(&snap));
                }
            }
            MemvidCommand::Compact(res_tx) => {
                if let Some(h) = harness.as_mut() {
                    match futures::executor::block_on(h.load()) {
                        Ok(snap) => {
                            match futures::executor::block_on(h.compact(&snap)) {
                                Ok(_) => { let _ = res_tx.send(Ok(())); }
                                Err(e) => { let _ = res_tx.send(Err(e.to_string())); }
                            }
                        }
                        Err(e) => { let _ = res_tx.send(Err(e.to_string())); }
                    }
                } else {
                    let _ = res_tx.send(Err("not open".into()));
                }
            }
        }
    }
}
