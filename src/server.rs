use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    http::{StatusCode, Uri},
    response::{Html, IntoResponse, Response},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::{
    Arc,
    atomic::{AtomicU64, Ordering},
};
use std::time::Duration;
use tokio::sync::mpsc;
use tracing::{error, info, warn};

use crate::page;

const MAX_WS_MESSAGE_SIZE: usize = 64 * 1024;
const ACQUIRE_RETRY_TIMEOUT: Duration = Duration::from_millis(500);
const ACQUIRE_RETRY_INTERVAL: Duration = Duration::from_millis(10);

#[derive(Clone)]
pub struct Server {
    audio_tx: mpsc::Sender<AudioFrame>,
    sessions: SessionRegistry,
    token: Arc<str>,
}

pub struct AudioFrame {
    pub session_id: u64,
    pub data: Vec<u8>,
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
        let _ = self
            .active
            .compare_exchange(session_id, 0, Ordering::AcqRel, Ordering::Acquire);
    }

    pub fn is_active(&self, session_id: u64) -> bool {
        self.active.load(Ordering::Acquire) == session_id
    }
}

impl Server {
    pub fn new(audio_tx: mpsc::Sender<AudioFrame>) -> Self {
        let token = format!(
            "{:016x}{:016x}",
            rand::random::<u64>(),
            rand::random::<u64>()
        );

        Self {
            audio_tx,
            sessions: SessionRegistry::default(),
            token: Arc::from(token),
        }
    }

    pub fn sessions(&self) -> SessionRegistry {
        self.sessions.clone()
    }

    pub fn router(&self) -> Router {
        Router::new()
            .route("/", get(index_handler))
            .route("/ws", get(ws_handler))
            .with_state(self.clone())
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn index_handler(State(state): State<Server>) -> Html<String> {
    Html(page::HTML.replace("__REMOTEMIC_TOKEN__", &state.token))
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Server>, uri: Uri) -> Response {
    let authorized = uri
        .query()
        .and_then(|query| {
            query
                .split('&')
                .find_map(|pair| pair.strip_prefix("token="))
        })
        .is_some_and(|token| token == state.token.as_ref());

    if !authorized {
        warn!("Rejected WebSocket upgrade with missing or invalid token");
        return (StatusCode::UNAUTHORIZED, "invalid token").into_response();
    }

    ws.max_message_size(MAX_WS_MESSAGE_SIZE)
        .max_frame_size(MAX_WS_MESSAGE_SIZE)
        .on_upgrade(move |socket| handle_socket(socket, state))
}

// ---------------------------------------------------------------------------
// WebSocket session
// ---------------------------------------------------------------------------

async fn handle_socket(socket: WebSocket, state: Server) {
    let (mut sender, mut receiver) = socket.split();

    let Some(session_id) = state.sessions.acquire().await else {
        warn!("Rejecting new connection — another client is already streaming");
        let _ = sender
            .send(Message::Text(
                "error: another client is already connected".into(),
            ))
            .await;
        return;
    };

    info!("Client connected (session {session_id})");
    let _ = sender.send(Message::Text("ok: connected".into())).await;

    drop(sender);

    let audio_tx = state.audio_tx.clone();
    let mut dropped_frames = 0_u64;

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                let frame = AudioFrame {
                    session_id,
                    data: data.to_vec(),
                };
                match audio_tx.try_send(frame) {
                    Ok(()) => {}
                    Err(mpsc::error::TrySendError::Full(_)) => {
                        dropped_frames += 1;
                        if dropped_frames == 1 || dropped_frames.is_multiple_of(100) {
                            warn!(
                                "Audio queue full for session {session_id}; dropping live frame ({dropped_frames} total)"
                            );
                        }
                    }
                    Err(mpsc::error::TrySendError::Closed(_)) => {
                        error!("Audio channel closed unexpectedly");
                        break;
                    }
                }
            }
            Ok(Message::Close(_)) => {
                info!("Client sent close frame");
                break;
            }
            Ok(_) => {}
            Err(e) => {
                error!("WebSocket receive error: {e}");
                break;
            }
        }
    }

    state.sessions.release(session_id);
    info!("Client disconnected (session {session_id}, dropped {dropped_frames} frames)");
}

#[cfg(test)]
mod tests {
    use super::SessionRegistry;

    #[tokio::test]
    async fn stale_release_cannot_clear_a_new_session() {
        let sessions = SessionRegistry::default();
        let first = sessions.acquire().await.unwrap();
        sessions.release(first);

        let second = sessions.acquire().await.unwrap();
        sessions.release(first);

        assert!(sessions.is_active(second));
        assert!(sessions.acquire().await.is_none());
    }
}
