use std::fs::{OpenOptions, File};
use std::io::{Result, Write, Seek, SeekFrom};
use memmap2::MmapOptions;
use bincode;
use roaring::RoaringBitmap;
use crate::mem::schema::{AppSnapshot, MessageEvent, Message, Shell};
#[cfg(test)]
use crate::mem::schema::{ShellMetadata, MessageType};
use crate::mem::World;
// use crate::log; - replaced with eprintln
use crate::mem::index::{self, IndexKey, IndexKeyKind, MemvidIndex, IndexState, IndexPayload};

// Frame type constants
const FRAME_TYPE_SNAPSHOT: u8 = 0;
const FRAME_TYPE_MESSAGE: u8 = 1;
const FRAME_TYPE_BITMAP: u8 = 3;  // Interleaved bitmap index for a key

// Magic number "MV2"
const MAGIC: u32 = 0x4D565332;

pub struct MemvidHarness {
    file: File,
    path: String,
    index_path: String,
    index_file: File,
    indexes: MemvidIndex,
    all_messages: Vec<(u32, Message)>,
}

impl MemvidHarness {
    pub fn open_sync(path: &str) -> std::io::Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        let index_path = format!("{}.idx", path);
        let index_file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(&index_path)?;
        Ok(Self {
            file,
            path: path.to_string(),
            index_path,
            index_file,
            indexes: MemvidIndex::new(),
            all_messages: Vec::new(),
        })
    }

    /// Async entry point (compatible with wasm target)
    pub async fn open(path: &str) -> Result<Self> {
        Self::open_sync(path)
    }

    /// Get current file length
    pub fn file_len(&self) -> u64 {
        self.file.metadata().map(|m| m.len()).unwrap_or(0)
    }

    fn write_header(&mut self, frame_type: u8, payload_len: u32) -> Result<u64> {
        let offset = self.file.seek(SeekFrom::End(0))?;
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = frame_type;
        // header[5] reserved
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        // rest zero
        self.file.write_all(&header)?;
        Ok(offset)
    }

    /// Append a MessageEvent frame (append-only message log)
    pub async fn append_message_event(&mut self, event: &MessageEvent) -> Result<u64> {
        let bytes = bincode::serialize(event)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize message failed: {}", e)))?;
        let payload_len = bytes.len() as u32;
        let offset = self.file.seek(SeekFrom::End(0))?;
        self.write_header(FRAME_TYPE_MESSAGE, payload_len)?;
        self.file.write_all(&bytes)?;
        // Pad to 16-byte boundary
        let total = 16u64 + payload_len as u64;
        let padding = (16 - (total % 16)) % 16;
        if padding > 0 {
            self.file.write_all(&vec![0u8; padding as usize])?;
        }
        self.file.flush()?;
        Ok(offset)
    }

    /// Append a Snapshot frame (UI state)
    pub async fn append_snapshot(&mut self, snapshot: &AppSnapshot) -> Result<u64> {
        let bytes = bincode::serialize(snapshot)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize snapshot failed: {}", e)))?;
        let payload_len = bytes.len() as u32;
        let offset = self.file.seek(SeekFrom::End(0))?;
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = FRAME_TYPE_SNAPSHOT;
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        self.file.write_all(&header)?;
        self.file.write_all(&bytes)?;
        // Pad to 16-byte boundary
        let total = 16u64 + payload_len as u64;
        let padding = (16 - (total % 16)) % 16;
        if padding > 0 {
            self.file.write_all(&vec![0u8; padding as usize])?;
        }
        self.file.flush()?;
        Ok(offset)
    }

    /// Append a bitmap index frame directly to the memvid stream
    pub async fn append_bitmap_frame(&mut self, payload: &IndexPayload) -> Result<u64> {
        let bytes = bincode::serialize(payload)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize bitmap failed: {}", e)))?;
        let payload_len = bytes.len() as u32;
        let offset = self.file.seek(SeekFrom::End(0))?;
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = FRAME_TYPE_BITMAP;
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        self.file.write_all(&header)?;
        self.file.write_all(&bytes)?;
        // Pad to 16-byte boundary
        let total = 16u64 + payload_len as u64;
        let padding = (16 - (total % 16)) % 16;
        if padding > 0 {
            self.file.write_all(&vec![0u8; padding as usize])?;
        }
        self.file.flush()?;
        Ok(offset)
    }

    /// Reconstruct the full world state: find latest snapshot and replay all later messages, updating indexes
    pub async fn reconstruct_world(&mut self) -> Result<World> {
        // Load index snapshots
        self.load_index_snapshots()?;
        eprintln!("Loaded {} index snapshots", self.indexes.states.len());

        let file_len = self.file.metadata()?.len();
        if file_len == 0 {
            eprintln!("Reconstruct: file empty");
            return Ok(World::new());
        }
        let mmap = unsafe { MmapOptions::new().map(&self.file)? };

        let mut offset = 0u64;
        let mut latest_snapshot: Option<AppSnapshot> = None;
        let mut latest_snapshot_offset: Option<u64> = None;
        let mut pending_messages: Vec<(u64, MessageEvent)> = vec![]; // (offset, event)

        let mut frame_index = 0;
        while offset < file_len {
            frame_index += 1;
            if offset + 16 > file_len {
                eprintln!("Reconstruct: incomplete header at offset {}", offset);
                break;
            }
            let hdr_bytes = &mmap[offset as usize..offset as usize + 16];
            let magic = u32::from_le_bytes([hdr_bytes[0], hdr_bytes[1], hdr_bytes[2], hdr_bytes[3]]);
            if magic != MAGIC {
                eprintln!("Reconstruct: bad magic at offset {}, stopping", offset);
                break;
            }
            let frame_type = hdr_bytes[4];
            let payload_len = u32::from_le_bytes([hdr_bytes[6], hdr_bytes[7], hdr_bytes[8], hdr_bytes[9]]);
            let payload_start = offset + 16;
            let payload_end = payload_start + payload_len as u64;
            if payload_end > file_len {
                eprintln!("Reconstruct: payload extends beyond file at offset {}, len={}, file_len={}", offset, payload_len, file_len);
                break;
            }
            let payload = &mmap[payload_start as usize..payload_end as usize];
            eprintln!("Frame #{}: offset={}, type={}, payload_len={}", frame_index, offset, frame_type, payload_len);

            match frame_type {
                FRAME_TYPE_SNAPSHOT => {
                    match bincode::deserialize::<AppSnapshot>(payload) {
                        Ok(snap) => {
                            let shell_count = snap.shells.len();
                            let msg_count: usize = snap.shells.iter().map(|s| s.messages.len()).sum();
                            eprintln!("Parsed snapshot: {} shells, {} messages in snapshot", shell_count, msg_count);
                            latest_snapshot = Some(snap);
                            latest_snapshot_offset = Some(offset);
                            pending_messages.clear();
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize snapshot: {}", e);
                        }
                    }
                }
                FRAME_TYPE_MESSAGE => {
                    match bincode::deserialize::<MessageEvent>(payload) {
                        Ok(msg) => {
                            eprintln!("Parsed message: shell_id={}, message id={}, content=\"{}\"", msg.shell_id, msg.message.id, msg.message.content);
                            let should_collect = if let Some(snap_offset) = latest_snapshot_offset {
                                offset > snap_offset
                            } else {
                                true
                            };
                            if should_collect {
                                pending_messages.push((offset, msg));
                            }
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize message event: {}", e);
                        }
                    }
                }
                FRAME_TYPE_BITMAP => {
                    match bincode::deserialize::<IndexPayload>(payload) {
                        Ok(bitmap_payload) => {
                            let key = IndexKey {
 kind: unsafe { std::mem::transmute(bitmap_payload.key_kind) },
 value: bitmap_payload.key_value,
 };
 let bitmap_len = bitmap_payload.bitmap.len();
 let mut state = IndexState {
 bitmap: bitmap_payload.bitmap,
                                base_offset: bitmap_payload.base_offset,
                                dirty: false,
                            };
                            // Keep the one with higher base_offset
                            match self.indexes.states.get(&key) {
                                Some(existing) => {
                                    if bitmap_payload.base_offset > existing.base_offset {
                                        self.indexes.states.insert(key.clone(), state);
                                    }
                                }
                                None => {
                                    self.indexes.states.insert(key.clone(), state);
                                }
                            }
                            eprintln!("Loaded bitmap index for key {:?} (base_offset={}, {} entries)", key, bitmap_payload.base_offset, bitmap_len);
                        }
                        Err(e) => {
                            eprintln!("Failed to deserialize bitmap index frame: {}", e);
                        }
                    }
                }
                _ => {
                    eprintln!("Unknown frame type: {}", frame_type);
                }
            }

            let total = 16 + payload_len as u64;
            let align = if total % 16 == 0 { 0 } else { 16 - (total % 16) };
            offset += total + align;
        }

        // Build world base from latest snapshot if any
        let mut world = if let Some(snap) = &latest_snapshot {
            World::from_snapshot(snap.clone())
        } else {
            World::new()
        };

        // Build all_messages, assign sequential indices, and update indexes
        let mut all_messages: Vec<(u32, Message)> = Vec::new();
        let mut msg_index = 0u32;

        // Process snapshot messages (if any) using snapshot offset
        if let Some(snap_offset) = latest_snapshot_offset {
            if let Some(snap) = &latest_snapshot {
                for shell_meta in snap.shells.iter() {
                    for msg in shell_meta.messages.iter() {
                        let event = MessageEvent { shell_id: shell_meta.id, message: msg.clone() };
                        all_messages.push((shell_meta.id, msg.clone()));
                        self.update_indexes_for_message(snap_offset, &event, msg_index);
                        msg_index += 1;
                    }
                }
            }
        }

        // Process pending messages
        for (offset, event) in pending_messages.iter() {
            all_messages.push((event.shell_id, event.message.clone()));
            self.update_indexes_for_message(*offset, event, msg_index);
            msg_index += 1;
        }

        self.all_messages = all_messages.clone();

        // Apply pending messages to world (snapshot already applied)
        for (_, event) in pending_messages.iter() {
            world.add_message(event.clone());
        }

        let total_msgs: usize = world.shells.iter().map(|s| s.messages.len()).sum();
        eprintln!("Reconstruct: world has {} shells and {} total messages", world.shells.len(), total_msgs);
        eprintln!("Indexes: {} keys loaded", self.indexes.states.len());

        // Checkpoint dirty indexes to index file (write snapshot frames)
        self.checkpoint_indexes().await?;

        Ok(world)
    }

    fn update_indexes_for_message(&mut self, frame_offset: u64, event: &MessageEvent, msg_index: u32) {
        let keys = self.extract_index_keys(event);
        for key in keys {
            // Get or create state with base_offset 0 for new keys
            let state = self.indexes.get_or_insert(key, 0);
            if frame_offset >= state.base_offset {
                state.bitmap.insert(msg_index);
                state.dirty = true;
            }
        }
    }

    fn extract_index_keys(&self, event: &MessageEvent) -> Vec<IndexKey> {
        let mut keys = Vec::new();
        keys.push(IndexKey::shell(event.shell_id));
        keys.push(IndexKey::user(&event.message.sender_id));
        keys.push(IndexKey::msg_type(&event.message.msg_type));
        if let Some(ref kind) = event.message.element_kind {
            keys.push(IndexKey::tag(kind));
        }
        if let Some(ref val) = event.message.value {
            keys.push(IndexKey::tag(val));
        }
        keys
    }

    fn load_index_snapshots(&mut self) -> Result<()> {
        use memmap2::MmapOptions;
        let file_len = self.index_file.metadata().map(|m| m.len()).unwrap_or(0);
        if file_len == 0 {
            return Ok(());
        }
        let mmap = unsafe { MmapOptions::new().map(&self.index_file)? };
        let mut offset = 0u64;
        while offset < file_len {
            if offset + 16 > file_len { break; }
            let hdr_bytes = &mmap[offset as usize..offset as usize + 16];
            let magic = u32::from_le_bytes([hdr_bytes[0], hdr_bytes[1], hdr_bytes[2], hdr_bytes[3]]);
            if magic != index::INDEX_MAGIC { break; }
            let frame_type = hdr_bytes[4];
            if frame_type != index::FRAME_TYPE_INDEX {
                let payload_len = u32::from_le_bytes([hdr_bytes[6], hdr_bytes[7], hdr_bytes[8], hdr_bytes[9]]);
                let total = 16 + payload_len as u64;
                let align = if total % 16 == 0 { 0 } else { 16 - (total % 16) };
                offset += total + align;
                continue;
            }
            let payload_len = u32::from_le_bytes([hdr_bytes[6], hdr_bytes[7], hdr_bytes[8], hdr_bytes[9]]);
            let payload_start = offset + 16;
            let payload_end = payload_start + payload_len as u64;
            if payload_end > file_len { break; }
            let payload_bytes = &mmap[payload_start as usize..payload_end as usize];
            let payload: IndexPayload = bincode::deserialize(payload_bytes)
                .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("deserialize index snapshot failed: {}", e)))?;
            let key = IndexKey {
                kind: unsafe { std::mem::transmute(payload.key_kind) },
                value: payload.key_value
            };
            let state = IndexState {
                bitmap: payload.bitmap,
                base_offset: payload.base_offset,
                dirty: false,
            };
            match self.indexes.states.get(&key) {
                Some(existing) => {
                    if payload.base_offset > existing.base_offset {
                        self.indexes.states.insert(key, state);
                    }
                }
                None => {
                    self.indexes.states.insert(key, state);
                }
            }
            let total = 16 + payload_len as u64;
            let align = (16 - (total % 16)) % 16;
            offset += total + align;
        }
        Ok(())
    }

    pub async fn checkpoint_indexes(&mut self) -> Result<()> {
        let file_len = self.file_len();
        for (key, state) in self.indexes.states.iter_mut() {
            if state.dirty {
                let payload = IndexPayload {
                    key_kind: match key.kind {
                        IndexKeyKind::Shell => 0,
                        IndexKeyKind::Tag => 1,
                        IndexKeyKind::User => 2,
                        IndexKeyKind::MessageType => 3,
                    },
                    key_value: key.value.clone(),
                    base_offset: file_len,
                    bitmap: state.bitmap.clone(),
 };
 // Write interleaved bitmap frame to main memvid stream
//  self.append_bitmap_frame(&payload).await?;
 // Also write to sidecar index file for backwards compatibility
 index::write_index_frame(&mut self.index_file, &payload)?;
 state.base_offset = file_len;
 state.dirty = false;
                eprintln!("Wrote index snapshot for key {:?}: base_offset={}", key, file_len);
            }
        }
        Ok(())
    }

    pub fn query_by_index(&self, key: &IndexKey) -> World {
        let state = match self.indexes.states.get(key) {
            Some(s) => s,
            None => return World { current_shell_idx: 0, dark_mode: true, shells: Vec::new() },
        };
        let mut world = World { current_shell_idx: 0, dark_mode: true, shells: Vec::new() };
        for (idx, (shell_id, msg)) in self.all_messages.iter().enumerate() {
            if state.bitmap.contains(idx as u32) {
                if let Some(shell) = world.shells.iter_mut().find(|s| s.id == *shell_id) {
                    shell.messages.push(msg.clone());
                } else {
                    world.shells.push(Shell {
                        id: *shell_id,
                        title: format!("Shell {}", shell_id),
                        messages: vec![msg.clone()],
                    });
                }
            }
        }
        world
    }

    pub fn get_bitmap(&self, key: &IndexKey) -> Option<&RoaringBitmap> {
        self.indexes.states.get(key).map(|s| &s.bitmap)
    }

    /// Ensure an index exists for the given key, building it from all_messages if needed.
    /// Placeholder for on-demand indexing.
    pub async fn ensure_index(&mut self, _key: IndexKey) {
        todo!("on-demand index building not implemented yet")
    }

    /// Record a new message that has been appended: update in-memory all_messages and indexes
    pub fn record_message(&mut self, offset: u64, event: MessageEvent) {
        self.all_messages.push((event.shell_id, event.message.clone()));
        let msg_index = self.all_messages.len() as u32 - 1;
        self.update_indexes_for_message(offset, &event, msg_index);
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use futures::executor::block_on;

    #[test]
    fn test_roundtrip() -> std::io::Result<()> {
        let path = "test_world_events_native.mv2";
        let _ = std::fs::remove_file(path);
        let mut harness = MemvidHarness::open_sync(path)?;

        let msg1 = MessageEvent {
            shell_id: 1,
            message: Message {
                id: 1,
                msg_type: MessageType::Display,
                content: "Hello".to_string(),
                element_kind: Some("p".to_string()),
                value: None,
                sender_id: "0".to_string(),
                sender_name: "HADES".to_string(),
                timestamp: "now".to_string(),
            },
        };
        block_on(harness.append_message_event(&msg1))?;

        let snap = AppSnapshot {
            current_shell_idx: 0,
            dark_mode: true,
            shells: vec![
                ShellMetadata { id: 1, title: "Echo".to_string(), last_message_id: 1, messages: vec![msg1.message.clone()] },
                ShellMetadata { id: 2, title: "Number".to_string(), last_message_id: 0, messages: vec![] },
                ShellMetadata { id: 3, title: "Toggle".to_string(), last_message_id: 0, messages: vec![] },
            ],
        };
        block_on(harness.append_snapshot(&snap))?;

        let msg2 = MessageEvent {
            shell_id: 1,
            message: Message {
                id: 2,
                msg_type: MessageType::Display,
                content: "World".to_string(),
                element_kind: Some("p".to_string()),
                value: None,
                sender_id: "0".to_string(),
                sender_name: "HADES".to_string(),
                timestamp: "now2".to_string(),
            },
        };
        block_on(harness.append_message_event(&msg2))?;

        let world = block_on(harness.reconstruct_world())?;
        assert_eq!(world.current_shell_idx, 0);
        assert!(world.dark_mode);
        assert_eq!(world.shells[0].messages.len(), 2);
        assert_eq!(world.shells[0].messages[1].content, "World");
        let _ = std::fs::remove_file(path);
        Ok::<(), std::io::Error>(())
    }

    #[test]
    fn test_index_query_and_persistence() -> std::io::Result<()> {
        use crate::mem::index::IndexKey;
        let path = "test_index_native.mv2";
        let idx_path = format!("{}.idx", path);
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(&idx_path);

        // First session: create data and build indexes
        {
            let mut harness = MemvidHarness::open_sync(path)?;
            // Message in shell 1
            let msg1 = MessageEvent {
                shell_id: 1,
                message: Message {
                    id: 1,
                    msg_type: MessageType::Display,
                    content: "Hello".to_string(),
                    element_kind: Some("p".to_string()),
                    value: None,
                    sender_id: "user1".to_string(),
                    sender_name: "Alice".to_string(),
                    timestamp: "now".to_string(),
                },
            };
            block_on(harness.append_message_event(&msg1))?;

            // Snapshot with two shells
            let snap = AppSnapshot {
                current_shell_idx: 0,
                dark_mode: true,
                shells: vec![
                    ShellMetadata { id: 1, title: "Shell1".to_string(), last_message_id: 1, messages: vec![msg1.message.clone()] },
                    ShellMetadata { id: 2, title: "Shell2".to_string(), last_message_id: 0, messages: vec![] },
                ],
            };
            block_on(harness.append_snapshot(&snap))?;

            // Message in shell 2
            let msg2 = MessageEvent {
                shell_id: 2,
                message: Message {
                    id: 2,
                    msg_type: MessageType::Display,
                    content: "World".to_string(),
                    element_kind: Some("p".to_string()),
                    value: None,
                    sender_id: "user2".to_string(),
                    sender_name: "Bob".to_string(),
                    timestamp: "now2".to_string(),
                },
            };
            block_on(harness.append_message_event(&msg2))?;

            // Reconstruct - this will build indexes and checkpoint them
            let world = block_on(harness.reconstruct_world())?;
            let total: usize = world.shells.iter().map(|s| s.messages.len()).sum();
            assert_eq!(total, 2);

            // Query by shell 1
            let world1 = harness.query_by_index(&IndexKey::shell(1));
            assert_eq!(world1.shells.len(), 1);
            assert_eq!(world1.shells[0].id, 1);
            assert_eq!(world1.shells[0].messages.len(), 1);
            assert_eq!(world1.shells[0].messages[0].content, "Hello");

            // Query by user2
            let world_u2 = harness.query_by_index(&IndexKey::user("user2"));
            assert_eq!(world_u2.shells[0].messages[0].content, "World");

            // Query by tag "p" returns both
            let world_p = harness.query_by_index(&IndexKey::tag("p"));
            let total_p: usize = world_p.shells.iter().map(|s| s.messages.len()).sum();
            assert_eq!(total_p, 2);
        }

        // Second session: reopen and verify indexes persisted
        {
            let mut harness = MemvidHarness::open_sync(path)?;
            // Reconstruct - should load index snapshots from .idx file
            let world = block_on(harness.reconstruct_world())?;
            let total: usize = world.shells.iter().map(|s| s.messages.len()).sum();
            assert_eq!(total, 2);

            // Index queries still work
            let world1 = harness.query_by_index(&IndexKey::shell(1));
            assert_eq!(world1.shells[0].messages[0].content, "Hello");

            let world_p = harness.query_by_index(&IndexKey::tag("p"));
            let total_p: usize = world_p.shells.iter().map(|s| s.messages.len()).sum();
            assert_eq!(total_p, 2);
        }

        // Cleanup
        let _ = std::fs::remove_file(path);
        let _ = std::fs::remove_file(idx_path);
        Ok(())
    }
}
