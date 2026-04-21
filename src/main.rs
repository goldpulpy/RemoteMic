mod audio;
mod page;
mod preflight;
mod server;

use audio::VirtualMic;
use server::Server;
use std::path::PathBuf;
use tokio::io::AsyncWriteExt;
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

    let port = parse_port_arg().unwrap_or(get_available_port().await?);

    info!("RemoteMic starting on port {port}");

    if let Err(msg) = preflight::check_pactl().await {
        error!("{msg}");
        std::process::exit(1);
    }

    preflight::check_audio_libs();

    let virtual_mic = VirtualMic::new();
    virtual_mic.load().await?;

    let pipe_path = virtual_mic.pipe_path();
    let (audio_tx, audio_rx) = mpsc::channel::<Vec<u8>>(256);

    let listener = tokio::net::TcpListener::bind(("0.0.0.0", port)).await?;
    let server = Server::new(audio_tx);

    print_access_urls(port);

    let writer = tokio::spawn(audio_writer_loop(pipe_path, audio_rx));
    let app = server.router();

    axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await?;

    info!("Shutting down…");
    writer.abort();
    let _ = writer.await;

    virtual_mic.unload().await?;

    info!("RemoteMic stopped");
    Ok(())
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn get_available_port() -> Result<u16, std::io::Error> {
    loop {
        let port = rand::random_range(49152..=65535);

        if let Ok(listener) = tokio::net::TcpListener::bind(("0.0.0.0", port)).await {
            drop(listener);
            return Ok(port);
        }
    }
}

fn parse_port_arg() -> Option<u16> {
    let args: Vec<String> = std::env::args().collect();
    let idx = args.iter().position(|a| a == "-p" || a == "--port")?;
    args.get(idx + 1)?.parse().ok()
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

async fn audio_writer_loop(path: PathBuf, mut rx: mpsc::Receiver<Vec<u8>>) {
    info!("Audio writer ready, waiting for data on {}", path.display());

    loop {
        let first = match rx.recv().await {
            Some(d) => d,
            None => return,
        };

        let p = path.clone();
        let file_result =
            tokio::task::spawn_blocking(move || std::fs::OpenOptions::new().write(true).open(&p))
                .await;

        let std_file = match file_result {
            Ok(Ok(f)) => f,
            Ok(Err(e)) => {
                error!("Failed to open pipe for writing: {e}");
                drain_channel(&mut rx).await;
                continue;
            }
            Err(e) => {
                error!("Pipe open task failed: {e}");
                return;
            }
        };

        let mut file = tokio::fs::File::from_std(std_file);
        info!("Pipe opened, streaming audio");

        if write_chunk(&mut file, &first).await.is_err() {
            continue;
        }

        loop {
            match rx.recv().await {
                Some(data) => {
                    if write_chunk(&mut file, &data).await.is_err() {
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

async fn write_chunk(file: &mut tokio::fs::File, data: &[u8]) -> Result<(), ()> {
    file.write_all(data).await.map_err(|e| {
        warn!("Pipe write error (client disconnected?): {e}");
    })
}

async fn drain_channel(rx: &mut mpsc::Receiver<Vec<u8>>) {
    while rx.try_recv().is_ok() {}
}
