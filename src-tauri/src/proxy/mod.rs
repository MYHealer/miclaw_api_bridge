//! Local HTTP proxy exposing OpenAI Chat Completions and Anthropic Messages
//! compatible endpoints, all routed to mimo PC.

mod anthropic;
mod openai;
mod transport;

pub use transport::emit_log;

use crate::error::{BridgeError, Result};
use crate::mimo::MimoClient;
use crate::state::LogEmitter;
use crate::storage::Storage;
use axum::{routing::get, routing::post, Router};
use parking_lot::Mutex;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

pub struct ProxyController {
    pub mimo: Arc<MimoClient>,
    pub emitter: LogEmitter,
    storage: Arc<Storage>,
    state: Mutex<RuntimeState>,
}

#[derive(Default)]
struct RuntimeState {
    running: bool,
    addr: Option<SocketAddr>,
    shutdown: Option<oneshot::Sender<()>>,
}

impl ProxyController {
    pub fn new(mimo: Arc<MimoClient>, storage: Arc<Storage>, emitter: LogEmitter) -> Self {
        Self {
            mimo,
            emitter,
            storage,
            state: Mutex::new(RuntimeState::default()),
        }
    }

    pub fn snapshot(&self) -> ProxySnapshot {
        let s = self.state.lock();
        ProxySnapshot {
            running: s.running,
            addr: s.addr.map(|a| a.to_string()),
            port: self.storage.settings().proxy_port,
        }
    }

    pub async fn start(self: &Arc<Self>) -> Result<ProxySnapshot> {
        {
            let s = self.state.lock();
            if s.running {
                drop(s);
                return Ok(self.snapshot());
            }
        }
        let port = self.storage.settings().proxy_port;
        let addr = SocketAddr::new(IpAddr::V4(Ipv4Addr::LOCALHOST), port);

        let app = Router::new()
            .route("/", get(openai::root))
            .route("/v1/models", get(openai::models))
            .route("/v1/chat/completions", post(openai::chat))
            .route("/v1/messages", post(anthropic::messages))
            .with_state(self.clone())
            .layer(CorsLayer::permissive());

        let listener = tokio::net::TcpListener::bind(addr)
            .await
            .map_err(|e| BridgeError::Proxy(format!("bind {addr}: {e}")))?;
        let bound = listener.local_addr().unwrap_or(addr);
        let (tx, rx) = oneshot::channel::<()>();
        tokio::spawn(async move {
            let _ = axum::serve(listener, app)
                .with_graceful_shutdown(async move {
                    let _ = rx.await;
                })
                .await;
        });

        let mut guard = self.state.lock();
        guard.running = true;
        guard.addr = Some(bound);
        guard.shutdown = Some(tx);
        Ok(ProxySnapshot {
            running: true,
            addr: Some(bound.to_string()),
            port,
        })
    }

    pub fn stop(&self) -> ProxySnapshot {
        let mut s = self.state.lock();
        if let Some(tx) = s.shutdown.take() {
            let _ = tx.send(());
        }
        s.running = false;
        s.addr = None;
        ProxySnapshot {
            running: false,
            addr: None,
            port: self.storage.settings().proxy_port,
        }
    }

    pub async fn restart(self: &Arc<Self>) -> Result<ProxySnapshot> {
        self.stop();
        self.start().await
    }
}

#[derive(Debug, Clone, serde::Serialize)]
pub struct ProxySnapshot {
    pub running: bool,
    pub addr: Option<String>,
    pub port: u16,
}
