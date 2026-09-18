mod audio;
mod page;
mod preflight;
mod server;

use audio::{AudioConfig, InstanceLock, VirtualMic};
use server::{AudioFrame, Server, SessionRegistry};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::unix::pipe;
use tokio::sync::mpsc;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::FmtSubscriber;
use tracing_subscriber::filter::LevelFilter;

const DEFAULT_QUEUE_SIZE: usize = 8;
const MAX_QUEUE_SIZE: usize = 1_024;

#[tokio::main]
async fn main() -> Result<(), Box<dyn std::error::Error>> {
    let options = match parse_args(std::env::args().skip(1)) {
        Ok(options) => options,
        Err(msg) => {
            eprintln!("error: {msg}\n\n{}", usage());
            std::process::exit(2);
        }
    };

    if options.help {
        println!("{}", usage());
        return Ok(());
    }
    if options.version {
        println!("remotemic {}", env!("CARGO_PKG_VERSION"));
        return Ok(());
    }

    let subscriber = FmtSubscriber::builder()
        .with_max_level(options.log_level)
        .with_target(false)
        .finish();
    tracing::subscriber::set_global_default(subscriber)?;
    debug!(?options, "Command-line options parsed");

    let _instance_lock = match InstanceLock::acquire() {
        Ok(lock) => lock,
        Err(message) => {
            error!("{message}");
            std::process::exit(1);
        }
    };
    debug!("Running startup checks");

    if let Err(msg) = preflight::check_pactl().await {
        error!("{msg}");
        std::process::exit(1);
    }

    preflight::check_audio_libs().await;
    debug!("Startup checks completed");

    let audio_config = options.quality.audio_config();
    info!(
        "Audio quality: {} ({} Hz, {}, mono)",
        options.quality.name(),
        audio_config.sample_rate,
        audio_config.sample_format.pulse_name()
    );
    let virtual_mic = VirtualMic::new(audio_config, options.source_name.clone())?;
    virtual_mic.load().await?;

    let service_result = run_service(&options, audio_config, virtual_mic.pipe_path()).await;

    info!("Shutting down…");
    debug!("Unloading virtual microphone");
    let unload_result = virtual_mic.unload().await;

    match (service_result, unload_result) {
        (Ok(()), Ok(())) => {
            info!("RemoteMic stopped");
            Ok(())
        }
        (Err(service_error), Ok(())) => Err(service_error),
        (Ok(()), Err(unload_error)) => {
            error!("{unload_error}");
            Err(std::io::Error::other(unload_error).into())
        }
        (Err(service_error), Err(unload_error)) => {
            error!("Failed to unload virtual microphone after server error: {unload_error}");
            Err(service_error)
        }
    }
}

async fn run_service(
    options: &Options,
    audio_config: AudioConfig,
    pipe_path: PathBuf,
) -> Result<(), Box<dyn std::error::Error>> {
    let (audio_tx, audio_rx) = mpsc::channel::<AudioFrame>(options.queue_size);

    let listener = bind_listener(options.bind, options.port).await?;
    let listen_addr = listener.local_addr()?;
    let port = listen_addr.port();
    info!("RemoteMic starting on {listen_addr}");
    info!(
        "Virtual source: {} (audio queue: {} frames)",
        options.source_name, options.queue_size
    );
    let server = Server::new(audio_tx, audio_config);

    print_access_urls(listen_addr.ip(), port);

    let writer = tokio::spawn(audio_writer_loop(pipe_path, audio_rx, server.sessions()));
    let app = server.router();

    debug!("HTTP and WebSocket router ready; entering server loop");
    let server_result = axum::serve(listener, app)
        .with_graceful_shutdown(shutdown_signal())
        .await;

    debug!("Stopping audio writer task");
    writer.abort();
    match writer.await {
        Ok(()) => debug!("Audio writer task exited normally"),
        Err(error) if error.is_cancelled() => debug!("Audio writer task cancelled"),
        Err(error) => warn!(%error, "Audio writer task failed"),
    }

    server_result.map_err(Box::<dyn std::error::Error>::from)
}

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

async fn bind_listener(
    address: IpAddr,
    port: Option<u16>,
) -> Result<tokio::net::TcpListener, std::io::Error> {
    match port {
        Some(port) => {
            debug!(%address, port, "Binding requested listen address");
            tokio::net::TcpListener::bind((address, port)).await
        }
        None => {
            debug!(%address, "Selecting a random dynamic port");
            loop {
                let candidate = rand::random_range(49152..=65535);
                trace!(%address, port = candidate, "Trying listen address");
                match tokio::net::TcpListener::bind((address, candidate)).await {
                    Ok(listener) => {
                        debug!(%address, port = candidate, "Selected random listen port");
                        return Ok(listener);
                    }
                    Err(error) if error.kind() == std::io::ErrorKind::AddrInUse => {
                        trace!(%address, port = candidate, "Random listen port is already in use");
                    }
                    Err(error) => return Err(error),
                }
            }
        }
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
    bind: IpAddr,
    port: Option<u16>,
    quality: Quality,
    source_name: String,
    queue_size: usize,
    log_level: LevelFilter,
    help: bool,
    version: bool,
}

fn parse_args(args: impl IntoIterator<Item = String>) -> Result<Options, String> {
    let mut args = args.into_iter();
    let mut options = Options {
        bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
        port: None,
        quality: Quality::Standard,
        source_name: "RemoteMic".to_string(),
        queue_size: DEFAULT_QUEUE_SIZE,
        log_level: LevelFilter::INFO,
        help: false,
        version: false,
    };

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "-b" | "--bind" => {
                let value = next_value(&mut args, "-b/--bind")?;
                options.bind = value.parse::<IpAddr>().map_err(|_| {
                    format!("Invalid bind address \"{value}\": expected an IPv4 or IPv6 address")
                })?;
            }
            "-p" | "--port" => {
                let value = next_value(&mut args, "-p/--port")?;
                options.port =
                    Some(value.parse::<u16>().map_err(|_| {
                        format!("Invalid port \"{value}\": expected a number 0-65535")
                    })?);
            }
            "-q" | "--quality" => {
                let value = next_value(&mut args, "-q/--quality")?;
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
            "-n" | "--source-name" => {
                let value = next_value(&mut args, "-n/--source-name")?;
                validate_source_name(&value)?;
                options.source_name = value;
            }
            "--queue-size" => {
                let value = next_value(&mut args, "--queue-size")?;
                options.queue_size = value.parse::<usize>().map_err(|_| {
                    format!(
                        "Invalid queue size \"{value}\": expected a number from 1 to {MAX_QUEUE_SIZE}"
                    )
                })?;
                if !(1..=MAX_QUEUE_SIZE).contains(&options.queue_size) {
                    return Err(format!(
                        "Invalid queue size \"{value}\": expected a number from 1 to {MAX_QUEUE_SIZE}"
                    ));
                }
            }
            "-l" | "--log-level" => {
                let value = next_value(&mut args, "-l/--log-level")?;
                options.log_level = match value.as_str() {
                    "error" => LevelFilter::ERROR,
                    "warn" => LevelFilter::WARN,
                    "info" => LevelFilter::INFO,
                    "debug" => LevelFilter::DEBUG,
                    "trace" => LevelFilter::TRACE,
                    _ => {
                        return Err(format!(
                            "Invalid log level \"{value}\": expected error, warn, info, debug, or trace"
                        ));
                    }
                };
            }
            "-h" | "--help" => options.help = true,
            "-V" | "--version" => options.version = true,
            _ => return Err(format!("Unknown argument \"{arg}\"")),
        }
    }

    Ok(options)
}

fn next_value(args: &mut impl Iterator<Item = String>, option: &str) -> Result<String, String> {
    args.next()
        .ok_or_else(|| format!("Missing value after {option}"))
}

fn validate_source_name(value: &str) -> Result<(), String> {
    if value.is_empty()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b'.'))
    {
        return Err(format!(
            "Invalid source name \"{value}\": use only ASCII letters, digits, '.', '-', and '_'"
        ));
    }
    Ok(())
}

fn usage() -> &'static str {
    r#"Usage: remotemic [OPTIONS]

Options:
  -b, --bind <ADDRESS>       Bind to this IP address (default: 0.0.0.0)
  -p, --port <PORT>          Listen on this port (default: random)
  -q, --quality <QUALITY>    low: 16 kHz/16-bit
                             standard: 44.1 kHz/16-bit (default)
                             high: 48 kHz/32-bit float
  -n, --source-name <NAME>   PulseAudio source name (default: RemoteMic)
      --queue-size <FRAMES>  Buffered audio frames, 1-1024 (default: 8)
  -l, --log-level <LEVEL>    error, warn, info, debug, or trace (default: info)
  -h, --help                 Print help
  -V, --version              Print version"#
}

fn print_access_urls(address: IpAddr, port: u16) {
    let connect_address = connect_address(address);
    let url_host = url_host(connect_address);

    info!("Open http://{url_host}:{port}");
    info!(
        "NOTE: Microphone access requires HTTPS on non-localhost origins. \
             If the mic does not work, run: npx localtunnel --port {port} \
             --local-host {connect_address}"
    );
}

fn connect_address(address: IpAddr) -> IpAddr {
    match address {
        IpAddr::V4(address) if address.is_unspecified() => IpAddr::V4(Ipv4Addr::LOCALHOST),
        IpAddr::V6(address) if address.is_unspecified() => {
            IpAddr::V6(std::net::Ipv6Addr::LOCALHOST)
        }
        address => address,
    }
}

fn url_host(address: IpAddr) -> String {
    match address {
        IpAddr::V4(address) => address.to_string(),
        IpAddr::V6(address) => format!("[{address}]"),
    }
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
    let mut stale_frames = 0_u64;

    loop {
        let first = match rx.recv().await {
            Some(d) => d,
            None => return,
        };

        if !sessions.is_active(first.session_id) {
            stale_frames += 1;
            trace!(
                session_id = first.session_id,
                stale_frames, "Discarding frame from inactive session"
            );
            continue;
        }

        debug!(session_id = first.session_id, "Opening audio pipe");
        let mut writer = match pipe::OpenOptions::new().read_write(true).open_sender(&path) {
            Ok(writer) => writer,
            Err(e) => {
                error!("Failed to open pipe for writing: {e}");
                let drained = drain_channel(&mut rx);
                debug!(
                    drained_frames = drained,
                    "Drained audio queue after pipe error"
                );
                continue;
            }
        };

        info!("Pipe opened, streaming audio");

        let mut written_frames = 1_u64;
        let mut written_bytes = first.data.len() as u64;
        if write_chunk(&mut writer, &first.data).await.is_err() {
            continue;
        }
        trace!(
            session_id = first.session_id,
            bytes = first.data.len(),
            "Wrote audio frame"
        );

        loop {
            match rx.recv().await {
                Some(frame) => {
                    if !sessions.is_active(frame.session_id) {
                        stale_frames += 1;
                        trace!(
                            session_id = frame.session_id,
                            stale_frames, "Discarding frame from inactive session"
                        );
                        continue;
                    }
                    if write_chunk(&mut writer, &frame.data).await.is_err() {
                        debug!(
                            session_id = frame.session_id,
                            written_frames, written_bytes, "Audio pipe stream interrupted"
                        );
                        break;
                    }
                    written_frames += 1;
                    written_bytes += frame.data.len() as u64;
                    trace!(
                        session_id = frame.session_id,
                        bytes = frame.data.len(),
                        written_frames,
                        written_bytes,
                        "Wrote audio frame"
                    );
                    if written_frames.is_multiple_of(256) {
                        debug!(
                            session_id = frame.session_id,
                            written_frames, written_bytes, "Audio pipe streaming progress"
                        );
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

fn drain_channel(rx: &mut mpsc::Receiver<AudioFrame>) -> usize {
    let mut drained = 0;
    while rx.try_recv().is_ok() {
        drained += 1;
    }
    drained
}

#[cfg(test)]
mod tests {
    use super::{DEFAULT_QUEUE_SIZE, Options, Quality, connect_address, parse_args, url_host};
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use tracing_subscriber::filter::LevelFilter;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn default_options() -> Options {
        Options {
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: None,
            quality: Quality::Standard,
            source_name: "RemoteMic".to_string(),
            queue_size: DEFAULT_QUEUE_SIZE,
            log_level: LevelFilter::INFO,
            help: false,
            version: false,
        }
    }

    #[test]
    fn defaults_to_current_audio_quality() {
        assert_eq!(parse_args(Vec::new()).unwrap(), default_options());
    }

    #[test]
    fn parses_high_quality_and_port() {
        let expected = Options {
            port: Some(9000),
            quality: Quality::High,
            ..default_options()
        };

        assert_eq!(
            parse_args(strings(&["--quality", "high", "--port", "9000"])).unwrap(),
            expected
        );
    }

    #[test]
    fn parses_low_quality() {
        let expected = Options {
            quality: Quality::Low,
            ..default_options()
        };

        assert_eq!(parse_args(strings(&["-q", "low"])).unwrap(), expected);
    }

    #[test]
    fn parses_server_and_diagnostics_options() {
        let expected = Options {
            bind: "127.0.0.1".parse().unwrap(),
            source_name: "studio_mic".to_string(),
            queue_size: 32,
            log_level: LevelFilter::DEBUG,
            ..default_options()
        };

        assert_eq!(
            parse_args(strings(&[
                "--bind",
                "127.0.0.1",
                "--source-name",
                "studio_mic",
                "--queue-size",
                "32",
                "--log-level",
                "debug",
            ]))
            .unwrap(),
            expected
        );
    }

    #[test]
    fn rejects_unknown_quality() {
        assert!(parse_args(strings(&["--quality", "lossless"])).is_err());
    }

    #[test]
    fn rejects_zero_queue_size() {
        assert!(parse_args(strings(&["--queue-size", "0"])).is_err());
    }

    #[test]
    fn rejects_source_name_with_spaces() {
        assert!(parse_args(strings(&["--source-name", "Studio Mic"])).is_err());
    }

    #[test]
    fn unspecified_ipv4_advertises_ipv4_loopback() {
        assert_eq!(
            connect_address(IpAddr::V4(Ipv4Addr::UNSPECIFIED)),
            IpAddr::V4(Ipv4Addr::LOCALHOST)
        );
    }

    #[test]
    fn unspecified_ipv6_advertises_ipv6_loopback() {
        assert_eq!(
            connect_address(IpAddr::V6(Ipv6Addr::UNSPECIFIED)),
            IpAddr::V6(Ipv6Addr::LOCALHOST)
        );
    }

    #[test]
    fn concrete_bind_address_is_advertised_unchanged() {
        let address = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));

        assert_eq!(connect_address(address), address);
    }

    #[test]
    fn ipv6_url_host_is_bracketed() {
        assert_eq!(url_host(IpAddr::V6(Ipv6Addr::LOCALHOST)), "[::1]");
    }
}
