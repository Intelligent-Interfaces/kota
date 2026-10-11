use axum::{
    extract::{
        ws::{Message, WebSocket, WebSocketUpgrade},
        Query, State,
    },
    http::StatusCode,
    response::{Html, IntoResponse, Response},
    routing::get,
    Router,
};
use futures_util::{sink::SinkExt, stream::StreamExt};
use serde::Deserialize;
use std::sync::Arc;
use tokio::sync::{broadcast, mpsc};

use crate::events::AgentEvent;

struct AppState {
    input_tx: mpsc::UnboundedSender<(String, String)>,
    event_tx: broadcast::Sender<AgentEvent>,
    password: Option<String>,
}

#[derive(Deserialize)]
struct WsQuery {
    token: Option<String>,
}

pub async fn start(
    host: &str,
    port: u16,
    password: Option<String>,
    input_tx: mpsc::UnboundedSender<(String, String)>,
    event_tx: broadcast::Sender<AgentEvent>,
) {
    let state = Arc::new(AppState {
        input_tx,
        event_tx,
        password,
    });

    let app = Router::new()
        .route("/", get(index_handler))
        .route("/ws", get(ws_handler))
        .with_state(state);

    let bind_addr = format!("{}:{}", host, port);
    match tokio::net::TcpListener::bind(&bind_addr).await {
        Ok(listener) => {
            let _ = axum::serve(listener, app).await;
        }
        Err(e) => {
            eprintln!("Failed to bind web controller to {}: {}", bind_addr, e);
        }
    }
}

async fn index_handler() -> Html<&'static str> {
    Html(include_str!("index.html"))
}

async fn ws_handler(
    ws: WebSocketUpgrade,
    Query(query): Query<WsQuery>,
    State(state): State<Arc<AppState>>,
) -> Response {
    if let Some(ref required_pw) = state.password {
        if query.token.as_deref() != Some(required_pw.as_str()) {
            return (
                StatusCode::UNAUTHORIZED,
                "Unauthorized: Invalid or missing token/password.",
            )
                .into_response();
        }
    }
    ws.on_upgrade(|socket| handle_socket(socket, state))
        .into_response()
}

async fn handle_socket(socket: WebSocket, state: Arc<AppState>) {
    let (mut sender, mut receiver) = socket.split();
    let mut rx = state.event_tx.subscribe();

    // Task to receive events from agent and send to websocket
    let mut send_task = tokio::spawn(async move {
        while let Ok(event) = rx.recv().await {
            if let Ok(json) = serde_json::to_string(&event) {
                if sender.send(Message::Text(json)).await.is_err() {
                    break;
                }
            }
        }
    });

    // Task to receive text from websocket and send to agent
    let input_tx = state.input_tx.clone();
    let mut recv_task = tokio::spawn(async move {
        while let Some(Ok(msg)) = receiver.next().await {
            if let Message::Text(text) = msg {
                let _ = input_tx.send((text, "remote".to_string()));
            }
        }
    });

    tokio::select! {
        _ = (&mut send_task) => recv_task.abort(),
        _ = (&mut recv_task) => send_task.abort(),
    };
}
