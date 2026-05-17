use std::io::Result;
use wasm_bindgen::JsCast;
use web_sys::{FileSystemFileHandle, FileSystemWritableFileStream, FileSystemGetFileOptions, FileSystemCreateWritableOptions, FileSystemDirectoryHandle, File};
use bincode;
use crate::mem::schema::{AppSnapshot, MessageEvent, Message, Shell};
use crate::mem::World;
// use crate::log; - replaced with eprintln
use crate::mem::index::{self, IndexKey, IndexKeyKind, MemvidIndex, IndexState, IndexPayload};
use roaring::RoaringBitmap;

// Frame type constants
const FRAME_TYPE_SNAPSHOT: u8 = 0;
const FRAME_TYPE_MESSAGE: u8 = 1;

// Magic number "MV2"
const MAGIC: u32 = 0x4D565332;

pub struct MemvidHarness {
    file_handle: FileSystemFileHandle,
    buffer: Vec<u8>,
    index_handle: FileSystemFileHandle,
    index_buffer: Vec<u8>,
    indexes: MemvidIndex,
    all_messages: Vec<(u32, Message)>,
}

impl MemvidHarness {
    pub async fn open(path: &str) -> Result<Self> {
        web_sys::console::log_1(&format!("Opening OPFS file (Async API): {}", path).into());
        let window = web_sys::window().ok_or_else(|| std::io::Error::new(std::io::ErrorKind::Other, "no window"))?;
        let navigator = window.navigator();
        let storage = navigator.storage();

        let root_js = wasm_bindgen_futures::JsFuture::from(storage.get_directory()).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "get_directory failed"))?;
        let root: FileSystemDirectoryHandle = root_js.unchecked_into();

        let options = FileSystemGetFileOptions::new();
        options.set_create(true);

        // Open main file
        let file_handle_js = wasm_bindgen_futures::JsFuture::from(root.get_file_handle_with_options(path, &options)).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "get_file_handle failed"))?;
        let file_handle: FileSystemFileHandle = file_handle_js.unchecked_into();

        // Read main file into buffer
        let file_promise = file_handle.get_file();
        let file_js = wasm_bindgen_futures::JsFuture::from(file_promise).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "get_file failed"))?;
        let file: File = file_js.unchecked_into();

        let array_buffer_js = wasm_bindgen_futures::JsFuture::from(file.array_buffer()).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "array_buffer failed"))?;
        let array_buffer: js_sys::ArrayBuffer = array_buffer_js.unchecked_into();
        let buffer = js_sys::Uint8Array::new(&array_buffer).to_vec();

        web_sys::console::log_1(&format!("Loaded {} bytes from OPFS", buffer.len()).into());

        // Open index file
        let index_path = format!("{}.idx", path);
        let index_handle_js = wasm_bindgen_futures::JsFuture::from(root.get_file_handle_with_options(&index_path, &options)).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "get index file handle failed"))?;
        let index_handle: FileSystemFileHandle = index_handle_js.unchecked_into();

        // Read index file into buffer
        let index_file_js = wasm_bindgen_futures::JsFuture::from(index_handle.get_file()).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "get index file failed"))?;
        let index_file: File = index_file_js.unchecked_into();
        let index_array_buffer_js = wasm_bindgen_futures::JsFuture::from(index_file.array_buffer()).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "index array_buffer failed"))?;
        let index_array_buffer: js_sys::ArrayBuffer = index_array_buffer_js.unchecked_into();
        let index_buffer = js_sys::Uint8Array::new(&index_array_buffer).to_vec();

        web_sys::console::log_1(&format!("Loaded {} bytes from index OPFS", index_buffer.len()).into());

        Ok(Self {
            file_handle,
            buffer,
            index_handle,
            index_buffer,
            indexes: MemvidIndex::new(),
            all_messages: Vec::new(),
        })
    }

    pub fn file_len(&self) -> u64 {
        self.buffer.len() as u64
    }

    pub async fn append_message_event(&mut self, event: &MessageEvent) -> Result<u64> {
        let payload = bincode::serialize(event)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize message failed: {}", e)))?;
        let payload_len = payload.len() as u32;
        let offset = self.buffer.len() as u64;

        let mut data = Vec::with_capacity(16 + payload_len as usize + 16);
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = FRAME_TYPE_MESSAGE;
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        data.extend_from_slice(&header);
        data.extend_from_slice(&payload);
        let padding = (16 - (data.len() % 16)) % 16;
        if padding > 0 {
            data.extend(std::iter::repeat(0).take(padding as usize));
        }

        self.persist_to_opfs(offset, &data).await?;
        self.buffer.extend_from_slice(&data);
        web_sys::console::log_1(&format!("Append message complete: offset {}, buffer now {} bytes", offset, self.buffer.len()).into());
        Ok(offset)
    }

    pub async fn append_snapshot(&mut self, snapshot: &AppSnapshot) -> Result<u64> {
        let payload = bincode::serialize(snapshot)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize snapshot failed: {}", e)))?;
        let payload_len = payload.len() as u32;
        let offset = self.buffer.len() as u64;

        let mut data = Vec::with_capacity(16 + payload_len as usize + 16);
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = FRAME_TYPE_SNAPSHOT;
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        data.extend_from_slice(&header);
        data.extend_from_slice(&payload);
        let padding = (16 - (data.len() % 16)) % 16;
        if padding > 0 {
            data.extend(std::iter::repeat(0).take(padding as usize));
        }

        self.persist_to_opfs(offset, &data).await?;
        self.buffer.extend_from_slice(&data);
        web_sys::console::log_1(&format!("Append snapshot complete: offset {}, buffer now {} bytes", offset, self.buffer.len()).into());
        Ok(offset)
    }

    async fn persist_to_opfs(&self, offset: u64, data: &[u8]) -> Result<()> {
        let options = FileSystemCreateWritableOptions::new();
        options.set_keep_existing_data(true);
        let writable_js = wasm_bindgen_futures::JsFuture::from(self.file_handle.create_writable_with_options(&options)).await
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "create_writable failed"))?;
        let writable: FileSystemWritableFileStream = writable_js.unchecked_into();

        let _ = wasm_bindgen_futures::JsFuture::from(writable.seek_with_f64(offset as f64)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "seek failed"))?).await;

        let data_js = js_sys::Uint8Array::from(data);
        let _ = wasm_bindgen_futures::JsFuture::from(writable.write_with_buffer_source(&data_js)
            .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "write failed"))?).await;

        if let Err(e) = wasm_bindgen_futures::JsFuture::from(writable.close()).await {
            let err_msg = format!("close failed: {:?}", e);
            eprintln!("{}", err_msg);
            return Err(std::io::Error::new(std::io::ErrorKind::Other, err_msg));
        }
        Ok(())
    }

    pub async fn reconstruct_world(&mut self) -> Result<World> {
        // Load index snapshots from index buffer
        self.load_index_snapshots()?;
        eprintln!("Loaded {} index snapshots from index buffer", self.indexes.states.len());

        let file_len = self.buffer.len() as u64;
        if file_len == 0 {
            eprintln!("Reconstruct: buffer empty");
            return Ok(World::new());
        }
        eprintln!("Reconstructing world from buffer size {} bytes", file_len);

        let mut offset = 0u64;
        let mut latest_snapshot: Option<AppSnapshot> = None;
        let mut latest_snapshot_offset: Option<u64> = None;
        let mut pending_messages: Vec<(u64, MessageEvent)> = vec!(); // (offset, event)

        let mut frame_index = 0;
        while offset < file_len {
            frame_index += 1;
            if offset + 16 > file_len {
                eprintln!("Reconstruct: incomplete header at offset {}", offset);
                break;
            }
            let hdr_bytes = &self.buffer[offset as usize..offset as usize + 16];
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
            let payload = &self.buffer[payload_start as usize..payload_end as usize];
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
            let state = self.indexes.get_or_insert(key, 0);
            if frame_offset >= state.base_offset {
                state.bitmap.insert(msg_index);
                state.dirty = true;
            }
        }
    }

    fn extract_index_keys(&self, event: &MessageEvent) -> Vec<IndexKey> {
        let mut keys = Vec::new();
            keys.push(IndexKey::app(event.shell_id));
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
        let file_len = self.index_buffer.len() as u64;
        if file_len == 0 {
            return Ok(());
        }
        let frames = index::load_index_frames_from_buffer(&self.index_buffer);
        for (_, payload) in frames {
            let key = IndexKey {
                kind: unsafe { std::mem::transmute(payload.key_kind) },
                value: payload.key_value,
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
        }
        Ok(())
    }

    pub async fn checkpoint_indexes(&mut self) -> Result<()> {
        let file_len = self.file_len() as u64;
        for (key, state) in self.indexes.states.iter_mut() {
            if state.dirty {
                let payload = IndexPayload {
                    key_kind: match key.kind {
                        IndexKeyKind::App => 0,
                        IndexKeyKind::Tag => 1,
                        IndexKeyKind::User => 2,
                        IndexKeyKind::MessageType => 3,
                    },
                    key_value: key.value.clone(),
                    base_offset: file_len,
                    bitmap: state.bitmap.clone(),
                };
                // Write to index file
                index::write_index_frame_web(&mut self.index_buffer, &self.index_handle, &payload).await?;
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
