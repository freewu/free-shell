/**
 * Terminal data bus.
 *
 * Backend `ssh://data` events arrive even before a terminal has mounted (for
 * example while a tab is being created). We buffer per-session chunks until a
 * sink registers, so no output is lost.
 */

type Sink = (bytes: Uint8Array) => void;

const sinks = new Map<string, Sink>();
const pending = new Map<string, Uint8Array[]>();

export function registerSink(sessionId: string, sink: Sink): void {
  sinks.set(sessionId, sink);
  const backlog = pending.get(sessionId);
  if (backlog) {
    pending.delete(sessionId);
    for (const chunk of backlog) sink(chunk);
  }
}

export function unregisterSink(sessionId: string): void {
  sinks.delete(sessionId);
}

export function pushData(sessionId: string, bytes: Uint8Array): void {
  const sink = sinks.get(sessionId);
  if (sink) {
    sink(bytes);
    return;
  }
  const backlog = pending.get(sessionId) ?? [];
  backlog.push(bytes);
  // Keep memory bounded for very chatty sessions.
  if (backlog.length > 512) backlog.splice(0, backlog.length - 512);
  pending.set(sessionId, backlog);
}

export function dropSession(sessionId: string): void {
  sinks.delete(sessionId);
  pending.delete(sessionId);
}
