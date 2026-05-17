use bincode;
use roaring::RoaringBitmap;
use serde::{Serialize, Deserialize};
// use crate::log; - replaced with eprintln

/// Index key kinds
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum IndexKeyKind {
    App = 0,
    Tag = 1,
    User = 2,
    MessageType = 3,
}

/// Index key: a kind + string value
#[derive(Debug, Clone, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct IndexKey {
    pub kind: IndexKeyKind,
    pub value: String,
}

impl IndexKey {
    pub fn app(app_id: u32) -> Self {
        Self { kind: IndexKeyKind::App, value: app_id.to_string() }
    }
    pub fn tag(tag: &str) -> Self {
        Self { kind: IndexKeyKind::Tag, value: tag.to_string() }
    }
    pub fn user(user_id: &str) -> Self {
        Self { kind: IndexKeyKind::User, value: user_id.to_string() }
    }
    pub fn msg_type(msg_type: &crate::mem::schema::MessageType) -> Self {
        let s = match msg_type {
            crate::mem::schema::MessageType::Display => "display",
            crate::mem::schema::MessageType::Request => "request",
            crate::mem::schema::MessageType::Commit => "commit",
            crate::mem::schema::MessageType::Reject => "reject",
        };
        Self { kind: IndexKeyKind::MessageType, value: s.to_string() }
    }
}

/// Index state: a bitmap of message indices and the base offset up to which it's valid
#[derive(Debug, Clone)]
pub struct IndexState {
    pub bitmap: RoaringBitmap,
    pub base_offset: u64,
    pub dirty: bool,
}

/// In-memory index container
#[derive(Debug, Default)]
pub struct MemvidIndex {
    /// Map from key to its state
    pub states: std::collections::HashMap<IndexKey, IndexState>,
}

impl MemvidIndex {
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or insert a state for a key, returning mutable reference
    pub fn get_or_insert(&mut self, key: IndexKey, base_offset: u64) -> &mut IndexState {
        self.states.entry(key.clone()).or_insert_with(|| IndexState {
            bitmap: RoaringBitmap::new(),
            base_offset,
            dirty: false,
        })
    }

    /// Mark state as dirty after updates
    pub fn mark_dirty(&mut self, key: &IndexKey) {
        if let Some(state) = self.states.get_mut(key) {
            state.dirty = true;
        }
    }

    /// Get state for a key, if exists
    pub fn get(&self, key: &IndexKey) -> Option<&IndexState> {
        self.states.get(key)
    }
}

/// Index frame payload stored in the index file
#[derive(Debug, Serialize, Deserialize)]
pub struct IndexPayload {
    pub key_kind: u8,
    pub key_value: String,
    pub base_offset: u64,
    pub bitmap: RoaringBitmap,
}

/// Magic for index file: "MV2I"
pub const INDEX_MAGIC: u32 = 0x4D325649;
pub const FRAME_TYPE_INDEX: u8 = 2;

/// Native: Write an index frame to a file
pub fn write_index_frame<W: std::io::Write + std::io::Seek>(
    writer: &mut W,
    payload: &IndexPayload,
) -> std::io::Result<u64> {
    let payload_bytes = bincode::serialize(payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize index failed: {}", e)))?;
    let payload_len = payload_bytes.len() as u32;
    let offset = writer.seek(std::io::SeekFrom::End(0))?;
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&INDEX_MAGIC.to_le_bytes());
    header[4] = FRAME_TYPE_INDEX;
    header[6..10].copy_from_slice(&payload_len.to_le_bytes());
    writer.write_all(&header)?;
    writer.write_all(&payload_bytes)?;
    let total = 16u64 + payload_len as u64;
    let padding = (16 - (total % 16)) % 16;
    if padding > 0 {
        writer.write_all(&vec![0u8; padding as usize])?;
    }
    writer.flush()?;
    Ok(offset)
}

/// Native: Load all index frames from an index file
pub fn load_index_frames<R: std::io::Read + std::io::Seek>(
    reader: &mut R,
) -> std::io::Result<Vec<(u64, IndexPayload)>> {
    let file_len = reader.seek(std::io::SeekFrom::End(0))?;
    reader.seek(std::io::SeekFrom::Start(0))?;
    let mut vec = Vec::new();
    let mut offset = 0u64;
    while offset < file_len {
        if offset + 16 > file_len {
            break;
        }
        let mut hdr = [0u8; 16];
        reader.read_exact(&mut hdr)?;
        let magic = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
        if magic != INDEX_MAGIC {
            break;
        }
        let _frame_type = hdr[4]; // currently always INDEX in .idx file
        let payload_len = u32::from_le_bytes([hdr[6], hdr[7], hdr[8], hdr[9]]);
        let payload_start = offset + 16;
        let payload_end = payload_start + payload_len as u64;
        if payload_end > file_len {
            break;
        }
        // Seek to payload
        reader.seek(std::io::SeekFrom::Start(payload_start))?;
        let mut payload_bytes = vec![0u8; payload_len as usize];
        reader.read_exact(&mut payload_bytes)?;
        let payload: IndexPayload = bincode::deserialize(&payload_bytes)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("deserialize index failed: {}", e)))?;
        vec.push((offset, payload));
        // Compute next offset with alignment
        let total = 16 + payload_len as u64;
        let align = if total % 16 == 0 { 0 } else { 16 - (total % 16) };
        offset += total + align;
        // Seek to next frame header
        reader.seek(std::io::SeekFrom::Start(offset))?;
    }
    Ok(vec)
}

#[cfg(target_arch = "wasm32")]
/// Web version: write index frame to OPFS buffer and persist
pub async fn write_index_frame_web(
    buffer: &mut Vec<u8>,
    file_handle: &web_sys::FileSystemFileHandle,
    payload: &IndexPayload,
) -> std::io::Result<u64> {
    let payload_bytes = bincode::serialize(payload)
        .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, format!("serialize index failed: {}", e)))?;
    let payload_len = payload_bytes.len() as u32;
    let offset = buffer.len() as u64;
    let mut data = Vec::with_capacity(16 + payload_bytes.len() + 16);
    let mut header = [0u8; 16];
    header[0..4].copy_from_slice(&INDEX_MAGIC.to_le_bytes());
    header[4] = FRAME_TYPE_INDEX;
    header[6..10].copy_from_slice(&payload_len.to_le_bytes());
    data.extend_from_slice(&header);
    data.extend_from_slice(&payload_bytes);
    let padding = (16 - (data.len() % 16)) % 16;
    if padding > 0 {
        data.extend(std::iter::repeat(0).take(padding as usize));
    }
    // Persist to OPFS
    persist_to_opfs_web(file_handle, offset, &data).await?;
    buffer.extend_from_slice(&data);
    Ok(offset)
}

#[cfg(target_arch = "wasm32")]
async fn persist_to_opfs_web(
    file_handle: &web_sys::FileSystemFileHandle,
    offset: u64,
    data: &[u8],
) -> std::io::Result<()> {
    use wasm_bindgen::JsCast;
    use web_sys::{FileSystemCreateWritableOptions, FileSystemWritableFileStream};
    let options = FileSystemCreateWritableOptions::new();
    options.set_keep_existing_data(true);
    let writable_js = wasm_bindgen_futures::JsFuture::from(file_handle.create_writable_with_options(&options))
        .await
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "create_writable failed"))?;
    let writable: FileSystemWritableFileStream = writable_js.unchecked_into();
    let _ = wasm_bindgen_futures::JsFuture::from(writable.seek_with_f64(offset as f64)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "seek failed"))?)
        .await;
    let data_js = js_sys::Uint8Array::from(data);
    let _ = wasm_bindgen_futures::JsFuture::from(writable.write_with_buffer_source(&data_js)
        .map_err(|_| std::io::Error::new(std::io::ErrorKind::Other, "write failed"))?)
        .await;
    if let Err(e) = wasm_bindgen_futures::JsFuture::from(writable.close()).await {
        let err_msg = format!("close failed: {:?}", e);
        crate::eprintln!("{}", err_msg);
        return Err(std::io::Error::new(std::io::ErrorKind::Other, err_msg));
    }
    Ok(())
}

/// Load index frames from a buffer (web)
pub fn load_index_frames_from_buffer(buffer: &[u8]) -> Vec<(u64, IndexPayload)> {
    let mut vec = Vec::new();
    let file_len = buffer.len() as u64;
    let mut offset = 0u64;
    while offset < file_len {
        if offset + 16 > file_len {
            break;
        }
        let hdr_bytes = &buffer[offset as usize..offset as usize + 16];
        let magic = u32::from_le_bytes([hdr_bytes[0], hdr_bytes[1], hdr_bytes[2], hdr_bytes[3]]);
        if magic != INDEX_MAGIC {
            break;
        }
        let _frame_type = hdr_bytes[4];
        if _frame_type != FRAME_TYPE_INDEX {
            let payload_len = u32::from_le_bytes([hdr_bytes[6], hdr_bytes[7], hdr_bytes[8], hdr_bytes[9]]);
            let total = 16 + payload_len as u64;
            let align = if total % 16 == 0 { 0 } else { 16 - (total % 16) };
            offset += total + align;
            continue;
        }
        let payload_len = u32::from_le_bytes([hdr_bytes[6], hdr_bytes[7], hdr_bytes[8], hdr_bytes[9]]);
        let payload_start = offset + 16;
        let payload_end = payload_start + payload_len as u64;
        if payload_end > file_len {
            break;
        }
        let payload_bytes = &buffer[payload_start as usize..payload_end as usize];
        match bincode::deserialize::<IndexPayload>(payload_bytes) {
            Ok(payload) => {
                vec.push((offset, payload));
            }
            Err(e) => {
                eprintln!("Failed to deserialize index frame at offset {}: {}", offset, e);
            }
        }
        let total = 16 + payload_len as u64;
        let align = (16 - (total % 16)) % 16;
        offset += total + align;
    }
    vec
}
