# Cubic Launcher — Agent Instructions

## Project Overview

Three-layer desktop application: **Rust/Tauri backend** (`src-tauri/src/`), **SolidJS frontend** (`src/`), and a **Java Agent** (`java-agent/`). Read `IMPLEMENTATION_PROGRESS.md` and `NEXT_STEPS_FOR_AI.md` before starting any new work.

---

## Build & Verification Commands

Run all three after every module:

```bash
# Frontend (from cubic_launcher/)
npm run dev          # dev server on port 1420 (strict)
npm run build        # production build — must pass before committing

# Rust backend (from cubic_launcher/)
cargo check          # fast type + borrow check — run this first
cargo test           # all unit tests
cargo build          # full debug build (slower; use cargo check normally)

# Java agent (from cubic_launcher/java-agent/)
./gradlew build      # or gradlew.bat build on Windows
```

### Running a Single Rust Test

```bash
# From cubic_launcher/src-tauri/ (or with --manifest-path)
cargo test <test_fn_name>
# e.g.
cargo test continues_to_next_option_when_first_option_is_incompatible
cargo test -p cubic_launcher -- resolver::tests
```

Tests live at the bottom of each `.rs` file in a `#[cfg(test)] mod tests { ... }` block.

### TypeScript Type Check (no separate tsc step needed — Vite uses esbuild)

```bash
npx tsc --noEmit    # runs the strict tsconfig checks without emitting
```

---

## Repository Structure

```
cubic_launcher/
├── src/                    # SolidJS frontend
│   ├── App.tsx             # Root component + Tauri IPC lifecycle
│   ├── store.ts            # All reactive signals, memos, action helpers
│   ├── components/         # UI components (one per file)
│   │   ├── icons.tsx       # Inline SVG icon components
│   │   ├── Sidebar.tsx
│   │   ├── ActionBar.tsx
│   │   ├── ModRuleItem.tsx
│   │   ├── ModListEditor.tsx
│   │   ├── LaunchPanel.tsx
│   │   ├── AddModDialog.tsx
│   │   └── Modals.tsx
│   └── lib/
│       └── types.ts        # Shared TypeScript types + constants
├── src-tauri/src/          # Rust modules (one domain per file)
│   ├── lib.rs              # Tauri builder + registered commands
│   ├── rules.rs            # rules.json schema + serde types
│   ├── resolver.rs         # Core mod resolution algorithm
│   ├── database.rs         # SQLite schema init
│   ├── editor_data.rs      # Editor snapshot & mutation commands
│   ├── app_shell.rs        # Shell snapshot & settings commands
│   └── ...                 # modrinth, loader_metadata, launch_*, etc.
└── java-agent/             # ByteBuddy config-attribution agent
    └── src/main/java/com/cubic/agent/
```

---

## TypeScript / SolidJS Style

### Imports
- Group: external libs → internal lib/types → internal store → sibling components → icons.
- No barrel `index.ts` files; import directly from the file that owns the export.
- Use named exports everywhere; no default exports except the root `App` component.

### SolidJS Specifics
- Use `class` (not `className`) in JSX.
- Use `<For each={...}>` and `<Show when={...}>` instead of `.map()` and `&&`.
- Module-level exported signals (`createSignal`) in `store.ts` serve as global state — no Context/Provider needed for this single-instance app.
- Reactive computations go in `store.ts` as `createMemo(...)` exports.
- Side-effect handlers (async Tauri calls) live in `App.tsx`, not in `store.ts`.

### Types
- All shared types in `src/lib/types.ts`; component-local types inline in the component file.
- Prefer explicit `interface` for object shapes, `type` for unions/aliases.
- `tsconfig.json` enforces `strict`, `noUnusedLocals`, `noUnusedParameters` — no suppression with `// @ts-ignore`.

### Naming
- Components: `PascalCase` (e.g., `ModRuleItem`, `LaunchPanel`).
- Signals/setters: `[value, setValue]` destructuring following SolidJS convention.
- Handler functions: `handleXxx` for UI event handlers, `loadXxx` / `saveXxx` for async data operations.
- Constants: `UPPER_SNAKE_CASE` (e.g., `LAUNCH_STAGES`, `MINECRAFT_VERSIONS`).

### Tauri IPC Guards
```ts
const isTauri = () => "__TAURI_INTERNALS__" in window;
if (isTauri()) { /* invoke(...) */ } else { /* browser fallback */ }
```
Every Tauri call must have a browser-safe fallback so `npm run build` and the plain Vite dev server always work.

### Error Handling (Frontend)
```ts
try {
  await invoke("some_command", { ... });
} catch (err) {
  pushUiError({ title: "...", message: "...", detail: String(err), severity: "error", scope: "launch" });
}
```
Never use `alert()` or `console.error()` as the sole error surface. Always call `pushUiError`.

---

## Rust Style

### Error Handling
```rust
use anyhow::{bail, Context, Result};

// Validation errors
bail!("field cannot be empty");

// Propagation with context
fs::read_to_string(path)
    .with_context(|| format!("failed to read {}", path.display()))?;
```
All public functions return `anyhow::Result<T>`. Never `.unwrap()` in non-test production code.

### Data Types
```rust
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct SomeThing { ... }
```
- Derive `Debug` and `Clone` on all data structs.
- Derive `Serialize` + `Deserialize` for types that cross the Tauri IPC boundary.
- Tauri command input structs use `#[serde(rename_all = "camelCase")]`.

### Naming
- Types/enums: `PascalCase`. Functions/variables: `snake_case`. Constants: `UPPER_SNAKE_CASE`.
- Tauri command functions end in `_command` (e.g., `load_shell_snapshot_command`).
- Repository structs end in `Repository` (e.g., `AccountsRepository`).
- Client structs end in `Client` (e.g., `ModrinthClient`).

### Module Visibility
- Modules used only within `src-tauri/src/` are declared `mod` (private) in `lib.rs`.
- Modules that expose types to Tauri commands are declared `pub mod`.

### Unit Tests
```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn describes_what_it_verifies() {
        // Arrange — use fake/stub implementations of traits
        // Act
        // Assert
    }
}
```
- Test module is always at the **bottom** of the file.
- Use fake trait implementations (e.g., `FakeCompatibilityChecker`) rather than mocking libraries.
- Test function names are full English sentences describing the expected behavior.
- Async tests use `#[tokio::test]`.

---

## Architectural Hard Rules

These are invariants from the spec — never violate them:

1. **SQLite only** — no cloud DB, no Supabase, no network-required storage.
2. **Symlinks/hardlinks only** — never copy or move JAR files from `cache/mods/`.
3. **Ignore Modrinth `incompatible` tag** — user manages incompatibility via the UI's exclusion system.
4. **No optional dependency auto-download** — only `required` deps are fetched automatically.
5. **No external modpack import** — CurseForge/Modrinth modpack formats are explicitly unsupported.
6. **No pre/post launch scripts** in MVP — security risk, out of scope.
7. **Deleting a parent rule cascades** to delete all its alternatives.
8. **Local search only** — the in-editor search bar filters the current list; it never queries Modrinth.
9. **World saves are isolated** per instance (version + loader combination).
10. **Dependency version conflicts** resolve by keeping only the newest version of any duplicate.

---

## After Each Module

```bash
npm run build   # must pass
cargo check     # must pass
cargo test      # must pass
```

Then update `IMPLEMENTATION_PROGRESS.md` with the completed scope and verification results before summarizing changes and asking to proceed.
