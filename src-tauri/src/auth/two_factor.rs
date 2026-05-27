use super::login::{finalize_with_location, parse_session_fields};
use super::{build_http_client, strip_prefix, AuthState, LoginFlowContext, Session};
use crate::error::{BridgeError, Result};
use crate::storage::Storage;
use parking_lot::RwLock;
use serde_json::Value;

const ACCOUNT_HOST: &str = "https://account.xiaomi.com";

/// 2FA flag → relative path on `account.xiaomi.com`.
fn paths_for(flag: i32) -> (&'static str, &'static str) {
    match flag {
        4 => ("/identity/auth/sendPhoneTicket", "/identity/auth/verifyPhone"),
        // 8 = email; the rest fall back to email which is the most common.
        _ => ("/identity/auth/sendEmailTicket", "/identity/auth/verifyEmail"),
    }
}

pub async fn send_ticket(state: &std::sync::Arc<RwLock<AuthState>>, flag: i32) -> Result<bool> {
    let id_session = state
        .read()
        .flow
        .identity_session
        .clone()
        .ok_or_else(|| BridgeError::Login("missing identity session".into()))?;
    let (client, _) = build_http_client(&Session::default())?;
    let (send_path, _) = paths_for(flag);
    let url = format!("{ACCOUNT_HOST}{send_path}");
    let dc = chrono::Utc::now().timestamp_millis();
    let resp = client
        .post(&url)
        .query(&[("_dc", dc.to_string())])
        .header("cookie", format!("identity_session={id_session}"))
        .form(&[("_json", "true"), ("retry", "0"), ("icode", "")])
        .send()
        .await?;
    Ok(resp.status().is_success())
}

pub async fn verify_ticket(
    state: &std::sync::Arc<RwLock<AuthState>>,
    storage: &std::sync::Arc<Storage>,
    flag: i32,
    ticket: String,
) -> Result<()> {
    let id_session = state
        .read()
        .flow
        .identity_session
        .clone()
        .ok_or_else(|| BridgeError::Login("missing identity session".into()))?;

    let (client, _) = build_http_client(&Session::default())?;
    let (_, verify_path) = paths_for(flag);
    let url = format!("{ACCOUNT_HOST}{verify_path}");
    let dc = chrono::Utc::now().timestamp_millis();
    let resp = client
        .post(&url)
        .query(&[("_dc", dc.to_string())])
        .header("cookie", format!("identity_session={id_session}"))
        .form(&[
            ("_flag", flag.to_string().as_str()),
            ("ticket", ticket.as_str()),
            ("trust", "true"),
            ("_json", "true"),
        ])
        .send()
        .await?
        .text()
        .await?;

    let body: Value = serde_json::from_str(strip_prefix(&resp))
        .map_err(|e| BridgeError::Login(format!("verify parse: {e}")))?;
    let code = body.get("code").and_then(|v| v.as_i64()).unwrap_or(-1);
    if code == 7014 {
        return Err(BridgeError::VerificationCodeError);
    }
    if code != 0 {
        return Err(BridgeError::Login(format!("verify code={code}")));
    }
    let location = body
        .get("location")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BridgeError::Login("verify missing location".into()))?
        .to_string();

    let session_seed = parse_session_fields(&body);
    let session = finalize_with_location(&client, location, session_seed).await?;

    {
        let mut guard = state.write();
        guard.session = session;
        guard.flow = LoginFlowContext::default();
        guard.save(storage)?;
    }
    Ok(())
}

pub fn extract_query_param<'a>(url: &'a str, key: &str) -> Option<&'a str> {
    let q = url.split_once('?').map(|x| x.1)?;
    for kv in q.split('&') {
        let mut it = kv.splitn(2, '=');
        let k = it.next()?;
        let v = it.next()?;
        if k == key {
            return Some(v);
        }
    }
    None
}
