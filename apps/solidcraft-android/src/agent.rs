//! The agent chat's host services on Android: API keys kept on the device, and HTTPS for the model
//! endpoint. There is no system keychain here (Android has none the desktop `keyring` crate reaches),
//! and no `solidcraft-cli` to start, so the tools run in this process (see
//! `solidcraft_ui_egui::agent_chat::api_client`).

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use solidcraft_ui_egui::agent_chat::backend::{HttpFn, HttpRequest, Secrets};

/// API keys, one per agent, in a JSON file of the app's own, readable only by it (`0600`).
pub struct FileSecrets {
    path: PathBuf,
}

impl FileSecrets {
    /// `dir`: the app's files folder (the same one the fonts and the recovery folder live in).
    pub fn new(dir: &Path) -> Arc<FileSecrets> {
        let dir = dir.join("agent-chat");
        if let Err(e) = std::fs::create_dir_all(&dir) {
            log::error!("agent chat: cannot make {}: {e}", dir.display());
        }
        Arc::new(FileSecrets { path: dir.join("keys.json") })
    }

    fn load(&self) -> Result<BTreeMap<String, String>, String> {
        match std::fs::read_to_string(&self.path) {
            Ok(text) => serde_json::from_str(&text).map_err(|e| format!("{}: {e}", self.path.display())),
            Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(BTreeMap::new()),
            Err(e) => Err(format!("{}: {e}", self.path.display())),
        }
    }

    /// Write the whole file through a temporary one, so a crash mid-write cannot lose every key.
    fn store(&self, keys: &BTreeMap<String, String>) -> Result<(), String> {
        let text = serde_json::to_string_pretty(keys).map_err(|e| e.to_string())?;
        let tmp = self.path.with_extension("json.new");
        std::fs::write(&tmp, text).map_err(|e| format!("{}: {e}", tmp.display()))?;
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            let _ = std::fs::set_permissions(&tmp, std::fs::Permissions::from_mode(0o600));
        }
        std::fs::rename(&tmp, &self.path).map_err(|e| format!("{}: {e}", self.path.display()))
    }
}

impl Secrets for FileSecrets {
    fn get(&self, id: &str) -> Result<Option<String>, String> {
        Ok(self.load()?.get(id).cloned().filter(|k| !k.is_empty()))
    }
    fn set(&self, id: &str, secret: &str) -> Result<(), String> {
        let mut keys = self.load()?;
        keys.insert(id.to_string(), secret.to_string());
        self.store(&keys)
    }
    fn delete(&self, id: &str) -> Result<(), String> {
        let mut keys = self.load()?;
        keys.remove(id);
        self.store(&keys)
    }
}

/// HTTPS for the API agents (OpenAI-compatible endpoints). The certificates are compiled in
/// (`RootCerts::WebPki`): `rustls-platform-verifier` reads Android's store through Java classes this
/// shell does not carry, and a stale bundle is better than no chat at all.
pub fn http() -> HttpFn {
    let tls = ureq::tls::TlsConfig::builder()
        .root_certs(ureq::tls::RootCerts::WebPki)
        .unversioned_rustls_crypto_provider(Arc::new(rustls::crypto::ring::default_provider()))
        .build();
    let agent: ureq::Agent = ureq::Agent::config_builder()
        .tls_config(tls)
        .http_status_as_error(false)
        .timeout_global(Some(std::time::Duration::from_secs(300)))
        .user_agent("SolidCraft-agent-chat")
        .build()
        .into();
    Arc::new(move |r: &HttpRequest| {
        let answer = if r.method == "POST" {
            let mut request = agent.post(&r.url);
            for (k, v) in &r.headers {
                request = request.header(k.as_str(), v.as_str());
            }
            request.send(r.body.as_deref().unwrap_or_default())
        } else {
            let mut request = agent.get(&r.url);
            for (k, v) in &r.headers {
                request = request.header(k.as_str(), v.as_str());
            }
            request.call()
        };
        let mut answer = answer.map_err(|e| e.to_string())?;
        let status = answer.status().as_u16();
        let body = answer.body_mut().with_config().limit(16 << 20).read_to_vec().map_err(|e| e.to_string())?;
        Ok((status, body))
    })
}
