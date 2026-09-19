use async_trait::async_trait;
use axum::{
    Router,
    body::Body,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, Uri, header},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use opus_rs::OpusDecoder;
use rtc::{
    interceptor::Registry,
    peer_connection::{
        configuration::{
            RTCConfigurationBuilder,
            interceptor_registry::register_default_interceptors,
            media_engine::{MIME_TYPE_OPUS, MediaEngine},
        },
        sdp::RTCSessionDescription,
    },
    rtp_transceiver::{
        RTCRtpTransceiverDirection, RTCRtpTransceiverInit,
        rtp_sender::{RTCRtpCodec, RTCRtpCodecParameters, RtpCodecKind},
    },
};
use serde::Serialize;
use std::{
    collections::VecDeque,
    net::IpAddr,
    sync::{
        Arc, Mutex,
        atomic::{AtomicU64, Ordering},
    },
    time::{Duration, Instant},
};
use tokio::sync::{Notify, watch};
use tracing::{debug, error, info, trace, warn};
use webrtc::{
    media_stream::track_remote::{TrackRemote, TrackRemoteEvent},
    peer_connection::{
        PeerConnection, PeerConnectionBuilder, PeerConnectionEventHandler, RTCIceGatheringState,
        RTCPeerConnectionState,
    },
};

use crate::{
    audio::{AudioConfig, SampleFormat},
    page,
};

const MAX_WS_MESSAGE_SIZE: usize = 256 * 1024;
const ACQUIRE_RETRY_TIMEOUT: Duration = Duration::from_millis(500);
const ACQUIRE_RETRY_INTERVAL: Duration = Duration::from_millis(10);
const SIGNALING_IDLE_TIMEOUT: Duration = Duration::from_secs(45);
const CONNECTION_ESTABLISH_TIMEOUT: Duration = Duration::from_secs(30);
const DISCONNECTED_TIMEOUT: Duration = Duration::from_secs(15);
const CODEC_LOOKUP_ATTEMPTS: usize = 25;
const CODEC_LOOKUP_INTERVAL: Duration = Duration::from_millis(20);
const MAX_PLC_PACKETS: u16 = 3;
const OPUS_CLOCK_RATE: u32 = 48_000;

#[derive(Clone)]
pub struct Server {
    audio_tx: AudioFrameSender,
    sessions: SessionRegistry,
    token: Arc<str>,
    audio_config: AudioConfig,
    ca_der: Arc<[u8]>,
    metrics: MetricsHub,
    ice_udp_addrs: Arc<[String]>,
}

pub struct AudioFrame {
    pub session_id: u64,
    pub data: Vec<u8>,
    pub enqueued_at: Instant,
}

struct AudioFrameQueue {
    frames: Mutex<VecDeque<AudioFrame>>,
    capacity: usize,
    ready: Notify,
}

#[derive(Clone)]
pub struct AudioFrameSender(Arc<AudioFrameQueue>);

pub struct AudioFrameReceiver(Arc<AudioFrameQueue>);

pub fn audio_frame_channel(capacity: usize) -> (AudioFrameSender, AudioFrameReceiver) {
    let queue = Arc::new(AudioFrameQueue {
        frames: Mutex::new(VecDeque::with_capacity(capacity.max(1))),
        capacity: capacity.max(1),
        ready: Notify::new(),
    });
    (
        AudioFrameSender(Arc::clone(&queue)),
        AudioFrameReceiver(queue),
    )
}

impl AudioFrameSender {
    fn send_latest(&self, frame: AudioFrame) -> bool {
        let mut frames = self
            .0
            .frames
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let replaced_oldest = frames.len() == self.0.capacity;
        if replaced_oldest {
            frames.pop_front();
        }
        frames.push_back(frame);
        drop(frames);
        self.0.ready.notify_one();
        replaced_oldest
    }
}

impl AudioFrameReceiver {
    pub async fn recv(&self) -> AudioFrame {
        loop {
            let ready = self.0.ready.notified();
            if let Some(frame) = self.try_recv() {
                return frame;
            }
            ready.await;
        }
    }

    pub fn try_recv(&self) -> Option<AudioFrame> {
        self.0
            .frames
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .pop_front()
    }

    pub fn drain(&self) -> usize {
        let mut frames = self
            .0
            .frames
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        let drained = frames.len();
        frames.clear();
        drained
    }
}

#[derive(Clone, Default)]
pub struct SessionRegistry {
    active: Arc<AtomicU64>,
    next: Arc<AtomicU64>,
}

impl SessionRegistry {
    async fn acquire(&self) -> Option<u64> {
        let session_id = self.next.fetch_add(1, Ordering::Relaxed) + 1;
        let deadline = tokio::time::Instant::now() + ACQUIRE_RETRY_TIMEOUT;
        loop {
            if self
                .active
                .compare_exchange(0, session_id, Ordering::AcqRel, Ordering::Acquire)
                .is_ok()
            {
                return Some(session_id);
            }
            if tokio::time::Instant::now() >= deadline {
                return None;
            }
            tokio::time::sleep(ACQUIRE_RETRY_INTERVAL).await;
        }
    }

    fn release(&self, session_id: u64) {
        self.active
            .compare_exchange(session_id, 0, Ordering::AcqRel, Ordering::Acquire)
            .ok();
    }

    pub fn is_active(&self, session_id: u64) -> bool {
        self.active.load(Ordering::Acquire) == session_id
    }
}

#[derive(Clone, Default)]
pub struct StreamMetrics(Arc<MetricsAtoms>);

#[derive(Default)]
struct MetricsAtoms {
    packets_received: AtomicU64,
    packets_lost: AtomicU64,
    packets_reordered: AtomicU64,
    decoded_frames: AtomicU64,
    dropped_frames: AtomicU64,
    jitter_micros: AtomicU64,
    queue_micros: AtomicU64,
}

#[derive(Serialize)]
#[serde(rename_all = "camelCase")]
struct MetricsSnapshot {
    packets_received: u64,
    packets_lost: u64,
    packets_reordered: u64,
    decoded_frames: u64,
    dropped_frames: u64,
    loss_percent: f64,
    jitter_ms: f64,
    queue_ms: f64,
}

impl StreamMetrics {
    fn snapshot(&self) -> MetricsSnapshot {
        let received = self.0.packets_received.load(Ordering::Relaxed);
        let lost = self.0.packets_lost.load(Ordering::Relaxed);
        let total = received.saturating_add(lost);
        MetricsSnapshot {
            packets_received: received,
            packets_lost: lost,
            packets_reordered: self.0.packets_reordered.load(Ordering::Relaxed),
            decoded_frames: self.0.decoded_frames.load(Ordering::Relaxed),
            dropped_frames: self.0.dropped_frames.load(Ordering::Relaxed),
            loss_percent: if total == 0 {
                0.0
            } else {
                lost as f64 * 100.0 / total as f64
            },
            jitter_ms: self.0.jitter_micros.load(Ordering::Relaxed) as f64 / 1_000.0,
            queue_ms: self.0.queue_micros.load(Ordering::Relaxed) as f64 / 1_000.0,
        }
    }

    pub fn record_queue_delay(&self, delay: Duration) {
        self.0.queue_micros.store(
            delay.as_micros().min(u128::from(u64::MAX)) as u64,
            Ordering::Relaxed,
        );
    }
}

/// Holds the metrics of the currently active session so that `/metrics` never
/// mixes counters from a finished session with a new one.
#[derive(Clone, Default)]
pub struct MetricsHub(Arc<Mutex<Option<StreamMetrics>>>);

impl MetricsHub {
    fn install(&self, metrics: &StreamMetrics) {
        *self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner) = Some(metrics.clone());
    }

    fn clear(&self, metrics: &StreamMetrics) {
        let mut current = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner);
        if current
            .as_ref()
            .is_some_and(|value| Arc::ptr_eq(&value.0, &metrics.0))
        {
            *current = None;
        }
    }

    pub fn record_queue_delay(&self, delay: Duration) {
        if let Some(metrics) = self
            .0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
        {
            metrics.record_queue_delay(delay);
        }
    }

    fn snapshot(&self) -> MetricsSnapshot {
        self.0
            .lock()
            .unwrap_or_else(std::sync::PoisonError::into_inner)
            .as_ref()
            .map(StreamMetrics::snapshot)
            .unwrap_or_else(|| StreamMetrics::default().snapshot())
    }
}

impl Server {
    pub fn new(
        audio_tx: AudioFrameSender,
        audio_config: AudioConfig,
        ca_der: Vec<u8>,
        bind: IpAddr,
    ) -> Self {
        let token = format!(
            "{:016x}{:016x}",
            rand::random::<u64>(),
            rand::random::<u64>()
        );
        Self {
            audio_tx,
            sessions: SessionRegistry::default(),
            token: Arc::from(token),
            audio_config,
            ca_der: Arc::from(ca_der),
            metrics: MetricsHub::default(),
            ice_udp_addrs: Arc::from(ice_udp_addrs(bind)),
        }
    }

    pub fn sessions(&self) -> SessionRegistry {
        self.sessions.clone()
    }
    pub fn metrics(&self) -> MetricsHub {
        self.metrics.clone()
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(index_handler))
            .route("/ws", get(ws_handler))
            .route("/remotemic-ca.crt", get(ca_handler))
            .route("/metrics", get(metrics_handler))
            .with_state(self.clone())
    }
}

async fn index_handler(State(state): State<Server>) -> Response {
    (
        [(header::CACHE_CONTROL, "no-store")],
        Html(render_page(&state.token, state.audio_config)),
    )
        .into_response()
}

async fn ca_handler(State(state): State<Server>) -> Response {
    Response::builder()
        .header(header::CONTENT_TYPE, "application/x-x509-ca-cert")
        .header(
            header::CONTENT_DISPOSITION,
            "attachment; filename=remotemic-ca.crt",
        )
        .header(header::CACHE_CONTROL, "no-store")
        .body(Body::from(state.ca_der.to_vec()))
        .unwrap_or_else(|_| StatusCode::INTERNAL_SERVER_ERROR.into_response())
}

async fn metrics_handler(State(state): State<Server>) -> impl IntoResponse {
    axum::Json(state.metrics.snapshot())
}

fn render_page(token: &str, audio_config: AudioConfig) -> String {
    page::HTML
        .replace("__REMOTEMIC_TOKEN__", token)
        .replace(
            "__REMOTEMIC_QUALITY__",
            &format!(
                "{} · {} · mono · Opus {} kb/s",
                sample_rate_label(audio_config.sample_rate),
                audio_config.sample_format.display_name(),
                audio_config.opus_bitrate / 1_000
            ),
        )
        .replace(
            "__REMOTEMIC_OPUS_BITRATE__",
            &audio_config.opus_bitrate.to_string(),
        )
}

fn sample_rate_label(sample_rate: u32) -> String {
    match sample_rate % 1_000 {
        0 => format!("{} kHz", sample_rate / 1_000),
        _ => format!("{:.1} kHz", sample_rate as f64 / 1_000.0),
    }
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Server>, uri: Uri) -> Response {
    let authorized = uri
        .query()
        .and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("token="))
        })
        .is_some_and(|token| constant_time_eq(token.as_bytes(), state.token.as_bytes()));
    if !authorized {
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }
    ws.max_message_size(MAX_WS_MESSAGE_SIZE)
        .max_frame_size(MAX_WS_MESSAGE_SIZE)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |diff, (a, b)| diff | (a ^ b))
        == 0
}

#[derive(Clone)]
struct WebRtcHandler {
    session_id: u64,
    audio_tx: AudioFrameSender,
    audio_config: AudioConfig,
    sessions: SessionRegistry,
    metrics: StreamMetrics,
    metrics_hub: MetricsHub,
    gather_complete: Arc<Notify>,
    connection_state: watch::Sender<RTCPeerConnectionState>,
}

impl WebRtcHandler {
    fn release(&self) {
        self.sessions.release(self.session_id);
        self.metrics_hub.clear(&self.metrics);
    }

    fn fail_track(&self, message: &str) {
        warn!(session_id = self.session_id, "{message}");
        self.connection_state
            .send_replace(RTCPeerConnectionState::Failed);
        self.release();
    }
}

#[async_trait]
impl PeerConnectionEventHandler for WebRtcHandler {
    async fn on_ice_gathering_state_change(&self, state: RTCIceGatheringState) {
        debug!(session_id = self.session_id, %state, "ICE gathering state changed");
        if state == RTCIceGatheringState::Complete {
            self.gather_complete.notify_one();
        }
    }

    async fn on_connection_state_change(&self, state: RTCPeerConnectionState) {
        info!(session_id = self.session_id, %state, "WebRTC connection state changed");
        self.connection_state.send_replace(state);
        if matches!(
            state,
            RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed
        ) {
            self.release();
        }
    }

    async fn on_track(&self, track: Arc<dyn TrackRemote>) {
        let Some(ssrc) = track.ssrcs().await.first().copied() else {
            self.fail_track("Remote audio track has no SSRC");
            return;
        };
        let mut is_opus = false;
        for attempt in 0..CODEC_LOOKUP_ATTEMPTS {
            match track.codec(ssrc).await {
                Some(codec) => {
                    is_opus = codec.mime_type.eq_ignore_ascii_case(MIME_TYPE_OPUS);
                    break;
                }
                None => {
                    if attempt + 1 < CODEC_LOOKUP_ATTEMPTS {
                        tokio::time::sleep(CODEC_LOOKUP_INTERVAL).await;
                    }
                }
            }
        }
        if !is_opus {
            self.fail_track("Remote audio track did not negotiate Opus");
            return;
        }
        let handler = self.clone();
        tokio::spawn(async move {
            if let Err(error) = receive_opus_track(track, &handler).await {
                error!(%error, "WebRTC audio track failed");
            }
            if handler.sessions.is_active(handler.session_id) {
                handler.fail_track("Remote audio track ended");
            }
        });
    }
}

async fn handle_socket(socket: WebSocket, state: Server) {
    let (mut sender, mut receiver) = socket.split();
    let Some(session_id) = state.sessions.acquire().await else {
        let _ = sender
            .send(Message::Text(
                r#"{"type":"error","message":"another microphone is active"}"#.into(),
            ))
            .await;
        return;
    };
    info!(session_id, "Signaling client connected");
    let metrics = StreamMetrics::default();
    state.metrics.install(&metrics);
    let gather_complete = Arc::new(Notify::new());
    let (connection_state_tx, mut connection_state_rx) =
        watch::channel(RTCPeerConnectionState::New);
    let handler = Arc::new(WebRtcHandler {
        session_id,
        audio_tx: state.audio_tx.clone(),
        audio_config: state.audio_config,
        sessions: state.sessions.clone(),
        metrics,
        metrics_hub: state.metrics.clone(),
        gather_complete: Arc::clone(&gather_complete),
        connection_state: connection_state_tx,
    });
    let peer =
        match create_peer_connection(Arc::clone(&handler), state.ice_udp_addrs.to_vec()).await {
            Ok(peer) => peer,
            Err(error) => {
                error!(session_id, %error, "Could not create WebRTC peer");
                handler.release();
                return;
            }
        };

    let offer = tokio::time::timeout(SIGNALING_IDLE_TIMEOUT, receive_offer(&mut receiver)).await;
    let offer = match offer {
        Ok(Ok(offer)) => offer,
        Ok(Err(error)) => {
            send_error(&mut sender, &error).await;
            close_peer(&peer, &handler).await;
            return;
        }
        Err(_) => {
            send_error(&mut sender, "timed out waiting for WebRTC offer").await;
            close_peer(&peer, &handler).await;
            return;
        }
    };
    if let Err(error) = negotiate(&peer, offer, &gather_complete, &mut sender).await {
        send_error(&mut sender, &error).await;
        close_peer(&peer, &handler).await;
        return;
    }
    match tokio::time::timeout(
        CONNECTION_ESTABLISH_TIMEOUT,
        wait_for_connected(&mut connection_state_rx),
    )
    .await
    {
        Ok(Ok(())) => {}
        Ok(Err(error)) => {
            send_error(&mut sender, error).await;
            close_peer(&peer, &handler).await;
            return;
        }
        Err(_) => {
            send_error(&mut sender, "timed out establishing WebRTC connection").await;
            close_peer(&peer, &handler).await;
            return;
        }
    }

    loop {
        tokio::select! {
            message = receiver.next() => match message {
                Some(Ok(Message::Close(_))) | None => break,
                Some(Err(error)) => {
                    debug!(session_id, %error, "Signaling WebSocket ended");
                    break;
                }
                _ => {}
            },
            changed = connection_state_rx.changed() => {
                if changed.is_err() {
                    break;
                }
                let state = *connection_state_rx.borrow_and_update();
                match state {
                    RTCPeerConnectionState::Disconnected => {
                        match tokio::time::timeout(
                            DISCONNECTED_TIMEOUT,
                            wait_for_connected(&mut connection_state_rx),
                        )
                        .await
                        {
                            Ok(Ok(())) => {}
                            Ok(Err(error)) => {
                                send_error(&mut sender, error).await;
                                break;
                            }
                            Err(_) => {
                                send_error(&mut sender, "WebRTC connection remained disconnected").await;
                                break;
                            }
                        }
                    }
                    RTCPeerConnectionState::Failed | RTCPeerConnectionState::Closed => break,
                    _ => {}
                }
            }
        }
    }
    close_peer(&peer, &handler).await;
    info!(session_id, "WebRTC session ended");
}

async fn wait_for_connected(
    states: &mut watch::Receiver<RTCPeerConnectionState>,
) -> Result<(), &'static str> {
    loop {
        match *states.borrow_and_update() {
            RTCPeerConnectionState::Connected => return Ok(()),
            RTCPeerConnectionState::Failed => return Err("WebRTC connection failed"),
            RTCPeerConnectionState::Closed => return Err("WebRTC connection closed"),
            _ => {}
        }
        states
            .changed()
            .await
            .map_err(|_| "WebRTC connection state channel closed")?;
    }
}

async fn create_peer_connection(
    handler: Arc<WebRtcHandler>,
    udp_addrs: Vec<String>,
) -> Result<Arc<dyn PeerConnection>, String> {
    let mut media_engine = MediaEngine::default();
    media_engine
        .register_codec(
            RTCRtpCodecParameters {
                rtp_codec: RTCRtpCodec {
                    mime_type: MIME_TYPE_OPUS.to_owned(),
                    clock_rate: OPUS_CLOCK_RATE,
                    // WebRTC advertises Opus as opus/48000/2 even when fmtp forces mono.
                    channels: 2,
                    sdp_fmtp_line: "minptime=10;useinbandfec=1;stereo=0;sprop-stereo=0".to_owned(),
                    rtcp_feedback: Vec::new(),
                },
                payload_type: 111,
            },
            RtpCodecKind::Audio,
        )
        .map_err(|error| error.to_string())?;
    let registry = register_default_interceptors(Registry::new(), &mut media_engine)
        .map_err(|error| error.to_string())?;
    let peer = PeerConnectionBuilder::new()
        .with_configuration(RTCConfigurationBuilder::new().build())
        .with_media_engine(media_engine)
        .with_interceptor_registry(registry)
        .with_handler(handler as Arc<dyn PeerConnectionEventHandler>)
        .with_udp_addrs(udp_addrs)
        .build()
        .await
        .map_err(|error| error.to_string())?;
    peer.add_transceiver_from_kind(
        RtpCodecKind::Audio,
        Some(RTCRtpTransceiverInit {
            direction: RTCRtpTransceiverDirection::Recvonly,
            ..Default::default()
        }),
    )
    .await
    .map_err(|error| error.to_string())?;
    Ok(Arc::new(peer))
}

async fn receive_offer(
    receiver: &mut futures_util::stream::SplitStream<WebSocket>,
) -> Result<RTCSessionDescription, String> {
    while let Some(message) = receiver.next().await {
        match message.map_err(|error| error.to_string())? {
            Message::Text(text) => {
                let value: serde_json::Value =
                    serde_json::from_str(&text).map_err(|_| "invalid signaling JSON")?;
                if value.get("type").and_then(serde_json::Value::as_str) == Some("offer") {
                    return serde_json::from_value(value).map_err(|error| error.to_string());
                }
            }
            Message::Close(_) => return Err("signaling connection closed".to_string()),
            _ => {}
        }
    }
    Err("signaling connection closed".to_string())
}

async fn negotiate(
    peer: &Arc<dyn PeerConnection>,
    offer: RTCSessionDescription,
    gather_complete: &Notify,
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
) -> Result<(), String> {
    peer.set_remote_description(offer)
        .await
        .map_err(|error| error.to_string())?;
    let answer = peer
        .create_answer(None)
        .await
        .map_err(|error| error.to_string())?;
    peer.set_local_description(answer)
        .await
        .map_err(|error| error.to_string())?;
    tokio::time::timeout(Duration::from_secs(10), gather_complete.notified())
        .await
        .map_err(|_| "ICE gathering timed out".to_string())?;
    let answer = peer
        .local_description()
        .await
        .ok_or_else(|| "WebRTC answer was not created".to_string())?;
    let json = serde_json::to_string(&answer).map_err(|error| error.to_string())?;
    sender
        .send(Message::Text(json.into()))
        .await
        .map_err(|error| error.to_string())
}

async fn close_peer(peer: &Arc<dyn PeerConnection>, handler: &WebRtcHandler) {
    if let Err(error) = peer.close().await {
        debug!(session_id = handler.session_id, %error, "WebRTC close failed");
    }
    handler.release();
}

async fn send_error(
    sender: &mut futures_util::stream::SplitSink<WebSocket, Message>,
    message: &str,
) {
    let json = serde_json::json!({"type": "error", "message": message});
    let _ = sender.send(Message::Text(json.to_string().into())).await;
}

async fn receive_opus_track(
    track: Arc<dyn TrackRemote>,
    handler: &WebRtcHandler,
) -> Result<(), String> {
    let mut decoder = OpusDecoder::new(handler.audio_config.sample_rate as i32, 1)
        .map_err(|error| format!("Opus decoder: {error}"))?;
    let max_samples = handler.audio_config.sample_rate as usize * 120 / 1_000;
    let mut f32_buffer = vec![0_f32; max_samples];
    let mut sequence = None;
    let mut previous_toc = None;
    let mut previous_arrival = None;
    let mut previous_timestamp = None;
    let mut jitter_ticks = 0.0_f64;

    while let Some(event) = track.poll().await {
        let TrackRemoteEvent::OnRtpPacket(packet) = event else {
            if matches!(event, TrackRemoteEvent::OnEnded | TrackRemoteEvent::OnError) {
                break;
            }
            continue;
        };
        if !handler.sessions.is_active(handler.session_id) {
            break;
        }
        let now = Instant::now();
        if let Some(previous) = sequence {
            let Some(delta) = forward_sequence_delta(previous, packet.header.sequence_number)
            else {
                handler
                    .metrics
                    .0
                    .packets_reordered
                    .fetch_add(1, Ordering::Relaxed);
                continue;
            };
            let missing = delta.saturating_sub(1);
            handler
                .metrics
                .0
                .packets_lost
                .fetch_add(u64::from(missing), Ordering::Relaxed);
            if let Some(toc) = previous_toc {
                // Force packet code 0 so opus-rs treats the single byte as a lost
                // frame; other codes reject a one-byte packet before PLC runs.
                let conceal_toc = concealment_toc(toc);
                for _ in 0..missing.min(MAX_PLC_PACKETS) {
                    if let Err(error) =
                        decode_and_queue(&mut decoder, &[conceal_toc], &mut f32_buffer, handler)
                    {
                        debug!(%error, "Opus packet-loss concealment failed");
                    }
                }
            }
        }
        handler
            .metrics
            .0
            .packets_received
            .fetch_add(1, Ordering::Relaxed);
        update_jitter(
            &handler.metrics,
            packet.header.timestamp,
            now,
            &mut previous_arrival,
            &mut previous_timestamp,
            &mut jitter_ticks,
        );
        sequence = Some(packet.header.sequence_number);
        if !has_opus_payload(&packet.payload) {
            trace!(
                sequence_number = packet.header.sequence_number,
                "Ignoring RTP packet without Opus payload"
            );
            continue;
        }
        if let Err(error) =
            decode_and_queue(&mut decoder, &packet.payload, &mut f32_buffer, handler)
        {
            warn!(
                session_id = handler.session_id,
                sequence_number = packet.header.sequence_number,
                %error,
                "Dropping undecodable Opus packet"
            );
            continue;
        }
        previous_toc = packet.payload.first().copied();
    }
    Ok(())
}

fn has_opus_payload(payload: &[u8]) -> bool {
    !payload.is_empty()
}

fn concealment_toc(toc: u8) -> u8 {
    // Clear the two packet-code bits so opus-rs parses a single (lost) frame.
    toc & 0xFC
}

fn decode_and_queue(
    decoder: &mut OpusDecoder,
    payload: &[u8],
    f32_buffer: &mut [f32],
    handler: &WebRtcHandler,
) -> Result<(), String> {
    let samples = decoder
        .decode(payload, f32_buffer.len(), f32_buffer)
        .map_err(|error| format!("Opus decode: {error}"))?;
    let data = match handler.audio_config.sample_format {
        SampleFormat::S16Le => {
            let mut bytes = Vec::with_capacity(samples * 2);
            for sample in &f32_buffer[..samples] {
                let scaled = (sample.clamp(-1.0, 1.0) * f32::from(i16::MAX)).round() as i16;
                bytes.extend_from_slice(&scaled.to_le_bytes());
            }
            bytes
        }
        SampleFormat::Float32Le => {
            let mut bytes = Vec::with_capacity(samples * 4);
            for sample in &f32_buffer[..samples] {
                bytes.extend_from_slice(&sample.to_le_bytes());
            }
            bytes
        }
    };
    let frame = AudioFrame {
        session_id: handler.session_id,
        data,
        enqueued_at: Instant::now(),
    };
    handler
        .metrics
        .0
        .decoded_frames
        .fetch_add(1, Ordering::Relaxed);
    if handler.audio_tx.send_latest(frame) {
        handler
            .metrics
            .0
            .dropped_frames
            .fetch_add(1, Ordering::Relaxed);
    }
    Ok(())
}

fn update_jitter(
    metrics: &StreamMetrics,
    timestamp: u32,
    arrival: Instant,
    previous_arrival: &mut Option<Instant>,
    previous_timestamp: &mut Option<u32>,
    jitter_ticks: &mut f64,
) {
    if let (Some(last_arrival), Some(last_timestamp)) = (*previous_arrival, *previous_timestamp) {
        let arrival_delta =
            arrival.duration_since(last_arrival).as_secs_f64() * f64::from(OPUS_CLOCK_RATE);
        let rtp_delta = timestamp.wrapping_sub(last_timestamp) as f64;
        let difference = (arrival_delta - rtp_delta).abs();
        *jitter_ticks += (difference - *jitter_ticks) / 16.0;
        metrics.0.jitter_micros.store(
            (*jitter_ticks * 1_000_000.0 / f64::from(OPUS_CLOCK_RATE)) as u64,
            Ordering::Relaxed,
        );
    }
    *previous_arrival = Some(arrival);
    *previous_timestamp = Some(timestamp);
    trace!(timestamp, jitter_ticks, "Updated RTP jitter");
}

fn forward_sequence_delta(previous: u16, current: u16) -> Option<u16> {
    let delta = current.wrapping_sub(previous);
    (delta != 0 && delta <= 0x8000).then_some(delta)
}

fn ice_udp_addrs(bind: IpAddr) -> Vec<String> {
    match bind {
        IpAddr::V4(address) => vec![format!("{address}:0")],
        IpAddr::V6(address) if address.is_unspecified() => {
            vec!["0.0.0.0:0".to_owned(), "[::]:0".to_owned()]
        }
        IpAddr::V6(address) => vec![format!("[{address}]:0")],
    }
}

#[cfg(test)]
mod tests {
    use super::{
        AudioFrame, MetricsHub, RTCPeerConnectionState, SessionRegistry, StreamMetrics,
        WebRtcHandler, audio_frame_channel, concealment_toc, constant_time_eq,
        forward_sequence_delta, has_opus_payload, ice_udp_addrs, render_page, wait_for_connected,
    };
    use crate::audio::AudioConfig;
    use std::sync::Arc;
    use std::sync::atomic::Ordering;
    use std::time::Instant;

    #[test]
    fn constant_time_eq_matches_only_identical_tokens() {
        assert!(constant_time_eq(b"deadbeef", b"deadbeef"));
        assert!(!constant_time_eq(b"deadbeef", b"deadbeee"));
        assert!(!constant_time_eq(b"short", b"longer-token"));
    }

    #[test]
    fn concealment_toc_forces_single_frame_code() {
        // Config/stereo bits are preserved while the packet code becomes 0.
        assert_eq!(concealment_toc(0b0000_0011), 0b0000_0000);
        assert_eq!(concealment_toc(0b1101_0111), 0b1101_0100);
    }

    #[test]
    fn metrics_hub_snapshot_tracks_installed_session() {
        let hub = MetricsHub::default();
        let metrics = StreamMetrics::default();
        hub.install(&metrics);
        metrics.0.packets_received.store(7, Ordering::Relaxed);
        assert_eq!(hub.snapshot().packets_received, 7);

        hub.clear(&metrics);
        assert_eq!(hub.snapshot().packets_received, 0);
    }

    #[tokio::test]
    async fn stale_release_cannot_clear_a_new_session() {
        let sessions = SessionRegistry::default();
        let first = sessions.acquire().await.unwrap();
        sessions.release(first);
        let second = sessions.acquire().await.unwrap();
        sessions.release(first);
        assert!(sessions.is_active(second));
    }

    #[tokio::test]
    async fn failed_peer_does_not_satisfy_connected_waiter() {
        let (sender, mut receiver) = tokio::sync::watch::channel(RTCPeerConnectionState::New);
        sender.send_replace(RTCPeerConnectionState::Failed);

        assert_eq!(
            wait_for_connected(&mut receiver).await,
            Err("WebRTC connection failed")
        );
    }

    #[tokio::test]
    async fn failed_audio_track_releases_session_and_notifies_connection_waiter() {
        let sessions = SessionRegistry::default();
        let session_id = sessions.acquire().await.unwrap();
        let metrics = StreamMetrics::default();
        let metrics_hub = MetricsHub::default();
        metrics_hub.install(&metrics);
        let (connection_state, receiver) =
            tokio::sync::watch::channel(RTCPeerConnectionState::Connected);
        let (audio_tx, _audio_rx) = audio_frame_channel(1);
        let handler = WebRtcHandler {
            session_id,
            audio_tx,
            audio_config: AudioConfig::STANDARD,
            sessions: sessions.clone(),
            metrics,
            metrics_hub,
            gather_complete: Arc::new(tokio::sync::Notify::new()),
            connection_state,
        };

        handler.fail_track("test track ended");

        assert_eq!(
            (sessions.is_active(session_id), *receiver.borrow()),
            (false, RTCPeerConnectionState::Failed)
        );
    }

    #[tokio::test]
    async fn disconnected_peer_can_recover_before_timeout() {
        let (sender, mut receiver) =
            tokio::sync::watch::channel(RTCPeerConnectionState::Disconnected);
        sender.send_replace(RTCPeerConnectionState::Connected);

        assert_eq!(wait_for_connected(&mut receiver).await, Ok(()));
    }

    #[test]
    fn metrics_reports_packet_loss_percentage() {
        let metrics = StreamMetrics::default();
        metrics.0.packets_received.store(90, Ordering::Relaxed);
        metrics.0.packets_lost.store(10, Ordering::Relaxed);
        assert_eq!(metrics.snapshot().loss_percent, 10.0);
    }

    #[test]
    fn high_quality_page_uses_configured_opus_bitrate() {
        let html = render_page("test-token", AudioConfig::HIGH);
        assert!(html.contains("const OPUS_BITRATE = 192000;"));
    }

    #[test]
    fn page_requests_ten_millisecond_opus_packets() {
        let html = render_page("test-token", AudioConfig::HIGH);
        assert!(html.contains("a=ptime:10"));
    }

    #[test]
    fn page_includes_ca_certificate_download_button() {
        let html = render_page("test-token", AudioConfig::STANDARD);
        assert!(html.contains(
            r#"id="cert-download"
        class="cert-download"
        href="/remotemic-ca.crt"
        download="remotemic-ca.crt""#
        ));
    }

    #[test]
    fn page_registers_capture_for_cleanup_before_audio_setup() {
        let html = render_page("test-token", AudioConfig::STANDARD);
        let registration = html.find("session = activeSession;").unwrap();
        let audio_setup = html.find("await audioContext.resume();").unwrap();

        assert!(registration < audio_setup);
        assert!(
            html.contains("activeSession.stream?.getTracks().forEach((track) => track.stop())")
        );
    }

    #[test]
    fn page_releases_wake_lock_obtained_after_disconnect() {
        let html = render_page("test-token", AudioConfig::STANDARD);

        assert!(html.contains("!isCurrentSession(activeSession) ||"));
        assert!(html.contains("await lock.release().catch(() => {});"));
        assert!(html.contains("if (!isCurrentSession(activeSession)) return;"));
    }

    #[test]
    fn page_requests_mono_without_requiring_device_support() {
        let html = render_page("test-token", AudioConfig::STANDARD);

        assert!(html.contains("channelCount: { ideal: 1 }"));
    }

    #[test]
    fn page_stops_session_when_microphone_track_ends() {
        let html = render_page("test-token", AudioConfig::STANDARD);

        assert!(html.contains("stop(\"Microphone access ended\", true)"));
    }

    #[test]
    fn page_times_out_or_rejects_socket_closed_before_opening() {
        let html = render_page("test-token", AudioConfig::STANDARD);

        assert!(html.contains("Signaling connection timed out"));
        assert!(html.contains("Signaling connection closed before opening"));
    }

    #[tokio::test]
    async fn full_audio_queue_replaces_oldest_frame() {
        let (sender, receiver) = audio_frame_channel(1);
        sender.send_latest(AudioFrame {
            session_id: 1,
            data: vec![1],
            enqueued_at: Instant::now(),
        });
        let replaced = sender.send_latest(AudioFrame {
            session_id: 1,
            data: vec![2],
            enqueued_at: Instant::now(),
        });

        assert_eq!((replaced, receiver.recv().await.data), (true, vec![2]));
    }

    #[test]
    fn empty_rtp_payload_is_not_passed_to_opus_decoder() {
        assert!(!has_opus_payload(&[]));
        assert!(has_opus_payload(&[0xf8, 0xff, 0xfe]));
    }

    #[test]
    fn reordered_and_duplicate_sequences_are_rejected() {
        assert_eq!(forward_sequence_delta(100, 100), None);
        assert_eq!(forward_sequence_delta(100, 99), None);
        assert_eq!(forward_sequence_delta(u16::MAX, 0), Some(1));
    }

    #[test]
    fn ipv6_listener_enables_ipv4_and_ipv6_ice_sockets() {
        assert_eq!(
            ice_udp_addrs("::".parse().unwrap()),
            ["0.0.0.0:0", "[::]:0"]
        );
    }

    #[test]
    fn ipv4_listener_uses_only_ipv4_ice_socket() {
        assert_eq!(ice_udp_addrs("0.0.0.0".parse().unwrap()), ["0.0.0.0:0"]);
    }

    #[test]
    fn concrete_listener_limits_ice_to_the_same_interface() {
        assert_eq!(ice_udp_addrs("127.0.0.1".parse().unwrap()), ["127.0.0.1:0"]);
    }

    #[test]
    fn concrete_ipv6_listener_limits_ice_to_the_same_interface() {
        assert_eq!(ice_udp_addrs("::1".parse().unwrap()), ["[::1]:0"]);
    }
}
