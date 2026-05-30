//! Local HTTP proxy exposing OpenAI Chat Completions and Anthropic Messages
//! compatible endpoints, all routed to mimo PC.

pub(crate) mod anthropic;
pub(crate) mod openai;
mod transport;

pub use transport::emit_log;

use crate::mimo::MimoClient;
use crate::state::LogEmitter;
use std::sync::Arc;

pub struct ProxyController {
    pub mimo: Arc<MimoClient>,
    pub emitter: LogEmitter,
}

impl ProxyController {
    pub fn new(mimo: Arc<MimoClient>, emitter: LogEmitter) -> Self {
        Self { mimo, emitter }
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
