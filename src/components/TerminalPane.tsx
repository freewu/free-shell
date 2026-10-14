import { useEffect, useRef } from "react";
import { Terminal } from "@xterm/xterm";
import { FitAddon } from "@xterm/addon-fit";
import { WebLinksAddon } from "@xterm/addon-web-links";
import { SearchAddon } from "@xterm/addon-search";

import { ipc } from "../lib/ipc";
import { bytesToBase64, textToBase64 } from "../lib/base64";
import { registerSink, unregisterSink } from "../lib/termBus";
import { useAppStore } from "../store/useAppStore";

interface Props {
  sessionId: string;
  active: boolean;
}

export function TerminalPane({ sessionId, active }: Props) {
  const hostRef = useRef<HTMLDivElement>(null);
  const termRef = useRef<Terminal | null>(null);
  const fitRef = useRef<FitAddon | null>(null);
  const settings = useAppStore((s) => s.settings);

  useEffect(() => {
    const host = hostRef.current;
    if (!host) return;

    const term = new Terminal({
      fontFamily: settings.terminalFontFamily,
      fontSize: settings.terminalFontSize,
      cursorBlink: true,
      allowProposedApi: true,
      macOptionIsMeta: true,
      scrollback: 10000,
      theme: {
        background: "#0b1120",
        foreground: "#e2e8f0",
        cursor: "#98d98e",
        selectionBackground: "#33415580",
        black: "#1e293b",
        red: "#f87171",
        green: "#34d399",
        yellow: "#fbbf24",
        blue: "#60a5fa",
        magenta: "#c084fc",
        cyan: "#22d3ee",
        white: "#e2e8f0",
        brightBlack: "#475569",
        brightWhite: "#f8fafc",
      },
    });

    const fit = new FitAddon();
    const search = new SearchAddon();
    term.loadAddon(fit);
    term.loadAddon(search);
    term.loadAddon(new WebLinksAddon());
    term.open(host);

    termRef.current = term;
    fitRef.current = fit;

    registerSink(sessionId, (bytes) => term.write(bytes));

    const dataSub = term.onData((data) => {
      void ipc.sshWrite(sessionId, textToBase64(data)).catch(() => undefined);
    });

    const doFit = () => {
      try {
        fit.fit();
      } catch {
        return;
      }
      void ipc.sshResize(sessionId, term.cols, term.rows).catch(() => undefined);
    };

    // Initial fit after layout settles.
    requestAnimationFrame(doFit);

    const observer = new ResizeObserver(() => doFit());
    observer.observe(host);

    return () => {
      observer.disconnect();
      dataSub.dispose();
      unregisterSink(sessionId);
      term.dispose();
      termRef.current = null;
      fitRef.current = null;
    };
    // Recreate only when the session or user font preferences change.
  }, [sessionId, settings.terminalFontFamily, settings.terminalFontSize]);

  useEffect(() => {
    if (!active) return;
    const id = requestAnimationFrame(() => {
      try {
        fitRef.current?.fit();
      } catch {
        /* ignore */
      }
      termRef.current?.focus();
    });
    return () => cancelAnimationFrame(id);
  }, [active]);

  return <div className="terminal-host" ref={hostRef} />;
}

export function encodePaste(text: string): string {
  return bytesToBase64(new TextEncoder().encode(text));
}
