use axum::{
    Router,
    extract::{
        State,
        ws::{Message, WebSocket, WebSocketUpgrade},
    },
    response::{Html, IntoResponse},
    routing::get,
};
use futures_util::{SinkExt, StreamExt};
use std::sync::Arc;
use tokio::sync::{Mutex, mpsc};
use tower_http::cors::{Any, CorsLayer};
use tracing::{error, info, warn};

use crate::page;

#[derive(Clone)]
pub struct Server {
    audio_tx: mpsc::Sender<Vec<u8>>,
    occupied: Arc<Mutex<bool>>,
}

impl Server {
    pub fn new(audio_tx: mpsc::Sender<Vec<u8>>) -> Self {
        Self {
            audio_tx,
            occupied: Arc::new(Mutex::new(false)),
        }
    }

    pub fn router(&self) -> Router {
        let cors = CorsLayer::new()
            .allow_origin(Any)
            .allow_methods(Any)
            .allow_headers(Any);

        Router::new()
            .route("/", get(index_handler))
            .route("/ws", get(ws_handler))
            .layer(cors)
            .with_state(self.clone())
    }
}

// ---------------------------------------------------------------------------
// HTTP handlers
// ---------------------------------------------------------------------------

async fn index_handler() -> Html<&'static str> {
    Html(page::HTML)
}

async fn ws_handler(ws: WebSocketUpgrade, State(state): State<Server>) -> impl IntoResponse {
    ws.on_upgrade(move |socket| handle_socket(socket, state))
}

// ---------------------------------------------------------------------------
// WebSocket session
// ---------------------------------------------------------------------------

async fn handle_socket(socket: WebSocket, state: Server) {
    let (mut sender, mut receiver) = socket.split();

    {
        let mut occupied = state.occupied.lock().await;
        if *occupied {
            warn!("Rejecting new connection — another client is already streaming");
            let _ = sender
                .send(Message::Text(
                    "error: another client is already connected".into(),
                ))
                .await;
            return;
        }
        *occupied = true;
    }

    info!("Client connected");
    let _ = sender.send(Message::Text("ok: connected".into())).await;

    drop(sender);

    let audio_tx = state.audio_tx.clone();

    while let Some(msg) = receiver.next().await {
        match msg {
            Ok(Message::Binary(data)) => {
                if audio_tx.send(data.to_vec()).await.is_err() {
                    error!("Audio channel closed unexpectedly");
                    break;
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

    *state.occupied.lock().await = false;
    info!("Client disconnected");
}
