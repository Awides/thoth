pub mod schema;
pub mod native;
pub mod worker;

pub use worker::{MemvidHandle, spawn_worker};
pub use schema::{ChatMessage, ConversationSnapshot, MemoryFact};

use std::path::PathBuf;

fn data_dir() -> Option<PathBuf> {
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    {
        directories::ProjectDirs::from("", "thoth", "Thoth")
            .map(|d| d.data_local_dir().to_path_buf())
    }
    #[cfg(target_os = "android")]
    {
        Some(PathBuf::from("/data/data/com.thoth.app/files"))
    }
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

pub fn memvid_path() -> Option<String> {
    data_dir().map(|d| {
        let dir = d.join("mem");
        let _ = std::fs::create_dir_all(&dir);
        dir.join("thoth.mv2").to_string_lossy().into_owned()
    })
}
