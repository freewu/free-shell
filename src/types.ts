// Data contracts mirrored from the Rust backend (`src-tauri/src/models.rs`).

export type AuthMethod =
  | { type: "password" }
  | { type: "key"; path: string; passphrase?: string | null }
  | { type: "agent" };

export interface Session {
  id: string;
  name: string;
  host: string;
  port: number;
  username: string;
  group?: string | null;
  color?: string | null;
  description?: string | null;
  auth: AuthMethod;
  password?: string | null;
  tags: string[];
}

export interface Settings {
  strictHostKeyChecking: boolean;
  acceptNewHostKeys: boolean;
  connectTimeoutSecs: number;
  keepaliveIntervalSecs: number;
  terminalFontSize: number;
  terminalFontFamily: string;
}

export interface BatchTarget {
  sessionId: string;
  password?: string | null;
}

export interface BatchRequest {
  commands: string[];
  timeoutSecs: number;
  concurrency: number;
  stopOnError: boolean;
}

export interface ExecResult {
  stdout: string;
  stderr: string;
  exitCode: number | null;
  durationMs: number;
  timedOut: boolean;
}

export interface BatchResult {
  sessionId: string;
  name: string;
  host: string;
  username: string;
  success: boolean;
  exitCode: number | null;
  stdout: string;
  stderr: string;
  durationMs: number;
  timedOut: boolean;
  error: string | null;
}

export interface ConnectInfo {
  sessionId: string;
  hostKeyFingerprint: string | null;
  serverBanner: string | null;
}

export interface BatchEvent {
  runId: string;
  sessionId: string;
  kind: "start" | "output" | "done";
  stream?: "stdout" | "stderr" | null;
  data?: string | null;
  result?: BatchResult | null;
  index: number;
  total: number;
}

export interface SshDataEvent {
  sessionId: string;
  data: string;
}

export interface SshStateEvent {
  sessionId: string;
  state: "connected" | "closed" | "error";
  message?: string | null;
}

export type ConnStatus = "connecting" | "connected" | "closed" | "error";

export interface ConnState {
  status: ConnStatus;
  message?: string | null;
}

export interface Tab {
  id: string;
  kind: "terminal" | "batch";
  title: string;
  subtitle?: string;
}

export interface BatchTargetState {
  sessionId: string;
  name: string;
  host: string;
  username: string;
  status: "pending" | "running" | "success" | "failure";
  stdout: string;
  stderr: string;
  exitCode: number | null;
  durationMs: number;
  error: string | null;
  timedOut: boolean;
  index: number;
}

export interface BatchState {
  runId: string | null;
  running: boolean;
  targets: BatchTargetState[];
  total: number;
  startedAt: number | null;
  finishedAt: number | null;
}

export interface BatchForm {
  selected: string[];
  commands: string;
  timeoutSecs: number;
  concurrency: number;
  stopOnError: boolean;
  password: string;
}

export function emptyBatchState(): BatchState {
  return {
    runId: null,
    running: false,
    targets: [],
    total: 0,
    startedAt: null,
    finishedAt: null,
  };
}

export const DEFAULT_BATCH_FORM: BatchForm = {
  selected: [],
  commands: "uptime\nwhoami",
  timeoutSecs: 60,
  concurrency: 8,
  stopOnError: false,
  password: "",
};
