use crate::auth::AuthState;
use crate::error::Result;
use crate::mimo::MimoClient;
use crate::storage::Storage;
use parking_lot::{Mutex, RwLock};
use serde_json::Value;
use std::collections::VecDeque;
use std::net::SocketAddr;
use std::sync::Arc;
use tokio::sync::broadcast;

pub struct BridgeState {
    pub storage: Arc<Storage>,
    pub auth: Arc<RwLock<AuthState>>,
    pub mimo: Arc<MimoClient>,
    pub proxy: Arc<crate::proxy::ProxyController>,
    pub logs: Arc<LogHub>,
    bound_addr: RwLock<Option<SocketAddr>>,
}

impl BridgeState {
    pub fn new() -> Result<Arc<Self>> {
        let storage = Storage::new()?;
        Self::with_storage(storage)
    }

    pub fn with_storage(storage: Arc<Storage>) -> Result<Arc<Self>> {
        let auth = Arc::new(RwLock::new(AuthState::load(&storage)?));
        let mimo = Arc::new(MimoClient::new(auth.clone()));
        let logs = Arc::new(LogHub::new(500));
        let emitter = LogEmitter::new(logs.clone());
        let proxy = Arc::new(crate::proxy::ProxyController::new(mimo.clone(), emitter));
        Ok(Arc::new(Self {
            storage,
            auth,
            mimo,
            proxy,
            logs,
            bound_addr: RwLock::new(None),
        }))
    }

    pub fn set_bound_addr(&self, addr: SocketAddr) {
        *self.bound_addr.write() = Some(addr);
    }

    pub fn clear_bound_addr(&self) {
        *self.bound_addr.write() = None;
    }

    pub fn bound_addr(&self) -> Option<SocketAddr> {
        *self.bound_addr.read()
    }
}

pub struct LogHub {
    rows: Mutex<VecDeque<Value>>,
    cap: usize,
    tx: broadcast::Sender<Value>,
}

impl LogHub {
    pub fn new(cap: usize) -> Self {
        let (tx, _) = broadcast::channel(512);
        Self {
            rows: Mutex::new(VecDeque::with_capacity(cap)),
            cap,
            tx,
        }
    }

    pub fn push(&self, payload: Value) {
        {
            let mut rows = self.rows.lock();
            if rows.len() >= self.cap {
                rows.pop_back();
            }
            rows.push_front(payload.clone());
        }
        let _ = self.tx.send(payload);
    }

    pub fn snapshot(&self) -> Vec<Value> {
        self.rows.lock().iter().cloned().collect()
    }

    pub fn subscribe(&self) -> broadcast::Receiver<Value> {
        self.tx.subscribe()
    }
}

#[derive(Clone)]
pub struct LogEmitter {
    hub: Arc<LogHub>,
}

impl LogEmitter {
    pub fn new(hub: Arc<LogHub>) -> Self {
        Self { hub }
    }

    pub fn emit(&self, payload: Value) {
        self.hub.push(payload);
    }
}
