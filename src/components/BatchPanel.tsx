import { useMemo, useState } from "react";
import {
  Play,
  Eraser,
  Download,
  ChevronDown,
  ChevronRight,
  CheckCircle2,
  XCircle,
  Loader2,
  Clock,
  Copy,
  Layers,
  Search,
} from "lucide-react";

import { useAppStore } from "../store/useAppStore";
import type { BatchTargetState, Session } from "../types";

const STATUS_META: Record<
  BatchTargetState["status"],
  { label: string; className: string }
> = {
  pending: { label: "等待", className: "st-pending" },
  running: { label: "执行中", className: "st-running" },
  success: { label: "成功", className: "st-success" },
  failure: { label: "失败", className: "st-failure" },
};

function StatusIcon({ status }: { status: BatchTargetState["status"] }) {
  switch (status) {
    case "running":
      return <Loader2 size={14} className="spin" />;
    case "success":
      return <CheckCircle2 size={14} />;
    case "failure":
      return <XCircle size={14} />;
    default:
      return <Clock size={14} />;
  }
}

function ResultCard({ target }: { target: BatchTargetState }) {
  const [open, setOpen] = useState(target.status === "failure");
  const meta = STATUS_META[target.status];
  const output = [target.stdout, target.stderr && `[stderr]\n${target.stderr}`]
    .filter(Boolean)
    .join("\n");

  return (
    <div className={`result-card ${meta.className}`}>
      <div className="result-head" onClick={() => setOpen((v) => !v)}>
        <button className="icon-btn tiny">
          {open ? <ChevronDown size={14} /> : <ChevronRight size={14} />}
        </button>
        <span className={`result-status ${meta.className}`}>
          <StatusIcon status={target.status} />
          {meta.label}
        </span>
        <div className="result-name">
          <strong>{target.name}</strong>
          <span className="muted">
            {target.username}@{target.host}
          </span>
        </div>
        <div className="result-metrics">
          {target.exitCode !== null && <span>exit {target.exitCode}</span>}
          <span>{target.durationMs} ms</span>
          <button
            className="icon-btn tiny"
            title="复制输出"
            onClick={(e) => {
              e.stopPropagation();
              void navigator.clipboard.writeText(output);
            }}
          >
            <Copy size={13} />
          </button>
        </div>
      </div>
      {open && (
        <div className="result-body">
          {target.error && <div className="result-error">{target.error}</div>}
          {target.timedOut && <div className="result-error">执行超时</div>}
          <pre>{output || "（无输出）"}</pre>
        </div>
      )}
    </div>
  );
}

function TargetPicker({ sessions }: { sessions: Session[] }) {
  const batchForm = useAppStore((s) => s.batchForm);
  const setBatchForm = useAppStore((s) => s.setBatchForm);
  const connected = useAppStore((s) => s.connected);
  const [query, setQuery] = useState("");

  const selected = useMemo(() => new Set(batchForm.selected), [batchForm.selected]);

  const filtered = useMemo(() => {
    const q = query.trim().toLowerCase();
    if (!q) return sessions;
    return sessions.filter(
      (s) =>
        s.name.toLowerCase().includes(q) ||
        s.host.toLowerCase().includes(q) ||
        (s.group ?? "").toLowerCase().includes(q),
    );
  }, [sessions, query]);

  function toggle(id: string) {
    const next = new Set(selected);
    if (next.has(id)) next.delete(id);
    else next.add(id);
    setBatchForm({ selected: [...next] });
  }

  return (
    <aside className="batch-targets">
      <div className="batch-targets-head">
        <span>目标主机</span>
        <span className="badge">{selected.size}/{sessions.length}</span>
      </div>
      <div className="batch-targets-tools">
        <button className="link" onClick={() => setBatchForm({ selected: sessions.map((s) => s.id) })}>
          全选
        </button>
        <button className="link" onClick={() => setBatchForm({ selected: [] })}>
          清空
        </button>
        <button
          className="link"
          onClick={() =>
            setBatchForm({ selected: sessions.filter((s) => !selected.has(s.id)).map((s) => s.id) })
          }
        >
          反选
        </button>
      </div>
      <div className="search-box compact">
        <Search size={13} />
        <input value={query} placeholder="筛选" onChange={(e) => setQuery(e.target.value)} />
      </div>
      <div className="batch-target-list">
        {filtered.map((s) => {
          const status = connected[s.id]?.status;
          return (
            <label className="target-row" key={s.id}>
              <input
                type="checkbox"
                checked={selected.has(s.id)}
                onChange={() => toggle(s.id)}
              />
              <span className="session-color" style={{ background: s.color || "#38bdf8" }} />
              <span className="target-name">{s.name}</span>
              <span className="muted target-host">{s.host}</span>
              <span className={`status-dot status-${status ?? "idle"}`} />
            </label>
          );
        })}
        {filtered.length === 0 && <div className="empty-hint">没有匹配的主机</div>}
      </div>
    </aside>
  );
}

export function BatchPanel() {
  const sessions = useAppStore((s) => s.sessions);
  const batchForm = useAppStore((s) => s.batchForm);
  const setBatchForm = useAppStore((s) => s.setBatchForm);
  const batch = useAppStore((s) => s.batch);
  const runBatch = useAppStore((s) => s.runBatch);
  const clearBatch = useAppStore((s) => s.clearBatch);

  const selectedCount = batchForm.selected.length;
  const commandCount = batchForm.commands
    .split("\n")
    .map((c) => c.trim())
    .filter(Boolean).length;

  const summary = useMemo(() => {
    const success = batch.targets.filter((t) => t.status === "success").length;
    const failure = batch.targets.filter((t) => t.status === "failure").length;
    const running = batch.targets.filter(
      (t) => t.status === "running" || t.status === "pending",
    ).length;
    return { success, failure, running };
  }, [batch.targets]);

  function exportResults() {
    const lines: string[] = [];
    lines.push(`# FreeShell 批量执行结果  ${new Date().toLocaleString()}`);
    lines.push(`# 主机数: ${batch.targets.length}`);
    lines.push("");
    for (const t of batch.targets) {
      lines.push("=".repeat(70));
      lines.push(`## ${t.name}  ${t.username}@${t.host}  [${STATUS_META[t.status].label}]`);
      if (t.exitCode !== null) lines.push(`exit code: ${t.exitCode}`);
      lines.push(`duration: ${t.durationMs} ms`);
      if (t.error) lines.push(`error: ${t.error}`);
      if (t.stdout.trim()) lines.push("--- stdout ---\n" + t.stdout.trimEnd());
      if (t.stderr.trim()) lines.push("--- stderr ---\n" + t.stderr.trimEnd());
      lines.push("");
    }
    const blob = new Blob([lines.join("\n")], { type: "text/plain;charset=utf-8" });
    const url = URL.createObjectURL(blob);
    const a = document.createElement("a");
    a.href = url;
    a.download = `freeshell-batch-${Date.now()}.txt`;
    a.click();
    URL.revokeObjectURL(url);
  }

  const canRun = !batch.running && selectedCount > 0 && commandCount > 0;

  return (
    <div className="batch">
      <div className="batch-top">
        <TargetPicker sessions={sessions} />

        <section className="batch-config">
          <div className="batch-config-head">
            <Layers size={14} />
            <span>批量命令</span>
            <span className="muted">每行一条，按顺序在每台主机上执行</span>
          </div>

          <textarea
            className="batch-editor"
            spellCheck={false}
            value={batchForm.commands}
            placeholder={"uptime\nfree -m\ndf -h /"}
            onChange={(e) => setBatchForm({ commands: e.target.value })}
          />

          <div className="batch-options">
            <label>
              <span>超时（秒）</span>
              <input
                type="number"
                value={batchForm.timeoutSecs}
                onChange={(e) => setBatchForm({ timeoutSecs: Number(e.target.value) })}
              />
            </label>
            <label>
              <span>并发数</span>
              <input
                type="number"
                value={batchForm.concurrency}
                onChange={(e) => setBatchForm({ concurrency: Number(e.target.value) })}
              />
            </label>
            <label className="grow">
              <span>统一密码（未保存密码的主机使用）</span>
              <input
                type="password"
                value={batchForm.password}
                placeholder="可选"
                onChange={(e) => setBatchForm({ password: e.target.value })}
              />
            </label>
            <label className="checkbox inline">
              <input
                type="checkbox"
                checked={batchForm.stopOnError}
                onChange={(e) => setBatchForm({ stopOnError: e.target.checked })}
              />
              <span>失败即停止（set -e）</span>
            </label>
          </div>

          <div className="batch-run-row">
            <button
              className="btn btn-primary"
              disabled={!canRun}
              onClick={() => void runBatch()}
            >
              <Play size={14} /> 在 {selectedCount} 台主机执行
            </button>
            <button
              className="btn"
              disabled={batch.running || batch.targets.length === 0}
              onClick={clearBatch}
            >
              <Eraser size={14} /> 清空结果
            </button>
            <button
              className="btn"
              disabled={batch.targets.length === 0}
              onClick={exportResults}
            >
              <Download size={14} /> 导出
            </button>
            {batch.running && (
              <span className="running-hint">
                <Loader2 size={14} className="spin" /> 执行中…
              </span>
            )}
          </div>
        </section>
      </div>

      <div className="batch-results">
        <div className="batch-results-head">
          <span>执行结果</span>
          <div className="batch-summary">
            <span className="chip success">成功 {summary.success}</span>
            <span className="chip failure">失败 {summary.failure}</span>
            <span className="chip pending">进行中 {summary.running}</span>
            {batch.finishedAt && batch.startedAt && (
              <span className="muted">
                耗时 {((batch.finishedAt - batch.startedAt) / 1000).toFixed(1)}s
              </span>
            )}
          </div>
        </div>
        <div className="result-list">
          {batch.targets.length === 0 && (
            <div className="empty-hint">
              选择左侧主机，填写命令后点击执行；结果将实时显示在这里。
            </div>
          )}
          {batch.targets.map((t) => (
            <ResultCard key={t.sessionId} target={t} />
          ))}
        </div>
      </div>
    </div>
  );
}
