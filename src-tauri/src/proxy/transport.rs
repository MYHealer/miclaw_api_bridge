use super::ProxyController;
use crate::error::BridgeError;
use axum::{
    body::Body,
    http::{header, HeaderMap, StatusCode},
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub fn map_err(e: BridgeError) -> Response {
    let (code, kind) = match &e {
        BridgeError::NotAuthenticated => (StatusCode::UNAUTHORIZED, "not_authenticated"),
        BridgeError::Login(_) => (StatusCode::UNAUTHORIZED, "login_failed"),
        _ => (StatusCode::BAD_GATEWAY, "upstream_error"),
    };
    let body = Json(json!({
        "error": {
            "type": kind,
            "message": e.to_string(),
        }
    }));
    (code, body).into_response()
}

pub async fn list_models(_ctrl: Arc<ProxyController>) -> Response {
    let data: Vec<Value> = crate::mimo::known_models()
        .into_iter()
        .map(|m| {
            json!({
                "id": m.id,
                "object": m.object,
                "owned_by": m.owned_by,
                "created": chrono::Utc::now().timestamp(),
            })
        })
        .collect();
    Json(json!({
        "object": "list",
        "data": data,
    }))
    .into_response()
}

/// Emit a structured log entry to the front-end's `proxy-log` event
/// channel. Safe to call from any handler.
pub fn emit_log(ctrl: &ProxyController, payload: Value) {
    ctrl.emitter.emit(payload);
}

/// Forward a JSON request to mimo, streaming the upstream bytes back.
pub async fn forward(
    ctrl: Arc<ProxyController>,
    upstream_path: &str,
    body: Value,
) -> Response {
    let stream_requested = body
        .get("stream")
        .and_then(|v| v.as_bool())
        .unwrap_or(false);
    let model = body
        .get("model")
        .and_then(|v| v.as_str())
        .unwrap_or("")
        .to_string();
    let started = std::time::Instant::now();
    tracing::debug!(
        target = "proxy",
        "→ mimo {upstream_path} stream={stream_requested} model={model}"
    );
    emit_log(
        &ctrl,
        json!({
            "ts": chrono::Utc::now().timestamp_millis(),
            "kind": "request",
            "path": upstream_path,
            "model": model,
            "stream": stream_requested,
        }),
    );
    match ctrl.mimo.post_json(upstream_path, body).await {
        Ok(upstream) => {
            let status = upstream.status();
            tracing::debug!(target = "proxy", "← mimo {upstream_path} status={status}");
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "response",
                    "path": upstream_path,
                    "status": status.as_u16(),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            proxy_response(upstream).await
        }
        Err(e) => {
            tracing::warn!(target = "proxy", "mimo {upstream_path} error: {e}");
            emit_log(
                &ctrl,
                json!({
                    "ts": chrono::Utc::now().timestamp_millis(),
                    "kind": "error",
                    "path": upstream_path,
                    "message": e.to_string(),
                    "elapsed_ms": started.elapsed().as_millis() as u64,
                }),
            );
            map_err(e)
        }
    }
}

pub async fn proxy_response(upstream: reqwest::Response) -> Response {
    let status =
        StatusCode::from_u16(upstream.status().as_u16()).unwrap_or(StatusCode::BAD_GATEWAY);
    let mut headers = HeaderMap::new();
    if let Some(ct) = upstream.headers().get(header::CONTENT_TYPE) {
        headers.insert(header::CONTENT_TYPE, ct.clone());
    } else {
        headers.insert(
            header::CONTENT_TYPE,
            header::HeaderValue::from_static("application/json"),
        );
    }
    headers.insert(
        header::CACHE_CONTROL,
        header::HeaderValue::from_static("no-cache"),
    );
    let stream = upstream.bytes_stream();
    let body = Body::from_stream(stream);
    let mut resp = Response::new(body);
    *resp.status_mut() = status;
    *resp.headers_mut() = headers;
    resp
}
