use crate::error::{BridgeError, Result};
use directories::ProjectDirs;
use parking_lot::RwLock;
use serde::{de::DeserializeOwned, Serialize};
use std::{fs, path::PathBuf, sync::Arc};

const QUALIFIER: &str = "com";
const ORG: &str = "neoruaa";
const APP: &str = "miclaw_api_bridge";

/// Application-wide on-disk storage. Stores plaintext settings (proxy port)
/// plus opaque blobs persisted by sub-modules. Sensitive credentials should
/// go through the keyring helpers in `storage::keyring` (TODO).
pub struct Storage {
    config_dir: PathBuf,
    data_dir: PathBuf,
    settings: RwLock<Settings>,
}

#[derive(Clone, Debug, Serialize, serde::Deserialize)]
pub struct Settings {
    #[serde(default = "default_port")]
    pub proxy_port: u16,
    /// Serve the WebUI + proxy over HTTPS (self-signed cert is auto-generated
    /// when no custom cert/key is configured).
    #[serde(default)]
    pub tls_enabled: bool,
    /// Optional custom TLS certificate chain (PEM). Falls back to a generated
    /// self-signed cert when empty.
    #[serde(default)]
    pub tls_cert_path: Option<String>,
    /// Optional custom TLS private key (PEM).
    #[serde(default)]
    pub tls_key_path: Option<String>,
    /// Require a valid API key (Authorization: Bearer …) on /v1 endpoints.
    /// Off by default so existing "any key works" setups keep working.
    #[serde(default)]
    pub api_key_required: bool,
}

fn default_port() -> u16 {
    8765
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            proxy_port: default_port(),
            tls_enabled: false,
            tls_cert_path: None,
            tls_key_path: None,
            api_key_required: false,
        }
    }
}

impl Storage {
    pub fn new() -> Result<Arc<Self>> {
        let dirs = ProjectDirs::from(QUALIFIER, ORG, APP)
            .ok_or_else(|| BridgeError::Storage("cannot resolve project dirs".into()))?;
        Self::open(
            dirs.config_dir().to_path_buf(),
            dirs.data_dir().to_path_buf(),
        )
    }

    /// Construct a `Storage` rooted at the given directories without going
    /// through platform directory discovery. Used by integration tests.
    pub fn for_paths(config_dir: PathBuf, data_dir: PathBuf) -> Result<Arc<Self>> {
        Self::open(config_dir, data_dir)
    }

    fn open(config_dir: PathBuf, data_dir: PathBuf) -> Result<Arc<Self>> {
        fs::create_dir_all(&config_dir)?;
        fs::create_dir_all(&data_dir)?;

        let settings_path = config_dir.join("settings.json");
        let settings = if settings_path.exists() {
            let raw = fs::read_to_string(&settings_path)?;
            serde_json::from_str(&raw).unwrap_or_default()
        } else {
            Settings::default()
        };

        let me = Arc::new(Self {
            config_dir,
            data_dir,
            settings: RwLock::new(settings),
        });
        me.persist_settings()?;
        Ok(me)
    }

    pub fn settings(&self) -> Settings {
        self.settings.read().clone()
    }

    /// Directory holding `settings.json`; also where the auto-generated
    /// self-signed TLS cert/key are written.
    pub fn config_dir(&self) -> &std::path::Path {
        &self.config_dir
    }

    pub fn update_settings<F: FnOnce(&mut Settings)>(&self, f: F) -> Result<Settings> {
        let mut guard = self.settings.write();
        f(&mut guard);
        let snapshot = guard.clone();
        drop(guard);
        self.persist_settings()?;
        Ok(snapshot)
    }

    fn persist_settings(&self) -> Result<()> {
        let snapshot = self.settings.read().clone();
        let path = self.config_dir.join("settings.json");
        fs::write(path, serde_json::to_vec_pretty(&snapshot)?)?;
        Ok(())
    }

    /// Load a JSON blob from data dir; returns None if absent.
    pub fn load_blob<T: DeserializeOwned>(&self, name: &str) -> Result<Option<T>> {
        let path = self.data_dir.join(format!("{name}.json"));
        if !path.exists() {
            return Ok(None);
        }
        let raw = fs::read_to_string(path)?;
        Ok(Some(serde_json::from_str(&raw)?))
    }

    /// Persist a JSON blob to data dir.
    pub fn save_blob<T: Serialize>(&self, name: &str, value: &T) -> Result<()> {
        let path = self.data_dir.join(format!("{name}.json"));
        fs::write(path, serde_json::to_vec_pretty(value)?)?;
        Ok(())
    }

    pub fn delete_blob(&self, name: &str) -> Result<()> {
        let path = self.data_dir.join(format!("{name}.json"));
        if path.exists() {
            fs::remove_file(path)?;
        }
        Ok(())
    }
}
