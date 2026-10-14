//! SSH engine: connection lifecycle, interactive shell channels and batch exec.

use std::collections::HashMap;
use std::sync::{Arc, Mutex as StdMutex};
use std::time::{Duration, Instant};

use base64::Engine as _;
use russh::client::{self, Config as ClientConfig, Handle, Msg};
use russh::keys::agent::client::AgentClient;
use russh::keys::{load_secret_key, PrivateKeyWithHashAlg, PublicKeyOrCertificate};
use russh::{ChannelMsg, ChannelWriteHalf};
use serde::Serialize;
use tauri::{AppHandle, Emitter, Runtime};
use tokio::sync::{Mutex as TokioMutex, Semaphore};

use crate::models::{
    AuthMethod, BatchEvent, BatchRequest, BatchResult, BatchTarget, ConnectInfo, ExecResult,
    LiveSessionInfo, SessionConfig,
};
use crate::store::Store;

pub const EVT_SSH_DATA: &str = "ssh://data";
pub const EVT_SSH_STATE: &str = "ssh://state";
pub const EVT_BATCH: &str = "batch://event";

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshDataEvent {
    session_id: String,
    /// base64 encoded raw bytes coming from the remote pty.
    data: String,
}

#[derive(Clone, Serialize)]
#[serde(rename_all = "camelCase")]
struct SshStateEvent {
    session_id: String,
    /// `connected` | `closed` | `error`
    state: String,
    message: Option<String>,
}

/// Verifies the remote host key and remembers the fingerprint we observed.
pub struct ClientHandler {
    store: Arc<Store>,
    target: String,
    strict: bool,
    accept_new: bool,
    captured: Arc<StdMutex<Option<String>>>,
    banner: Arc<StdMutex<Option<String>>>,
}

impl client::Handler for ClientHandler {
    type Error = russh::Error;

    async fn check_server_key(
        &mut self,
        server_public_key: &PublicKeyOrCertificate,
    ) -> Result<bool, Self::Error> {
        let fp = fingerprint_of(server_public_key);
        {
            let mut slot = self.captured.lock().unwrap();
            *slot = Some(fp.clone());
        }
        if !self.strict {
            return Ok(true);
        }
        match self.store.known_host_fingerprint(&self.target) {
            Some(known) => Ok(known == fp),
            None => Ok(self.accept_new),
        }
    }

    async fn auth_banner(
        &mut self,
        banner: &str,
        _session: &mut client::Session,
    ) -> Result<(), Self::Error> {
        let mut slot = self.banner.lock().unwrap();
        *slot = Some(banner.to_string());
        Ok(())
    }
}

fn fingerprint_of(key: &PublicKeyOrCertificate) -> String {
    use russh::keys::HashAlg;
    match key {
        PublicKeyOrCertificate::PublicKey { key, .. } => {
            key.fingerprint(HashAlg::Sha256).to_string()
        }
        PublicKeyOrCertificate::Certificate(cert) => {
            cert.public_key().fingerprint(HashAlg::Sha256).to_string()
        }
    }
}

/// A live, authenticated SSH connection.
pub struct LiveSession {
    pub id: String,
    pub config: SessionConfig,
    /// `russh` handle. Guarded because opening channels needs `&self` and the
    /// guard is released immediately afterwards.
    pub handle: TokioMutex<Handle<ClientHandler>>,
    /// Write half of the interactive shell channel, when one is open.
    pub shell: TokioMutex<Option<ChannelWriteHalf<Msg>>>,
    pub fingerprint: Option<String>,
    pub connected_at: i64,
    pub exit_flag: StdMutex<bool>,
}

/// Owns every live connection and drives batch executions.
///
/// Generic over the tauri runtime so the engine can be exercised with
/// [`tauri::test::mock_app`] during integration tests.
pub struct SshManager<R: Runtime = tauri::Wry> {
    app: AppHandle<R>,
    store: Arc<Store>,
    live: StdMutex<HashMap<String, Arc<LiveSession>>>,
}

impl<R: Runtime> SshManager<R> {
    pub fn new(app: AppHandle<R>, store: Arc<Store>) -> Self {
        Self {
            app,
            store,
            live: StdMutex::new(HashMap::new()),
        }
    }

    pub fn is_connected(&self, id: &str) -> bool {
        self.live.lock().unwrap().contains_key(id)
    }

    pub fn connected_ids(&self) -> Vec<String> {
        self.live.lock().unwrap().keys().cloned().collect()
    }

    /// Snapshot of every live connection (used by the UI to resync).
    pub fn live_infos(&self) -> Vec<LiveSessionInfo> {
        self.live
            .lock()
            .unwrap()
            .values()
            .map(|live| LiveSessionInfo {
                session_id: live.id.clone(),
                name: live.config.name.clone(),
                host: live.config.host.clone(),
                port: live.config.port,
                username: live.config.username.clone(),
                connected_at: live.connected_at,
                host_key_fingerprint: live.fingerprint.clone(),
            })
            .collect()
    }

    pub fn get(&self, id: &str) -> Option<Arc<LiveSession>> {
        self.live.lock().unwrap().get(id).cloned()
    }

    /// Reuse an existing connection or establish a fresh one.
    pub async fn ensure(
        self: &Arc<Self>,
        config: &SessionConfig,
        password: Option<String>,
    ) -> Result<Arc<LiveSession>, String> {
        if self.is_connected(&config.id) {
            if let Some(live) = self.get(&config.id) {
                return Ok(live);
            }
        }
        let live = self.connect(config.clone(), password, false).await?;
        Ok(live)
    }

    /// Connect (optionally opening an interactive shell) and register the session.
    pub async fn connect(
        self: &Arc<Self>,
        config: SessionConfig,
        password: Option<String>,
        open_shell: bool,
    ) -> Result<Arc<LiveSession>, String> {
        // Drop a stale entry for the same id.
        self.disconnect(&config.id).await;

        let settings = self.store.settings();
        let target = format!("{}:{}", config.host, config.port);

        let captured = Arc::new(StdMutex::new(None));
        let banner = Arc::new(StdMutex::new(None));
        let handler = ClientHandler {
            store: Arc::clone(&self.store),
            target: target.clone(),
            strict: settings.strict_host_key_checking,
            accept_new: settings.accept_new_host_keys,
            captured: Arc::clone(&captured),
            banner: Arc::clone(&banner),
        };

        let cfg = ClientConfig {
            inactivity_timeout: Some(Duration::from_secs(3600)),
            keepalive_interval: Some(Duration::from_secs(settings.keepalive_interval_secs.max(5))),
            keepalive_max: 3,
            nodelay: true,
            ..Default::default()
        };

        let connect_timeout = Duration::from_secs(settings.connect_timeout_secs.clamp(3, 300));
        let mut handle = tokio::time::timeout(
            connect_timeout,
            client::connect(Arc::new(cfg), (config.host.as_str(), config.port), handler),
        )
        .await
        .map_err(|_| format!("连接超时 ({})", target))?
        .map_err(|e| map_connect_error(e, &target))?;

        authenticate(&mut handle, &config, password.as_deref()).await?;

        let fingerprint = captured.lock().unwrap().clone();
        if let Some(fp) = &fingerprint {
            let _ = self.store.remember_host(&target, fp);
        }

        let live = Arc::new(LiveSession {
            id: config.id.clone(),
            config: config.clone(),
            handle: TokioMutex::new(handle),
            shell: TokioMutex::new(None),
            fingerprint: fingerprint.clone(),
            connected_at: now_secs(),
            exit_flag: StdMutex::new(false),
        });

        {
            let mut map = self.live.lock().unwrap();
            map.insert(config.id.clone(), Arc::clone(&live));
        }

        let _ = self.app.emit(
            EVT_SSH_STATE,
            SshStateEvent {
                session_id: config.id.clone(),
                state: "connected".into(),
                message: None,
            },
        );

        if open_shell {
            if let Err(e) = self.open_shell(&live).await {
                let _ = self.app.emit(
                    EVT_SSH_STATE,
                    SshStateEvent {
                        session_id: config.id.clone(),
                        state: "error".into(),
                        message: Some(e.clone()),
                    },
                );
                return Err(e);
            }
        }

        let _ = banner;
        Ok(live)
    }

    pub fn connect_info(&self, live: &LiveSession, banner: Option<String>) -> ConnectInfo {
        ConnectInfo {
            session_id: live.id.clone(),
            host_key_fingerprint: live.fingerprint.clone(),
            server_banner: banner,
        }
    }

    /// Open a pty + interactive shell and pump remote output to the frontend.
    pub async fn open_shell(self: &Arc<Self>, live: &Arc<LiveSession>) -> Result<(), String> {
        if live.shell.lock().await.is_some() {
            return Ok(());
        }

        let channel = {
            let handle = live.handle.lock().await;
            handle
                .channel_open_session()
                .await
                .map_err(|e| format!("无法打开会话通道: {e}"))?
        };

        channel
            .request_pty(true, "xterm-256color", 120, 32, 0, 0, &[])
            .await
            .map_err(|e| format!("无法请求 pty: {e}"))?;
        channel
            .request_shell(true)
            .await
            .map_err(|e| format!("无法启动 shell: {e}"))?;

        let (mut reader, writer) = channel.split();
        *live.shell.lock().await = Some(writer);

        let app = self.app.clone();
        let session_id = live.id.clone();
        let weak = Arc::downgrade(live);
        let manager_live = Arc::clone(live);

        tokio::spawn(async move {
            let encoder = base64::engine::general_purpose::STANDARD;
            while let Some(msg) = reader.wait().await {
                match msg {
                    ChannelMsg::Data { data } => {
                        let _ = app.emit(
                            EVT_SSH_DATA,
                            SshDataEvent {
                                session_id: session_id.clone(),
                                data: encoder.encode(&data),
                            },
                        );
                    }
                    ChannelMsg::ExtendedData { data, .. } => {
                        let _ = app.emit(
                            EVT_SSH_DATA,
                            SshDataEvent {
                                session_id: session_id.clone(),
                                data: encoder.encode(&data),
                            },
                        );
                    }
                    ChannelMsg::Eof | ChannelMsg::Close => break,
                    _ => {}
                }
            }
            *manager_live.shell.lock().await = None;
            let _ = weak;
            let _ = app.emit(
                EVT_SSH_STATE,
                SshStateEvent {
                    session_id,
                    state: "closed".into(),
                    message: None,
                },
            );
        });

        Ok(())
    }

    /// Send keystrokes / pasted data to the remote shell.
    pub async fn write(&self, id: &str, data: Vec<u8>) -> Result<(), String> {
        let live = self.get(id).ok_or_else(|| "会话未连接".to_string())?;
        let guard = live.shell.lock().await;
        let writer = guard.as_ref().ok_or_else(|| "shell 未就绪".to_string())?;
        writer
            .data_bytes(data)
            .await
            .map_err(|e| format!("写入失败: {e}"))
    }

    /// Propagate a terminal resize to the remote pty.
    pub async fn resize(&self, id: &str, cols: u32, rows: u32) -> Result<(), String> {
        let live = self.get(id).ok_or_else(|| "会话未连接".to_string())?;
        let guard = live.shell.lock().await;
        let writer = guard.as_ref().ok_or_else(|| "shell 未就绪".to_string())?;
        writer
            .window_change(cols.max(1), rows.max(1), 0, 0)
            .await
            .map_err(|e| format!("调整窗口失败: {e}"))
    }

    /// Close a connection and forget it.
    pub async fn disconnect(&self, id: &str) {
        let removed = {
            let mut map = self.live.lock().unwrap();
            map.remove(id)
        };
        if let Some(live) = removed {
            {
                let mut flag = live.exit_flag.lock().unwrap();
                *flag = true;
            }
            let handle = live.handle.lock().await;
            let _ = handle
                .disconnect(russh::Disconnect::ByApplication, "bye", "")
                .await;
        }
    }

    pub async fn disconnect_all(&self) {
        let ids: Vec<String> = self.live.lock().unwrap().keys().cloned().collect();
        for id in ids {
            self.disconnect(&id).await;
        }
    }

    /// Run a one-shot command on an already connected session.
    pub async fn exec(
        self: &Arc<Self>,
        id: &str,
        command: &str,
        timeout: Duration,
    ) -> Result<ExecResult, String> {
        let live = self.get(id).ok_or_else(|| "会话未连接".to_string())?;
        run_exec(&self.app, &live, command, timeout, None).await
    }

    /// Execute the same script across many hosts, streaming results as they arrive.
    pub async fn batch_exec(
        self: &Arc<Self>,
        targets: Vec<BatchTarget>,
        request: BatchRequest,
    ) -> Result<Vec<BatchResult>, String> {
        let run_id = uuid::Uuid::new_v4().to_string();
        let total = targets.len();
        if total == 0 {
            return Ok(Vec::new());
        }

        let mut script = String::new();
        if request.stop_on_error {
            script.push_str("set -e\n");
        }
        for cmd in &request.commands {
            script.push_str(cmd);
            if !cmd.ends_with('\n') {
                script.push('\n');
            }
        }
        let script = Arc::new(script);

        let semaphore = Arc::new(Semaphore::new(request.concurrency.clamp(1, 64) as usize));
        let mut joins = Vec::with_capacity(total);

        for (index, target) in targets.into_iter().enumerate() {
            let this = Arc::clone(self);
            let sem = Arc::clone(&semaphore);
            let script = Arc::clone(&script);
            let request = request.clone();
            let run_id = run_id.clone();

            joins.push(tokio::spawn(async move {
                let _permit = sem.acquire().await;
                this.run_batch_target(&run_id, index, total, target, script, request)
                    .await
            }));
        }

        let mut results = Vec::with_capacity(total);
        for join in joins {
            match join.await {
                Ok(result) => results.push(result),
                Err(e) => results.push(BatchResult {
                    session_id: String::new(),
                    name: "unknown".into(),
                    host: String::new(),
                    username: String::new(),
                    success: false,
                    exit_code: None,
                    stdout: String::new(),
                    stderr: String::new(),
                    duration_ms: 0,
                    timed_out: false,
                    error: Some(format!("任务异常: {e}")),
                }),
            }
        }

        Ok(results)
    }

    async fn run_batch_target(
        self: &Arc<Self>,
        run_id: &str,
        index: usize,
        total: usize,
        target: BatchTarget,
        script: Arc<String>,
        request: BatchRequest,
    ) -> BatchResult {
        let Some(config) = self.store.session(&target.session_id) else {
            return BatchResult {
                session_id: target.session_id,
                name: "已删除的会话".into(),
                host: String::new(),
                username: String::new(),
                success: false,
                exit_code: None,
                stdout: String::new(),
                stderr: String::new(),
                duration_ms: 0,
                timed_out: false,
                error: Some("找不到该会话配置".into()),
            };
        };

        let base = BatchResult {
            session_id: config.id.clone(),
            name: config.name.clone(),
            host: config.host.clone(),
            username: config.username.clone(),
            success: false,
            exit_code: None,
            stdout: String::new(),
            stderr: String::new(),
            duration_ms: 0,
            timed_out: false,
            error: None,
        };

        let _ = self.app.emit(
            EVT_BATCH,
            BatchEvent {
                run_id: run_id.to_string(),
                session_id: config.id.clone(),
                kind: "start".into(),
                stream: None,
                data: None,
                result: None,
                index,
                total,
            },
        );

        let live = match self.ensure(&config, target.password).await {
            Ok(live) => live,
            Err(e) => {
                let failed = BatchResult {
                    success: false,
                    error: Some(e),
                    ..base
                };
                self.emit_batch_done(run_id, index, total, &failed);
                return failed;
            }
        };

        let timeout = Duration::from_secs(request.timeout_secs.clamp(1, 86_400));
        let ctx = BatchCtx {
            run_id: run_id.to_string(),
            session_id: config.id.clone(),
            index,
            total,
        };

        let result = match run_exec(&self.app, &live, &script, timeout, Some(ctx)).await {
            Ok(out) => BatchResult {
                success: out.exit_code.unwrap_or(0) == 0 && !out.timed_out,
                exit_code: out.exit_code,
                stdout: out.stdout,
                stderr: out.stderr,
                duration_ms: out.duration_ms,
                timed_out: out.timed_out,
                error: None,
                ..base
            },
            Err(e) => BatchResult {
                success: false,
                error: Some(e),
                ..base
            },
        };

        self.emit_batch_done(run_id, index, total, &result);
        result
    }

    fn emit_batch_done(&self, run_id: &str, index: usize, total: usize, result: &BatchResult) {
        let _ = self.app.emit(
            EVT_BATCH,
            BatchEvent {
                run_id: run_id.to_string(),
                session_id: result.session_id.clone(),
                kind: "done".into(),
                stream: None,
                data: None,
                result: Some(result.clone()),
                index,
                total,
            },
        );
    }
}

/// Context used to tag streamed batch output.
struct BatchCtx {
    run_id: String,
    session_id: String,
    index: usize,
    total: usize,
}

async fn run_exec<R: Runtime>(
    app: &AppHandle<R>,
    live: &LiveSession,
    command: &str,
    timeout: Duration,
    ctx: Option<BatchCtx>,
) -> Result<ExecResult, String> {
    let started = Instant::now();

    let channel = {
        let handle = live.handle.lock().await;
        handle
            .channel_open_session()
            .await
            .map_err(|e| format!("无法打开执行通道: {e}"))?
    };
    let mut channel = channel;
    channel
        .exec(true, command.as_bytes().to_vec())
        .await
        .map_err(|e| format!("无法下发命令: {e}"))?;

    let encoder = base64::engine::general_purpose::STANDARD;
    let mut stdout: Vec<u8> = Vec::new();
    let mut stderr: Vec<u8> = Vec::new();
    let mut exit_code: Option<u32> = None;
    let mut timed_out = false;
    let deadline = tokio::time::Instant::now() + timeout;

    loop {
        let remaining = deadline.saturating_duration_since(tokio::time::Instant::now());
        if remaining.is_zero() {
            timed_out = true;
            break;
        }
        match tokio::time::timeout(remaining, channel.wait()).await {
            Err(_) => {
                timed_out = true;
                break;
            }
            Ok(None) => break,
            Ok(Some(msg)) => match msg {
                ChannelMsg::Data { data } => {
                    stdout.extend_from_slice(&data);
                    if let Some(ctx) = &ctx {
                        let _ = app.emit(
                            EVT_BATCH,
                            BatchEvent {
                                run_id: ctx.run_id.clone(),
                                session_id: ctx.session_id.clone(),
                                kind: "output".into(),
                                stream: Some("stdout".into()),
                                data: Some(encoder.encode(&data)),
                                result: None,
                                index: ctx.index,
                                total: ctx.total,
                            },
                        );
                    }
                }
                ChannelMsg::ExtendedData { data, .. } => {
                    stderr.extend_from_slice(&data);
                    if let Some(ctx) = &ctx {
                        let _ = app.emit(
                            EVT_BATCH,
                            BatchEvent {
                                run_id: ctx.run_id.clone(),
                                session_id: ctx.session_id.clone(),
                                kind: "output".into(),
                                stream: Some("stderr".into()),
                                data: Some(encoder.encode(&data)),
                                result: None,
                                index: ctx.index,
                                total: ctx.total,
                            },
                        );
                    }
                }
                ChannelMsg::ExitStatus { exit_status } => exit_code = Some(exit_status),
                ChannelMsg::ExitSignal { .. } => exit_code = Some(255),
                ChannelMsg::Eof | ChannelMsg::Close => break,
                _ => {}
            },
        }
    }

    let _ = channel.close().await;

    Ok(ExecResult {
        stdout: String::from_utf8_lossy(&stdout).into_owned(),
        stderr: String::from_utf8_lossy(&stderr).into_owned(),
        exit_code,
        duration_ms: started.elapsed().as_millis() as u64,
        timed_out,
    })
}

async fn authenticate(
    handle: &mut Handle<ClientHandler>,
    config: &SessionConfig,
    password: Option<&str>,
) -> Result<(), String> {
    let user = config.username.clone();
    match &config.auth {
        AuthMethod::Password => {
            let pw = password
                .map(|s| s.to_string())
                .or_else(|| config.password.clone())
                .filter(|s| !s.is_empty())
                .ok_or_else(|| "缺少密码".to_string())?;
            let result = handle
                .authenticate_password(&user, pw)
                .await
                .map_err(|e| format!("密码认证失败: {e}"))?;
            if result.success() {
                Ok(())
            } else {
                Err("密码认证被拒绝".into())
            }
        }
        AuthMethod::Key { path, passphrase } => {
            let path = expand_tilde(path);
            if path.trim().is_empty() {
                return Err("未配置私钥路径".into());
            }
            let key = load_secret_key(&path, passphrase.as_deref())
                .map_err(|e| format!("读取私钥失败 ({path}): {e}"))?;
            let hash_alg = handle
                .best_supported_rsa_hash()
                .await
                .ok()
                .flatten()
                .flatten();
            let key = PrivateKeyWithHashAlg::new(Arc::new(key), hash_alg);
            let result = handle
                .authenticate_publickey(&user, key)
                .await
                .map_err(|e| format!("公钥认证失败: {e}"))?;
            if result.success() {
                Ok(())
            } else {
                Err("公钥认证被拒绝".into())
            }
        }
        AuthMethod::Agent => authenticate_agent(handle, &user).await,
    }
}

#[cfg(unix)]
async fn authenticate_agent(handle: &mut Handle<ClientHandler>, user: &str) -> Result<(), String> {
    let mut agent = AgentClient::connect_env()
        .await
        .map_err(|e| format!("无法连接 ssh-agent: {e}"))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| format!("无法读取 agent 身份: {e}"))?;
    agent_auth_with(handle, user, &mut agent, identities).await
}

#[cfg(windows)]
async fn authenticate_agent(handle: &mut Handle<ClientHandler>, user: &str) -> Result<(), String> {
    let mut agent = AgentClient::connect_named_pipe(r"\\.\pipe\openssh-ssh-agent")
        .await
        .map_err(|e| format!("无法连接 ssh-agent: {e}"))?;
    let identities = agent
        .request_identities()
        .await
        .map_err(|e| format!("无法读取 agent 身份: {e}"))?;
    agent_auth_with(handle, user, &mut agent, identities).await
}

async fn agent_auth_with<S>(
    handle: &mut Handle<ClientHandler>,
    user: &str,
    agent: &mut S,
    identities: Vec<russh::keys::agent::AgentIdentity>,
) -> Result<(), String>
where
    S: russh::Signer + Send,
    S::Error: std::fmt::Display,
{
    if identities.is_empty() {
        return Err("ssh-agent 中没有可用的身份".into());
    }
    let mut last_err = String::from("ssh-agent 认证被拒绝");
    for identity in identities {
        let public = identity.public_key().into_owned();
        match handle
            .authenticate_publickey_with(user.to_string(), public, None, agent)
            .await
        {
            Ok(result) if result.success() => return Ok(()),
            Ok(_) => last_err = "ssh-agent 认证被拒绝".into(),
            Err(e) => last_err = format!("ssh-agent 认证失败: {e}"),
        }
    }
    Err(last_err)
}

fn expand_tilde(path: &str) -> String {
    if let Some(rest) = path.strip_prefix("~/") {
        if let Ok(home) = std::env::var("HOME").or_else(|_| std::env::var("USERPROFILE")) {
            return format!("{home}/{rest}");
        }
    }
    path.to_string()
}

fn map_connect_error(err: russh::Error, target: &str) -> String {
    match err {
        russh::Error::IO(ref e) => format!("无法连接 {target}: {e}"),
        russh::Error::NoCommonAlgo { .. } => format!("与 {target} 没有共同支持的算法"),
        russh::Error::ConnectionTimeout => format!("连接 {target} 超时"),
        other => format!("连接 {target} 失败: {other}"),
    }
}

fn now_secs() -> i64 {
    std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(0)
}
