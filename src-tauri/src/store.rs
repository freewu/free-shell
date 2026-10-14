//! File backed persistence for profiles, settings and known hosts.
//!
//! Everything lives in the platform specific app-data directory as plain JSON
//! so it can be inspected / version controlled by the user.

use std::collections::BTreeMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::RwLock;

use serde::{Deserialize, Serialize};
use tauri::{AppHandle, Manager};

use crate::models::{SessionConfig, Settings};

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnownHost {
    pub fingerprint: String,
    pub added_at: i64,
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}

#[derive(Default)]
struct Inner {
    sessions: Vec<SessionConfig>,
    known_hosts: BTreeMap<String, KnownHost>,
    settings: Settings,
}

/// Thread-safe, disk-backed store.
pub struct Store {
    dir: PathBuf,
    inner: RwLock<Inner>,
}

impl Store {
    /// Create a store rooted at the application data directory.
    pub fn from_app(app: &AppHandle) -> Result<Self, String> {
        let dir = app
            .path()
            .app_data_dir()
            .map_err(|e| format!("cannot resolve app data dir: {e}"))?;
        Self::new(dir)
    }

    pub fn new(dir: PathBuf) -> Result<Self, String> {
        fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {e}", dir.display()))?;

        let sessions =
            read_json::<Vec<SessionConfig>>(&dir.join("sessions.json"))?.unwrap_or_default();
        let known_hosts = read_json::<BTreeMap<String, KnownHost>>(&dir.join("known_hosts.json"))?
            .unwrap_or_default();
        let settings = read_json::<Settings>(&dir.join("settings.json"))?.unwrap_or_default();

        Ok(Self {
            dir,
            inner: RwLock::new(Inner {
                sessions,
                known_hosts,
                settings,
            }),
        })
    }

    pub fn data_dir(&self) -> &Path {
        &self.dir
    }

    // ---- sessions ---------------------------------------------------------

    pub fn sessions(&self) -> Vec<SessionConfig> {
        self.inner.read().unwrap().sessions.clone()
    }

    pub fn session(&self, id: &str) -> Option<SessionConfig> {
        self.inner
            .read()
            .unwrap()
            .sessions
            .iter()
            .find(|s| s.id == id)
            .cloned()
    }

    pub fn upsert_session(&self, cfg: SessionConfig) -> Result<(), String> {
        {
            let mut inner = self.inner.write().unwrap();
            match inner.sessions.iter_mut().find(|s| s.id == cfg.id) {
                Some(slot) => *slot = cfg,
                None => inner.sessions.push(cfg),
            }
        }
        self.flush_sessions()
    }

    pub fn delete_session(&self, id: &str) -> Result<(), String> {
        {
            let mut inner = self.inner.write().unwrap();
            inner.sessions.retain(|s| s.id != id);
        }
        self.flush_sessions()
    }

    fn flush_sessions(&self) -> Result<(), String> {
        let sessions = self.inner.read().unwrap().sessions.clone();
        write_json(&self.dir.join("sessions.json"), &sessions)
    }

    // ---- settings ---------------------------------------------------------

    pub fn settings(&self) -> Settings {
        self.inner.read().unwrap().settings.clone()
    }

    pub fn save_settings(&self, settings: Settings) -> Result<(), String> {
        {
            let mut inner = self.inner.write().unwrap();
            inner.settings = settings.clone();
        }
        write_json(&self.dir.join("settings.json"), &settings)
    }

    // ---- known hosts ------------------------------------------------------

    pub fn known_host_fingerprint(&self, target: &str) -> Option<String> {
        self.inner
            .read()
            .unwrap()
            .known_hosts
            .get(target)
            .map(|k| k.fingerprint.clone())
    }

    pub fn remember_host(&self, target: &str, fingerprint: &str) -> Result<(), String> {
        {
            let mut inner = self.inner.write().unwrap();
            inner.known_hosts.insert(
                target.to_string(),
                KnownHost {
                    fingerprint: fingerprint.to_string(),
                    added_at: now_secs(),
                },
            );
        }
        let known = self.inner.read().unwrap().known_hosts.clone();
        write_json(&self.dir.join("known_hosts.json"), &known)
    }

    pub fn known_hosts(&self) -> BTreeMap<String, KnownHost> {
        self.inner.read().unwrap().known_hosts.clone()
    }
}

fn read_json<T: for<'de> Deserialize<'de>>(path: &Path) -> Result<Option<T>, String> {
    match fs::read_to_string(path) {
        Ok(raw) => {
            // Windows editors (and PowerShell's `Set-Content -Encoding utf8`)
            // happily prepend a BOM, which serde_json rejects.
            let raw = raw.trim_start_matches('\u{feff}');
            if raw.trim().is_empty() {
                return Ok(None);
            }
            match serde_json::from_str(raw) {
                Ok(value) => Ok(Some(value)),
                Err(err) => {
                    // A hand-edited or half-written file must never keep the app
                    // from starting: move it aside and fall back to defaults.
                    let backup = quarantine(path);
                    eprintln!(
                        "[store] cannot parse {}: {err} (moved to {})",
                        path.display(),
                        backup.display()
                    );
                    Ok(None)
                }
            }
        }
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => Ok(None),
        Err(e) => Err(format!("cannot read {}: {e}", path.display())),
    }
}

/// Renames an unreadable file out of the way, returning its new path.
fn quarantine(path: &Path) -> PathBuf {
    let mut candidate = path.with_extension("corrupt");
    let mut n = 1;
    while candidate.exists() {
        candidate = path.with_extension(format!("corrupt{n}"));
        n += 1;
    }
    if fs::rename(path, &candidate).is_ok() {
        candidate
    } else {
        path.to_path_buf()
    }
}

fn write_json<T: Serialize>(path: &Path, value: &T) -> Result<(), String> {
    let raw = serde_json::to_string_pretty(value).map_err(|e| e.to_string())?;
    let tmp = path.with_extension("json.tmp");
    fs::write(&tmp, raw).map_err(|e| format!("cannot write {}: {e}", tmp.display()))?;
    fs::rename(&tmp, path).map_err(|e| format!("cannot replace {}: {e}", path.display()))?;
    Ok(())
}
