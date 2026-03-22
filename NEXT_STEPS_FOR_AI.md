# Cubic Launcher - Next Steps For AI

## Current State

The implementation plan from Phase 1 through Phase 7 has been completed at scaffold level and recorded in `IMPLEMENTATION_PROGRESS.md`.

The project now contains:
- Rust/Tauri backend modules for database, rules schema, resolver foundations, Modrinth integration, dependency handling, cache planning, Java/runtime/loader metadata, launch command building, process streaming, auth/account/offline flows, and related helpers.
- SolidJS/Tailwind UI scaffolding for the main launcher shell, editor workspace, groups, alternatives, incompatibilities, quick launch, logs, settings, accounts, export, warnings, and verification notes.
- A separate Java agent scaffold in `java-agent/`.

Important: much of the frontend still uses mock/demo state. The first post-plan integration step is now done for shell snapshot loading, but several UI actions still need real mutation commands and persistence wiring.

## Planned Next Steps

1. Continue connecting the UI to real Tauri commands and backend state.
   - The shell now loads real Mod-list/account/settings snapshot data on startup.
   - Account switching, settings persistence, and selected Mod-list snapshot refresh are connected through Tauri commands.
   - The Play surface is backend-event-driven through a launch preview command and Tauri event listeners.
   - The editor list now loads rule-derived rows from the selected Mod-list `rules.json`.
   - Alternative ordering persists back to `rules.json` through a backend mutation command.
   - Incompatibility edits now persist back to `rules.json` through backend mutation and roundtrip into editor snapshots.
   - Next, add more editor mutation commands for other editing flows, especially rule metadata, grouping persistence where appropriate, and eventually Add Mod / local-upload backed mutations.

2. Integrate the full Play flow end-to-end.
   - Wire the existing backend launch pipeline pieces together through Tauri commands/events.
   - Connect resolver, cache checks, loader metadata, launch command construction, process spawning, log streaming, and progress events to the UI.

3. Replace remaining demo/editor mock data with persisted/project-backed data.
   - Mod-lists
   - rules.json editing state
   - settings persistence
   - account switching state
   - export selections where appropriate

4. Add integration-focused validation.
   - Exercise real backend flows with representative data.
   - Confirm event-driven UI updates with Tauri IPC.
   - Re-run `npm run build`, `cargo check`, and `cargo test` after each substantial module.

## Instructions For The Next AI

When you start the next step, follow these instructions exactly:

1. Read `IMPLEMENTATION_PROGRESS.md` first.
2. Read this file second.
3. Treat the previously completed work as approved unless the user explicitly asks for changes.
4. Do not refactor approved code unless it is necessary to complete the requested module safely.
5. Work one module at a time.
6. After finishing each module:
   - run relevant verification commands
   - update `IMPLEMENTATION_PROGRESS.md`
   - summarize what changed
   - ask whether to proceed to the next module
7. Prefer connecting existing backend pieces before inventing new abstractions.
8. Preserve the project rules from the original Cubic Launcher specification:
   - SQLite only
   - no CurseForge
   - no Modrinth incompatible tag handling
   - no external modpack import
   - version-agnostic Mod-list behavior remains central
9. When moving from UI scaffold to real integration, keep the established Cubic visual language intact.

## Recommended Immediate Starting Module

Start with: backend-backed mutation commands for additional editor flows beyond alternatives and incompatibilities.
