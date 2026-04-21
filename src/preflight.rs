use std::path::Path;
use tokio::process::Command;
use tracing::{info, warn};

async fn which(cmd: &str) -> bool {
    Command::new("which")
        .arg(cmd)
        .output()
        .await
        .map(|o| o.status.success())
        .unwrap_or(false)
}

pub async fn check_pactl() -> Result<(), String> {
    if which("pactl").await {
        info!("pactl: OK");
        Ok(())
    } else {
        Err("pactl not found. Install PulseAudio or PipeWire-pulse:\n  \
             Debian/Ubuntu: sudo apt install pulseaudio-utils\n  \
             Fedora:        sudo dnf install pulseaudio-utils\n  \
             Arch:          sudo pacman -S libpulse"
            .to_string())
    }
}

pub fn check_audio_libs() {
    let required: &[&str] = &["libpulse.so.0", "libasound.so.2"];
    let optional: &[&str] = &["libpipewire-0.3.so.0"];

    for lib in required {
        if lib_exists(lib) {
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
        if lib_exists(lib) {
            info!("lib {lib}: OK (PipeWire native)");
        } else {
            info!("lib {lib}: not found (optional, PipeWire native support disabled)");
        }
    }
}

fn lib_exists(name: &str) -> bool {
    let search_paths = [
        "/usr/lib",
        "/usr/lib/x86_64-linux-gnu",
        "/usr/lib/aarch64-linux-gnu",
        "/usr/lib64",
        "/usr/local/lib",
        "/lib",
        "/lib/x86_64-linux-gnu",
        "/lib64",
    ];
    search_paths
        .iter()
        .any(|dir| Path::new(dir).join(name).exists())
}
