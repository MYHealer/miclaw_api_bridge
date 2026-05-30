use crate::auth::login::LoginRequest;
use crate::error::{BridgeError, Result};
use crate::service::{SendTicketRequest, SetPortRequest, VerifyTicketRequest};
use crate::state::BridgeState;
use axum::extract::State;
use axum::http::{header, HeaderMap, HeaderValue, StatusCode, Uri};
use axum::response::sse::{Event, KeepAlive, Sse};
use axum::response::{IntoResponse, Response};
use axum::routing::{get, post};
use axum::{Json, Router};
use futures_util::stream::{self, Stream};
use rust_embed::RustEmbed;
use serde::Serialize;
use serde_json::json;
use std::borrow::Cow;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use tokio::sync::oneshot;
use tower_http::cors::CorsLayer;

#[derive(Debug, Clone)]
pub struct ServerConfig {
    pub host: IpAddr,
    pub port: u16,
}

impl Default for ServerConfig {
    fn default() -> Self {
        Self {
            host: IpAddr::V4(Ipv4Addr::LOCALHOST),
            port: 8765,
        }
    }
}

pub struct HttpServer {
    pub addr: SocketAddr,
    shutdown: Option<oneshot::Sender<()>>,
    state: Arc<BridgeState>,
}

impl HttpServer {
    pub fn webui_url(&self) -> String {
        format!("http://{}", self.addr)
    }

    pub fn shutdown(mut self) {
        if let Some(tx) = self.shutdown.take() {
            let _ = tx.send(());
        }
        self.state.clear_bound_addr();
    }
}

pub async fn start_http(state: Arc<BridgeState>, config: ServerConfig) -> Result<HttpServer> {
    let addr = SocketAddr::new(config.host, config.port);
    let app = router(state.clone());
    let listener = tokio::net::TcpListener::bind(addr)
        .await
        .map_err(|e| BridgeError::Proxy(format!("bind {addr}: {e}")))?;
    let bound = listener.local_addr().unwrap_or(addr);
    state.set_bound_addr(bound);

    let (tx, rx) = oneshot::channel::<()>();
    tokio::spawn(async move {
        if let Err(e) = axum::serve(listener, app)
            .with_graceful_shutdown(async move {
                let _ = rx.await;
            })
            .await
        {
            tracing::error!(target = "server", "http server failed: {e}");
        }
    });

    Ok(HttpServer {
        addr: bound,
        shutdown: Some(tx),
        state,
    })
}

pub fn router(state: Arc<BridgeState>) -> Router {
    let api = Router::new()
        .route("/api/auth/status", get(api_auth_status))
        .route("/api/auth/login", post(api_login))
        .route("/api/auth/two-factor/send", post(api_send_ticket))
        .route("/api/auth/two-factor/verify", post(api_verify_ticket))
        .route("/api/auth/refresh", post(api_refresh_session))
        .route("/api/auth/logout", post(api_logout))
        .route("/api/proxy/status", get(api_proxy_status))
        .route("/api/settings/port", post(api_set_port))
        .route("/api/models", get(api_models))
        .route("/api/logs", get(api_logs))
        .route("/api/logs/stream", get(api_logs_stream))
        .with_state(state.clone());

    let proxy = Router::new()
        .route("/v1/models", get(crate::proxy::openai::models))
        .route("/v1/chat/completions", post(crate::proxy::openai::chat))
        .route("/v1/responses", post(crate::proxy::openai::responses))
        .route("/v1/messages", post(crate::proxy::anthropic::messages))
        .with_state(state.proxy.clone());

    Router::new()
        .merge(api)
        .merge(proxy)
        .fallback(static_asset)
        .layer(CorsLayer::permissive())
}

async fn api_auth_status(State(state): State<Arc<BridgeState>>) -> Response {
    json_result(crate::service::auth_status(&state).await)
}

async fn api_login(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<LoginRequest>,
) -> Response {
    json_result(crate::service::login(&state, req).await)
}

async fn api_send_ticket(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<SendTicketRequest>,
) -> Response {
    json_result(crate::service::send_two_factor_ticket(&state, req.flag).await)
}

async fn api_verify_ticket(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<VerifyTicketRequest>,
) -> Response {
    json_result(crate::service::verify_two_factor(&state, req.flag, req.ticket).await)
}

async fn api_refresh_session(State(state): State<Arc<BridgeState>>) -> Response {
    json_result(crate::service::refresh_session(&state).await)
}

async fn api_logout(State(state): State<Arc<BridgeState>>) -> Response {
    json_result(crate::service::logout(&state).await)
}

async fn api_proxy_status(State(state): State<Arc<BridgeState>>) -> Response {
    Json(crate::service::proxy_status(&state)).into_response()
}

async fn api_set_port(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<SetPortRequest>,
) -> Response {
    json_result(crate::service::set_proxy_port(&state, req.port).await)
}

async fn api_models(State(_state): State<Arc<BridgeState>>) -> Response {
    Json(crate::service::list_models()).into_response()
}

async fn api_logs(State(state): State<Arc<BridgeState>>) -> Response {
    Json(state.logs.snapshot()).into_response()
}

async fn api_logs_stream(
    State(state): State<Arc<BridgeState>>,
) -> Sse<impl Stream<Item = std::result::Result<Event, Infallible>>> {
    let rx = state.logs.subscribe();
    let stream = stream::unfold(rx, |mut rx| async {
        loop {
            match rx.recv().await {
                Ok(payload) => {
                    let event = Event::default().json_data(payload).unwrap_or_else(|_| {
                        Event::default().data("{\"kind\":\"error\",\"message\":\"encode log\"}")
                    });
                    return Some((Ok(event), rx));
                }
                Err(tokio::sync::broadcast::error::RecvError::Lagged(_)) => continue,
                Err(tokio::sync::broadcast::error::RecvError::Closed) => return None,
            }
        }
    });
    Sse::new(stream).keep_alive(KeepAlive::default())
}

fn json_result<T: Serialize>(result: Result<T>) -> Response {
    match result {
        Ok(value) => Json(value).into_response(),
        Err(e) => error_response(e),
    }
}

fn error_response(e: BridgeError) -> Response {
    let status = match &e {
        BridgeError::NotAuthenticated => StatusCode::UNAUTHORIZED,
        BridgeError::Login(_) | BridgeError::VerificationCodeError => StatusCode::UNAUTHORIZED,
        BridgeError::Proxy(_) | BridgeError::Storage(_) => StatusCode::BAD_REQUEST,
        _ => StatusCode::BAD_GATEWAY,
    };
    (
        status,
        Json(json!({
            "error": {
                "message": e.to_string(),
            }
        })),
    )
        .into_response()
}

#[derive(RustEmbed)]
#[folder = "../dist"]
struct Assets;

async fn static_asset(uri: Uri) -> Response {
    let path = uri.path().trim_start_matches('/');
    if path.starts_with("api/") || path.starts_with("v1/") {
        return (
            StatusCode::NOT_FOUND,
            Json(json!({"error": {"message": "not found"}})),
        )
            .into_response();
    }

    let asset_path = if path.is_empty() { "index.html" } else { path };
    match Assets::get(asset_path) {
        Some(asset) => asset_response(asset_path, asset.data),
        None => match Assets::get("index.html") {
            Some(asset) => asset_response("index.html", asset.data),
            None => (
                StatusCode::INTERNAL_SERVER_ERROR,
                "webui assets are not embedded; run pnpm build before cargo build",
            )
                .into_response(),
        },
    }
}

fn asset_response(path: &str, data: Cow<'static, [u8]>) -> Response {
    let mime = mime_guess::from_path(path).first_or_octet_stream();
    let mut headers = HeaderMap::new();
    headers.insert(
        header::CONTENT_TYPE,
        HeaderValue::from_str(mime.as_ref())
            .unwrap_or_else(|_| HeaderValue::from_static("application/octet-stream")),
    );
    (headers, data.into_owned()).into_response()
}
