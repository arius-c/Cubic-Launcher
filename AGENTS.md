# Repository Guidelines

## Project Overview
`Cubic-Launcher` is a desktop Minecraft mod-list manager and launcher built with Tauri. The frontend is a SolidJS + TypeScript application, and the backend is a Rust Tauri layer that owns persistence, filesystem access, account flows, mod resolution, downloads, and game launch orchestration.

The app is not a generic CRUD shell. It has a few core product flows that should shape code changes:
- Load a launcher-wide shell snapshot containing mod lists, accounts, and settings.
- Open one mod list into an editor snapshot with ordered rules, alternatives, links, incompatibilities, content packs, and presentation data.
- Resolve the active mod set for a selected Minecraft version and mod loader.
- Prepare and launch Minecraft, streaming progress and process logs back to the UI.
- Persist all meaningful state through Tauri commands instead of local-only frontend mutations.

## Project Structure & Module Organization
### Frontend (`src/`)
- `src/App.tsx`: root orchestrator. It wires `invoke()` calls, Tauri event listeners, optimistic UI updates, resolution reruns, and modal actions.
- `src/store.ts`: central SolidJS signals/memos. Treat this as the UI source of truth.
- `src/components/ModListEditor.tsx`: main rules editor.
- `src/components/ModRuleItem.tsx`: single mod row rendering and interaction.
- `src/components/AltSection.tsx`: nested alternatives UI.
- `src/components/Modals.tsx`: create/settings/accounts/links/export and other modal flows.
- `src/components/LaunchPanel.tsx`: launch controls, version/loader selection, progress UI.
- `src/components/Sidebar.tsx`: mod-list switching and launcher shell navigation.
- `src/components/AdvancedModPanel.tsx`: advanced per-mod editing.
- `src/components/DebugPanel.tsx`: frontend debug trace tooling.
- `src/lib/types.ts`: shared TypeScript-side data contracts.
- `src/lib/dragEngine.ts` and `src/lib/dragUtils.ts`: reorder and drag helpers. Be careful here because row identity stability matters for UX.
- `src/lib/logger.ts` and `src/lib/debugTrace.ts`: logging/debug helpers.

### Backend (`src-tauri/src/`)
- `lib.rs`: Tauri app setup, launcher path initialization, database initialization, and command registration.
- `app_shell.rs`: shell snapshot loading, account switching, Microsoft login entry points, global settings, per-modlist overrides.
- `editor_data.rs`: editor snapshot loading and most rule CRUD operations.
- `rules.rs`: rule structs, serialization, schema/version handling.
- `resolver.rs`: mod resolution and availability backfill.
- `modlist_manager.rs`: create/delete/import/copy-local-jar operations for mod lists.
- `modlist_assets.rs`: presentation metadata, groups, export, file listing, image loading.
- `content_packs.rs`: resource packs, shaders, datapacks, and other non-mod content management.
- `launch_preview.rs` and `launch_command.rs`: launch preparation and process execution.
- `process_streaming.rs`: streamed launch/process events.
- `minecraft_downloader.rs`, `java_runtime.rs`, `adoptium.rs`, `loader_metadata.rs`: runtime and Minecraft asset setup.
- `modrinth.rs`, `dependencies.rs`, `mod_cache.rs`: mod discovery, dependency handling, caching.
- `microsoft_auth.rs`, `offline_account.rs`, `account_manager.rs`, `token_storage.rs`: account and auth flows.
- `database.rs` and `launcher_paths.rs`: SQLite setup and on-disk path management.
- `instance_configs.rs`, `instance_mods.rs`, `config_attribution.rs`: instance/config file handling.

### Other Important Paths
- `public/`: static frontend assets.
- `dist/`: generated frontend build output.
- `src-tauri/tauri.conf.json`: Tauri bundle/runtime config.
- `src-tauri/launcher_data.db`: local dev database artifact in this repo. Do not assume production data lives here; runtime paths are created by `launcher_paths`.

## Architecture & Data Flow
Frontend and backend communicate through Tauri `invoke()` commands and event listeners.
- Shell load: `load_shell_snapshot_command` returns mod lists, accounts, global settings, and selected mod-list overrides.
- Editor load: `load_modlist_editor_command` returns the selected mod list's rules, alternatives, links, incompatibilities, version rules, and custom configs.
- Mutations: granular commands such as `add_mod_rule_command`, `rename_rule_command`, `reorder_rules_command`, `toggle_rule_enabled_command`, `save_rule_advanced_command`, `save_incompatibilities_command`, and content/group/presentation commands persist changes.
- Resolution: `resolve_modlist_command` recalculates the active mod set for the selected Minecraft version and loader.
- Launch: `start_launch_command` starts backend-driven launch orchestration; progress and logs stream back through Tauri events.

Important implementation detail:
- The frontend often uses synthetic row IDs derived from rule position and nesting for rendering and drag state.
- The backend persists by actual mod IDs / rule identifiers.
- When changing rename, reorder, delete, or alternative-tree logic, preserve the mapping between frontend row IDs and backend IDs, or selection/groups/advanced panels will desync.

## Build, Test, and Development Commands
- `npm install`: install JS dependencies.
- `npm run dev`: run the Vite frontend only.
- `npm run tauri dev`: run the full desktop app with the Rust backend.
- `npm run build`: build frontend assets.
- `npm run serve`: preview the frontend production build.
- `npm run tauri build`: create production desktop bundles.
- `npx tsc --noEmit`: strict TypeScript type check.
- `cargo check --manifest-path src-tauri/Cargo.toml`: Rust compile check.

## Coding Style & Naming Conventions
- TypeScript is strict. Keep code compatible with `strict`, `noUnusedLocals`, and `noUnusedParameters`.
- Match existing formatting: frontend files use 2-space indentation; Rust should stay rustfmt-friendly.
- Use `PascalCase` for Solid components, `camelCase` for TS utilities/functions/signals, and `snake_case` for Rust modules.
- Keep frontend/backend boundaries clean: frontend code should call `invoke()` instead of bypassing backend persistence.
- Prefer extending existing shared types in `src/lib/types.ts` and backend command payloads rather than introducing parallel shapes.
- Keep logging structured where possible. Existing code already uses `logger` and debug trace helpers.

## Project-Specific Working Rules
- Preserve SolidJS identity where possible. `App.tsx` contains logic such as smart row merging and row ID rebuilding specifically to avoid destructive rerenders and drag flicker.
- Treat `src/store.ts` as the central UI state container. Avoid scattering duplicate state into individual components unless it is truly local/transient.
- Do not mutate launcher persistence formats casually. Changes touching `rules.rs`, `database.rs`, exported archives, or on-disk config handling should be reviewed for backward compatibility.
- Keep launch work backend-driven. The frontend should request a launch and render streamed progress, not reimplement launch steps.
- Keep resolution semantics stable. Changes in `resolver.rs`, incompatibility handling, links/dependencies, or version rule evaluation can cause silent correctness regressions.
- Account/auth work spans multiple modules. If you touch Microsoft login, token persistence, account switching, or offline accounts, inspect the full flow before editing.
- For UI features involving mod grouping, alternatives, links, incompatibilities, or advanced metadata, verify both the optimistic frontend path and the backend reload path.

## Testing Guidelines
There is no automated test suite yet, so minimum validation should match the area changed.
- Always run `npx tsc --noEmit` after frontend TypeScript changes.
- Always run `cargo check --manifest-path src-tauri/Cargo.toml` after Rust changes.
- For cross-layer changes, run both checks.
- Smoke test with `npm run tauri dev` when changing IPC flows, launch logic, account flows, drag/reorder behavior, import/export, or persistence.
- For bug fixes, record reproduction steps and the exact verification path.
- Add focused Rust unit tests with `#[cfg(test)]` when touching pure backend logic that can be tested without Tauri runtime setup.

Manual verification targets that are usually worth checking:
- Create/select/delete a mod list.
- Add, rename, reorder, enable/disable, and delete rules.
- Edit alternatives and nested alternatives.
- Save links, incompatibilities, version rules, and custom configs.
- Switch Minecraft version/mod loader and confirm resolution refreshes.
- Open launch flow and verify progress/log streaming still works.

## Commit & Pull Request Guidelines
Recent history uses short, informal commit messages such as `fixs`, `alpha 0.2`, and `beta`, but new work should be clearer.
- Prefer scoped imperative commits, for example `frontend: preserve alt row identity on reorder` or `backend: fix launch preview dependency resolution`.
- Keep commits focused on one concern.
- PRs should include summary, rationale, manual verification steps, and screenshots/GIFs for UI changes.
- Call out data/model/config impact explicitly, especially for `src-tauri/` persistence, launch, auth, and export changes.
