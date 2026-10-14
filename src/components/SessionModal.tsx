import { useEffect, useState } from "react";
import { open as openDialog } from "@tauri-apps/plugin-dialog";
import { Eye, EyeOff, FolderOpen, X } from "lucide-react";

import { useAppStore } from "../store/useAppStore";
import type { AuthMethod, Session } from "../types";

interface Props {
  session: Session | null;
  onClose: () => void;
}

const COLORS = ["#98d98e", "#34d399", "#fbbf24", "#f87171", "#c084fc", "#22d3ee"];

function blankSession(): Session {
  return {
    id: crypto.randomUUID(),
    name: "",
    host: "",
    port: 22,
    username: "root",
    group: "",
    color: COLORS[0],
    description: "",
    auth: { type: "password" },
    password: "",
    tags: [],
  };
}

export function SessionModal({ session, onClose }: Props) {
  const saveSession = useAppStore((s) => s.saveSession);
  const existing = useAppStore((s) => s.sessions);

  const [draft, setDraft] = useState<Session>(() => session ?? blankSession());
  const [showPassword, setShowPassword] = useState(false);
  const [error, setError] = useState<string | null>(null);

  useEffect(() => {
    setDraft(session ?? blankSession());
  }, [session]);

  const auth = draft.auth;

  function setAuth(next: AuthMethod) {
    setDraft((d) => ({ ...d, auth: next }));
  }

  async function submit() {
    if (!draft.name.trim()) return setError("请填写会话名称");
    if (!draft.host.trim()) return setError("请填写主机地址");
    if (!draft.username.trim()) return setError("请填写用户名");
    if (draft.port < 1 || draft.port > 65535) return setError("端口范围应为 1-65535");
    if (auth.type === "key" && !auth.path.trim()) return setError("请选择私钥文件");

    const duplicate = existing.some(
      (s) => s.id !== draft.id && s.name.trim() === draft.name.trim(),
    );
    if (duplicate) return setError("已存在同名会话");

    try {
      await saveSession({
        ...draft,
        name: draft.name.trim(),
        host: draft.host.trim(),
        username: draft.username.trim(),
        group: draft.group?.trim() || null,
      });
      onClose();
    } catch (err) {
      setError(String(err));
    }
  }

  async function browseKey() {
    const selected = await openDialog({
      multiple: false,
      directory: false,
      title: "选择私钥文件",
    });
    if (typeof selected === "string") {
      setAuth({ type: "key", path: selected, passphrase: auth.type === "key" ? auth.passphrase : "" });
    }
  }

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>{session ? "编辑会话" : "新建会话"}</h2>
          <button className="icon-btn" onClick={onClose}>
            <X size={16} />
          </button>
        </div>

        <div className="modal-body">
          <div className="form-row two">
            <label>
              <span>会话名称</span>
              <input
                autoFocus
                value={draft.name}
                placeholder="生产环境 Web"
                onChange={(e) => setDraft({ ...draft, name: e.target.value })}
              />
            </label>
            <label>
              <span>分组</span>
              <input
                value={draft.group ?? ""}
                placeholder="生产 / 测试"
                onChange={(e) => setDraft({ ...draft, group: e.target.value })}
              />
            </label>
          </div>

          <div className="form-row host-row">
            <label className="grow">
              <span>主机地址</span>
              <input
                value={draft.host}
                placeholder="10.0.0.1 或 example.com"
                onChange={(e) => setDraft({ ...draft, host: e.target.value })}
              />
            </label>
            <label className="port">
              <span>端口</span>
              <input
                type="number"
                value={draft.port}
                onChange={(e) => setDraft({ ...draft, port: Number(e.target.value) })}
              />
            </label>
          </div>

          <label>
            <span>用户名</span>
            <input
              value={draft.username}
              onChange={(e) => setDraft({ ...draft, username: e.target.value })}
            />
          </label>

          <div className="form-row">
            <label>
              <span>认证方式</span>
              <select
                value={auth.type}
                onChange={(e) => {
                  const type = e.target.value as AuthMethod["type"];
                  if (type === "password") setAuth({ type: "password" });
                  else if (type === "agent") setAuth({ type: "agent" });
                  else setAuth({ type: "key", path: "", passphrase: "" });
                }}
              >
                <option value="password">密码</option>
                <option value="key">公钥文件</option>
                <option value="agent">SSH Agent</option>
              </select>
            </label>
          </div>

          {auth.type === "password" && (
            <label>
              <span>密码（留空表示每次连接时输入）</span>
              <div className="input-with-btn">
                <input
                  type={showPassword ? "text" : "password"}
                  value={draft.password ?? ""}
                  onChange={(e) => setDraft({ ...draft, password: e.target.value })}
                />
                <button className="icon-btn" onClick={() => setShowPassword((v) => !v)}>
                  {showPassword ? <EyeOff size={15} /> : <Eye size={15} />}
                </button>
              </div>
            </label>
          )}

          {auth.type === "key" && (
            <>
              <label>
                <span>私钥文件</span>
                <div className="input-with-btn">
                  <input
                    value={auth.path}
                    placeholder="~/.ssh/id_ed25519"
                    onChange={(e) => setAuth({ ...auth, path: e.target.value })}
                  />
                  <button className="btn" onClick={() => void browseKey()}>
                    <FolderOpen size={14} /> 浏览
                  </button>
                </div>
              </label>
              <label>
                <span>私钥口令（可选）</span>
                <input
                  type="password"
                  value={auth.passphrase ?? ""}
                  onChange={(e) => setAuth({ ...auth, passphrase: e.target.value })}
                />
              </label>
            </>
          )}

          {auth.type === "agent" && (
            <div className="hint">将使用系统 ssh-agent / Pageant 中已加载的密钥。</div>
          )}

          <label>
            <span>备注</span>
            <input
              value={draft.description ?? ""}
              onChange={(e) => setDraft({ ...draft, description: e.target.value })}
            />
          </label>

          <div className="form-row">
            <label>
              <span>标记颜色</span>
              <div className="color-picker">
                {COLORS.map((c) => (
                  <button
                    key={c}
                    className={`color-swatch ${draft.color === c ? "active" : ""}`}
                    style={{ background: c }}
                    onClick={() => setDraft({ ...draft, color: c })}
                  />
                ))}
              </div>
            </label>
          </div>

          {error && <div className="form-error">{error}</div>}
        </div>

        <div className="modal-foot">
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button className="btn btn-primary" onClick={() => void submit()}>
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
