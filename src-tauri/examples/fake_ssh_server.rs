//! A tiny local SSH server used to exercise FreeShell end-to-end.
//!
//! It is intentionally dependency-free (no `clap`, no extra crates) so it can be
//! started straight from the workspace:
//!
//! ```text
//! cargo run --manifest-path src-tauri/Cargo.toml --example fake_ssh_server -- --port 2222
//! ```
//!
//! Behaviour:
//! * accepts exactly one user/password pair (defaults `tester` / `s3cret`);
//! * speaks a POSIX-ish shell: prompt, local echo, backspace, Ctrl-C and a
//!   handful of built-in commands (`echo`, `whoami`, `hostname`, `uname`,
//!   `pwd`, `stderr`, `sleep`, `fail`, `exit`);
//! * anything else is handed to the host shell (`cmd /c` on Windows, `sh -c`
//!   elsewhere) so real commands can be used for batch tests.
//!
//! It prints `LISTENING <addr>` on stdout once the socket is bound, which makes
//! it easy to wait for readiness from a script.

use std::collections::HashMap;
use std::net::SocketAddr;
use std::sync::Arc;
use std::time::Duration;

use russh::keys::{Algorithm, PrivateKey};
use russh::server::{Auth, ChannelOpenHandle, Msg, Server as _, Session};
use russh::{Channel, ChannelId};
use tokio::net::TcpListener;

const BANNER: &str = "FreeShell fake SSH server ready.\r\n";

#[derive(Clone)]
struct FakeServer {
    user: String,
    password: String,
}

struct FakeHandler {
    user: String,
    password: String,
    /// Pending, not yet submitted input per channel (the "line editor").
    buffers: HashMap<ChannelId, String>,
    /// Echo the client's keystrokes back, like a real pty would.
    echo: bool,
}

impl russh::server::Server for FakeServer {
    type Handler = FakeHandler;

    fn new_client(&mut self, _peer: Option<SocketAddr>) -> FakeHandler {
        FakeHandler {
            user: self.user.clone(),
            password: self.password.clone(),
            buffers: HashMap::new(),
            echo: std::env::var("FAKE_SSH_ECHO")
                .map(|v| v != "0")
                .unwrap_or(true),
        }
    }
}

impl russh::server::Handler for FakeHandler {
    type Error = russh::Error;

    async fn auth_password(&mut self, user: &str, password: &str) -> Result<Auth, Self::Error> {
        if user == self.user && password == self.password {
            Ok(Auth::Accept)
        } else {
            Ok(Auth::reject())
        }
    }

    async fn channel_open_session(
        &mut self,
        channel: Channel<Msg>,
        reply: ChannelOpenHandle,
        _session: &mut Session,
    ) -> Result<(), Self::Error> {
        self.buffers.insert(channel.id(), String::new());
        reply.accept().await;
        Ok(())
    }

    #[allow(clippy::too_many_arguments)]
    async fn pty_request(
        &mut self,
        channel: ChannelId,
        _term: &str,
        _col_width: u32,
        _row_height: u32,
        _pix_width: u32,
        _pix_height: u32,
        _modes: &[(russh::Pty, u32)],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        Ok(())
    }

    async fn shell_request(
        &mut self,
        channel: ChannelId,
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        session.channel_success(channel)?;
        let welcome = format!("{BANNER}Welcome, {}.\r\n$ ", self.user);
        session.data(channel, welcome.into_bytes())?;
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

        let (stdout, stderr, code) = run_script(&script).await;
        if !stdout.is_empty() {
            session.data(channel, stdout.into_bytes())?;
        }
        if !stderr.is_empty() {
            session.extended_data(channel, 1, stderr.into_bytes())?;
        }
        session.exit_status_request(channel, code)?;
        session.eof(channel)?;
        session.close(channel)?;
        Ok(())
    }

    async fn data(
        &mut self,
        channel: ChannelId,
        data: &[u8],
        session: &mut Session,
    ) -> Result<(), Self::Error> {
        let mut out: Vec<u8> = Vec::new();
        let mut submitted: Vec<String> = Vec::new();
        let mut quit = false;

        {
            let buffer = self.buffers.entry(channel).or_default();
            for byte in data {
                match *byte {
                    // Ctrl-C: drop the pending line.
                    3 => {
                        buffer.clear();
                        out.extend_from_slice(b"^C\r\n$ ");
                    }
                    // Backspace / delete.
                    8 | 127 => {
                        if buffer.pop().is_some() {
                            out.extend_from_slice(b"\x08 \x08");
                        }
                    }
                    b'\r' | b'\n' => {
                        let line = std::mem::take(buffer);
                        out.extend_from_slice(b"\r\n");
                        submitted.push(line);
                    }
                    byte => {
                        buffer.push(byte as char);
                        if self.echo {
                            out.push(byte);
                        }
                    }
                }
            }
        }

        for line in submitted {
            match line.trim() {
                "" => {}
                "exit" | "logout" => {
                    quit = true;
                }
                other => {
                    let (stdout, stderr, _) = run_script(other).await;
                    out.extend_from_slice(stdout.as_bytes());
                    if !stderr.is_empty() {
                        session.extended_data(channel, 1, stderr.into_bytes())?;
                    }
                }
            }
            if !quit {
                out.extend_from_slice(b"$ ");
            }
        }

        if !out.is_empty() {
            session.data(channel, out)?;
        }
        if quit {
            session.eof(channel)?;
            session.close(channel)?;
        }
        Ok(())
    }
}

/// Runs a (possibly multi-line) script, mirroring how a remote shell would.
///
/// `set -e` is honoured the way `/bin/sh` would: the script stops at the first
/// failing command. The app prepends it for "stop on error" batch runs.
async fn run_script(script: &str) -> (String, String, u32) {
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
        Some((cmd, arg)) => (cmd, arg.trim()),
        None => (line, ""),
    };

    match cmd {
        "echo" => (format!("{arg}\r\n"), String::new(), 0),
        "whoami" => ("tester\r\n".to_string(), String::new(), 0),
        "hostname" => ("fake-host\r\n".to_string(), String::new(), 0),
        "pwd" => ("/home/tester\r\n".to_string(), String::new(), 0),
        "uname" => (
            "Linux fake-host 6.1.0-fake #1 SMP x86_64 GNU/Linux\r\n".to_string(),
            String::new(),
            0,
        ),
        "stderr" => (String::new(), format!("{arg}\r\n"), 0),
        "sleep" => {
            let secs: f64 = arg.parse().unwrap_or(1.0);
            tokio::time::sleep(Duration::from_secs_f64(secs)).await;
            (String::new(), String::new(), 0)
        }
        "fail" => (String::new(), "boom\r\n".to_string(), 1),
        "env" => (
            "SHELL=/bin/bash\r\nTERM=xterm-256color\r\n".to_string(),
            String::new(),
            0,
        ),
        "exit" => (String::new(), String::new(), arg.parse().unwrap_or(0)),
        other => run_host_shell(other).await,
    }
}

/// Falls back to the platform shell for everything that is not a built-in.
async fn run_host_shell(command: &str) -> (String, String, u32) {
    #[cfg(windows)]
    let mut cmd = {
        let mut c = tokio::process::Command::new("cmd");
        c.arg("/c").arg(command);
        c
    };
    #[cfg(not(windows))]
    let mut cmd = {
        let mut c = tokio::process::Command::new("sh");
        c.arg("-c").arg(command);
        c
    };

    match cmd.output().await {
        Ok(output) => (
            String::from_utf8_lossy(&output.stdout).into_owned(),
            String::from_utf8_lossy(&output.stderr).into_owned(),
            output.status.code().unwrap_or(0) as u32,
        ),
        Err(err) => (String::new(), format!("{err}\r\n"), 127),
    }
}

fn parse_arg(args: &[String], flag: &str, fallback: &str) -> String {
    args.iter()
        .position(|a| a == flag)
        .and_then(|i| args.get(i + 1))
        .cloned()
        .unwrap_or_else(|| fallback.to_string())
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let port: u16 = parse_arg(&args, "--port", "2222").parse().unwrap_or(2222);
    let user = parse_arg(&args, "--user", "tester");
    let password = parse_arg(&args, "--password", "s3cret");

    let runtime = tokio::runtime::Builder::new_multi_thread()
        .worker_threads(2)
        .enable_all()
        .build()
        .expect("tokio runtime");

    runtime.block_on(async move {
        let config = russh::server::Config {
            inactivity_timeout: Some(Duration::from_secs(3600)),
            auth_rejection_time: Duration::from_millis(50),
            auth_rejection_time_initial: Some(Duration::from_millis(0)),
            keys: vec![PrivateKey::random(&mut rand::rng(), Algorithm::Ed25519).expect("host key")],
            ..Default::default()
        };

        let listener = TcpListener::bind(("127.0.0.1", port))
            .await
            .unwrap_or_else(|e| panic!("failed to bind 127.0.0.1:{port}: {e}"));
        println!("LISTENING {}", listener.local_addr().expect("addr"));
        use std::io::Write as _;
        let _ = std::io::stdout().flush();

        let mut server = FakeServer { user, password };
        if let Err(err) = server.run_on_socket(Arc::new(config), &listener).await {
            eprintln!("server stopped: {err}");
        }
    });
}
