use super::transport::{forward, list_models};
use super::ProxyController;
use axum::{
    extract::State,
    response::{IntoResponse, Response},
    Json,
};
use serde_json::{json, Value};
use std::sync::Arc;

pub async fn chat(State(ctrl): State<Arc<ProxyController>>, Json(body): Json<Value>) -> Response {
    forward(ctrl.clone(), crate::mimo::PATH_CHAT, body).await
}

pub async fn models(State(ctrl): State<Arc<ProxyController>>) -> Response {
    list_models(ctrl).await
}

pub async fn root(State(_): State<Arc<ProxyController>>) -> Response {
    Json(json!({
        "service": "mimo-bridge",
        "endpoints": [
            "/v1/models",
            "/v1/chat/completions",
            "/v1/messages",
        ]
    }))
    .into_response()
}
