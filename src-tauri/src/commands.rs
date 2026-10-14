//! Thin `#[tauri::command]` wrappers exposed to the React frontend.

use std::sync::Arc;
use std::time::Duration;

use base64::Engine as _;
use tauri::State;

use crate::models::{
    BatchRequest, BatchResult, ConnectInfo, ExecResult, LiveSessionInfo, SessionConfig, Settings,
};
use crate::ssh::SshManager;
use crate::store::{KnownHost, Store};

type StoreState<'a> = State<'a, Arc<Store>>;
type ManagerState<'a> = State<'a, Arc<SshManager>>;

// ---------------------------------------------------------------------------
// Profiles / settings
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn list_sessions(store: StoreState<'_>) -> Vec<SessionConfig> {
    store.sessions()
}

#[tauri::command]
pub fn save_session(store: StoreState<'_>, session: SessionConfig) -> Result<(), String> {
    store.upsert_session(session)
}

#[tauri::command]
pub fn delete_session(store: StoreState<'_>, id: String) -> Result<(), String> {
    store.delete_session(&id)
}

#[tauri::command]
pub fn get_settings(store: StoreState<'_>) -> Settings {
    store.settings()
}

#[tauri::command]
pub fn save_settings(store: StoreState<'_>, settings: Settings) -> Result<(), String> {
    store.save_settings(settings)
}

#[tauri::command]
pub fn data_dir(store: StoreState<'_>) -> String {
    store.data_dir().display().to_string()
}

// ---------------------------------------------------------------------------
// Connection lifecycle
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn ssh_connect(
    manager: ManagerState<'_>,
    session: SessionConfig,
    password: Option<String>,
    open_shell: Option<bool>,
) -> Result<ConnectInfo, String> {
    let live = manager
        .connect(session, password, open_shell.unwrap_or(true))
        .await?;
    Ok(manager.connect_info(&live, None))
}

#[tauri::command]
pub async fn ssh_disconnect(manager: ManagerState<'_>, id: String) -> Result<(), String> {
    manager.disconnect(&id).await;
    Ok(())
}

#[tauri::command]
pub fn ssh_connected(manager: ManagerState<'_>) -> Vec<String> {
    manager.connected_ids()
}

#[tauri::command]
pub fn ssh_sessions(manager: ManagerState<'_>) -> Vec<LiveSessionInfo> {
    manager.live_infos()
}

#[tauri::command]
pub fn list_known_hosts(store: StoreState<'_>) -> std::collections::BTreeMap<String, KnownHost> {
    store.known_hosts()
}

#[tauri::command]
pub async fn ssh_write(manager: ManagerState<'_>, id: String, data: String) -> Result<(), String> {
    let bytes = base64::engine::general_purpose::STANDARD
        .decode(data.as_bytes())
        .map_err(|e| format!("数据解码失败: {e}"))?;
    manager.write(&id, bytes).await
}

#[tauri::command]
pub async fn ssh_resize(
    manager: ManagerState<'_>,
    id: String,
    cols: u32,
    rows: u32,
) -> Result<(), String> {
    manager.resize(&id, cols, rows).await
}

#[tauri::command]
pub async fn ssh_exec(
    manager: ManagerState<'_>,
    id: String,
    command: String,
    timeout_secs: Option<u64>,
) -> Result<ExecResult, String> {
    let timeout = Duration::from_secs(timeout_secs.unwrap_or(60).clamp(1, 86_400));
    manager.exec(&id, &command, timeout).await
}

// ---------------------------------------------------------------------------
// Batch execution
// ---------------------------------------------------------------------------

#[tauri::command]
pub async fn batch_exec(
    manager: ManagerState<'_>,
    targets: Vec<crate::models::BatchTarget>,
    request: BatchRequest,
) -> Result<Vec<BatchResult>, String> {
    manager.batch_exec(targets, request).await
}
