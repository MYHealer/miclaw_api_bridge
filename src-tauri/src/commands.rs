use crate::auth;
use crate::auth::login::{LoginOutcome, LoginRequest};
use crate::error::{BridgeError, Result};
use crate::mimo::{known_models, AuthSnapshot, ModelInfo};
use crate::proxy::ProxySnapshot;
use crate::state::AppState;
use std::sync::Arc;
use tauri::State;

#[tauri::command]
pub async fn auth_status(state: State<'_, Arc<AppState>>) -> Result<AuthSnapshot> {
    Ok(state.mimo.quick_status())
}

#[tauri::command]
pub async fn login(state: State<'_, Arc<AppState>>, req: LoginRequest) -> Result<LoginOutcome> {
    auth::login(&state.auth, &state.storage, req).await
}

#[tauri::command]
pub async fn send_two_factor_ticket(state: State<'_, Arc<AppState>>, flag: i32) -> Result<bool> {
    auth::send_ticket(&state.auth, flag).await
}

#[tauri::command]
pub async fn verify_two_factor(
    state: State<'_, Arc<AppState>>,
    flag: i32,
    ticket: String,
) -> Result<()> {
    auth::verify_ticket(&state.auth, &state.storage, flag, ticket).await
}

#[tauri::command]
pub async fn refresh_session(state: State<'_, Arc<AppState>>) -> Result<AuthSnapshot> {
    auth::refresh_session(&state.auth, &state.storage).await?;
    Ok(state.mimo.quick_status())
}

#[tauri::command]
pub async fn logout(state: State<'_, Arc<AppState>>) -> Result<()> {
    {
        let mut guard = state.auth.write();
        guard.session = Default::default();
        guard.flow = Default::default();
    }
    crate::auth::AuthState::clear(&state.storage)?;
    Ok(())
}

#[tauri::command]
pub async fn proxy_status(state: State<'_, Arc<AppState>>) -> Result<ProxySnapshot> {
    Ok(state.proxy.snapshot())
}

#[tauri::command]
pub async fn start_proxy(state: State<'_, Arc<AppState>>) -> Result<ProxySnapshot> {
    state.proxy.start().await
}

#[tauri::command]
pub async fn stop_proxy(state: State<'_, Arc<AppState>>) -> Result<ProxySnapshot> {
    Ok(state.proxy.stop())
}

#[tauri::command]
pub async fn set_proxy_port(state: State<'_, Arc<AppState>>, port: u16) -> Result<ProxySnapshot> {
    if port < 1024 {
        return Err(BridgeError::Proxy("port must be >= 1024".into()));
    }
    state.storage.update_settings(|s| s.proxy_port = port)?;
    if state.proxy.snapshot().running {
        state.proxy.restart().await
    } else {
        Ok(state.proxy.snapshot())
    }
}

#[tauri::command]
pub async fn list_models(_state: State<'_, Arc<AppState>>) -> Result<Vec<ModelInfo>> {
    Ok(known_models())
}
