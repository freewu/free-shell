//! End-to-end tests for the SSH engine.
//!
//! A miniature in-process SSH server is started on a random loopback port and
//! driven through the real [`SshManager`], so the whole path — key exchange,
//! password auth, host-key verification, `exec` channels and batch fan-out —
//! is exercised without needing an external host.

use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use free_shell_lib::models::{AuthMethod, BatchRequest, BatchTarget, SessionConfig, Settings};
use free_shell_lib::ssh::SshManager;
use free_shell_lib::store::Store;
use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Server, Session};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;

const USER: &str = "tester";
const PASSWORD: &str = "s3cret";

// --------------------------------------------------------------------------
// A tiny SSH server that understands a handful of fake shell commands.
// --------------------------------------------------------------------------

struct TestServer;

struct TestHandler;

impl Server for TestServer {
    type Handler = TestHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> TestHandler {
        TestHandler
    }
}

impl russh::server::Handler for TestHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == USER && password == PASSWORD {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        _channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        reply.accept().await;
        Ok(())
    }

    async fn exec_request(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let script = String::from_utf8_lossy(data).into_owned();
        session.channel_success(channel)?;
        let handle = session.handle();

        tokio::spawn(async move {
            let (stdout, stderr, code) = interpret(&script).await;
            if !stdout.is_empty() {
                let _ = handle.data(channel, stdout.into_bytes()).await;
            }
            if !stderr.is_empty() {
                let _ = handle.extended_data(channel, 1, stderr.into_bytes()).await;
            }
            let _ = handle.exit_status_request(channel, code).await;
            let _ = handle.eof(channel).await;
            let _ = handle.close(channel).await;
        });

        Ok(())
    }
}

/// Runs a multi-line script, honouring `set -e`.
async fn interpret(script: &str) -> (String, String, u32) {
    let mut stdout = String::new();
    let mut stderr = String::new();
    let mut code = 0u32;
    let mut stop_on_error = false;

    for raw in script.split('\n') {
        let line = raw.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if line == "set -e" {
            stop_on_error = true;
            continue;
        }

        let (out, err, status) = run_line(line).await;
        stdout.push_str(&out);
        stderr.push_str(&err);
        if status != 0 {
            code = status;
            if stop_on_error {
                break;
            }
        }
    }

    (stdout, stderr, code)
}

async fn run_line(line: &str) -> (String, String, u32) {
    let (cmd, arg) = match line.split_once(' ') {
        Some((c, a)) => (c, a.trim()),
        None => (line, ""),
    };

    match cmd {
        "echo" => (format!("{arg}\n"), String::new(), 0),
        "whoami" => (format!("{USER}\n"), String::new(), 0),
        "hostname" => ("test-host\n".to_string(), String::new(), 0),
        "stderr" => (String::new(), format!("{arg}\n"), 0),
        "sleep" => {
            let secs: f64 = arg.parse().unwrap_or(1.0);
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            (String::new(), String::new(), 0)
        }
        "fail" => (String::new(), "boom\n".to_string(), 1),
        "exit" => (String::new(), String::new(), arg.parse().unwrap_or(0)),
        other => (format!("unknown: {other}\n"), String::new(), 127),
    }
}

/// Boots the test server on an ephemeral port and returns its address.
fn start_test_server() -> SocketAddr {
    let (tx, rx) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let runtime = tokio::runtime::Builder::new_multi_thread()
            .worker_threads(2)
            .enable_all()
            .build()
            .expect("server runtime");

        runtime.block_on(async move {
            let config = russh::server::Config {
                inactivity_timeout: Some(Duration::from_secs(60)),
                auth_rejection_time: Duration::from_millis(10),
                auth_rejection_time_initial: Some(Duration::from_millis(0)),
                keys: vec![
                    PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("host key")
                ],
                ..Default::default()
            };

            let listener = TcpListener::bind(("127.0.0.1", 0)).await.expect("bind");
            tx.send(listener.local_addr().unwrap()).unwrap();

            let mut server = TestServer;
            let _ = server.run_on_socket(Arc::new(config), &listener).await;
        });
    });

    rx.recv().expect("server address")
}

// --------------------------------------------------------------------------
// Helpers
// --------------------------------------------------------------------------

fn temp_store() -> Arc<Store> {
    let dir = std::env::temp_dir().join(format!("freeshell-test-{}", uuid::Uuid::new_v4()));
    Arc::new(Store::new(dir).expect("store"))
}

fn manager(store: &Arc<Store>) -> Arc<SshManager<tauri::test::MockRuntime>> {
    let app = tauri::test::mock_app();
    let handle = app.handle().clone();
    Arc::new(SshManager::new(handle, Arc::clone(store)))
}

fn profile(id: &str, name: &str, addr: SocketAddr, password: Option<&str>) -> SessionConfig {
    SessionConfig {
        id: id.to_string(),
        name: name.to_string(),
        host: addr.ip().to_string(),
        port: addr.port(),
        username: USER.to_string(),
        group: None,
        color: None,
        description: None,
        auth: AuthMethod::Password,
        password: password.map(|p| p.to_string()),
        tags: Vec::new(),
    }
}

// --------------------------------------------------------------------------
// Tests
// --------------------------------------------------------------------------

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn connect_and_exec_captures_stdout_stderr_and_exit_code() {
    let addr = start_test_server();
    let store = temp_store();
    let manager = manager(&store);

    let cfg = profile("s1", "Test host", addr, Some(PASSWORD));
    let live = manager
        .connect(cfg.clone(), Some(PASSWORD.to_string()), false)
        .await
        .expect("connect");
    assert_eq!(live.id, "s1");

    let result = manager
        .exec(
            &cfg.id,
            "echo hello\nstderr problem\nwhoami",
            Duration::from_secs(10),
        )
        .await
        .expect("exec");

    assert!(
        result.stdout.contains("hello"),
        "stdout: {:?}",
        result.stdout
    );
    assert!(result.stdout.contains(USER), "stdout: {:?}", result.stdout);
    assert!(
        result.stderr.contains("problem"),
        "stderr: {:?}",
        result.stderr
    );
    assert_eq!(result.exit_code, Some(0));
    assert!(!result.timed_out);

    manager.disconnect(&cfg.id).await;
    assert!(!manager.is_connected(&cfg.id));
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn wrong_password_is_rejected() {
    let addr = start_test_server();
    let store = temp_store();
    let manager = manager(&store);

    let cfg = profile("s1", "Test host", addr, Some("nope"));
    let err = manager
        .connect(cfg, Some("nope".to_string()), false)
        .await
        .err()
        .expect("must fail");
    assert!(err.contains("密码") || err.contains("认证"), "error: {err}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn timeouts_are_reported() {
    let addr = start_test_server();
    let store = temp_store();
    let manager = manager(&store);

    let cfg = profile("s1", "Test host", addr, Some(PASSWORD));
    manager
        .connect(cfg.clone(), Some(PASSWORD.to_string()), false)
        .await
        .expect("connect");

    let result = manager
        .exec(&cfg.id, "sleep 30", Duration::from_secs(1))
        .await
        .expect("exec");
    assert!(result.timed_out, "expected timeout, got {result:?}");
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn host_keys_are_verified_and_remembered() {
    let addr = start_test_server();
    let store = temp_store();

    // Strict mode + unknown host ⇒ refused, and nothing is persisted.
    store
        .save_settings(Settings {
            strict_host_key_checking: true,
            accept_new_host_keys: false,
            ..Default::default()
        })
        .unwrap();

    let manager = manager(&store);
    let cfg = profile("s1", "Test host", addr, Some(PASSWORD));
    assert!(manager
        .connect(cfg.clone(), Some(PASSWORD.to_string()), false)
        .await
        .is_err());
    assert!(store.known_hosts().is_empty());

    // Trust-on-first-use ⇒ accepted, fingerprint recorded, later runs match.
    store
        .save_settings(Settings {
            strict_host_key_checking: true,
            accept_new_host_keys: true,
            ..Default::default()
        })
        .unwrap();

    let target = format!("{}:{}", cfg.host, cfg.port);
    let live = manager
        .connect(cfg.clone(), Some(PASSWORD.to_string()), false)
        .await
        .expect("connect with tofu");
    let fingerprint = live.fingerprint.clone().expect("fingerprint");
    assert_eq!(
        store.known_host_fingerprint(&target).as_deref(),
        Some(fingerprint.as_str())
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_runs_on_every_target_and_streams_results() {
    let addr = start_test_server();
    let store = temp_store();
    let manager = manager(&store);

    let mut targets = Vec::new();
    for i in 0..3 {
        let cfg = profile(
            &format!("host-{i}"),
            &format!("Host {i}"),
            addr,
            Some(PASSWORD),
        );
        store.upsert_session(cfg.clone()).unwrap();
        targets.push(BatchTarget {
            session_id: cfg.id,
            password: None,
        });
    }

    let results = manager
        .batch_exec(
            targets,
            BatchRequest {
                commands: vec!["echo batch-ok".into(), "hostname".into()],
                timeout_secs: 15,
                concurrency: 2,
                stop_on_error: false,
            },
        )
        .await
        .expect("batch");

    assert_eq!(results.len(), 3);
    for r in &results {
        assert!(r.success, "target failed: {r:?}");
        assert_eq!(r.exit_code, Some(0));
        assert!(r.stdout.contains("batch-ok"), "stdout: {:?}", r.stdout);
        assert!(r.stdout.contains("test-host"), "stdout: {:?}", r.stdout);
        assert_eq!(r.host, addr.ip().to_string());
    }

    let names: Vec<_> = results.iter().map(|r| r.name.as_str()).collect();
    assert!(names.contains(&"Host 0") && names.contains(&"Host 2"));

    // Connections are pooled, so every host stays live after the run.
    assert_eq!(manager.connected_ids().len(), 3);
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_reports_failures_without_stopping_others() {
    let addr = start_test_server();
    let store = temp_store();
    let manager = manager(&store);

    let good = profile("good", "Good host", addr, Some(PASSWORD));
    let bad = profile("bad", "Bad host", addr, Some("wrong-password"));
    store.upsert_session(good.clone()).unwrap();
    store.upsert_session(bad.clone()).unwrap();

    let results = manager
        .batch_exec(
            vec![
                BatchTarget {
                    session_id: good.id.clone(),
                    password: None,
                },
                BatchTarget {
                    session_id: bad.id.clone(),
                    password: None,
                },
            ],
            BatchRequest {
                commands: vec!["echo ok".into()],
                timeout_secs: 15,
                concurrency: 8,
                stop_on_error: false,
            },
        )
        .await
        .expect("batch");

    let good_result = results.iter().find(|r| r.session_id == "good").unwrap();
    let bad_result = results.iter().find(|r| r.session_id == "bad").unwrap();
    assert!(good_result.success);
    assert!(!bad_result.success);
    assert!(bad_result.error.is_some());

    // `stopOnError` aborts the script at the first failing line.
    let stop = manager
        .batch_exec(
            vec![BatchTarget {
                session_id: good.id.clone(),
                password: None,
            }],
            BatchRequest {
                commands: vec!["fail".into(), "echo unreachable".into()],
                timeout_secs: 15,
                concurrency: 1,
                stop_on_error: true,
            },
        )
        .await
        .expect("batch");
    assert!(!stop[0].success);
    assert_eq!(stop[0].exit_code, Some(1));
    assert!(
        !stop[0].stdout.contains("unreachable"),
        "stdout: {:?}",
        stop[0].stdout
    );
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn batch_skips_unknown_sessions() {
    let store = temp_store();
    let manager = manager(&store);

    let results = manager
        .batch_exec(
            vec![BatchTarget {
                session_id: "does-not-exist".into(),
                password: None,
            }],
            BatchRequest {
                commands: vec!["echo hi".into()],
                timeout_secs: 5,
                concurrency: 1,
                stop_on_error: false,
            },
        )
        .await
        .expect("batch");

    assert_eq!(results.len(), 1);
    assert!(!results[0].success);
    assert!(results[0].error.as_deref().unwrap_or("").contains("会话"));
}
