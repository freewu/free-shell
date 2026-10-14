import { invoke } from "@tauri-apps/api/core";

import type {
  BatchRequest,
  BatchResult,
  BatchTarget,
  ConnectInfo,
  ExecResult,
  Session,
  Settings,
} from "../types";

export const ipc = {
  listSessions: () => invoke<Session[]>("list_sessions"),
  saveSession: (session: Session) => invoke<void>("save_session", { session }),
  deleteSession: (id: string) => invoke<void>("delete_session", { id }),

  getSettings: () => invoke<Settings>("get_settings"),
  saveSettings: (settings: Settings) => invoke<void>("save_settings", { settings }),
  dataDir: () => invoke<string>("data_dir"),

  sshConnect: (session: Session, password?: string, openShell = true) =>
    invoke<ConnectInfo>("ssh_connect", { session, password: password ?? null, openShell }),
  sshDisconnect: (id: string) => invoke<void>("ssh_disconnect", { id }),
  sshConnected: () => invoke<string[]>("ssh_connected"),
  sshWrite: (id: string, data: string) => invoke<void>("ssh_write", { id, data }),
  sshResize: (id: string, cols: number, rows: number) =>
    invoke<void>("ssh_resize", { id, cols, rows }),
  sshExec: (id: string, command: string, timeoutSecs?: number) =>
    invoke<ExecResult>("ssh_exec", { id, command, timeoutSecs: timeoutSecs ?? null }),

  batchExec: (targets: BatchTarget[], request: BatchRequest) =>
    invoke<BatchResult[]>("batch_exec", { targets, request }),
};
