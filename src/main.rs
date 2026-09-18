mod audio;
mod page;
mod preflight;
mod server;

use audio::{AudioConfig, VirtualMic};
use server::{AudioFrame, Server, SessionRegistry};
use std::path::PathBuf;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::unix::pipe;
use tokio::sync::mpsc;
use tracing::{Level, error, info, warn};
use tracing_subscriber::FmtSubscriber;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let subscriber = FmtSubscriber::builder()
        .with_max_level(Level::INFO)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;

    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(msg) => {
            error!("{msg}\n\n{}", usage());
            std::process::exit(1);
        }
    };
    if options.help {
        println!("{}", usage());
        return Ok(());
    }

    if let Err(msg) = preflight::check_pactl().await {
        error!("{msg}");
        std::process::exit(1);
    }

    preflight::check_audio_libs().await;

    let audio_config = options.quality.audio_config();
    info!(
        "Audio quality: {} ({} Hz, {}, mono)",
        options.quality.name(),
        audio_config.sample_rate,
        audio_config.sample_format.pulse_name()
    );
    let virtual_mic = VirtualMic::new(audio_config);
    virtual_mic.load().await?;

    let pipe_path = virtual_mic.pipe_path();
    let (audio_tx, audio_rx) = mpsc::channel::<AudioFrame>(8);

    let listener = bind_listener(options.port).await?;
    let port = listener.local_addr()?.port();
    info!("RemoteMic starting on port {port}");
    let server = Server::new(audio_tx, audio_config);

    print_access_urls(port);

    let writer = tokio::spawn(audio_writer_loop(pipe_path, audio_rx, server.sessions()));
    let app = server.router();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Shutting down…");
    writer.abort();
    let _ = writer.await;

    if let Err(e) = virtual_mic.unload().await {
        error!("{e}");
    }

    info!("RemoteMic stopped");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn bind_listener(port: Option<u16>) -> Result<tokio::net::TcpListener, std::io::Error> {
    match port {
        Some(port) => tokio::net::TcpListener::bind(("0.0.0.0", port)).await,
        None => loop {
            let candidate = rand::random_range(49152..=65535);
            if let Ok(listener) = tokio::net::TcpListener::bind(("0.0.0.0", candidate)).await {
                return Ok(listener);
            }
        },
    }
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
enum Quality {
    Low,
    Standard,
    High,
}

impl Quality {
    const fn audio_config(self) -> AudioConfig {
        match self {
            Self::Low => AudioConfig::LOW,
            Self::Standard => AudioConfig::STANDARD,
            Self::High => AudioConfig::HIGH,
        }
    }

    const fn name(self) -> &'static str {
        match self {
            Self::Low => "low",
            Self::Standard => "standard",
            Self::High => "high",
        }
    }
}

#[derive(Debug, Eq, PartialEq)]
struct Options {
    port: Option<u16>,
    quality: Quality,
    help: bool,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let mut options = Options {
        port: None,
        quality: Quality::Standard,
        help: false,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-p" | "--port" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value after -p/--port".to_string())?;
                options.port =
                    Some(value.parse::<u16>().map_err(|_| {
                        format!("Invalid port \"{value}\": expected a number 0-65535")
                    })?);
            }
            "-q" | "--quality" => {
                let value = args
                    .next()
                    .ok_or_else(|| "Missing value after -q/--quality".to_string())?;
                options.quality = match value.as_str() {
                    "low" => Quality::Low,
                    "standard" => Quality::Standard,
                    "high" => Quality::High,
                    _ => {
                        return Err(format!(
                            "Invalid quality \"{value}\": expected low, standard, or high"
                        ));
                    }
                };
            }
            "-h" | "--help" => options.help = true,
            _ => return Err(format!("Unknown argument \"{arg}\"")),
        }
    }

    Ok(options)
}

fn usage() -> &'static str {
    r#"Usage: remotemic [OPTIONS]

Options:
  -p, --port <PORT>          Listen on this port (default: random)
  -q, --quality <QUALITY>    low: 16 kHz/16-bit
                             standard: 44.1 kHz/16-bit (default)
                             high: 48 kHz/32-bit float
  -h, --help                 Print help"#
}

fn print_access_urls(port: u16) {
    info!("Open http://localhost:{port}");
    info!(
        "NOTE: Microphone access requires HTTPS on non-localhost origins. \
             If the mic does not work, run: npx localtunnel --port {port}"
    );
}

async fn shutdown_signal() {
    use tokio::signal;

    let ctrl_c = async {
        signal::ctrl_c()
            .await
            .expect("failed to install Ctrl-C handler");
    };

    #[cfg(unix)]
    let terminate = async {
        signal::unix::signal(signal::unix::SignalKind::terminate())
            .expect("failed to install SIGTERM handler")
            .recv()
            .await;
    };

    #[cfg(not(unix))]
    let terminate = std::future::pending::<()>();

    tokio::select! {
        _ = ctrl_c => info!("Received SIGINT"),
        _ = terminate => info!("Received SIGTERM"),
    }
}

// ---------------------------------------------------------------------------
// Audio pipe writer
// ---------------------------------------------------------------------------

async fn audio_writer_loop(
    path: PathBuf,
    mut rx: mpsc::Receiver<AudioFrame>,
    sessions: SessionRegistry,
) {
    info!("Audio writer ready, waiting for data on {}", path.display());

    loop {
        let first = match rx.recv().await {
            Some(d) => d,
            None => return,
        };

        if !sessions.is_active(first.session_id) {
            continue;
        }

        let mut writer = match pipe::OpenOptions::new().read_write(true).open_sender(&path) {
            Ok(writer) => writer,
            Err(e) => {
                error!("Failed to open pipe for writing: {e}");
                drain_channel(&mut rx).await;
                continue;
            }
        };

        info!("Pipe opened, streaming audio");

        if write_chunk(&mut writer, &first.data).await.is_err() {
            continue;
        }

        loop {
            match rx.recv().await {
                Some(frame) => {
                    if !sessions.is_active(frame.session_id) {
                        continue;
                    }
                    if write_chunk(&mut writer, &frame.data).await.is_err() {
                        break;
                    }
                }
                None => {
                    info!("Audio channel closed, writer exiting");
                    return;
                }
            }
        }
    }
}

async fn write_chunk<W>(writer: &mut W, data: &[u8]) -> Result<(), ()>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(data).await.map_err(|e| {
        warn!("Pipe write error (client disconnected?): {e}");
    })
}

async fn drain_channel(rx: &mut mpsc::Receiver<AudioFrame>) {
    while rx.try_recv().is_ok() {}
}

#[cfg(test)]
mod tests {
    use super::{Options, Quality, parse_args};

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    #[test]
    fn defaults_to_current_audio_quality() {
        assert_eq!(
            parse_args(Vec::new()).unwrap(),
            Options {
                port: None,
                quality: Quality::Standard,
                help: false,
            }
        );
    }

    #[test]
    fn parses_high_quality_and_port() {
        assert_eq!(
            parse_args(strings(&["--quality", "high", "--port", "9000"])).unwrap(),
            Options {
                port: Some(9000),
                quality: Quality::High,
                help: false,
            }
        );
    }

    #[test]
    fn parses_low_quality() {
        assert_eq!(
            parse_args(strings(&["-q", "low"])).unwrap(),
            Options {
                port: None,
                quality: Quality::Low,
                help: false,
            }
        );
    }

    #[test]
    fn rejects_unknown_quality() {
        assert!(parse_args(strings(&["--quality", "lossless"])).is_err());
    }
}
