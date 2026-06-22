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
use axum_server::tls_rustls::RustlsConfig;
use axum_server::Handle;
use futures_util::stream::{self, Stream};
use rust_embed::RustEmbed;
use serde::Serialize;
use serde_json::json;
use std::borrow::Cow;
use std::convert::Infallible;
use std::net::{IpAddr, Ipv4Addr, SocketAddr};
use std::sync::Arc;
use std::time::Duration;
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
    pub tls: bool,
    handle: Option<Handle>,
    state: Arc<BridgeState>,
}

impl HttpServer {
    pub fn webui_url(&self) -> String {
        let scheme = if self.tls { "https" } else { "http" };
        format!("{scheme}://{}", self.addr)
    }

    pub fn shutdown(mut self) {
        if let Some(handle) = self.handle.take() {
            handle.graceful_shutdown(Some(Duration::from_secs(3)));
        }
        self.state.clear_bound_addr();
    }
}

pub async fn start_http(state: Arc<BridgeState>, config: ServerConfig) -> Result<HttpServer> {
    // Install the ring CryptoProvider once. Required because axum-server is
    // built with `tls-rustls-no-provider`; harmless if already installed.
    let _ = rustls::crypto::ring::default_provider().install_default();

    let addr = SocketAddr::new(config.host, config.port);
    let app = router(state.clone());
    let handle = Handle::new();
    let tls_enabled = state.storage.settings().tls_enabled;

    // Bind synchronously so we can surface "port in use" before returning.
    let std_listener = std::net::TcpListener::bind(addr)
        .map_err(|e| BridgeError::Proxy(format!("bind {addr}: {e}")))?;
    std_listener
        .set_nonblocking(true)
        .map_err(|e| BridgeError::Proxy(format!("listener: {e}")))?;
    let bound = std_listener.local_addr().unwrap_or(addr);
    state.set_bound_addr(bound);

    if tls_enabled {
        let tls_config = load_or_make_tls(&state).await?;
        let handle2 = handle.clone();
        let state2 = state.clone();
        let make_service = app.into_make_service();
        tokio::spawn(async move {
            if let Err(e) = axum_server::from_tcp_rustls(std_listener, tls_config)
                .handle(handle2)
                .serve(make_service)
                .await
            {
                tracing::error!(target = "server", "https server failed: {e}");
            }
            state2.clear_bound_addr();
        });
    } else {
        let handle2 = handle.clone();
        let state2 = state.clone();
        let make_service = app.into_make_service();
        tokio::spawn(async move {
            if let Err(e) = axum_server::from_tcp(std_listener)
                .handle(handle2)
                .serve(make_service)
                .await
            {
                tracing::error!(target = "server", "http server failed: {e}");
            }
            state2.clear_bound_addr();
        });
    }

    Ok(HttpServer {
        addr: bound,
        tls: tls_enabled,
        handle: Some(handle),
        state,
    })
}

/// Resolve the TLS config: use the configured cert/key pair if present,
/// otherwise generate (and cache) a self-signed cert in the config dir.
async fn load_or_make_tls(state: &Arc<BridgeState>) -> Result<RustlsConfig> {
    let settings = state.storage.settings();
    if let (Some(cert), Some(key)) = (settings.tls_cert_path.clone(), settings.tls_key_path.clone())
    {
        return RustlsConfig::from_pem_file(&cert, &key)
            .await
            .map_err(|e| BridgeError::Proxy(format!("load tls cert/key: {e}")));
    }

    let dir = state.storage.config_dir();
    let cert_path = dir.join("tls-cert.pem");
    let key_path = dir.join("tls-key.pem");
    if !cert_path.exists() || !key_path.exists() {
        let sans = vec![
            "localhost".to_string(),
            "127.0.0.1".to_string(),
            "::1".to_string(),
        ];
        let generated =
            rcgen::generate_simple_self_signed(sans).map_err(|e| BridgeError::Proxy(e.to_string()))?;
        std::fs::write(&cert_path, generated.cert.pem())?;
        std::fs::write(&key_path, generated.key_pair.serialize_pem())?;
        tracing::info!(
            target = "server",
            "generated self-signed TLS cert at {}",
            cert_path.display()
        );
    }
    RustlsConfig::from_pem_file(&cert_path, &key_path)
        .await
        .map_err(|e| BridgeError::Proxy(format!("load self-signed cert: {e}")))
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
        .route(
            "/api/logs/verbose",
            get(api_logs_verbose_get).post(api_logs_verbose_set),
        )
        .route("/api/logs/stream", get(api_logs_stream))
        // admin auth (control-plane password)
        .route("/api/admin/session", get(api_admin_session))
        .route("/api/admin/setup", post(api_admin_setup))
        .route("/api/admin/login", post(api_admin_login))
        .route("/api/admin/logout", post(api_admin_logout))
        .route("/api/admin/password", post(api_admin_password))
        // everything under /api requires a valid admin session once configured
        // (the guard whitelists session/setup/login itself)
        .route_layer(axum::middleware::from_fn_with_state(
            state.clone(),
            admin_guard,
        ))
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

/// Endpoints reachable without an admin session (so the login UI can work).
fn admin_open_path(path: &str) -> bool {
    matches!(
        path,
        "/api/admin/session" | "/api/admin/setup" | "/api/admin/login"
    )
}

fn session_cookie(headers: &HeaderMap) -> Option<String> {
    let raw = headers.get(header::COOKIE)?.to_str().ok()?;
    raw.split(';').find_map(|kv| {
        let kv = kv.trim();
        let (k, v) = kv.split_once('=')?;
        if k == "mb_session" {
            Some(v.to_string())
        } else {
            None
        }
    })
}

/// Guard `/api/*`: once an admin password is configured, every endpoint except
/// the auth ones requires a valid `mb_session` cookie.
async fn admin_guard(
    State(state): State<Arc<BridgeState>>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> Response {
    let path = req.uri().path();
    if admin_open_path(path) || !state.security.is_configured() {
        return next.run(req).await;
    }
    if let Some(token) = session_cookie(req.headers()) {
        if state.security.validate_session(&token) {
            return next.run(req).await;
        }
    }
    (
        StatusCode::UNAUTHORIZED,
        Json(json!({"error": {"message": "admin authentication required"}})),
    )
        .into_response()
}

#[derive(serde::Deserialize)]
struct PasswordReq {
    password: String,
}

#[derive(serde::Deserialize)]
struct ChangePasswordReq {
    old_password: String,
    new_password: String,
}

async fn api_admin_session(State(state): State<Arc<BridgeState>>, headers: HeaderMap) -> Response {
    let configured = state.security.is_configured();
    let authenticated = !configured
        || session_cookie(&headers)
            .map(|t| state.security.validate_session(&t))
            .unwrap_or(false);
    Json(json!({ "configured": configured, "authenticated": authenticated })).into_response()
}

async fn api_admin_setup(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<PasswordReq>,
) -> Response {
    match state.security.setup(&req.password) {
        Ok(()) => match state.security.login(&req.password) {
            Ok(token) => login_response(&state, token),
            Err(e) => error_response(e),
        },
        Err(e) => error_response(e),
    }
}

async fn api_admin_login(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<PasswordReq>,
) -> Response {
    match state.security.login(&req.password) {
        Ok(token) => login_response(&state, token),
        Err(e) => error_response(e),
    }
}

async fn api_admin_logout(State(state): State<Arc<BridgeState>>, headers: HeaderMap) -> Response {
    if let Some(token) = session_cookie(&headers) {
        state.security.revoke_session(&token);
    }
    let mut resp = Json(json!({"ok": true})).into_response();
    let cleared = "mb_session=; HttpOnly; SameSite=Strict; Path=/; Max-Age=0";
    if let Ok(v) = HeaderValue::from_str(cleared) {
        resp.headers_mut().insert(header::SET_COOKIE, v);
    }
    resp
}

async fn api_admin_password(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<ChangePasswordReq>,
) -> Response {
    match state
        .security
        .change_password(&req.old_password, &req.new_password)
    {
        Ok(()) => match state.security.login(&req.new_password) {
            Ok(token) => login_response(&state, token),
            Err(e) => error_response(e),
        },
        Err(e) => error_response(e),
    }
}

/// Build a JSON `{ok:true}` response that also sets the session cookie.
fn login_response(state: &Arc<BridgeState>, token: String) -> Response {
    let secure = if state.storage.settings().tls_enabled {
        "; Secure"
    } else {
        ""
    };
    let cookie =
        format!("mb_session={token}; HttpOnly; SameSite=Strict; Path=/; Max-Age=604800{secure}");
    let mut resp = Json(json!({"ok": true})).into_response();
    if let Ok(v) = HeaderValue::from_str(&cookie) {
        resp.headers_mut().insert(header::SET_COOKIE, v);
    }
    resp
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

async fn api_logs_verbose_get(State(state): State<Arc<BridgeState>>) -> Response {
    Json(json!({ "enabled": state.proxy.verbose() })).into_response()
}

#[derive(serde::Deserialize)]
struct VerboseRequest {
    enabled: bool,
}

async fn api_logs_verbose_set(
    State(state): State<Arc<BridgeState>>,
    Json(req): Json<VerboseRequest>,
) -> Response {
    state.proxy.set_verbose(req.enabled);
    Json(json!({ "enabled": req.enabled })).into_response()
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
