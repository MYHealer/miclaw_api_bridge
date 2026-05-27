use crate::auth::AuthState;
use crate::error::Result;
use crate::mimo::MimoClient;
use crate::storage::Storage;
use parking_lot::RwLock;
use std::sync::Arc;
use tauri::AppHandle;

pub struct AppState {
    pub app_handle: AppHandle,
    pub storage: Arc<Storage>,
    pub auth: Arc<RwLock<AuthState>>,
    pub mimo: Arc<MimoClient>,
    pub proxy: Arc<crate::proxy::ProxyController>,
}

impl AppState {
    pub fn new(app_handle: AppHandle) -> Result<Self> {
        let storage = Storage::new(&app_handle)?;
        let auth = Arc::new(RwLock::new(AuthState::load(&storage)?));
        let mimo = Arc::new(MimoClient::new(auth.clone()));
        let proxy = Arc::new(crate::proxy::ProxyController::new(mimo.clone(), storage.clone()));
        Ok(Self {
            app_handle,
            storage,
            auth,
            mimo,
            proxy,
        })
    }
}
