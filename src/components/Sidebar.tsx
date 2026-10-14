import { useMemo, useState } from "react";
import {
  Search,
  Plus,
  Pencil,
  Trash2,
  Settings as SettingsIcon,
  Layers,
  Terminal,
  Server,
  KeyRound,
} from "lucide-react";

import { useAppStore } from "../store/useAppStore";
import type { Session } from "../types";

interface Props {
  onConnect: (session: Session) => void;
  onNewSession: () => void;
  onEditSession: (session: Session) => void;
  onOpenSettings: () => void;
}

function authLabel(session: Session): string {
  switch (session.auth.type) {
    case "key":
      return "公钥";
    case "agent":
      return "Agent";
    default:
      return "密码";
  }
}

export function Sidebar({ onConnect, onNewSession, onEditSession, onOpenSettings }: Props) {
  const sessions = useAppStore((s) => s.sessions);
  const connected = useAppStore((s) => s.connected);
  const deleteSession = useAppStore((s) => s.deleteSession);
  const openBatchTab = useAppStore((s) => s.openBatchTab);

  const [query, setQuery] = useState("");

  const grouped = useMemo(() => {
    const q = query.trim().toLowerCase();
    const filtered = sessions.filter((s) => {
      if (!q) return true;
      return (
        s.name.toLowerCase().includes(q) ||
        s.host.toLowerCase().includes(q) ||
        s.username.toLowerCase().includes(q) ||
        (s.group ?? "").toLowerCase().includes(q) ||
        s.tags.some((t) => t.toLowerCase().includes(q))
      );
    });
    const map = new Map<string, Session[]>();
    for (const s of filtered) {
      const key = s.group?.trim() || "未分组";
      const list = map.get(key) ?? [];
      list.push(s);
      map.set(key, list);
    }
    return [...map.entries()].sort((a, b) => a[0].localeCompare(b[0]));
  }, [sessions, query]);

  return (
    <aside className="sidebar">
      <div className="sidebar-head">
        <div className="brand">
          <Terminal size={16} />
          <span>FreeShell</span>
        </div>
        <button className="icon-btn" title="设置" onClick={onOpenSettings}>
          <SettingsIcon size={16} />
        </button>
      </div>

      <div className="sidebar-actions">
        <button className="btn btn-primary" onClick={onNewSession}>
          <Plus size={14} /> 新建会话
        </button>
        <button className="btn" onClick={openBatchTab} title="批量执行">
          <Layers size={14} /> 批量
        </button>
      </div>

      <div className="search-box">
        <Search size={14} />
        <input
          value={query}
          placeholder="搜索主机 / 用户 / 分组"
          onChange={(e) => setQuery(e.target.value)}
        />
      </div>

      <div className="session-list">
        {grouped.length === 0 && (
          <div className="empty-hint">
            还没有会话，点击「新建会话」添加一台主机。
          </div>
        )}
        {grouped.map(([group, items]) => (
          <div className="group" key={group}>
            <div className="group-title">{group}</div>
            {items.map((session) => {
              const status = connected[session.id]?.status;
              return (
                <div
                  className="session-item"
                  key={session.id}
                  onDoubleClick={() => onConnect(session)}
                  title={`${session.username}@${session.host}:${session.port}`}
                >
                  <span
                    className="session-color"
                    style={{ background: session.color || "#38bdf8" }}
                  />
                  <div className="session-meta">
                    <div className="session-name">{session.name}</div>
                    <div className="session-sub">
                      <Server size={11} />
                      {session.host}:{session.port}
                      <span className="dot-sep">·</span>
                      <KeyRound size={11} />
                      {authLabel(session)}
                    </div>
                  </div>
                  <span className={`status-dot status-${status ?? "idle"}`} />
                  <div className="session-actions">
                    <button
                      className="icon-btn tiny"
                      title="编辑"
                      onClick={(e) => {
                        e.stopPropagation();
                        onEditSession(session);
                      }}
                    >
                      <Pencil size={13} />
                    </button>
                    <button
                      className="icon-btn tiny danger"
                      title="删除"
                      onClick={(e) => {
                        e.stopPropagation();
                        void deleteSession(session.id);
                      }}
                    >
                      <Trash2 size={13} />
                    </button>
                  </div>
                </div>
              );
            })}
          </div>
        ))}
      </div>
    </aside>
  );
}
