use std::path::PathBuf;
use tokio::process::Command;
use tracing::{debug, info, warn};

const COMMAND_TIMEOUT: std::time::Duration = std::time::Duration::from_secs(5);

pub async fn check_pactl() -> Result<(), String> {
    debug!("Checking pactl availability");
    let mut command = Command::new("pactl");
    command.arg("--version").kill_on_drop(true);
    match tokio::time::timeout(COMMAND_TIMEOUT, command.output()).await {
        Err(_) => Err("pactl --version timed out".to_string()),
        Ok(Ok(output)) if output.status.success() => {
            info!("pactl: OK");
            Ok(())
        }
        Ok(Ok(output)) => Err(format!(
            "pactl is present but not working (exit status {})",
            output.status
        )),
        Ok(Err(e)) if e.kind() == std::io::ErrorKind::NotFound => {
            Err("pactl not found. Install PulseAudio or PipeWire-pulse:\n  \
             Debian/Ubuntu: sudo apt install pulseaudio-utils\n  \
             Fedora:        sudo dnf install pulseaudio-utils\n  \
             Arch:          sudo pacman -S libpulse"
                .to_string())
        }
        Ok(Err(e)) => Err(format!("pactl could not be executed: {e}")),
    }
}

pub async fn check_audio_libs() {
    let required: &[&str] = &["libpulse.so.0", "libasound.so.2"];
    let optional: &[&str] = &["libpipewire-0.3.so.0"];
    let cache = ldconfig_cache().await;
    debug!(
        ldconfig_cache_available = cache.is_some(),
        "Checking audio library availability"
    );

    for lib in required {
        if lib_available(lib, cache.as_deref()) {
            info!("lib {lib}: OK");
        } else {
            warn!(
                "Required audio library not found: {lib}\n  \
                 Debian/Ubuntu: sudo apt install libpulse0 libasound2\n  \
                 Fedora:        sudo dnf install pulseaudio-libs alsa-lib\n  \
                 Arch:          sudo pacman -S libpulse alsa-lib"
            );
        }
    }

    for lib in optional {
        if lib_available(lib, cache.as_deref()) {
            info!("lib {lib}: OK (PipeWire native)");
        } else {
            info!("lib {lib}: not found (optional, PipeWire native support disabled)");
        }
    }
}

async fn ldconfig_cache() -> Option<String> {
    debug!("Reading dynamic linker cache with ldconfig");
    let mut command = Command::new("ldconfig");
    command.arg("-p").kill_on_drop(true);
    let output = tokio::time::timeout(COMMAND_TIMEOUT, command.output())
        .await
        .ok()?
        .ok()?;
    output
        .status
        .success()
        .then(|| String::from_utf8_lossy(&output.stdout).into_owned())
}

fn lib_available(name: &str, cache: Option<&str>) -> bool {
    if let Some(cache) = cache
        && cache.lines().any(|line| line.contains(name))
    {
        return true;
    }

    lib_exists_in_paths(name)
}

fn lib_exists_in_paths(name: &str) -> bool {
    let mut dirs: Vec<PathBuf> = Vec::new();

    if let Some(paths) = std::env::var_os("LD_LIBRARY_PATH") {
        dirs.extend(std::env::split_paths(&paths));
    }

    dirs.extend(
        [
            "/usr/lib",
            "/usr/lib64",
            "/usr/local/lib",
            "/lib",
            "/lib64",
            "/usr/lib/x86_64-linux-gnu",
            "/lib/x86_64-linux-gnu",
            "/usr/lib/aarch64-linux-gnu",
            "/lib/aarch64-linux-gnu",
            "/usr/lib/arm-linux-gnueabihf",
            "/lib/arm-linux-gnueabihf",
            "/run/current-system/sw/lib",
            "/nix/var/nix/profiles/default/lib",
        ]
        .map(PathBuf::from),
    );

    dirs.iter().any(|dir| dir.join(name).exists())
}
