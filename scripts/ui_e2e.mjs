//! Full-app end-to-end driver.
//!
//! Runs *inside* the real Tauri window (via the Chrome DevTools Protocol), so
//! everything below exercises the shipping code path: React components, the
//! zustand store, the Tauri IPC layer, the Rust SSH engine and a real SSH
//! connection to `examples/fake_ssh_server.rs`.
//!
//! Usage: node scripts/ui_e2e.mjs   (see scripts/ui_e2e.ps1 for the wrapper)

const PORT = Number(process.env.CDP_PORT || 9222);
const SESSION_ID = process.env.E2E_SESSION_ID || "e2e-local";

const DRIVER = `(async () => {
  const SESSION_ID = ${JSON.stringify(SESSION_ID)};
  const sleep = (ms) => new Promise((r) => setTimeout(r, ms));
  const invoke = (cmd, args) => window.__TAURI_INTERNALS__.invoke(cmd, args);
  const text = (sel) => document.querySelector(sel)?.innerText?.replace(/\\s+/g, ' ').trim() ?? '';
  const allText = (sel) => [...document.querySelectorAll(sel)].map((e) => e.innerText ?? '').join('\\n');
  const checks = [];
  const record = (name, ok, detail) => checks.push({ name, ok: !!ok, detail: detail ?? null });

  const until = async (fn, timeout = 20000, step = 150) => {
    const started = Date.now();
    for (;;) {
      let value = null;
      try {
        value = fn();
      } catch {
        value = null;
      }
      if (value) return value;
      if (Date.now() - started > timeout) return null;
      await sleep(step);
    }
  };

  const setField = (el, value) => {
    const proto =
      el instanceof HTMLTextAreaElement ? HTMLTextAreaElement.prototype : HTMLInputElement.prototype;
    Object.getOwnPropertyDescriptor(proto, 'value').set.call(el, value);
    el.dispatchEvent(new Event('input', { bubbles: true }));
  };

  const findButton = (scope, label) =>
    [...document.querySelectorAll(scope + ' button')].find((b) => (b.innerText || '').includes(label));

  // --- the app is mounted and loading from disk ---------------------------
  record('app mounted', await until(() => document.querySelector('#root')?.children.length > 0));
  const item = await until(() =>
    [...document.querySelectorAll('.session-item')].find((el) =>
      el.innerText.includes(SESSION_ID),
    ),
  );
  record('seeded session rendered in sidebar', !!item, item ? item.innerText.replace(/\\s+/g, ' ').trim() : 'not found');
  if (!item) return { checks };

  // --- connect by double clicking the session -----------------------------
  item.dispatchEvent(new MouseEvent('dblclick', { bubbles: true }));
  const connected = await until(() => text('.statusbar').includes('已连接 1'), 25000);
  record('status bar reports one live session', !!connected, text('.statusbar'));
  record('terminal tab opened', document.querySelectorAll('.tab').length >= 1);

  const banner = await until(() => allText('.xterm-rows').includes('FreeShell fake SSH server ready'), 25000);
  record('shell banner streamed into the terminal', !!banner, allText('.xterm-rows').slice(0, 160));

  // --- keystrokes travel through ssh_write --------------------------------
  await invoke('ssh_write', { id: SESSION_ID, data: btoa('echo via-terminal\\r') });
  const echoed = await until(() => allText('.xterm-rows').includes('via-terminal'), 20000);
  record('typed command echoed back by the remote shell', !!echoed);

  // --- one-shot exec ------------------------------------------------------
  const exec = await invoke('ssh_exec', { id: SESSION_ID, command: 'echo via-ipc', timeoutSecs: 10 });
  record('ssh_exec captures stdout', exec.stdout.includes('via-ipc'), JSON.stringify(exec));
  record('ssh_exec reports exit code 0', exec.exitCode === 0, String(exec.exitCode));

  const multi = await invoke('ssh_exec', { id: SESSION_ID, command: 'echo first\\nhostname', timeoutSecs: 10 });
  record('ssh_exec runs multi-line scripts', multi.stdout.includes('first') && multi.stdout.includes('fake-host'), JSON.stringify(multi.stdout));

  const failed = await invoke('ssh_exec', { id: SESSION_ID, command: 'fail', timeoutSecs: 10 });
  record('failing command surfaces stderr + exit code', failed.exitCode === 1 && failed.stderr.includes('boom'), JSON.stringify(failed));

  const timedOut = await invoke('ssh_exec', { id: SESSION_ID, command: 'sleep 30', timeoutSecs: 1 });
  record('ssh_exec enforces the timeout', timedOut.timedOut === true, JSON.stringify(timedOut));

  // --- live session introspection ----------------------------------------
  const live = await invoke('ssh_sessions');
  record('ssh_sessions lists the live connection', Array.isArray(live) && live.length === 1 && live[0].sessionId === SESSION_ID, JSON.stringify(live));

  const known = await invoke('list_known_hosts');
  record('host key was remembered (TOFU)', Object.keys(known).some((k) => k.includes('127.0.0.1')), JSON.stringify(known));

  await invoke('ssh_resize', { id: SESSION_ID, cols: 100, rows: 30 });
  record('ssh_resize accepted', true);

  // --- batch execution straight through the IPC layer --------------------
  const batch = await invoke('batch_exec', {
    targets: [{ sessionId: SESSION_ID }],
    request: { commands: ['echo batch-ipc', 'hostname'], timeoutSecs: 15, concurrency: 2, stopOnError: false },
  });
  record('batch_exec succeeds on the target', batch[0]?.success === true, JSON.stringify(batch[0]?.error ?? null));
  record('batch_exec aggregates stdout', (batch[0]?.stdout ?? '').includes('batch-ipc') && (batch[0]?.stdout ?? '').includes('fake-host'), JSON.stringify(batch[0]?.stdout));

  const batchFail = await invoke('batch_exec', {
    targets: [{ sessionId: SESSION_ID }],
    request: { commands: ['fail'], timeoutSecs: 15, concurrency: 1, stopOnError: true },
  });
  record('batch_exec reports failures', batchFail[0]?.success === false && batchFail[0]?.exitCode === 1, JSON.stringify(batchFail[0]));

  // --- the same batch flow through the actual UI -------------------------
  findButton('.sidebar-actions', '批量')?.click();
  await until(() => document.querySelector('.batch-editor'));
  const row = await until(() =>
    [...document.querySelectorAll('.target-row')].find((el) => el.innerText.includes(SESSION_ID)),
  );
  record('batch panel lists the session', !!row);
  const checkbox = row?.querySelector('input[type=checkbox]');
  if (checkbox && !checkbox.checked) checkbox.click();
  const editor = await until(() => document.querySelector('.batch-editor'));
  setField(editor, 'echo ui-batch\\nhostname');
  await sleep(150);
  record('run button reflects the selection', text('.batch-run-row .btn-primary').includes('1 台主机'), text('.batch-run-row .btn-primary'));
  findButton('.batch-run-row', '执行')?.click();
  const card = await until(() => document.querySelector('.result-card'), 25000);
  record('batch UI renders a result card', !!card, card ? card.innerText.replace(/\\s+/g, ' ').slice(0, 120) : 'no card');
  record('batch UI card reports success', !!card && card.className.includes('success'), card?.className ?? '');

  // The output is collapsed by default, expand it like a user would.
  card?.querySelector('.result-head')?.click();
  const body = await until(() => {
    const value = document.querySelector('.result-card .result-body')?.innerText ?? '';
    return value.includes('ui-batch') ? value : null;
  }, 8000);
  record('batch UI renders the command output', !!body && body.includes('fake-host'), (body ?? '').replace(/\\s+/g, ' ').slice(0, 200));
  record('batch UI summary counts the successes', text('.batch-summary').includes('成功 1'), text('.batch-summary'));

  // --- disconnect ---------------------------------------------------------
  await invoke('ssh_disconnect', { id: SESSION_ID });
  const gone = await until(() => text('.statusbar').includes('已连接 0'), 15000);
  record('disconnect updates the status bar', !!gone, text('.statusbar'));
  const liveAfter = await invoke('ssh_connected');
  record('no live sessions left', Array.isArray(liveAfter) && liveAfter.length === 0, JSON.stringify(liveAfter));

  return { checks };
})()`;

function connect(url) {
  return new Promise((resolve, reject) => {
    const ws = new WebSocket(url);
    ws.addEventListener("open", () => resolve(ws), { once: true });
    ws.addEventListener("error", () => reject(new Error("websocket error")), { once: true });
  });
}

async function main() {
  const targets = await (await fetch(`http://127.0.0.1:${PORT}/json/list`)).json();
  const page = targets.find((t) => t.type === "page");
  if (!page) throw new Error("no debuggable page found");
  if (!page.url.startsWith("http://tauri.localhost")) {
    throw new Error(`unexpected page url: ${page.url} (did the window pick up the vite dev server?)`);
  }

  const ws = await connect(page.webSocketDebuggerUrl);
  let nextId = 0;
  const pending = new Map();
  const consoleErrors = [];

  ws.addEventListener("message", (event) => {
    const msg = JSON.parse(event.data);
    if (msg.id && pending.has(msg.id)) {
      pending.get(msg.id)(msg);
      pending.delete(msg.id);
      return;
    }
    if (msg.method === "Runtime.exceptionThrown") {
      consoleErrors.push(msg.params.exceptionDetails?.exception?.description ?? "exception");
    }
    if (msg.method === "Runtime.consoleAPICalled" && msg.params.type === "error") {
      consoleErrors.push(msg.params.args.map((a) => a.value ?? a.description).join(" "));
    }
  });

  const send = (method, params = {}) =>
    new Promise((resolve) => {
      const id = ++nextId;
      pending.set(id, resolve);
      ws.send(JSON.stringify({ id, method, params }));
    });

  await send("Runtime.enable");
  await new Promise((r) => setTimeout(r, 800));

  const response = await send("Runtime.evaluate", {
    expression: DRIVER,
    awaitPromise: true,
    returnByValue: true,
  });

  if (response.result?.exceptionDetails) {
    console.error("driver threw:", JSON.stringify(response.result.exceptionDetails, null, 2));
    ws.close();
    process.exitCode = 1;
    return;
  }

  const checks = response.result?.result?.value?.checks ?? [];
  let failed = 0;
  for (const check of checks) {
    if (!check.ok) failed += 1;
    const suffix = check.detail ? `  → ${check.detail}` : "";
    console.log(`${check.ok ? "PASS" : "FAIL"}  ${check.name}${suffix}`);
  }
  console.log(`\n${checks.length - failed}/${checks.length} UI end-to-end checks passed`);
  if (consoleErrors.length) {
    console.log(`console errors: ${JSON.stringify(consoleErrors, null, 2)}`);
  }
  if (failed || consoleErrors.length) process.exitCode = 1;
  ws.close();
}

main().catch((err) => {
  console.error("FAILED:", err.message);
  process.exitCode = 1;
});
