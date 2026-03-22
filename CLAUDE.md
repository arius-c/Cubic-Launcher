# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Before Starting Work

Read `IMPLEMENTATION_PROGRESS.md` and `NEXT_STEPS_FOR_AI.md` before beginning any new work. Treat previously completed modules as approved unless the user explicitly requests changes.

## Build & Verification Commands

Run all three after every module:

```bash
# Frontend (from cubic_launcher/)
npm run dev          # dev server on port 1420
npm run build        # production build — must pass before committing
npx tsc --noEmit     # strict TypeScript checks without emitting

# Rust backend (from cubic_launcher/)
cargo check          # fast type + borrow check — run this first
cargo test           # all unit tests
cargo build          # full debug build

# Java agent (from cubic_launcher/java-agent/)
./gradlew build      # or gradlew.bat build on Windows
```

### Running a Single Rust Test

```bash
# From cubic_launcher/ (uses --manifest-path internally via cargo)
cargo test <test_fn_name>
cargo test -p cubic_launcher -- resolver::tests
```

Tests live at the bottom of each `.rs` file in a `#[cfg(test)] mod tests { ... }` block.

### Running the Full App

```bash
npm run tauri dev    # launches the full Tauri app (frontend + backend together)
```

## Architecture

Three-layer desktop application:

1. **SolidJS frontend** (`src/`) — UI built with SolidJS + Tailwind CSS
2. **Rust/Tauri backend** (`src-tauri/src/`) — all business logic, file I/O, SQLite, process spawning
3. **Java agent** (`java-agent/`) — ByteBuddy config-attribution agent

### Frontend Architecture (`src/`)

- `App.tsx` — root component; owns all Tauri IPC lifecycle (event listeners, startup data loading, side effects)
- `store.ts` — **all** reactive signals, memos, and action helpers; module-level exports serve as global state (no Context/Provider)
- `src/lib/types.ts` — all shared TypeScript types and constants
- `src/components/` — one component per file; side-effect-free

### Backend Architecture (`src-tauri/src/`)

- `lib.rs` — Tauri builder, plugin registration, and the full list of registered `invoke_handler` commands
- `rules.rs` — `rules.json` schema and serde types
- `resolver.rs` — core mod resolution algorithm
- `database.rs` — SQLite schema initialization (private module)
- `editor_data.rs` — editor snapshot & all mutation commands for `rules.json`
- `app_shell.rs` — shell snapshot and settings commands
- `launcher_paths.rs` — path resolution (private module)
- `modrinth.rs`, `dependencies.rs`, `mod_cache.rs` — Modrinth API, dependency fetching, cache management
- `launch_command.rs`, `launch_preview.rs`, `process_streaming.rs` — launch pipeline
- `microsoft_auth.rs`, `account_manager.rs`, `offline_account.rs`, `token_storage.rs` — auth stack

Data flow: frontend calls `invoke("command_name", args)` → Rust command function → mutates SQLite/filesystem → returns serialized result. Events flow back via Tauri's event system (not polling).

## TypeScript / SolidJS Style

- Use `class` (not `className`) in JSX
- Use `<For each={...}>` and `<Show when={...}>` instead of `.map()` and `&&`
- Side-effect handlers (async Tauri calls) live in `App.tsx`, not in `store.ts`
- No barrel `index.ts` files; import directly from the owning file
- Named exports everywhere; no default exports except root `App`
- `tsconfig.json` enforces `strict`, `noUnusedLocals`, `noUnusedParameters` — no `// @ts-ignore`

### Tauri IPC Pattern

```ts
const isTauri = () => "__TAURI_INTERNALS__" in window;
if (isTauri()) { /* invoke(...) */ } else { /* browser fallback */ }
```

Every Tauri call must have a browser-safe fallback so `npm run build` and plain Vite dev always work.

### Frontend Error Handling

```ts
try {
  await invoke("some_command", { ... });
} catch (err) {
  pushUiError({ title: "...", message: "...", detail: String(err), severity: "error", scope: "launch" });
}
```

Never use `alert()` or `console.error()` as the sole error surface.

## Rust Style

- All public functions return `anyhow::Result<T>`; never `.unwrap()` in non-test code
- Derive `Debug` and `Clone` on all data structs; add `Serialize + Deserialize` for IPC types
- Tauri command input structs use `#[serde(rename_all = "camelCase")]`
- Tauri command functions are named `*_command` (e.g., `load_shell_snapshot_command`)
- Async tests use `#[tokio::test]`; use fake trait implementations instead of mocking libraries

## Architectural Invariants

Never violate these:

1. **SQLite only** — no cloud DB, no Supabase, no network-required storage
2. **Symlinks/hardlinks only** — never copy or move JAR files from `cache/mods/`
3. **Ignore Modrinth `incompatible` tag** — incompatibility is managed via the UI's exclusion system
4. **No optional dependency auto-download** — only `required` deps are fetched automatically
5. **No external modpack import** — CurseForge/Modrinth modpack formats are explicitly unsupported
6. **No pre/post launch scripts** in MVP
7. **Deleting a parent rule cascades** to delete all its alternatives
8. **Local search only** — the editor search bar filters the current list; never queries Modrinth
9. **World saves are isolated** per instance (version + loader combination)
10. **Dependency version conflicts** resolve by keeping only the newest version of any duplicate

## After Each Module

```bash
npm run build   # must pass
cargo check     # must pass
cargo test      # must pass
```

Then update `IMPLEMENTATION_PROGRESS.md` with completed scope and verification results, summarize changes, and ask whether to proceed.
