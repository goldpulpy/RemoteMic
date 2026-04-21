use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

const SOURCE_NAME: &str = "RemoteMic";
const PIPE_FILE: &str = "/tmp/remotemic.pipe";

#[derive(Clone)]
pub struct VirtualMic {
    state: Arc<Mutex<VirtualMicState>>,
}

#[derive(Default)]
enum VirtualMicState {
    #[default]
    Unloaded,
    Loaded {
        module_index: u32,
    },
}

impl VirtualMic {
    pub fn new() -> Self {
        Self {
            state: Arc::new(Mutex::new(VirtualMicState::Unloaded)),
        }
    }

    pub fn pipe_path(&self) -> PathBuf {
        PathBuf::from(PIPE_FILE)
    }

    pub async fn load(&self) -> Result<(), String> {
        let mut state = self.state.lock().await;

        if matches!(*state, VirtualMicState::Loaded { .. }) {
            info!("Virtual microphone already loaded, skipping");
            return Ok(());
        }

        if std::path::Path::new(PIPE_FILE).exists() {
            warn!("Stale pipe file found at {}, removing", PIPE_FILE);
            std::fs::remove_file(PIPE_FILE)
                .map_err(|e| format!("Failed to remove stale pipe file: {e}"))?;
        }

        info!("Loading PulseAudio module-pipe-source");

        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new("pactl")
                .args([
                    "load-module",
                    "module-pipe-source",
                    &format!("source_name={SOURCE_NAME}"),
                    &format!("file={PIPE_FILE}"),
                    "format=s16le",
                    "rate=44100",
                    "channels=1",
                ])
                .output()
        })
        .await
        .map_err(|e| format!("spawn_blocking panicked: {e}"))?
        .map_err(|e| format!("Failed to execute pactl: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("pactl load-module failed: {}", stderr);
            return Err(format!("Failed to load module-pipe-source: {stderr}"));
        }

        let stdout = String::from_utf8_lossy(&output.stdout);
        let module_index: u32 = stdout
            .trim()
            .parse()
            .map_err(|e| format!("Unexpected pactl output {:?}: {e}", stdout.trim()))?;

        info!("module-pipe-source loaded (index {module_index})");
        *state = VirtualMicState::Loaded { module_index };
        Ok(())
    }

    pub async fn unload(&self) -> Result<(), String> {
        let module_index = {
            let mut state = self.state.lock().await;
            match std::mem::replace(&mut *state, VirtualMicState::Unloaded) {
                VirtualMicState::Loaded { module_index } => module_index,
                VirtualMicState::Unloaded => {
                    info!("Virtual microphone already unloaded");
                    return Ok(());
                }
            }
        };

        info!("Unloading module-pipe-source (index {module_index})");

        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("pactl")
                .args(["unload-module", &module_index.to_string()])
                .output()
        })
        .await
        .map_err(|e| format!("spawn_blocking panicked: {e}"))?
        .map_err(|e| format!("Failed to execute pactl: {e}"))?;

        if !output.status.success() {
            let stderr = String::from_utf8_lossy(&output.stderr);
            error!("pactl unload-module failed: {}", stderr);
        }

        if std::path::Path::new(PIPE_FILE).exists()
            && let Err(e) = std::fs::remove_file(PIPE_FILE)
        {
            warn!("Failed to remove pipe file {}: {}", PIPE_FILE, e);
        }

        info!("Virtual microphone unloaded");
        Ok(())
    }
}

impl Default for VirtualMic {
    fn default() -> Self {
        Self::new()
    }
}
