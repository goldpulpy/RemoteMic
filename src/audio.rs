use std::os::unix::fs::{DirBuilderExt, MetadataExt};
use std::path::PathBuf;
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{error, info, warn};

const SOURCE_NAME: &str = "RemoteMic";
const PIPE_FILE_NAME: &str = "remotemic.pipe";

#[derive(Clone)]
pub struct VirtualMic {
    state: Arc<Mutex<VirtualMicState>>,
    pipe_path: PathBuf,
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
            pipe_path: default_pipe_path(),
        }
    }

    pub fn pipe_path(&self) -> PathBuf {
        self.pipe_path.clone()
    }

    pub async fn load(&self) -> Result<(), String> {
        let mut state = self.state.lock().await;

        if matches!(*state, VirtualMicState::Loaded { .. }) {
            info!("Virtual microphone already loaded, skipping");
            return Ok(());
        }

        match tokio::fs::try_exists(&self.pipe_path).await {
            Ok(true) => {
                warn!(
                    "Stale pipe file found at {}, removing",
                    self.pipe_path.display()
                );
                tokio::fs::remove_file(&self.pipe_path)
                    .await
                    .map_err(|e| format!("Failed to remove stale pipe file: {e}"))?;
            }
            Ok(false) => {}
            Err(e) => warn!("Could not check for stale pipe file: {e}"),
        }

        info!("Loading PulseAudio module-pipe-source");

        let pipe_path = self.pipe_path.clone();
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("pactl")
                .args([
                    "load-module",
                    "module-pipe-source",
                    &format!("source_name={SOURCE_NAME}"),
                    &format!("file={}", pipe_path.display()),
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
            error!("pactl load-module failed: {}", stderr.trim());
            return Err(format!(
                "Failed to load module-pipe-source: {}",
                stderr.trim()
            ));
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

        let unload_error = if output.status.success() {
            None
        } else {
            let stderr = String::from_utf8_lossy(&output.stderr).trim().to_string();
            error!("pactl unload-module failed: {stderr}");
            Some(stderr)
        };

        self.remove_pipe_file().await;

        if let Some(stderr) = unload_error {
            let mut state = self.state.lock().await;
            *state = VirtualMicState::Loaded { module_index };
            return Err(format!("Failed to unload module-pipe-source: {stderr}"));
        }

        info!("Virtual microphone unloaded");
        Ok(())
    }

    async fn remove_pipe_file(&self) {
        match tokio::fs::remove_file(&self.pipe_path).await {
            Ok(()) => {}
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {}
            Err(e) => warn!(
                "Failed to remove pipe file {}: {}",
                self.pipe_path.display(),
                e
            ),
        }
    }
}

impl Default for VirtualMic {
    fn default() -> Self {
        Self::new()
    }
}

fn default_pipe_path() -> PathBuf {
    let dir = pipe_dir();

    if let Err(e) = std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(&dir)
    {
        warn!("Could not create pipe directory {}: {e}", dir.display());
    }

    dir.join(PIPE_FILE_NAME)
}

fn pipe_dir() -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        && !runtime.is_empty()
    {
        return PathBuf::from(runtime).join("remotemic");
    }

    let uid = std::fs::metadata("/proc/self")
        .map(|meta| meta.uid())
        .unwrap_or(0);
    std::env::temp_dir().join(format!("remotemic-{uid}"))
}
