import { createSignal } from "solid-js";

export type LogLevel = "trace" | "debug" | "info" | "warn" | "error";

export interface LogEntry {
  id: number;       // auto-increment
  ts: number;       // Date.now()
  level: LogLevel;
  tag: string;      // e.g. "App", "ModListEditor"
  msg: string;
  data?: unknown;
}

const MAX_ENTRIES = 500;
let nextId = 0;

const consoleMethods: Record<LogLevel, (...args: unknown[]) => void> = {
  trace: console.debug.bind(console),
  debug: console.debug.bind(console),
  info:  console.info.bind(console),
  warn:  console.warn.bind(console),
  error: console.error.bind(console),
};

const [logEntries, setLogEntries] = createSignal<LogEntry[]>([]);
export { logEntries };

function addEntry(level: LogLevel, tag: string, msg: string, data?: unknown): void {
  const entry: LogEntry = { id: nextId++, ts: Date.now(), level, tag, msg, data };
  consoleMethods[level](`[${tag}] ${msg}`, data ?? "");
  setLogEntries(prev => {
    const next = [entry, ...prev];
    return next.length > MAX_ENTRIES ? next.slice(0, MAX_ENTRIES) : next;
  });
}

export const logger = {
  trace: (tag: string, msg: string, data?: unknown) => addEntry("trace", tag, msg, data),
  debug: (tag: string, msg: string, data?: unknown) => addEntry("debug", tag, msg, data),
  info:  (tag: string, msg: string, data?: unknown) => addEntry("info",  tag, msg, data),
  warn:  (tag: string, msg: string, data?: unknown) => addEntry("warn",  tag, msg, data),
  error: (tag: string, msg: string, data?: unknown) => addEntry("error", tag, msg, data),
  clear: () => { setLogEntries([]); },
};
