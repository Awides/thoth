use std::path::PathBuf;
use directories::ProjectDirs;
use anyhow::Result;

/// Returns the path to the model if it can be found locally.
/// Checks (in order):
/// - Desktop bundle: ./assets/models/Bonsai-1.7B-Q1_0.gguf relative to executable
/// - Development project: ./assets/models/Bonsai-1.7B-Q1_0.gguf relative to CWD
/// - User data directory: ~/.local/share/thoth/models/Bonsai-1.7B-Q1_0.gguf
/// - Android: extracted model at /data/data/com.thoth.app/files/models/Bonsai-1.7B-Q1_0.gguf
/// For web (WASM): returns None.
pub fn get_model_path() -> Option<PathBuf> {
    // Desktop (Linux/macOS/Windows)
    #[cfg(all(not(target_arch = "wasm32"), not(target_os = "android")))]
    {
        // 1. Check bundled assets (next to executable in the app bundle)
        if let Ok(exe) = std::env::current_exe() {
            if let Some(exe_dir) = exe.parent() {
                let bundled_path = exe_dir.join("assets/models/Bonsai-1.7B-Q1_0.gguf");
                if bundled_path.exists() {
                    return Some(bundled_path);
                }
            }
        }

        // 2. Check development project directory (running via `dx serve`)
        if let Ok(cwd) = std::env::current_dir() {
            let dev_path = cwd.join("assets/models/Bonsai-1.7B-Q1_0.gguf");
            if dev_path.exists() {
                return Some(dev_path);
            }
        }

        // 3. Check user data directory
        if let Some(config_dir) = ProjectDirs::from("", "thoth", "Thoth") {
            let models_dir = config_dir.data_local_dir().join("models");
            let user_path = models_dir.join("Bonsai-1.7B-Q1_0.gguf");
            if user_path.exists() {
                return Some(user_path);
            }
        }

        None
    }

    // Android
    #[cfg(target_os = "android")]
    {
        let path = "/data/data/com.thoth.app/files/models/Bonsai-1.7B-Q1_0.gguf";
        if std::path::Path::new(path).exists() {
            Some(PathBuf::from(path))
        } else {
            None
        }
    }

    // Web (WASM) - no local model
    #[cfg(target_arch = "wasm32")]
    {
        None
    }
}

/// Ensures a model is available, downloading from HuggingFace if needed.
/// Returns the path to the model file.
pub async fn ensure_model_available() -> Result<PathBuf> {
    if let Some(path) = get_model_path() {
        return Ok(path);
    }

    // Download from HuggingFace Hub (desktop only for now)
    #[cfg(not(any(target_os = "android", target_arch = "wasm32")))]
    {
        use hf_hub::api::sync::Api;
        use std::fs;

        let api = Api::new()?;
        let repo = hf_hub::Repo::with_revision(
            "TheBloke/Bonsai-1.7B-GGUF".to_string(),
            hf_hub::RepoType::Model,
            "main".to_string(),
        );
        let file = api.repo(repo).get("Bonsai-1.7B-Q1_0.gguf")?;

        // Save to user data directory
        if let Some(config_dir) = ProjectDirs::from("", "thoth", "Thoth") {
            let models_dir = config_dir.data_local_dir().join("models");
            fs::create_dir_all(&models_dir)?;
            let dest = models_dir.join("Bonsai-1.7B-Q1_0.gguf");
            fs::copy(&file, &dest)?;
            return Ok(dest);
        }

        Err(anyhow::anyhow!("Could not determine user data directory for model storage"))
    }

    #[cfg(any(target_os = "android", target_arch = "wasm32"))]
    {
        Err(anyhow::anyhow!("Model not available on this platform and automatic download is not supported"))
    }
}
