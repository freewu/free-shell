import { useEffect, useState } from "react";
import { Terminal, Layers, Plus, Keyboard } from "lucide-react";

import { Sidebar } from "./components/Sidebar";
import { TabBar } from "./components/TabBar";
import { TerminalPane } from "./components/TerminalPane";
import { BatchPanel } from "./components/BatchPanel";
import { StatusBar } from "./components/StatusBar";
import { SessionModal } from "./components/SessionModal";
import { SettingsModal } from "./components/SettingsModal";
import { PasswordPrompt } from "./components/PasswordPrompt";
import { BATCH_TAB_ID, useAppStore } from "./store/useAppStore";
import type { Session } from "./types";

export default function App() {
  const ready = useAppStore((s) => s.ready);
  const bootstrap = useAppStore((s) => s.bootstrap);
  const tabs = useAppStore((s) => s.tabs);
  const activeTab = useAppStore((s) => s.activeTab);
  const connectSession = useAppStore((s) => s.connectSession);
  const saveSession = useAppStore((s) => s.saveSession);
  const openBatchTab = useAppStore((s) => s.openBatchTab);

  const [editing, setEditing] = useState<Session | null>(null);
  const [modalOpen, setModalOpen] = useState(false);
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [passwordFor, setPasswordFor] = useState<Session | null>(null);

  useEffect(() => {
    void bootstrap();
  }, [bootstrap]);

  function handleConnect(session: Session) {
    if (session.auth.type === "password" && !session.password) {
      setPasswordFor(session);
      return;
    }
    void connectSession(session, session.password ?? undefined);
  }

  function openNewSession() {
    setEditing(null);
    setModalOpen(true);
  }

  function openEditSession(session: Session) {
    setEditing(session);
    setModalOpen(true);
  }

  const hasTerminalTabs = tabs.some((t) => t.kind === "terminal");
  const batchActive = activeTab === BATCH_TAB_ID;

  return (
    <div className="app">
      <Sidebar
        onConnect={handleConnect}
        onNewSession={openNewSession}
        onEditSession={openEditSession}
        onOpenSettings={() => setSettingsOpen(true)}
      />

      <main className="main">
        <TabBar />

        <div className="workspace">
          {tabs.length === 0 && (
            <div className="welcome">
              <div className="welcome-logo">
                <Terminal size={42} />
              </div>
              <h1>FreeShell</h1>
              <p>基于 Tauri 2 + React 的 SSH 客户端，支持多会话与批量执行。</p>
              <div className="welcome-actions">
                <button className="btn btn-primary" onClick={openNewSession}>
                  <Plus size={15} /> 新建会话
                </button>
                <button className="btn" onClick={openBatchTab}>
                  <Layers size={15} /> 批量执行
                </button>
              </div>
              <div className="welcome-tips">
                <Keyboard size={13} />
                双击左侧会话即可连接；在批量面板中可对多台主机同时下发命令。
              </div>
            </div>
          )}

          {hasTerminalTabs &&
            tabs
              .filter((t) => t.kind === "terminal")
              .map((tab) => (
                <div
                  key={tab.id}
                  className="terminal-wrapper"
                  style={{ display: activeTab === tab.id ? "flex" : "none" }}
                >
                  <TerminalPane sessionId={tab.id} active={activeTab === tab.id} />
                </div>
              ))}

          {batchActive && (
            <div className="terminal-wrapper batch-wrapper" style={{ display: "flex" }}>
              <BatchPanel />
            </div>
          )}
        </div>

        <StatusBar />
      </main>

      {modalOpen && (
        <SessionModal session={editing} onClose={() => setModalOpen(false)} />
      )}
      {settingsOpen && <SettingsModal onClose={() => setSettingsOpen(false)} />}
      {passwordFor && (
        <PasswordPrompt
          session={passwordFor}
          onCancel={() => setPasswordFor(null)}
          onConfirm={(password, remember) => {
            const target = passwordFor;
            setPasswordFor(null);
            void (async () => {
              if (remember) {
                await saveSession({ ...target, password });
              }
              await connectSession({ ...target, password }, password);
            })();
          }}
        />
      )}
      {!ready && <div className="boot">正在加载…</div>}
    </div>
  );
}
