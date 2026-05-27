use super::login::{finalize_with_location, parse_session_fields};
use super::{build_http_client, strip_prefix, AuthState, Session, SERVICE_LOGIN_URL};
use crate::error::{BridgeError, Result};
use crate::storage::Storage;
use parking_lot::RwLock;
use serde_json::Value;
use std::sync::Arc;

/// Refresh `serviceToken` using the persisted `passToken`.
pub async fn refresh(
    state: &Arc<RwLock<AuthState>>,
    storage: &Arc<Storage>,
) -> Result<Session> {
    let session_snapshot = state.read().session.clone();
    if session_snapshot.pass_token.is_none() || session_snapshot.user_id.is_none() {
        return Err(BridgeError::NotAuthenticated);
    }

    let (client, _) = build_http_client(&session_snapshot)?;
    let resp = client
        .get(SERVICE_LOGIN_URL)
        .send()
        .await?
        .text()
        .await?;
    let body: Value = serde_json::from_str(strip_prefix(&resp))
        .map_err(|e| BridgeError::Login(format!("refresh parse: {e}")))?;
    let location = body
        .get("location")
        .and_then(|v| v.as_str())
        .ok_or_else(|| BridgeError::Login("refresh missing location".into()))?
        .to_string();

    let mut next = parse_session_fields(&body);
    // Keep prior fields when server doesn't re-send them.
    if next.user_id.is_none() {
        next.user_id = session_snapshot.user_id.clone();
    }
    if next.pass_token.is_none() {
        next.pass_token = session_snapshot.pass_token.clone();
    }
    if next.c_user_id.is_none() {
        next.c_user_id = session_snapshot.c_user_id.clone();
    }
    if next.nick.is_none() {
        next.nick = session_snapshot.nick.clone();
    }

    let next = finalize_with_location(&client, location, next).await?;
    {
        let mut guard = state.write();
        guard.session = next.clone();
        guard.save(storage)?;
    }
    Ok(next)
}
