//! Serializable data models shared with the frontend.

use serde::{Deserialize, Serialize};

/// A saved SSH connection profile.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SessionConfig {
    pub id: String,
    pub name: String,
    pub host: String,
    #[serde(default = "default_port")]
    pub port: u16,
    pub username: String,
    #[serde(default)]
    pub group: Option<String>,
    #[serde(default)]
    pub color: Option<String>,
    #[serde(default)]
    pub description: Option<String>,
    #[serde(default)]
    pub auth: AuthMethod,
    /// Only meaningful when `auth` is [`AuthMethod::Password`].
    #[serde(default)]
    pub password: Option<String>,
    /// Free-form tags used for filtering / grouping.
    #[serde(default)]
    pub tags: Vec<String>,
}

fn default_port() -> u16 {
    22
}

/// How to authenticate against the remote host.
#[derive(Debug, Clone, Serialize, Deserialize, Default, PartialEq, Eq)]
#[serde(tag = "type", rename_all = "camelCase")]
pub enum AuthMethod {
    /// Username + password (password lives on [`SessionConfig::password`]).
    #[default]
    Password,
    /// Public key file on disk.
    Key {
        #[serde(default)]
        path: String,
        #[serde(default)]
        passphrase: Option<String>,
    },
    /// Delegate to a running ssh-agent (OpenSSH agent / Pageant).
    Agent,
}

/// Application settings persisted next to the session list.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Settings {
    /// When true the server key must be present in `known_hosts.json`.
    pub strict_host_key_checking: bool,
    /// When true, an unknown server key is remembered automatically (TOFU).
    pub accept_new_host_keys: bool,
    pub connect_timeout_secs: u64,
    pub keepalive_interval_secs: u64,
    pub terminal_font_size: u32,
    pub terminal_font_family: String,
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            strict_host_key_checking: false,
            accept_new_host_keys: true,
            connect_timeout_secs: 20,
            keepalive_interval_secs: 30,
            terminal_font_size: 14,
            terminal_font_family: "JetBrains Mono, Cascadia Mono, Consolas, Menlo, monospace"
                .to_string(),
        }
    }
}

/// One host participating in a batch run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchTarget {
    pub session_id: String,
    /// Optional ad-hoc password used when the stored profile has none.
    #[serde(default)]
    pub password: Option<String>,
}

/// Parameters for a batch execution run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchRequest {
    /// Commands executed top-to-bottom on every target.
    pub commands: Vec<String>,
    #[serde(default = "default_timeout")]
    pub timeout_secs: u64,
    #[serde(default = "default_concurrency")]
    pub concurrency: u32,
    /// Prepend `set -e` so the script aborts on the first failure.
    #[serde(default)]
    pub stop_on_error: bool,
}

fn default_timeout() -> u64 {
    60
}

fn default_concurrency() -> u32 {
    8
}

/// Result of a single command execution.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExecResult {
    pub stdout: String,
    pub stderr: String,
    pub exit_code: Option<u32>,
    pub duration_ms: u64,
    pub timed_out: bool,
}

/// Per-target outcome of a batch run.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchResult {
    pub session_id: String,
    pub name: String,
    pub host: String,
    pub username: String,
    pub success: bool,
    pub exit_code: Option<u32>,
    pub stdout: String,
    pub stderr: String,
    pub duration_ms: u64,
    pub timed_out: bool,
    pub error: Option<String>,
}

/// Connection state reported back to the UI.
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ConnectInfo {
    pub session_id: String,
    pub host_key_fingerprint: Option<String>,
    pub server_banner: Option<String>,
}

/// Read-only snapshot of a live connection.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct LiveSessionInfo {
    pub session_id: String,
    pub name: String,
    pub host: String,
    pub port: u16,
    pub username: String,
    pub connected_at: i64,
    pub host_key_fingerprint: Option<String>,
}

/// Streaming update emitted while a batch run is in flight.
#[derive(Debug, Clone, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct BatchEvent {
    pub run_id: String,
    pub session_id: String,
    /// `start` | `output` | `done`
    pub kind: String,
    /// `stdout` | `stderr` — only set for `output`.
    pub stream: Option<String>,
    /// base64 encoded payload — only set for `output`.
    pub data: Option<String>,
    /// Only set for `done`.
    pub result: Option<BatchResult>,
    pub index: usize,
    pub total: usize,
}
