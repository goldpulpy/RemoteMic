use std::os::unix::fs::{DirBuilderExt, MetadataExt, OpenOptionsExt};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use tokio::sync::Mutex;
use tracing::{debug, error, info, warn};

const PIPE_FILE_NAME: &str = "remotemic.pipe";
const LOCK_FILE_NAME: &str = "remotemic.lock";

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum SampleFormat {
    S16Le,
    Float32Le,
}

impl SampleFormat {
    pub const fn pulse_name(self) -> &'static str {
        match self {
            Self::S16Le => "s16le",
            Self::Float32Le => "float32le",
        }
    }

    pub const fn browser_name(self) -> &'static str {
        match self {
            Self::S16Le => "s16le",
            Self::Float32Le => "float32le",
        }
    }

    pub const fn bytes_per_sample(self) -> usize {
        match self {
            Self::S16Le => 2,
            Self::Float32Le => 4,
        }
    }

    pub const fn display_name(self) -> &'static str {
        match self {
            Self::S16Le => "16-bit PCM",
            Self::Float32Le => "32-bit float",
        }
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct AudioConfig {
    pub sample_rate: u32,
    pub sample_format: SampleFormat,
}

impl AudioConfig {
    pub const LOW: Self = Self {
        sample_rate: 16_000,
        sample_format: SampleFormat::S16Le,
    };

    pub const STANDARD: Self = Self {
        sample_rate: 44_100,
        sample_format: SampleFormat::S16Le,
    };

    pub const HIGH: Self = Self {
        sample_rate: 48_000,
        sample_format: SampleFormat::Float32Le,
    };
}

#[derive(Clone)]
pub struct VirtualMic {
    state: Arc<Mutex<VirtualMicState>>,
    pipe_path: PathBuf,
    config: AudioConfig,
    source_name: Arc<str>,
}

#[derive(Debug)]
pub struct InstanceLock {
    _file: std::fs::File,
}

impl InstanceLock {
    pub fn acquire() -> Result<Self, String> {
        let uid = current_uid()?;
        let dir = pipe_dir(uid);
        ensure_private_directory(&dir, uid)
            .map_err(|e| format!("Unsafe runtime directory {}: {e}", dir.display()))?;

        let path = dir.join(LOCK_FILE_NAME);
        debug!(lock_path = %path.display(), "Acquiring RemoteMic instance lock");
        Self::acquire_at(&path).map_err(|error| {
            if error.kind() == std::io::ErrorKind::AlreadyExists {
                "RemoteMic is already running for this user".to_string()
            } else {
                format!(
                    "Could not acquire instance lock {}: {error}",
                    path.display()
                )
            }
        })
    }

    fn acquire_at(path: &Path) -> std::io::Result<Self> {
        let file = std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .create(true)
            .truncate(false)
            .mode(0o600)
            .open(path)?;

        file.try_lock().map_err(|error| match error {
            std::fs::TryLockError::WouldBlock => std::io::Error::new(
                std::io::ErrorKind::AlreadyExists,
                "another RemoteMic instance holds the lock",
            ),
            std::fs::TryLockError::Error(error) => error,
        })?;

        Ok(Self { _file: file })
    }
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
    pub fn new(config: AudioConfig, source_name: impl Into<Arc<str>>) -> Result<Self, String> {
        Ok(Self {
            state: Arc::new(Mutex::new(VirtualMicState::Unloaded)),
            pipe_path: default_pipe_path()?,
            config,
            source_name: source_name.into(),
        })
    }

    pub fn pipe_path(&self) -> PathBuf {
        self.pipe_path.clone()
    }

    pub async fn load(&self) -> Result<(), String> {
        debug!(
            source_name = %self.source_name,
            pipe_path = %self.pipe_path.display(),
            sample_rate = self.config.sample_rate,
            sample_format = self.config.sample_format.pulse_name(),
            "Preparing virtual microphone"
        );
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
        let source_name = self.source_name.clone();
        let format_arg = format!("format={}", self.config.sample_format.pulse_name());
        let rate_arg = format!("rate={}", self.config.sample_rate);
        let output = tokio::task::spawn_blocking(move || {
            std::process::Command::new("pactl")
                .args([
                    "load-module",
                    "module-pipe-source",
                    &format!("source_name={source_name}"),
                    &format!("file={}", pipe_path.display()),
                    &format_arg,
                    &rate_arg,
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
        debug!(module_index, source_name = %self.source_name, "Virtual microphone is ready");
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
        debug!(pipe_path = %self.pipe_path.display(), "Removing audio pipe file");
        match tokio::fs::remove_file(&self.pipe_path).await {
            Ok(()) => debug!(pipe_path = %self.pipe_path.display(), "Audio pipe file removed"),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => {
                debug!(pipe_path = %self.pipe_path.display(), "Audio pipe file already absent");
            }
            Err(e) => warn!(
                "Failed to remove pipe file {}: {}",
                self.pipe_path.display(),
                e
            ),
        }
    }
}

fn default_pipe_path() -> Result<PathBuf, String> {
    let uid = current_uid()?;
    let dir = pipe_dir(uid);

    ensure_private_directory(&dir, uid)
        .map_err(|e| format!("Unsafe pipe directory {}: {e}", dir.display()))?;

    Ok(dir.join(PIPE_FILE_NAME))
}

fn current_uid() -> Result<u32, String> {
    std::fs::metadata("/proc/self")
        .map_err(|e| format!("Could not determine the current user: {e}"))
        .map(|metadata| metadata.uid())
}

fn ensure_private_directory(dir: &Path, uid: u32) -> std::io::Result<()> {
    std::fs::DirBuilder::new()
        .recursive(true)
        .mode(0o700)
        .create(dir)?;

    let metadata = std::fs::symlink_metadata(dir)?;
    if !metadata.file_type().is_dir() {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "path is not a directory",
        ));
    }
    if metadata.uid() != uid {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("owned by uid {}, expected uid {uid}", metadata.uid()),
        ));
    }

    let permissions = metadata.mode() & 0o777;
    if permissions != 0o700 {
        return Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            format!("permissions are {permissions:#o}, expected 0o700"),
        ));
    }

    Ok(())
}

fn pipe_dir(uid: u32) -> PathBuf {
    if let Some(runtime) = std::env::var_os("XDG_RUNTIME_DIR")
        && !runtime.is_empty()
    {
        return PathBuf::from(runtime).join("remotemic");
    }

    std::env::temp_dir().join(format!("remotemic-{uid}"))
}

#[cfg(test)]
mod tests {
    use super::{InstanceLock, ensure_private_directory};
    use std::os::unix::fs::{MetadataExt, PermissionsExt};

    fn unique_test_path(name: &str) -> std::path::PathBuf {
        std::env::temp_dir().join(format!(
            "remotemic-test-{name}-{}-{}",
            std::process::id(),
            rand::random::<u64>()
        ))
    }

    #[test]
    fn ensure_private_directory_rejects_permissive_existing_directory() {
        let path = unique_test_path("permissive");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o755)).unwrap();
        let uid = std::fs::metadata("/proc/self").unwrap().uid();

        let result = ensure_private_directory(&path, uid);
        std::fs::remove_dir(&path).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn ensure_private_directory_accepts_private_existing_directory() {
        let path = unique_test_path("private");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let uid = std::fs::metadata("/proc/self").unwrap().uid();

        let result = ensure_private_directory(&path, uid);
        std::fs::remove_dir(&path).unwrap();

        assert!(result.is_ok(), "unexpected error: {result:?}");
    }

    #[test]
    fn ensure_private_directory_rejects_unexpected_owner() {
        let path = unique_test_path("owner");
        std::fs::create_dir(&path).unwrap();
        std::fs::set_permissions(&path, std::fs::Permissions::from_mode(0o700)).unwrap();
        let uid = std::fs::metadata("/proc/self").unwrap().uid();

        let result = ensure_private_directory(&path, uid.wrapping_add(1));
        std::fs::remove_dir(&path).unwrap();

        assert!(result.is_err());
    }

    #[test]
    fn instance_lock_rejects_a_second_holder_and_recovers_after_drop() {
        let dir = unique_test_path("instance-lock");
        std::fs::create_dir(&dir).unwrap();
        let path = dir.join("remotemic.lock");

        let first = InstanceLock::acquire_at(&path).unwrap();
        let second = InstanceLock::acquire_at(&path);
        assert_eq!(
            second.unwrap_err().kind(),
            std::io::ErrorKind::AlreadyExists
        );

        drop(first);
        let third = InstanceLock::acquire_at(&path);
        assert!(third.is_ok(), "lock was not released: {third:?}");

        drop(third);
        std::fs::remove_file(path).unwrap();
        std::fs::remove_dir(dir).unwrap();
    }
}
