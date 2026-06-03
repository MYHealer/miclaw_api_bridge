//! Local HTTP proxy exposing OpenAI Chat Completions and Anthropic Messages
//! compatible endpoints, all routed to mimo PC.

pub(crate) mod anthropic;
pub(crate) mod openai;
mod transport;

pub use transport::emit_log;

use crate::mimo::MimoClient;
use crate::state::LogEmitter;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;

pub struct ProxyController {
    pub mimo: Arc<MimoClient>,
    pub emitter: LogEmitter,
    verbose: AtomicBool,
}

impl ProxyController {
    pub fn new(mimo: Arc<MimoClient>, emitter: LogEmitter) -> Self {
        Self {
            mimo,
            emitter,
            verbose: AtomicBool::new(false),
        }
    }

    /// Whether request logs should include the full request body (prompt).
    pub fn verbose(&self) -> bool {
        self.verbose.load(Ordering::Relaxed)
    }

    pub fn set_verbose(&self, on: bool) {
        self.verbose.store(on, Ordering::Relaxed);
    }
}

#[derive(Debug, Clone, serde::Serialize, serde::Deserialize)]
pub struct ProxySnapshot {
    pub running: bool,
    pub addr: Option<String>,
    pub port: u16,
    pub active_port: Option<u16>,
    pub restart_required: bool,
}
