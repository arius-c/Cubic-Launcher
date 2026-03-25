import { createSignal, For, Show } from "solid-js";
import { logEntries, logger, type LogLevel } from "../lib/logger";

type FilterLevel = "all" | LogLevel;

const LEVEL_COLORS: Record<LogLevel, string> = {
  trace: "#9ca3af",
  debug: "#60a5fa",
  info:  "#4ade80",
  warn:  "#facc15",
  error: "#f87171",
};

const LEVEL_ORDER: Record<LogLevel, number> = {
  trace: 0, debug: 1, info: 2, warn: 3, error: 4,
};

function formatTime(ts: number): string {
  const d = new Date(ts);
  return [d.getHours(), d.getMinutes(), d.getSeconds()]
    .map(n => String(n).padStart(2, "0"))
    .join(":");
}

export function DebugPanel() {
  const [open, setOpen] = createSignal(false);
  const [filter, setFilter] = createSignal<FilterLevel>("all");

  const filtered = () => {
    const f = filter();
    const entries = logEntries().slice(0, 200);
    if (f === "all") return entries;
    const min = LEVEL_ORDER[f as LogLevel];
    return entries.filter(e => LEVEL_ORDER[e.level] >= min);
  };

  return (
    <div style={{ position: "fixed", bottom: "16px", right: "16px", "z-index": "9999" }}>
      <Show
        when={open()}
        fallback={
          <button
            onClick={() => setOpen(true)}
            style={{
              padding: "6px 12px",
              background: "#1e1e2e",
              color: "#cdd6f4",
              border: "1px solid #45475a",
              "border-radius": "6px",
              cursor: "pointer",
              "font-size": "13px",
            }}
          >
            🪲 Logs
          </button>
        }
      >
        <div
          style={{
            width: "480px",
            "max-height": "400px",
            background: "#1e1e2e",
            border: "1px solid #45475a",
            "border-radius": "8px",
            display: "flex",
            "flex-direction": "column",
            overflow: "hidden",
          }}
        >
          {/* Toolbar */}
          <div
            style={{
              display: "flex",
              "align-items": "center",
              gap: "8px",
              padding: "6px 10px",
              "border-bottom": "1px solid #45475a",
              "flex-shrink": "0",
            }}
          >
            <span style={{ color: "#cdd6f4", "font-size": "13px", "font-weight": "600" }}>🪲 Logs</span>
            <select
              value={filter()}
              onChange={e => setFilter(e.currentTarget.value as FilterLevel)}
              style={{
                "margin-left": "auto",
                background: "#313244",
                color: "#cdd6f4",
                border: "1px solid #45475a",
                "border-radius": "4px",
                padding: "2px 6px",
                "font-size": "12px",
              }}
            >
              <option value="all">All</option>
              <option value="debug">debug+</option>
              <option value="info">info+</option>
              <option value="warn">warn+</option>
              <option value="error">error</option>
            </select>
            <button
              onClick={() => logger.clear()}
              style={{
                background: "#313244",
                color: "#cdd6f4",
                border: "1px solid #45475a",
                "border-radius": "4px",
                padding: "2px 8px",
                "font-size": "12px",
                cursor: "pointer",
              }}
            >
              Clear
            </button>
            <button
              onClick={() => setOpen(false)}
              style={{
                background: "transparent",
                color: "#6c7086",
                border: "none",
                cursor: "pointer",
                "font-size": "14px",
                padding: "0 4px",
              }}
            >
              ✕
            </button>
          </div>

          {/* Log list — newest at top */}
          <div style={{ overflow: "auto", flex: "1", padding: "4px 0" }}>
            <For each={filtered()}>
              {entry => (
                <div
                  style={{
                    padding: "2px 10px",
                    "font-family": "monospace",
                    "font-size": "11px",
                    "line-height": "1.5",
                  }}
                >
                  <span style={{ color: "#6c7086" }}>[{formatTime(entry.ts)}]</span>
                  {" "}
                  <span
                    style={{
                      color: LEVEL_COLORS[entry.level],
                      "font-weight": "600",
                      "text-transform": "uppercase",
                    }}
                  >
                    {entry.level}
                  </span>
                  {" "}
                  <span style={{ color: "#89b4fa" }}>[{entry.tag}]</span>
                  {" "}
                  <span style={{ color: "#cdd6f4" }}>{entry.msg}</span>
                  <Show when={entry.data !== undefined && entry.data !== ""}>
                    <details style={{ "margin-left": "20px" }}>
                      <summary style={{ color: "#6c7086", cursor: "pointer" }}>data</summary>
                      <pre
                        style={{
                          color: "#a6e3a1",
                          margin: "2px 0 0 0",
                          "font-size": "10px",
                          "white-space": "pre-wrap",
                          "word-break": "break-all",
                        }}
                      >
                        {JSON.stringify(entry.data, null, 2)}
                      </pre>
                    </details>
                  </Show>
                </div>
              )}
            </For>
          </div>
        </div>
      </Show>
    </div>
  );
}
