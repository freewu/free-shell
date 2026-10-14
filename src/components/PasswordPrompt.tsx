import { useState } from "react";
import { X } from "lucide-react";

import { useAppStore } from "../store/useAppStore";
import type { Session } from "../types";

interface Props {
  session: Session;
  onCancel: () => void;
  onConfirm: (password: string, remember: boolean) => void;
}

export function PasswordPrompt({ session, onCancel, onConfirm }: Props) {
  const [password, setPassword] = useState("");
  const [remember, setRemember] = useState(false);

  return (
    <div className="modal-backdrop" onMouseDown={onCancel}>
      <div className="modal small" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>输入密码</h2>
          <button className="icon-btn" onClick={onCancel}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <div className="hint">
            {session.username}@{session.host}:{session.port}
          </div>
          <label>
            <span>密码</span>
            <input
              autoFocus
              type="password"
              value={password}
              onChange={(e) => setPassword(e.target.value)}
              onKeyDown={(e) => {
                if (e.key === "Enter") onConfirm(password, remember);
              }}
            />
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={remember}
              onChange={(e) => setRemember(e.target.checked)}
            />
            <span>记住密码（保存到本地配置文件）</span>
          </label>
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={onCancel}>
            取消
          </button>
          <button
            className="btn btn-primary"
            disabled={!password}
            onClick={() => onConfirm(password, remember)}
          >
            连接
          </button>
        </div>
      </div>
    </div>
  );
}

export function useSessionPasswordLookup() {
  const sessions = useAppStore((s) => s.sessions);
  return (id: string) => sessions.find((s) => s.id === id)?.password ?? "";
}
