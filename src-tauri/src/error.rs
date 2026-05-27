use serde::Serialize;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum BridgeError {
    #[error("network error: {0}")]
    Http(#[from] reqwest::Error),

    #[error("io error: {0}")]
    Io(#[from] std::io::Error),

    #[error("json error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("login: {0}")]
    Login(String),

    #[error("two-factor required")]
    TwoFactorRequired,

    #[error("verification code error")]
    VerificationCodeError,

    #[error("not authenticated")]
    NotAuthenticated,

    #[error("companion offline")]
    CompanionOffline,

    #[error("companion error: {0}")]
    Companion(String),

    #[error("proxy: {0}")]
    Proxy(String),

    #[error("storage: {0}")]
    Storage(String),

    #[error("unexpected: {0}")]
    Other(String),
}

impl BridgeError {
    pub fn other<E: std::fmt::Display>(e: E) -> Self {
        Self::Other(e.to_string())
    }
}

impl From<anyhow::Error> for BridgeError {
    fn from(e: anyhow::Error) -> Self {
        BridgeError::Other(e.to_string())
    }
}

impl Serialize for BridgeError {
    fn serialize<S>(&self, ser: S) -> std::result::Result<S::Ok, S::Error>
    where
        S: serde::Serializer,
    {
        ser.serialize_str(&self.to_string())
    }
}

pub type Result<T> = std::result::Result<T, BridgeError>;
