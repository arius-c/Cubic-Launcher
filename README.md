# Cubic Launcher

Cubic Launcher is a desktop Minecraft mod-list manager built with Tauri, SolidJS, TypeScript, and Rust. It is focused on building, resolving, caching, and launching curated mod lists rather than acting as a generic file launcher.

## What The App Does

- Loads a launcher shell with mod lists, accounts, settings, and per-mod-list overrides.
- Opens a selected mod list into an editor with rules, alternatives, links, incompatibilities, content packs, and presentation metadata.
- Resolves the active mod set for a selected Minecraft version and mod loader.
- Downloads and caches Minecraft assets, Java runtimes, loader metadata, Modrinth artifacts, dependencies, local jars, and content packs.
- Launches Minecraft through the Rust backend and streams progress, process output, and launch logs back to the UI.

## Repository Layout

- `src/` contains the SolidJS frontend.
- `src/app/` contains frontend orchestration helpers used by `App.tsx`.
- `src/components/` contains UI components and feature-specific subfolders.
- `src/lib/` contains frontend shared types, drag helpers, logging, and debug utilities.
- `src-tauri/src/` contains the Rust Tauri backend.
- `src-tauri/src/launch_preview*.rs` is intentionally split by launch responsibility while still registered from `launch_preview.rs`.
- `src-tauri/src/editor_data*.rs` contains the editor command surface, payload models, and tests.

## Architecture Notes

The frontend persists meaningful state through Tauri commands. Avoid adding frontend-only mutations for launcher data unless the backend also owns the durable update.

The backend is the source of truth for filesystem access, rule persistence, cache lookup, dependency resolution, account handling, Java/runtime setup, and Minecraft launch orchestration.

Some backend modules are still flat files in `src-tauri/src/`. That is deliberate for now: the large files have been split by responsibility, but the repo has not moved those split files into nested Rust module folders yet to keep the refactor low-risk.

## Important Maintenance Notes

- Cache-only launch must use the same resolved mod set as an online launch when all artifacts are already cached.
- Modrinth rules may use slugs while cached records use canonical project ids; keep the alias logic intact.
- Dependency rows are refreshed during online launch so cache-only mode does not use stale dependencies from another Minecraft target.
- Rule identity matters. Frontend rows can use synthetic ids, but backend persistence uses actual rule/mod identifiers.
- Changes to `rules.rs`, database schema, exported archives, and cache layout should be reviewed for backward compatibility.

## Development

```sh
npm install
npm run tauri dev
npx tsc --noEmit
cargo check --manifest-path src-tauri/Cargo.toml
```

Run both TypeScript and Rust checks when touching cross-layer behavior.
