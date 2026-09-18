mod audio;
mod page;
mod preflight;
mod server;
mod tls;

use audio::{AudioConfig, InstanceLock, VirtualMic};
use nix::ifaddrs::getifaddrs;
use nix::net::if_::InterfaceFlags;
use server::{AudioFrameReceiver, MetricsHub, Server, SessionRegistry, audio_frame_channel};
use std::net::{IpAddr, Ipv4Addr};
use std::path::PathBuf;
use std::time::Duration;
use tokio::io::{AsyncWrite, AsyncWriteExt};
use tokio::net::unix::pipe;
use tracing::{debug, error, info, trace, warn};
use tracing_subscriber::FmtSubscriber;
use tracing_subscriber::filter::LevelFilter;

const DEFAULT_QUEUE_SIZE: usize = 1;
const DEFAULT_PORT: u16 = 59_152;
const MAX_QUEUE_SIZE: usize = 1_024;
const LOW_LATENCY_PIPE_SIZE_BYTES: i32 = 4_096;
const CONTAINER_RANK: u8 = 3;

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
    let (audio_tx, audio_rx) = audio_frame_channel(options.queue_size);

    let listener = bind_listener(options.bind, options.port).await?;
    let listen_addr = listener.local_addr()?;
    let port = listen_addr.port();
    let advertised_ips = advertised_addresses(listen_addr.ip())?;
    let certificate_dir = tls::certificate_directory().map_err(std::io::Error::other)?;
    let local_tls = tls::prepare(&certificate_dir, &advertised_ips)
        .await
        .map_err(std::io::Error::other)?;
    info!("RemoteMic starting on {listen_addr}");
    info!(
        "Virtual source: {} (audio queue: {} frames)",
        options.source_name, options.queue_size
    );
    let server = Server::new(audio_tx, audio_config, local_tls.ca_der);

    print_access_urls(&advertised_ips, port, &local_tls.ca_path);

    let writer = tokio::spawn(audio_writer_loop(
        pipe_path,
        audio_rx,
        server.sessions(),
        server.metrics(),
    ));
    let app = server.router();

    debug!("HTTPS/WebRTC signaling router ready; entering server loop");
    let std_listener = listener.into_std()?;
    let handle = axum_server::Handle::new();
    let shutdown_handle = handle.clone();
    tokio::spawn(async move {
        shutdown_signal().await;
        shutdown_handle.graceful_shutdown(Some(Duration::from_secs(3)));
    });
    let server_result = axum_server::from_tcp_rustls(std_listener, local_tls.config)?
        .handle(handle)
        .serve(app.into_make_service())
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
    port: u16,
) -> Result<tokio::net::TcpListener, std::io::Error> {
    debug!(%address, port, "Binding listen address");
    tokio::net::TcpListener::bind((address, port)).await
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
    port: u16,
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
        port: DEFAULT_PORT,
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
                options.port = value
                    .parse::<u16>()
                    .map_err(|_| format!("Invalid port \"{value}\": expected a number 0-65535"))?;
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
  -p, --port <PORT>          Listen on this port (default: 59152)
  -q, --quality <QUALITY>    low: 16 kHz/16-bit
                             standard: 24 kHz/16-bit (default)
                             high: 48 kHz/32-bit float
  -n, --source-name <NAME>   PulseAudio source name (default: RemoteMic)
      --queue-size <FRAMES>  Buffered audio frames, 1-1024 (default: 1)
  -l, --log-level <LEVEL>    error, warn, info, debug, or trace (default: info)
  -h, --help                 Print help
  -V, --version              Print version"#
}

fn print_access_urls(addresses: &[IpAddr], port: u16, ca_path: &std::path::Path) {
    for address in addresses {
        info!("Open https://{}:{port}", url_host(*address));
    }
    info!(
        "Install the local CA on the sending device once: {}",
        ca_path.display()
    );
    info!("Enable full trust for RemoteMic Local CA in the device certificate settings");
}

fn advertised_addresses(bind: IpAddr) -> Result<Vec<IpAddr>, std::io::Error> {
    match bind {
        IpAddr::V4(address) if address.is_unspecified() => {
            let mut addresses = global_ipv4_addresses();
            if addresses.is_empty() {
                addresses.push(advertised_address(IpAddr::V4(address))?);
            }
            Ok(addresses)
        }
        address => Ok(vec![advertised_address(address)?]),
    }
}

fn global_ipv4_addresses() -> Vec<IpAddr> {
    let Ok(interfaces) = getifaddrs() else {
        return Vec::new();
    };

    let mut candidates: Vec<(u8, IpAddr)> = Vec::new();
    for interface in interfaces {
        if !interface
            .flags
            .contains(InterfaceFlags::IFF_UP | InterfaceFlags::IFF_RUNNING)
        {
            continue;
        }
        let rank = interface_rank(&interface.interface_name);
        if rank == CONTAINER_RANK {
            continue;
        }
        let Some(storage) = interface.address.as_ref() else {
            continue;
        };
        let Some(inet) = storage.as_sockaddr_in() else {
            continue;
        };
        let address = IpAddr::V4(inet.ip());
        if !usable_ipv4(address) {
            continue;
        }
        candidates.push((rank, address));
    }

    candidates.sort_by_key(|(rank, address)| (*rank, address.to_string()));
    candidates.dedup_by(|a, b| a.1 == b.1);
    let Some(best_rank) = candidates.first().map(|(rank, _)| *rank) else {
        return Vec::new();
    };
    candidates
        .into_iter()
        .filter(|(rank, _)| *rank == best_rank)
        .map(|(_, address)| address)
        .collect()
}

fn usable_ipv4(address: IpAddr) -> bool {
    match address {
        IpAddr::V4(address) => {
            !address.is_unspecified()
                && !address.is_loopback()
                && !address.is_link_local()
                && !address.is_multicast()
                && !address.is_broadcast()
        }
        IpAddr::V6(_) => false,
    }
}

fn interface_rank(name: &str) -> u8 {
    if name.starts_with("docker")
        || name.starts_with("veth")
        || name.starts_with("br-")
        || name.starts_with("virbr")
        || name.starts_with("vmnet")
        || name.starts_with("vboxnet")
    {
        CONTAINER_RANK
    } else if name.starts_with("tun")
        || name.starts_with("tap")
        || name.starts_with("wg")
        || name.starts_with("tailscale")
        || name.starts_with("zt")
        || name.starts_with("utun")
    {
        2
    } else if name.starts_with("wl")
        || name.starts_with("en")
        || name.starts_with("eth")
        || name.starts_with("ww")
        || name.starts_with("usb")
    {
        0
    } else {
        1
    }
}

fn advertised_address(address: IpAddr) -> Result<IpAddr, std::io::Error> {
    match address {
        IpAddr::V4(address) if address.is_unspecified() => {
            let socket = std::net::UdpSocket::bind((Ipv4Addr::UNSPECIFIED, 0))?;
            match socket.connect((Ipv4Addr::new(192, 0, 2, 1), 9)) {
                Ok(()) => Ok(socket.local_addr()?.ip()),
                Err(error) => {
                    warn!(%error, "No routable IPv4 address found; advertising localhost");
                    Ok(IpAddr::V4(Ipv4Addr::LOCALHOST))
                }
            }
        }
        IpAddr::V6(address) if address.is_unspecified() => {
            let socket = std::net::UdpSocket::bind((std::net::Ipv6Addr::UNSPECIFIED, 0))?;
            match socket.connect((std::net::Ipv6Addr::new(0x2001, 0xdb8, 0, 0, 0, 0, 0, 1), 9)) {
                Ok(()) => Ok(socket.local_addr()?.ip()),
                Err(error) => {
                    warn!(%error, "No routable IPv6 address found; advertising localhost");
                    Ok(IpAddr::V6(std::net::Ipv6Addr::LOCALHOST))
                }
            }
        }
        address => Ok(address),
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
        if let Err(error) = signal::ctrl_c().await {
            error!(%error, "Failed to install Ctrl-C handler");
            std::future::pending::<()>().await;
        }
    };

    #[cfg(unix)]
    let terminate = async {
        match signal::unix::signal(signal::unix::SignalKind::terminate()) {
            Ok(mut stream) => {
                stream.recv().await;
            }
            Err(error) => {
                error!(%error, "Failed to install SIGTERM handler");
                std::future::pending::<()>().await;
            }
        }
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
    rx: AudioFrameReceiver,
    sessions: SessionRegistry,
    metrics: MetricsHub,
) {
    info!("Audio writer ready, waiting for data on {}", path.display());
    let mut stale_frames = 0_u64;

    loop {
        let first = rx.recv().await;

        if !sessions.is_active(first.session_id) {
            stale_frames += 1;
            trace!(
                session_id = first.session_id,
                stale_frames, "Discarding frame from inactive session"
            );
            continue;
        }
        metrics.record_queue_delay(first.enqueued_at.elapsed());

        debug!(session_id = first.session_id, "Opening audio pipe");
        let mut writer = match pipe::OpenOptions::new().open_sender(&path) {
            Ok(writer) => writer,
            Err(e) => {
                // ENXIO/NotFound simply mean no reader is attached yet; retry on
                // the next frame instead of stalling the writer indefinitely.
                debug!("Audio pipe is not ready for writing: {e}");
                let drained = rx.drain();
                trace!(
                    drained_frames = drained,
                    "Drained audio queue while waiting for a pipe reader"
                );
                continue;
            }
        };
        limit_pipe_buffer(&writer);

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
            let frame = rx.recv().await;
            if !sessions.is_active(frame.session_id) {
                stale_frames += 1;
                trace!(
                    session_id = frame.session_id,
                    stale_frames, "Discarding frame from inactive session"
                );
                continue;
            }
            metrics.record_queue_delay(frame.enqueued_at.elapsed());
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
    }
}

async fn write_chunk<W>(writer: &mut W, data: &[u8]) -> Result<(), ()>
where
    W: AsyncWrite + Unpin,
{
    writer.write_all(data).await.map_err(|e| {
        warn!("Audio pipe write failed (reader closed?): {e}");
    })
}

fn limit_pipe_buffer(writer: &pipe::Sender) {
    use nix::fcntl::{FcntlArg, fcntl};

    match fcntl(writer, FcntlArg::F_SETPIPE_SZ(LOW_LATENCY_PIPE_SIZE_BYTES)) {
        Ok(actual_size) => debug!(actual_size, "Limited audio FIFO buffer"),
        Err(error) => warn!(%error, "Could not limit audio FIFO buffer"),
    }
}

#[cfg(test)]
mod tests {
    use super::{
        DEFAULT_PORT, DEFAULT_QUEUE_SIZE, Options, Quality, advertised_addresses, interface_rank,
        parse_args, url_host, usable_ipv4,
    };
    use std::net::{IpAddr, Ipv4Addr, Ipv6Addr};
    use tracing_subscriber::filter::LevelFilter;

    fn strings(values: &[&str]) -> Vec<String> {
        values.iter().map(|value| (*value).to_string()).collect()
    }

    fn default_options() -> Options {
        Options {
            bind: IpAddr::V4(Ipv4Addr::UNSPECIFIED),
            port: DEFAULT_PORT,
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
    fn defaults_to_stable_service_port() {
        assert_eq!(parse_args(Vec::new()).unwrap().port, 59_152);
    }

    #[test]
    fn parses_high_quality_and_port() {
        let expected = Options {
            port: 9000,
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
    fn concrete_bind_address_is_advertised_unchanged() {
        let address = IpAddr::V4(Ipv4Addr::new(192, 0, 2, 10));

        assert_eq!(advertised_addresses(address).unwrap(), vec![address]);
    }

    #[test]
    fn physical_interfaces_rank_before_vpn_and_container_interfaces() {
        assert!(interface_rank("wlan0") < interface_rank("tun0"));
        assert!(interface_rank("eth0") < interface_rank("tailscale0"));
        assert!(interface_rank("tun0") < interface_rank("docker0"));
    }

    #[test]
    fn loopback_and_link_local_are_not_advertisable() {
        assert!(!usable_ipv4(IpAddr::V4(Ipv4Addr::LOCALHOST)));
        assert!(!usable_ipv4(IpAddr::V4(Ipv4Addr::new(169, 254, 1, 2))));
        assert!(usable_ipv4(IpAddr::V4(Ipv4Addr::new(192, 168, 1, 2))));
    }

    #[test]
    fn ipv6_url_host_is_bracketed() {
        assert_eq!(url_host(IpAddr::V6(Ipv6Addr::LOCALHOST)), "[::1]");
    }
}
