use crate::error::{BridgeError, Result};
use crate::storage::Storage;
use reqwest::cookie::Jar;
use reqwest::header::{HeaderMap, HeaderValue, COOKIE, USER_AGENT};
use serde::{Deserialize, Serialize};
use std::sync::Arc;
use url::Url;

pub mod login;
pub mod refresh;
pub mod two_factor;

/// Mimics miclaw's User-Agent format observed from the HAR capture.
/// The UA is required by some Xiaomi endpoints to bucket as the miclaw client.
pub const DEFAULT_USER_AGENT: &str = "25098PN5AC;OS3.0.306.4.WBLCNXM";
pub const APK_VERSION: &str = "0.3.4699.b604ddf";
pub const DEVICE_MODEL: &str = "Xiaomi 17 Pro";

/// `sid` used by the miclaw client when it talks to passport. This value is
/// hard-coded in `MiPassportLoginActivity` of the decompiled APK.
pub const SID: &str = "xiaomihome";

const ACCOUNT_HOST: &str = "https://account.xiaomi.com";
pub const SERVICE_LOGIN_URL: &str = "https://account.xiaomi.com/pass/serviceLogin?sid=xiaomihome&_json=true";
pub const SERVICE_LOGIN_AUTH2_URL: &str = "https://account.xiaomi.com/pass/serviceLoginAuth2?_json=true";

/// Persisted snapshot of the Xiaomi account session.
#[derive(Debug, Clone, Default, Serialize, Deserialize)]
pub struct Session {
    pub user_id: Option<String>,
    pub c_user_id: Option<String>,
    pub pass_token: Option<String>,
    pub ssecurity: Option<String>,
    pub service_token: Option<String>,
    pub nick: Option<String>,
    /// Unix-ms when the session was last refreshed. Used by UI only.
    pub refreshed_at: Option<i64>,
}

impl Session {
    pub fn is_authenticated(&self) -> bool {
        self.service_token.is_some() && self.pass_token.is_some() && self.user_id.is_some()
    }

    pub fn cookie_header(&self) -> Option<String> {
        match (&self.c_user_id, &self.user_id, &self.service_token) {
            (Some(c), Some(u), Some(t)) => Some(format!(
                "cUserId={c}; userId={u}; serviceToken={t}"
            )),
            _ => None,
        }
    }
}

/// Holds the live session plus the in-flight login flow context (captcha,
/// notification url for 2FA, etc).
#[derive(Debug, Default)]
pub struct AuthState {
    pub session: Session,
    pub flow: LoginFlowContext,
}

#[derive(Debug, Default, Clone)]
pub struct LoginFlowContext {
    pub identity_session: Option<String>,
    pub notification_url: Option<String>,
    pub two_factor_options: Vec<i32>,
    pub captcha_url: Option<String>,
}

const SESSION_BLOB: &str = "session";

impl AuthState {
    pub fn load(storage: &Storage) -> Result<Self> {
        let session: Session = storage
            .load_blob(SESSION_BLOB)?
            .unwrap_or_default();
        Ok(Self {
            session,
            flow: LoginFlowContext::default(),
        })
    }

    pub fn save(&self, storage: &Storage) -> Result<()> {
        storage.save_blob(SESSION_BLOB, &self.session)?;
        Ok(())
    }

    pub fn clear(storage: &Storage) -> Result<()> {
        storage.delete_blob(SESSION_BLOB)?;
        Ok(())
    }
}

/// Build a fresh HTTP client equipped with a cookie jar that already contains
/// `passToken` / `userId` from the persisted session, and the Xiaomi UA.
pub fn build_http_client(session: &Session) -> Result<(reqwest::Client, Arc<Jar>)> {
    let jar = Arc::new(Jar::default());
    if let (Some(token), Some(uid)) = (&session.pass_token, &session.user_id) {
        let url: Url = ACCOUNT_HOST.parse().expect("static url");
        jar.add_cookie_str(&format!("passToken={token}; Domain=.xiaomi.com; Path=/"), &url);
        jar.add_cookie_str(&format!("userId={uid}; Domain=.xiaomi.com; Path=/"), &url);
        if let Some(c) = &session.c_user_id {
            jar.add_cookie_str(&format!("cUserId={c}; Domain=.xiaomi.com; Path=/"), &url);
        }
    }

    let mut headers = HeaderMap::new();
    headers.insert(USER_AGENT, HeaderValue::from_static(DEFAULT_USER_AGENT));

    let client = reqwest::Client::builder()
        .cookie_provider(jar.clone())
        .default_headers(headers)
        .gzip(true)
        .timeout(std::time::Duration::from_secs(30))
        .build()
        .map_err(BridgeError::from)?;
    Ok((client, jar))
}

/// Strip Xiaomi's anti-CSRF JSON prefix `&&&START&&&`.
pub fn strip_prefix(body: &str) -> &str {
    body.trim_start_matches("&&&START&&&")
}

/// Minimal compile-time guard so unused imports don't slip in: tests later.
#[allow(dead_code)]
pub fn cookie_value(headers: &HeaderMap, name: &str) -> Option<String> {
    headers
        .get_all(COOKIE)
        .iter()
        .find_map(|v| v.to_str().ok().and_then(|s| extract_cookie(s, name)))
}

fn extract_cookie(header: &str, name: &str) -> Option<String> {
    for part in header.split(';') {
        let trimmed = part.trim();
        if let Some(rest) = trimmed.strip_prefix(&format!("{name}=")) {
            return Some(rest.to_string());
        }
    }
    None
}

/// Re-exports.
pub use login::login;
pub use refresh::refresh as refresh_session;
pub use two_factor::{send_ticket, verify_ticket};
