//! Admin password authentication for the WebUI control plane plus API key
//! management for the `/v1` proxy.
//!
//! - Admin password: argon2 hash persisted in the `security` blob. First run
//!   has no password (the WebUI prompts to set one); once set, `/api/*` (except
//!   the auth endpoints themselves) requires a valid session cookie.
//! - Sessions: opaque random tokens kept in memory with a TTL. A restart drops
//!   sessions, which is fine for a local admin panel.
//! - API keys: only the sha256 hash + a display prefix are stored; the raw key
//!   is shown exactly once at creation time.

use crate::error::{BridgeError, Result};
use crate::storage::Storage;
use argon2::password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString};
use argon2::Argon2;
use parking_lot::Mutex;
use rand::RngCore;
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};
use std::collections::HashMap;
use std::sync::Arc;

const BLOB: &str = "security";
const SESSION_TTL_MS: i64 = 7 * 24 * 60 * 60 * 1000;
const MIN_PASSWORD_LEN: usize = 6;

#[derive(Default, Serialize, Deserialize)]
struct SecurityBlob {
    admin_password_hash: Option<String>,
    #[serde(default)]
    api_keys: Vec<ApiKeyRecord>,
}

#[derive(Clone, Serialize, Deserialize)]
struct ApiKeyRecord {
    id: String,
    name: String,
    /// sha256(raw_key) hex.
    hash: String,
    /// First few chars for display, e.g. `sk-1a2b3c4d…`.
    prefix: String,
    created_at: i64,
    last_used: Option<i64>,
}

/// Masked API key info safe to return to the WebUI.
#[derive(Serialize)]
pub struct ApiKeyView {
    pub id: String,
    pub name: String,
    pub prefix: String,
    pub created_at: i64,
    pub last_used: Option<i64>,
}

pub struct Security {
    storage: Arc<Storage>,
    blob: Mutex<SecurityBlob>,
    sessions: Mutex<HashMap<String, i64>>,
}

fn now_ms() -> i64 {
    chrono::Utc::now().timestamp_millis()
}

fn sha256_hex(s: &str) -> String {
    hex::encode(Sha256::digest(s.as_bytes()))
}

fn rand_hex(bytes: usize) -> String {
    let mut buf = vec![0u8; bytes];
    rand::thread_rng().fill_bytes(&mut buf);
    hex::encode(buf)
}

impl Security {
    pub fn load(storage: Arc<Storage>) -> Result<Arc<Self>> {
        let blob = storage.load_blob::<SecurityBlob>(BLOB)?.unwrap_or_default();
        Ok(Arc::new(Self {
            storage,
            blob: Mutex::new(blob),
            sessions: Mutex::new(HashMap::new()),
        }))
    }

    fn persist(&self) -> Result<()> {
        self.storage.save_blob(BLOB, &*self.blob.lock())
    }

    // ---- admin password -----------------------------------------------------

    pub fn is_configured(&self) -> bool {
        self.blob.lock().admin_password_hash.is_some()
    }

    fn hash_password(password: &str) -> Result<String> {
        let mut salt = [0u8; 16];
        rand::thread_rng().fill_bytes(&mut salt);
        let salt = SaltString::encode_b64(&salt).map_err(|e| BridgeError::Other(e.to_string()))?;
        Ok(Argon2::default()
            .hash_password(password.as_bytes(), &salt)
            .map_err(|e| BridgeError::Other(e.to_string()))?
            .to_string())
    }

    fn check_password(&self, password: &str) -> bool {
        let guard = self.blob.lock();
        let Some(stored) = guard.admin_password_hash.as_ref() else {
            return false;
        };
        let Ok(parsed) = PasswordHash::new(stored) else {
            return false;
        };
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    }

    /// First-run setup. Only allowed while no password is configured.
    pub fn setup(&self, password: &str) -> Result<()> {
        if password.len() < MIN_PASSWORD_LEN {
            return Err(BridgeError::Other(format!(
                "password too short (min {MIN_PASSWORD_LEN})"
            )));
        }
        if self.is_configured() {
            return Err(BridgeError::Other("admin password already set".into()));
        }
        let hash = Self::hash_password(password)?;
        self.blob.lock().admin_password_hash = Some(hash);
        self.persist()
    }

    pub fn change_password(&self, old: &str, new: &str) -> Result<()> {
        if !self.check_password(old) {
            return Err(BridgeError::Other("current password is incorrect".into()));
        }
        if new.len() < MIN_PASSWORD_LEN {
            return Err(BridgeError::Other(format!(
                "password too short (min {MIN_PASSWORD_LEN})"
            )));
        }
        let hash = Self::hash_password(new)?;
        self.blob.lock().admin_password_hash = Some(hash);
        self.sessions.lock().clear();
        self.persist()
    }

    // ---- sessions -----------------------------------------------------------

    pub fn login(&self, password: &str) -> Result<String> {
        if !self.check_password(password) {
            return Err(BridgeError::Other("wrong password".into()));
        }
        let token = rand_hex(32);
        self.sessions
            .lock()
            .insert(token.clone(), now_ms() + SESSION_TTL_MS);
        Ok(token)
    }

    pub fn validate_session(&self, token: &str) -> bool {
        let mut sessions = self.sessions.lock();
        let now = now_ms();
        sessions.retain(|_, exp| *exp > now);
        sessions.contains_key(token)
    }

    pub fn revoke_session(&self, token: &str) {
        self.sessions.lock().remove(token);
    }

    // ---- API keys -----------------------------------------------------------

    pub fn list_keys(&self) -> Vec<ApiKeyView> {
        self.blob
            .lock()
            .api_keys
            .iter()
            .map(|k| ApiKeyView {
                id: k.id.clone(),
                name: k.name.clone(),
                prefix: k.prefix.clone(),
                created_at: k.created_at,
                last_used: k.last_used,
            })
            .collect()
    }

    /// Create a key, returning the masked view and the raw secret (shown once).
    pub fn create_key(&self, name: &str) -> Result<(ApiKeyView, String)> {
        let raw = format!("sk-{}", rand_hex(24));
        let prefix = format!("{}…", &raw[..raw.len().min(12)]);
        let rec = ApiKeyRecord {
            id: rand_hex(8),
            name: name.trim().to_string(),
            hash: sha256_hex(&raw),
            prefix,
            created_at: now_ms(),
            last_used: None,
        };
        let view = ApiKeyView {
            id: rec.id.clone(),
            name: rec.name.clone(),
            prefix: rec.prefix.clone(),
            created_at: rec.created_at,
            last_used: None,
        };
        self.blob.lock().api_keys.push(rec);
        self.persist()?;
        Ok((view, raw))
    }

    pub fn revoke_key(&self, id: &str) -> Result<()> {
        let mut guard = self.blob.lock();
        let before = guard.api_keys.len();
        guard.api_keys.retain(|k| k.id != id);
        let removed = guard.api_keys.len() != before;
        drop(guard);
        if !removed {
            return Err(BridgeError::Other("api key not found".into()));
        }
        self.persist()
    }

    pub fn has_keys(&self) -> bool {
        !self.blob.lock().api_keys.is_empty()
    }

    /// Validate a raw bearer key; updates `last_used` and persists on a hit.
    pub fn verify_key(&self, raw: &str) -> bool {
        let hash = sha256_hex(raw);
        let mut guard = self.blob.lock();
        if let Some(rec) = guard.api_keys.iter_mut().find(|k| k.hash == hash) {
            rec.last_used = Some(now_ms());
            let snapshot = SecurityBlob {
                admin_password_hash: guard.admin_password_hash.clone(),
                api_keys: guard.api_keys.clone(),
            };
            drop(guard);
            let _ = self.storage.save_blob(BLOB, &snapshot);
            true
        } else {
            false
        }
    }
}
