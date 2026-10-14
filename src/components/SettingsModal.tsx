import { useEffect, useState } from "react";
import { X } from "lucide-react";

import { useAppStore } from "../store/useAppStore";
import type { Settings } from "../types";

interface Props {
  onClose: () => void;
}

export function SettingsModal({ onClose }: Props) {
  const settings = useAppStore((s) => s.settings);
  const updateSettings = useAppStore((s) => s.updateSettings);
  const dataDir = useAppStore((s) => s.dataDir);

  const [draft, setDraft] = useState<Settings>(settings);

  useEffect(() => setDraft(settings), [settings]);

  function patch(p: Partial<Settings>) {
    setDraft((d) => ({ ...d, ...p }));
  }

  return (
    <div className="modal-backdrop" onMouseDown={onClose}>
      <div className="modal" onMouseDown={(e) => e.stopPropagation()}>
        <div className="modal-head">
          <h2>设置</h2>
          <button className="icon-btn" onClick={onClose}>
            <X size={16} />
          </button>
        </div>
        <div className="modal-body">
          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.strictHostKeyChecking}
              onChange={(e) => patch({ strictHostKeyChecking: e.target.checked })}
            />
            <span>严格校验主机密钥（开启后只信任 known_hosts 中的指纹）</span>
          </label>
          <label className="checkbox">
            <input
              type="checkbox"
              checked={draft.acceptNewHostKeys}
              onChange={(e) => patch({ acceptNewHostKeys: e.target.checked })}
            />
            <span>自动信任并记录新主机（TOFU）</span>
          </label>

          <div className="form-row two">
            <label>
              <span>连接超时（秒）</span>
              <input
                type="number"
                value={draft.connectTimeoutSecs}
                onChange={(e) => patch({ connectTimeoutSecs: Number(e.target.value) })}
              />
            </label>
            <label>
              <span>心跳间隔（秒）</span>
              <input
                type="number"
                value={draft.keepaliveIntervalSecs}
                onChange={(e) => patch({ keepaliveIntervalSecs: Number(e.target.value) })}
              />
            </label>
          </div>

          <div className="form-row two">
            <label>
              <span>终端字号</span>
              <input
                type="number"
                value={draft.terminalFontSize}
                onChange={(e) => patch({ terminalFontSize: Number(e.target.value) })}
              />
            </label>
            <label>
              <span>终端字体</span>
              <input
                value={draft.terminalFontFamily}
                onChange={(e) => patch({ terminalFontFamily: e.target.value })}
              />
            </label>
          </div>

          <label>
            <span>数据目录</span>
            <input readOnly value={dataDir} />
          </label>
        </div>
        <div className="modal-foot">
          <button className="btn" onClick={onClose}>
            取消
          </button>
          <button
            className="btn btn-primary"
            onClick={() => {
              void updateSettings(draft);
              onClose();
            }}
          >
            保存
          </button>
        </div>
      </div>
    </div>
  );
}
