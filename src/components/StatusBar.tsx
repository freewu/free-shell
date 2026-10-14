import { Activity, Server, Layers, Wifi } from "lucide-react";

import { useAppStore } from "../store/useAppStore";

export function StatusBar() {
  const sessions = useAppStore((s) => s.sessions);
  const connected = useAppStore((s) => s.connected);
  const batch = useAppStore((s) => s.batch);

  const online = Object.values(connected).filter((c) => c.status === "connected").length;
  const errors = Object.values(connected).filter((c) => c.status === "error").length;

  return (
    <footer className="statusbar">
      <span className="status-item">
        <Server size={12} /> 会话 {sessions.length}
      </span>
      <span className="status-item">
        <Wifi size={12} /> 已连接 {online}
      </span>
      {errors > 0 && (
        <span className="status-item danger">
          <Activity size={12} /> 异常 {errors}
        </span>
      )}
      {batch.targets.length > 0 && (
        <span className="status-item">
          <Layers size={12} /> 批量 {batch.targets.length} 台
          {batch.running ? "（执行中）" : ""}
        </span>
      )}
      <span className="spacer" />
      <span className="status-item muted">FreeShell · Tauri 2 + React</span>
    </footer>
  );
}
