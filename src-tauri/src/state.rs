use crate::auth::AuthState;
use crate::error::Result;
use crate::mimo::MimoClient;
use crate::storage::Storage;
use parking_lot::RwLock;
use std::sync::Arc;
use tauri::{AppHandle, Emitter};

pub struct AppState {
    pub app_handle: AppHandle,
    pub storage: Arc<Storage>,
    pub auth: Arc<RwLock<AuthState>>,
    pub mimo: Arc<MimoClient>,
    pub proxy: Arc<crate::proxy::ProxyController>,
}

/// Lightweight event emitter handed to the proxy so it can push log lines
/// to the front-end without needing a direct AppHandle reference.
#[derive(Clone)]
pub struct LogEmitter {
    app: AppHandle,
}

impl LogEmitter {
    pub fn new(app: AppHandle) -> Self {
        Self { app }
    }

    pub fn emit(&self, payload: serde_json::Value) {
        let _ = self.app.emit("proxy-log", payload);
    }
}

impl AppState {
    pub fn new(app_handle: AppHandle) -> Result<Self> {
        let storage = Storage::new(&app_handle)?;
        let auth = Arc::new(RwLock::new(AuthState::load(&storage)?));
        let mimo = Arc::new(MimoClient::new(auth.clone()));
        let emitter = LogEmitter::new(app_handle.clone());
        let proxy = Arc::new(crate::proxy::ProxyController::new(
            mimo.clone(),
            storage.clone(),
            emitter,
        ));
        Ok(Self {
            app_handle,
            storage,
            auth,
            mimo,
            proxy,
        })
    }
}
