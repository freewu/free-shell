import { create } from "zustand";
import { listen, type UnlistenFn } from "@tauri-apps/api/event";

import { ipc } from "../lib/ipc";
import { base64ToBytes, base64ToText } from "../lib/base64";
import { dropSession, pushData } from "../lib/termBus";
import {
  DEFAULT_BATCH_FORM,
  emptyBatchState,
  type BatchEvent,
  type BatchForm,
  type BatchState,
  type ConnState,
  type Session,
  type Settings,
  type SshDataEvent,
  type SshStateEvent,
  type Tab,
} from "../types";

const BATCH_TAB_ID = "__batch__";

const DEFAULT_SETTINGS: Settings = {
  strictHostKeyChecking: false,
  acceptNewHostKeys: true,
  connectTimeoutSecs: 20,
  keepaliveIntervalSecs: 30,
  terminalFontSize: 14,
  terminalFontFamily: "JetBrains Mono, Cascadia Mono, Consolas, Menlo, monospace",
};

const BATCH_FORM_KEY = "freeshell.batchForm";

function loadBatchForm(): BatchForm {
  try {
    const raw = localStorage.getItem(BATCH_FORM_KEY);
    if (!raw) return { ...DEFAULT_BATCH_FORM };
    return { ...DEFAULT_BATCH_FORM, ...(JSON.parse(raw) as Partial<BatchForm>) };
  } catch {
    return { ...DEFAULT_BATCH_FORM };
  }
}

function saveBatchForm(form: BatchForm) {
  try {
    localStorage.setItem(BATCH_FORM_KEY, JSON.stringify(form));
  } catch {
    /* ignore quota errors */
  }
}

let unlisteners: UnlistenFn[] = [];

interface AppState {
  ready: boolean;
  sessions: Session[];
  settings: Settings;
  connected: Record<string, ConnState>;
  tabs: Tab[];
  activeTab: string | null;
  batch: BatchState;
  batchForm: BatchForm;
  dataDir: string;

  bootstrap: () => Promise<void>;
  reloadSessions: () => Promise<void>;
  saveSession: (session: Session) => Promise<void>;
  deleteSession: (id: string) => Promise<void>;
  updateSettings: (settings: Settings) => Promise<void>;

  openTab: (tab: Tab) => void;
  closeTab: (id: string) => Promise<void>;
  setActiveTab: (id: string) => void;

  connectSession: (session: Session, password?: string) => Promise<void>;
  disconnectSession: (id: string) => Promise<void>;

  setBatchForm: (patch: Partial<BatchForm>) => void;
  openBatchTab: () => void;
  runBatch: () => Promise<void>;
  clearBatch: () => void;
  appendBatchOutput: (event: BatchEvent) => void;
}

export const useAppStore = create<AppState>()((set, get) => ({
  ready: false,
  sessions: [],
  settings: DEFAULT_SETTINGS,
  connected: {},
  tabs: [],
  activeTab: null,
  batch: emptyBatchState(),
  batchForm: loadBatchForm(),
  dataDir: "",

  bootstrap: async () => {
    const [sessions, settings, dataDir] = await Promise.all([
      ipc.listSessions(),
      ipc.getSettings(),
      ipc.dataDir(),
    ]);

    // Mark anything already connected (e.g. after a frontend reload).
    const live = await ipc.sshConnected();
    const connected: Record<string, ConnState> = {};
    for (const id of live) connected[id] = { status: "connected" };

    set({ sessions, settings, dataDir, connected, ready: true });

    if (unlisteners.length > 0) return;

    unlisteners = await Promise.all([
      listen<SshDataEvent>("ssh://data", (event) => {
        pushData(event.payload.sessionId, base64ToBytes(event.payload.data));
      }),
      listen<SshStateEvent>("ssh://state", (event) => {
        const { sessionId, state, message } = event.payload;
        set((prev) => ({
          connected: { ...prev.connected, [sessionId]: { status: state, message } },
        }));
        if (state === "closed") {
          dropSession(sessionId);
        }
      }),
      listen<BatchEvent>("batch://event", (event) => {
        get().appendBatchOutput(event.payload);
      }),
    ]);
  },

  reloadSessions: async () => {
    set({ sessions: await ipc.listSessions() });
  },

  saveSession: async (session) => {
    await ipc.saveSession(session);
    await get().reloadSessions();
  },

  deleteSession: async (id) => {
    await ipc.deleteSession(id);
    await get().reloadSessions();
  },

  updateSettings: async (settings) => {
    await ipc.saveSettings(settings);
    set({ settings });
  },

  openTab: (tab) => {
    set((prev) => {
      if (prev.tabs.some((t) => t.id === tab.id)) {
        return { activeTab: tab.id };
      }
      return { tabs: [...prev.tabs, tab], activeTab: tab.id };
    });
  },

  closeTab: async (id) => {
    const { tabs } = get();
    const closing = tabs.find((t) => t.id === id);
    if (closing?.kind === "terminal") {
      await ipc.sshDisconnect(id).catch(() => undefined);
      dropSession(id);
    }
    set((prev) => {
      const nextTabs = prev.tabs.filter((t) => t.id !== id);
      let nextActive = prev.activeTab;
      if (prev.activeTab === id) {
        const idx = prev.tabs.findIndex((t) => t.id === id);
        nextActive = nextTabs[Math.min(idx, nextTabs.length - 1)]?.id ?? null;
      }
      const connected = { ...prev.connected };
      if (closing?.kind === "terminal") delete connected[id];
      return { tabs: nextTabs, activeTab: nextActive, connected };
    });
  },

  setActiveTab: (id) => set({ activeTab: id }),

  connectSession: async (session, password) => {
    const { openTab } = get();
    openTab({
      id: session.id,
      kind: "terminal",
      title: session.name,
      subtitle: `${session.username}@${session.host}`,
    });
    set((prev) => ({
      connected: { ...prev.connected, [session.id]: { status: "connecting" } },
    }));
    try {
      await ipc.sshConnect(session, password, true);
      set((prev) => ({
        connected: { ...prev.connected, [session.id]: { status: "connected" } },
      }));
    } catch (err) {
      set((prev) => ({
        connected: {
          ...prev.connected,
          [session.id]: { status: "error", message: String(err) },
        },
      }));
    }
  },

  disconnectSession: async (id) => {
    await ipc.sshDisconnect(id).catch(() => undefined);
    set((prev) => ({
      connected: { ...prev.connected, [id]: { status: "closed" } },
    }));
  },

  setBatchForm: (patch) => {
    const next = { ...get().batchForm, ...patch };
    saveBatchForm(next);
    set({ batchForm: next });
  },

  openBatchTab: () => {
    const { openTab, tabs } = get();
    const existing = tabs.find((t) => t.id === BATCH_TAB_ID);
    openTab(existing ?? { id: BATCH_TAB_ID, kind: "batch", title: "批量执行" });
  },

  runBatch: async () => {
    const { batchForm, sessions } = get();
    const selected = batchForm.selected
      .map((id) => sessions.find((s) => s.id === id))
      .filter((s): s is Session => Boolean(s));

    if (selected.length === 0) return;

    const commands = batchForm.commands
      .split("\n")
      .map((c) => c.trim())
      .filter((c) => c.length > 0);

    if (commands.length === 0) return;

    const now = Date.now();
    const targets = selected.map((s, index) => ({
      sessionId: s.id,
      password: batchForm.password ? batchForm.password : null,
      name: s.name,
      host: s.host,
      username: s.username,
      status: "pending" as const,
      stdout: "",
      stderr: "",
      exitCode: null,
      durationMs: 0,
      error: null,
      timedOut: false,
      index,
    }));

    set({
      batch: {
        runId: null,
        running: true,
        targets,
        total: targets.length,
        startedAt: now,
        finishedAt: null,
      },
    });

    try {
      await ipc.batchExec(
        selected.map((s) => ({
          sessionId: s.id,
          password: batchForm.password ? batchForm.password : null,
        })),
        {
          commands,
          timeoutSecs: batchForm.timeoutSecs,
          concurrency: batchForm.concurrency,
          stopOnError: batchForm.stopOnError,
        },
      );
    } catch (err) {
      set((prev) => ({
        batch: {
          ...prev.batch,
          running: false,
          finishedAt: Date.now(),
          targets: prev.batch.targets.map((t) =>
            t.status === "pending" || t.status === "running"
              ? { ...t, status: "failure", error: String(err) }
              : t,
          ),
        },
      }));
    }
  },

  clearBatch: () => set({ batch: emptyBatchState() }),

  appendBatchOutput: (event) => {
    set((prev) => {
      const targets = prev.batch.targets.map((t) => {
        if (t.sessionId !== event.sessionId) return t;
        if (event.kind === "start") {
          return { ...t, status: "running" as const, index: event.index };
        }
        if (event.kind === "output") {
          const chunk = event.data ? base64ToText(event.data) : "";
          if (event.stream === "stderr") {
            return { ...t, stderr: t.stderr + chunk };
          }
          return { ...t, stdout: t.stdout + chunk };
        }
        // done
        const result = event.result;
        if (!result) return { ...t, status: "success" as const };
        return {
          ...t,
          status: result.success ? ("success" as const) : ("failure" as const),
          exitCode: result.exitCode,
          durationMs: result.durationMs,
          error: result.error,
          timedOut: result.timedOut,
          stdout: result.stdout || t.stdout,
          stderr: result.stderr || t.stderr,
        };
      });

      const stillRunning = targets.some(
        (t) => t.status === "pending" || t.status === "running",
      );

      return {
        batch: {
          ...prev.batch,
          runId: event.runId,
          targets,
          running: stillRunning,
          finishedAt: stillRunning ? null : Date.now(),
        },
      };
    });
  },
}));

export { BATCH_TAB_ID };
