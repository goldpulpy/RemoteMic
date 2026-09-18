mod audio;
mod page;
mod preflight;
mod server;

use audio::VirtualMic;
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

    let requested_port = match parse_port_arg() {
        Ok(port) => port,
        Err(msg) => {
            error!("{msg}\nUsage: remotemic [-p <port>]");
            std::process::exit(1);
        }
    };

    if let Err(msg) = preflight::check_pactl().await {
        error!("{msg}");
        std::process::exit(1);
    }

    preflight::check_audio_libs().await;

    let virtual_mic = VirtualMic::new();
    virtual_mic.load().await?;

    let pipe_path = virtual_mic.pipe_path();
    let (audio_tx, audio_rx) = mpsc::channel::<AudioFrame>(8);

    let listener = bind_listener(requested_port).await?;
    let port = listener.local_addr()?.port();
    info!("RemoteMic starting on port {port}");
    let server = Server::new(audio_tx);

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

fn parse_port_arg() -> Result<Option<u16>, String> {
    let args: Vec<String> = std::env::args().collect();
    let Some(idx) = args.iter().position(|a| a == "-p" || a == "--port") else {
        return Ok(None);
    };
    let value = args
        .get(idx + 1)
        .ok_or_else(|| "Missing value after -p/--port".to_string())?;
    let port = value
        .parse::<u16>()
        .map_err(|_| format!("Invalid port \"{value}\": expected a number 0-65535"))?;
    Ok(Some(port))
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
