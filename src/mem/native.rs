use std::fs::{OpenOptions, File};
use std::io::{Result, Write, Seek, SeekFrom};
use memmap2::MmapOptions;
use bincode;
use crate::mem::schema::{ChatMessage, ConversationSnapshot};

const FRAME_TYPE_SNAPSHOT: u8 = 0;
const FRAME_TYPE_MESSAGE: u8 = 1;
const MAGIC: u32 = 0x4D565332;

pub struct MemvidHarness {
    file: File,
    path: String,
}

impl MemvidHarness {
    pub fn open_sync(path: &str) -> Result<Self> {
        let file = OpenOptions::new()
            .read(true)
            .append(true)
            .create(true)
            .open(path)?;
        Ok(Self {
            file,
            path: path.to_string(),
        })
    }

    pub async fn open(path: &str) -> Result<Self> {
        Self::open_sync(path)
    }

    fn write_frame(&mut self, frame_type: u8, bytes: &[u8]) -> Result<u64> {
        let offset = self.file.seek(SeekFrom::End(0))?;
        let payload_len = bytes.len() as u32;
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = frame_type;
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        self.file.write_all(&header)?;
        self.file.write_all(bytes)?;
        let total = 16u64 + payload_len as u64;
        let padding = (16 - (total % 16)) % 16;
        if padding > 0 {
            self.file.write_all(&vec![0u8; padding as usize])?;
        }
        self.file.flush()?;
        Ok(offset)
    }

    pub async fn append_message(&mut self, msg: &ChatMessage) -> Result<u64> {
        let bytes = bincode::serialize(msg)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        self.write_frame(FRAME_TYPE_MESSAGE, &bytes)
    }

    pub async fn append_snapshot(&mut self, snap: &ConversationSnapshot) -> Result<u64> {
        let bytes = bincode::serialize(snap)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        self.write_frame(FRAME_TYPE_SNAPSHOT, &bytes)
    }

    pub async fn load(&mut self) -> Result<ConversationSnapshot> {
        let file_len = self.file.metadata()?.len();
        if file_len == 0 {
            return Ok(ConversationSnapshot { next_id: 0, messages: Vec::new(), facts: Vec::new() });
        }
        let mmap = unsafe { MmapOptions::new().map(&self.file)? };
        let mut offset = 0u64;
        let mut latest_snapshot: Option<ConversationSnapshot> = None;
        let mut messages_after_snapshot: Vec<ChatMessage> = Vec::new();

        while offset + 16 <= file_len {
            let hdr = &mmap[offset as usize..offset as usize + 16];
            let magic = u32::from_le_bytes([hdr[0], hdr[1], hdr[2], hdr[3]]);
            if magic != MAGIC { break; }
            let frame_type = hdr[4];
            let payload_len = u32::from_le_bytes([hdr[6], hdr[7], hdr[8], hdr[9]]);
            let payload_end = offset + 16 + payload_len as u64;
            if payload_end > file_len { break; }
            let payload = &mmap[offset as usize + 16..payload_end as usize];

            match frame_type {
                FRAME_TYPE_SNAPSHOT => {
                    if let Ok(snap) = bincode::deserialize::<ConversationSnapshot>(payload) {
                        latest_snapshot = Some(snap);
                        messages_after_snapshot.clear();
                    }
                }
                FRAME_TYPE_MESSAGE => {
                    if let Ok(msg) = bincode::deserialize::<ChatMessage>(payload) {
                        messages_after_snapshot.push(msg);
                    }
                }
                _ => {}
            }

            let total = 16 + payload_len as u64;
            let align = (16 - (total % 16)) % 16;
            offset += total + align;
        }

        let mut snap = latest_snapshot.unwrap_or(ConversationSnapshot { next_id: 0, messages: Vec::new(), facts: Vec::new() });
        for msg in messages_after_snapshot {
            if msg.id >= snap.next_id {
                snap.next_id = msg.id + 1;
            }
            snap.messages.push(msg);
        }
        Ok(snap)
    }

    pub async fn compact(&mut self, snap: &ConversationSnapshot) -> Result<u64> {
        let new_path = format!("{}.tmp", self.path);
        let mut new_file = OpenOptions::new()
            .write(true)
            .create(true)
            .truncate(true)
            .open(&new_path)?;
        let bytes = bincode::serialize(snap)
            .map_err(|e| std::io::Error::new(std::io::ErrorKind::Other, e))?;
        let payload_len = bytes.len() as u32;
        let mut header = [0u8; 16];
        header[0..4].copy_from_slice(&MAGIC.to_le_bytes());
        header[4] = FRAME_TYPE_SNAPSHOT;
        header[6..10].copy_from_slice(&payload_len.to_le_bytes());
        new_file.write_all(&header)?;
        new_file.write_all(&bytes)?;
        let total = 16u64 + payload_len as u64;
        let padding = (16 - (total % 16)) % 16;
        if padding > 0 {
            new_file.write_all(&vec![0u8; padding as usize])?;
        }
        new_file.flush()?;
        drop(new_file);
        std::fs::rename(&new_path, &self.path)?;
        self.file = OpenOptions::new()
            .read(true)
            .append(true)
            .open(&self.path)?;
        Ok(0)
    }
}
