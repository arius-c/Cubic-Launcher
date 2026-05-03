import { invoke } from "@tauri-apps/api/core";

const isTauri = () => "__TAURI_INTERNALS__" in window;

export async function clearDebugTrace() {
  if (!isTauri()) return null;

  try {
    return await invoke<string>("clear_debug_trace_command");
  } catch {
    return null;
  }
}

export function appendDebugTrace(scope: string, payload: unknown) {
  if (!isTauri()) return;

  const detail =
    typeof payload === "string"
      ? payload
      : JSON.stringify(payload, null, 0);

  void invoke("append_debug_trace_command", {
    entry: `[${scope}] ${detail}`,
  }).catch(() => {
    // Intentionally ignore debug trace write failures.
  });
}
