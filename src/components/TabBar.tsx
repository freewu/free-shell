import { X, Terminal as TerminalIcon, Layers } from "lucide-react";

import { useAppStore } from "../store/useAppStore";
import type { Tab } from "../types";

const STATUS_COLOR: Record<string, string> = {
  connected: "#34d399",
  connecting: "#fbbf24",
  error: "#f87171",
  closed: "#64748b",
};

function TabButton({ tab }: { tab: Tab }) {
  const activeTab = useAppStore((s) => s.activeTab);
  const connected = useAppStore((s) => s.connected);
  const setActiveTab = useAppStore((s) => s.setActiveTab);
  const closeTab = useAppStore((s) => s.closeTab);

  const active = activeTab === tab.id;
  const status = tab.kind === "terminal" ? connected[tab.id]?.status : undefined;

  return (
    <div
      className={`tab ${active ? "tab-active" : ""}`}
      onClick={() => setActiveTab(tab.id)}
      title={tab.subtitle}
    >
      {tab.kind === "batch" ? (
        <Layers size={13} className="tab-icon" />
      ) : (
        <TerminalIcon size={13} className="tab-icon" />
      )}
      {status && (
        <span
          className="tab-dot"
          style={{ background: STATUS_COLOR[status] ?? "#64748b" }}
        />
      )}
      <span className="tab-title">{tab.title}</span>
      <button
        className="tab-close"
        onClick={(e) => {
          e.stopPropagation();
          void closeTab(tab.id);
        }}
        title="关闭"
      >
        <X size={12} />
      </button>
    </div>
  );
}

export function TabBar() {
  const tabs = useAppStore((s) => s.tabs);

  if (tabs.length === 0) return null;

  return (
    <div className="tabbar">
      {tabs.map((tab) => (
        <TabButton key={tab.id} tab={tab} />
      ))}
    </div>
  );
}
