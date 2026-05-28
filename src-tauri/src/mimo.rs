use crate::auth::{build_http_client, AuthState};
use crate::error::{BridgeError, Result};
use bytes::Bytes;
use futures::stream::BoxStream;
use futures::StreamExt;
use parking_lot::RwLock;
use reqwest::header::{HeaderMap, HeaderName, HeaderValue};
use reqwest::Method;
use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::sync::Arc;

/// All mimo PC traffic terminates at this host.
pub const MIMO_HOST: &str = "https://api.miclaw.xiaomi.net";

/// PC-style endpoints (observed in macOS miclaw HAR captures). The PC
/// client speaks plain OpenAI Chat Completions; no device signature, no
/// `userId`/`cUserId` cookies, only `serviceToken`.
pub const PATH_CHAT: &str = "/osbot/pc/llm/v1/chat/completions";

/// MCP host service exposed by miclaw PC. Out of scope for the bridge today;
/// kept here so we don't accidentally collide with it.
#[allow(dead_code)]
pub const PATH_MCP_STREAMABLE: &str = "/osbot/pc/mcp/v1/streamable";

/// Default model when callers don't specify one.
pub const MODEL_DEFAULT: &str = "mimo-omni";

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelInfo {
    pub id: String,
    pub object: &'static str,
    pub owned_by: &'static str,
    pub family: &'static str,
}

pub fn known_models() -> Vec<ModelInfo> {
    vec![ModelInfo {
        id: "mimo-omni".into(),
        object: "model",
        owned_by: "xiaomi",
        family: "chat",
    }]
}

pub struct MimoClient {
    auth: Arc<RwLock<AuthState>>,
}

impl MimoClient {
    pub fn new(auth: Arc<RwLock<AuthState>>) -> Self {
        Self { auth }
    }

    pub fn auth_handle(&self) -> Arc<RwLock<AuthState>> {
        self.auth.clone()
    }

    fn snapshot(&self) -> Result<crate::auth::Session> {
        let snap = self.auth.read().session.clone();
        if !snap.is_authenticated() {
            return Err(BridgeError::NotAuthenticated);
        }
        Ok(snap)
    }

    /// Headers used by the macOS miclaw client: a `node` UA, a
    /// `serviceToken + cUserId` cookie pair, JSON content. Everything else
    /// is decoration. (HAR shows no `userId`, no device signature on PC.)
    fn build_headers(&self, session: &crate::auth::Session) -> Result<HeaderMap> {
        let token = session
            .service_token
            .as_ref()
            .ok_or(BridgeError::NotAuthenticated)?;
        let cookie = match &session.c_user_id {
            Some(c) => format!("serviceToken={token}; cUserId={c}"),
            None => format!("serviceToken={token}"),
        };
        let mut h = HeaderMap::new();
        h.insert(
            HeaderName::from_static("user-agent"),
            HeaderValue::from_static("node"),
        );
        h.insert(
            HeaderName::from_static("accept"),
            HeaderValue::from_static("*/*"),
        );
        h.insert(
            HeaderName::from_static("accept-language"),
            HeaderValue::from_static("*"),
        );
        h.insert(
            HeaderName::from_static("sec-fetch-mode"),
            HeaderValue::from_static("cors"),
        );
        h.insert(
            HeaderName::from_static("accept-encoding"),
            HeaderValue::from_static("gzip"),
        );
        h.insert(
            HeaderName::from_static("cookie"),
            HeaderValue::from_str(&cookie).map_err(BridgeError::other)?,
        );
        Ok(h)
    }

    /// Forward a JSON body to mimo. Streaming is requested by the JSON body
    /// itself (`"stream": true`); upstream returns SSE in that case.
    pub async fn post_json(&self, path: &str, body: Value) -> Result<reqwest::Response> {
        let session = self.snapshot()?;
        let (client, _) = build_http_client(&session)?;
        let headers = self.build_headers(&session)?;
        // Diagnostic: cookie shape (lengths only, never values).
        if let Some(c) = headers.get("cookie").and_then(|v| v.to_str().ok()) {
            let parts: Vec<String> = c
                .split(';')
                .map(str::trim)
                .filter_map(|kv| {
                    let mut it = kv.splitn(2, '=');
                    let k = it.next()?;
                    let v = it.next().unwrap_or("");
                    Some(format!("{k}(len={})", v.len()))
                })
                .collect();
            tracing::debug!(target = "mimo", "cookie shape: [{}]", parts.join(", "));
        }
        let resp = client
            .request(Method::POST, format!("{MIMO_HOST}{path}"))
            .headers(headers)
            .json(&body)
            .send()
            .await?;
        Ok(resp)
    }

    pub async fn post_stream(
        &self,
        path: &str,
        body: Value,
    ) -> Result<(
        reqwest::StatusCode,
        HeaderMap,
        BoxStream<'static, std::result::Result<Bytes, reqwest::Error>>,
    )> {
        let resp = self.post_json(path, body).await?;
        let status = resp.status();
        let headers = resp.headers().clone();
        let stream = resp.bytes_stream().boxed();
        Ok((status, headers, stream))
    }

    pub async fn chat(&self, body: Value) -> Result<reqwest::Response> {
        self.post_json(PATH_CHAT, body).await
    }

    pub fn quick_status(&self) -> AuthSnapshot {
        let auth = self.auth.read();
        AuthSnapshot {
            authenticated: auth.session.is_authenticated(),
            nick: auth.session.nick.clone(),
            user_id: auth.session.user_id.clone(),
            refreshed_at: auth.session.refreshed_at,
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub struct AuthSnapshot {
    pub authenticated: bool,
    pub nick: Option<String>,
    pub user_id: Option<String>,
    pub refreshed_at: Option<i64>,
}
