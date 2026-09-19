use tokio::process::Command;
use tracing::{debug, info};

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
