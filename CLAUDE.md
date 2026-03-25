# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

**Cubic Launcher** is a desktop mod list manager for Minecraft, built with Tauri (Rust backend + SolidJS frontend). It manages complex rule-based mod resolution, account authentication, and game launching.

## Commands

```bash
# Frontend dev server only (localhost:1420)
npm run dev

# Full app in dev mode with hot reload
npm run tauri dev

# Production build
npm run tauri build

# Frontend build only
npm run build
```

There are no automated tests in this codebase.

## Architecture

### Tech Stack
- **Frontend:** SolidJS + TypeScript + Tailwind CSS + Vite
- **Backend:** Rust + Tauri
- **Storage:** SQLite (embedded via `rusqlite`)

### Frontend → Backend Communication
All IPC goes through Tauri's `invoke()` system. Commands are defined in `src-tauri/src/lib.rs` and dispatch to the relevant Rust modules. The frontend never directly mutates backend state — it calls commands and receives updated snapshots.

**Main data flow:**
1. App loads `ShellSnapshot` (all mod lists + accounts + global settings) via `load_shell_snapshot_command`
2. Selecting a mod list triggers `load_modlist_editor_command` → returns `EditorSnapshot` (rules, groups, alternatives for that list)
3. Edits invoke granular commands (`add_mod_rule_command`, `rename_rule_command`, etc.)
4. Launch flow: `start_launch_command` → streams `LaunchProgressEvent` and `ProcessLogEvent` events back to frontend

### Key Rust Modules (`src-tauri/src/`)

| Module | Role |
|--------|------|
| `lib.rs` | Tauri command registration and app setup |
| `app_shell.rs` | Shell snapshot, accounts, global settings persistence |
| `editor_data.rs` | Rule CRUD, alternatives, metadata, grouping (largest module) |
| `rules.rs` | Rule structs, file I/O, schema versioning |
| `resolver.rs` | Two-pass mod resolution: resolves which mods are active for a given MC version + loader |
| `modlist_manager.rs` | Create/delete mod lists |
| `modlist_assets.rs` | Cosmetic data, presentation, groups, export |
| `launch_preview.rs` | Orchestrates launch: resolution → download queue → game start |
| `minecraft_downloader.rs` | Fetches MC versions and binaries |
| `modrinth.rs` | Modrinth API integration for mod search/download |
| `microsoft_auth.rs` | OAuth2 authentication for Microsoft/Xbox/Minecraft accounts |
| `java_runtime.rs` | Java detection and setup via Adoptium |
| `dependencies.rs` | Mod dependency resolution |

### Key Frontend Components (`src/`)

| File | Role |
|------|------|
| `App.tsx` | Root orchestrator; manages all signals, coordinates IPC calls |
| `components/ModListEditor.tsx` | Primary rule editing interface |
| `components/ModRuleItem.tsx` | Single rule row with drag-and-drop |
| `components/AltSection.tsx` | Alternatives panel (38KB — complex nested UI) |
| `components/Modals.tsx` | All modal dialogs (add mod, create list, incompatibilities, links, settings) |
| `components/LaunchPanel.tsx` | Launch UI with progress tracking |
| `components/Sidebar.tsx` | Mod list switcher |
| `lib/types.ts` | All shared TypeScript types |
| `lib/store.ts` | All SolidJS signals and memos (global state) |
| `lib/dragUtils.ts` | Drag-and-drop utilities for rule reordering |

### Core Data Model

**ModRow** — a hierarchical rule entry:
- Has primary mod ID + alternatives (each alternative can also have alternatives — tree structure)
- Contains `VersionRule[]` (MC version include/exclude), `CustomConfig[]`, incompatibility links

**Resolution (two-pass):**
1. Pass 1: Walk rules in order, build active mod set
2. Pass 2: Re-resolve with full active set to correctly apply incompatibility losers that were added after their winners

**IncompatibilityRule** — `winner` mod presence causes `loser` mod to be excluded from resolution.

**LinkRule** — `fromId` requires `toId` (dependency relationship).

### State Management Pattern
SolidJS signals in `lib/store.ts` are the single source of truth for UI state. Computed values use `createMemo`. Backend is source of truth for persisted data — after any mutation command, the frontend re-fetches or incrementally updates signals.
