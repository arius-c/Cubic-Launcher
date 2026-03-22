# Cubic Launcher Implementation Progress

## Completed - Phase 1 / Module 1

Scope completed: initialize the project with Tauri 2, Rust, SolidJS, and TailwindCSS.

- Scaffolded the desktop application in `cubic_launcher/` using Tauri 2 with the Solid TypeScript template.
- Installed and configured TailwindCSS with the initial Cubic color tokens and base styling.
- Replaced the default starter screen with a branded foundation UI aligned with the Cubic visual direction.
- Cleaned the default Tauri Rust bootstrap so the backend is ready for upcoming core modules.
- Updated application metadata, title, identifier, and initial window sizing.
- Removed starter assets and unused demo code.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Completed - Phase 1 / Module 2

Scope completed: configure SQLite with the complete schema required by the specification.

- Added SQLite support with bundled `rusqlite` and backend error handling support.
- Implemented database initialization in `src-tauri/src/database.rs` using the full required schema:
  - `accounts`
  - `java_installations`
  - `mod_cache`
  - `dependencies`
  - `config_attribution`
  - `global_settings`
  - `modlist_settings`
- Wired database initialization into Tauri startup so `launcher_data.db` is created automatically on app boot.
- Added Rust unit tests covering schema creation and idempotent initialization.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 1 / Module 3: implement first-run creation of the required folder structure.

## Completed - Phase 1 / Module 3

Scope completed: implement first-run creation of the required folder structure.

- Added launcher path management in `src-tauri/src/launcher_paths.rs`.
- Implemented automatic creation of the required root directories on startup:
  - `cache/`
  - `cache/mods/`
  - `cache/configs/`
  - `mod-lists/`
  - `java-runtimes/`
- Centralized computation of `launcher_data.db` path so startup now uses the launcher path module instead of building the DB path inline.
- Wired directory bootstrap into Tauri startup before database initialization.
- Added Rust unit tests covering directory creation, idempotency, and database path resolution.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 1 / Module 4: implement the typed `rules.json` schema with serde read/write support.

## Completed - Phase 1 / Module 4

Scope completed: implement the typed `rules.json` schema with serde read/write support.

- Added the Mod-list schema module in `src-tauri/src/rules.rs`.
- Implemented strongly typed serde models for:
  - `ModList`
  - `Rule`
  - `RuleOption`
  - `ModReference`
  - `FallbackStrategy`
  - `ModSource`
- Added `serde(default)` handling for rule arrays, `exclude_if_present`, and `fallback_strategy`, with `continue` as the default fallback strategy.
- Implemented `read_from_file` and `write_to_file` helpers for `rules.json` persistence.
- Added schema validation to enforce key constraints from the specification, including:
  - non-empty `modlist_name`
  - non-empty `author`
  - non-empty `rule_name`
  - each option must contain at least one mod
  - `local` mods must include `file_name`
  - `modrinth` mods must not include `file_name`
- Exposed the rules module from the Tauri library for upcoming backend integration.
- Added Rust unit tests covering defaults, validation failures, and read/write roundtrip persistence.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 2 / Module 1: implement the core Mod-list resolution algorithm from Section 15 steps 1-3.

## Completed - Phase 2 / Module 1

Scope completed: implement the core Mod-list resolution algorithm from Section 15 steps 1-3.

- Added the resolver engine in `src-tauri/src/resolver.rs`.
- Implemented ordered evaluation of rules from top to bottom while building `active_mods` incrementally.
- Implemented ordered evaluation of options within each rule, preserving fallback priority from index `0` upward.
- Enforced the single code path for both single mods and groups by always resolving `option.mods` as one atomic group.
- Implemented `exclude_if_present` handling against the current `active_mods` set.
- Implemented `fallback_strategy` behavior for both exclusion-driven skips and incompatibility-driven failures:
  - `continue` skips to the next option
  - `abort` stops evaluation of the current rule
- Added resolution result types that record per-rule outcomes and the final `active_mods` set.
- Introduced a compatibility-check abstraction so the algorithm is ready to plug into the upcoming Modrinth client and local-mod compatibility logic without changing the resolver flow.
- Added Rust unit tests covering:
  - continue fallback after incompatible options
  - abort on excluded options
  - atomic failure of grouped mods
  - abort on incompatible groups
  - priority propagation through `active_mods` across later rules

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 2 / Module 2: implement the Modrinth API client for compatibility lookup and version selection.

## Completed - Phase 2 / Module 2

Scope completed: implement the Modrinth API client for compatibility lookup and version selection.

- Added the Modrinth client module in `src-tauri/src/modrinth.rs`.
- Added the required async HTTP dependencies in `src-tauri/Cargo.toml`:
  - `reqwest`
  - `tokio`
- Implemented an async `ModrinthClient` with methods to:
  - fetch project versions for a target Minecraft version and loader
  - fetch the latest compatible version for a project
- Implemented typed Modrinth response models for:
  - version metadata
  - dependency metadata
  - downloadable files
  - dependency type enum
- Implemented Modrinth query URL construction using the required loader and game version filters.
- Added compatibility filtering helpers that match versions against the selected Minecraft version and mod loader.
- Added latest-version selection logic based on the most recent compatible `date_published` value.
- Added primary-file selection logic so the caller can obtain the preferred JAR file from a selected version.
- Added loader-to-Modrinth mapping via `ModLoader::as_modrinth_loader()` for backend integration with the existing resolver.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 2 / Module 3: implement dependency handling for required Modrinth dependencies and duplicate dependency version resolution.

## Completed - Phase 2 / Module 3

Scope completed: implement dependency handling for required Modrinth dependencies and duplicate dependency version resolution.

- Added the dependency resolution module in `src-tauri/src/dependencies.rs`.
- Implemented extraction of dependency requests from Modrinth version metadata while strictly keeping only `required` dependencies.
- Explicitly ignored non-required dependency types during extraction, including `optional` and `incompatible`.
- Added support for both dependency selector styles from Modrinth metadata:
  - `project_id` for any compatible version
  - `version_id` for exact-version lookup
- Implemented synchronous dependency resolution helpers for testable business logic and an async integration path with `ModrinthClient` for real backend usage.
- Added exact-version fetching support to the Modrinth client in `src-tauri/src/modrinth.rs`.
- Implemented duplicate dependency conflict resolution that keeps only the newest resolved dependency version per dependency project.
- Added finalized dependency link records that preserve parent-to-dependency relationships while pointing every parent to the selected kept version.
- Added selected dependency metadata ready for the next cache/download module, including:
  - dependency project id
  - version id
  - jar filename
  - download URL
  - SHA-1 hash when available
- Exported the dependency module from the Tauri library for later integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 2 / Module 4: implement cache-aware mod acquisition flow before download, using Modrinth version metadata and the existing database schema.

## Completed - Phase 2 / Module 4

Scope completed: implement cache-aware mod acquisition flow before download, using Modrinth version metadata and the existing database schema.

- Added the cache acquisition module in `src-tauri/src/mod_cache.rs`.
- Implemented typed cache records mapped to the existing SQLite `mod_cache` table schema.
- Added a SQLite-backed mod cache repository with:
  - lookup by `modrinth_version_id`
  - upsert support for resolved Modrinth versions
- Implemented file-aware cache validation so a database row counts as a cache hit only if the corresponding JAR actually exists in `cache/mods/`.
- Implemented conversion helpers from Modrinth version metadata into:
  - persistent cache records
  - pending download entries
- Implemented acquisition planning that checks cache before download and splits resolved versions into:
  - already cached artifacts
  - artifacts that still need downloading
- Added duplicate version deduplication inside acquisition planning so the same version is not downloaded multiple times in one resolution pass.
- Exported the cache module from the Tauri library for later integration with download and launch preparation.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 3 / Module 1: implement instance `mods/` symlink cleanup and recreation from resolved cache artifacts.

## Completed - Phase 3 / Module 1

Scope completed: implement instance `mods/` symlink cleanup and recreation from resolved cache artifacts.

- Added the instance mods preparation module in `src-tauri/src/instance_mods.rs`.
- Implemented cleanup of the instance `mods/` directory before each rebuild of links.
- Implemented recreation of links from `cache/mods/` into the instance `mods/` directory.
- Implemented cross-platform link creation with the required strategy:
  - symlink first
  - hardlink fallback if symlink creation fails
- Added validation that each required cached JAR actually exists before link creation begins.
- Added duplicate filename deduplication so the same cached JAR is not linked more than once in the same preparation pass.
- Returned a structured preparation result containing the linked target paths for later launch orchestration.
- Exported the module from the Tauri library for integration with launch preparation.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 3 / Module 2: implement config cache placement logic for instance `config/` preparation.

## Completed - Phase 3 / Module 2

Scope completed: implement config cache placement logic for instance `config/` preparation.

- Added the instance config preparation module in `src-tauri/src/instance_configs.rs`.
- Implemented placement of cached config sets from `cache/configs/<hash_or_id>/` into the instance `config/` directory.
- Implemented recursive traversal so nested config file trees are preserved when materialized into the instance.
- Implemented file materialization with a flexible strategy:
  - symlink first
  - hardlink fallback
  - direct file copy as a final fallback
- Implemented overwrite behavior for conflicting target config files so newer placement operations can replace earlier ones.
- Intentionally preserved unrelated existing files in the instance `config/` directory so generated runtime configs are not wiped by this preparation step.
- Added validation that every requested cached config directory exists and is a directory before placement.
- Exported the module from the Tauri library for later launch integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 3 / Module 3: implement the Java Agent project scaffold and backend-side attribution ingestion contract.

## Completed - Phase 3 / Module 3

Scope completed: implement the Java Agent project scaffold and backend-side attribution ingestion contract.

- Added the backend attribution contract module in `src-tauri/src/config_attribution.rs`.
- Implemented a file-based NDJSON ingestion contract between Rust and the Java Agent, including:
  - typed attribution event model
  - NDJSON append/read helpers
  - SQLite persistence into `config_attribution`
  - JVM argument generation for launching the agent with output-path and mods-cache-dir system properties
- Added Rust unit tests covering JVM arg generation, NDJSON roundtrip, and SQLite upsert persistence.
- Added a separate Java Agent project scaffold in `java-agent/`.
- Configured the Java Agent project with:
  - Gradle wrapper
  - Java 21 toolchain
  - ByteBuddy dependencies
  - agent manifest entries for `Premain-Class` and `Agent-Class`
- Added Java scaffold classes for:
  - agent bootstrap
  - runtime configuration loading from JVM system properties
  - NDJSON attribution event writing
  - JSON escaping helper
  - attribution event model
- Implemented creation of the agent output file and a no-op ByteBuddy installation path so the project now builds cleanly and is ready for the later interception layer.
- Exported the Rust attribution module from the Tauri library for future launch integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed
- `java-agent/gradlew.bat build` - passed

## Next Planned Module

Phase 4 / Module 1: implement Java runtime detection across system paths and launcher-managed runtimes.

## Completed - Phase 4 / Module 1

Scope completed: implement Java runtime detection across system paths and launcher-managed runtimes.

- Added the Java runtime detection module in `src-tauri/src/java_runtime.rs`.
- Implemented discovery of Java binary candidates from:
  - `PATH`
  - platform-specific Java installation roots
  - launcher-managed `java-runtimes/`
- Implemented support for both standard Java home layouts and macOS-style `Contents/Home/bin/java` layouts.
- Implemented probing of Java binaries via `java -version` using a production command runner.
- Implemented parsing of Java major versions for both legacy and modern version strings.
- Implemented architecture detection from Java runtime output with `x64` and `arm64` normalization.
- Implemented Minecraft-version to required-Java-version mapping according to the project rules.
- Implemented selection of the best matching installed Java runtime for a given Minecraft version.
- Added persistence helpers for syncing discovered Java installations into the existing SQLite `java_installations` table.
- Exported the Java runtime module from the Tauri library for future launch integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 4 / Module 2: implement automatic Java runtime download planning and Adoptium API integration.

## Completed - Phase 4 / Module 2

Scope completed: implement automatic Java runtime download planning and Adoptium API integration.

- Added the Adoptium integration module in `src-tauri/src/adoptium.rs`.
- Implemented an async `AdoptiumClient` for querying the latest Java runtime assets from the Adoptium API.
- Implemented Adoptium URL generation for latest HotSpot JRE asset lookup using:
  - Java feature version
  - operating system
  - architecture
- Implemented typed response models for Adoptium asset metadata and runtime package selection.
- Implemented package selection logic for the latest available JRE asset returned by Adoptium.
- Implemented launcher-side download planning that maps a selected runtime package to:
  - `java-runtimes/java-<version>/` install directory
  - archive destination path under `java-runtimes/`
- Implemented host OS and architecture normalization helpers for Adoptium-compatible values.
- Implemented an async archive download helper for Java runtime packages.
- Exported the Adoptium module from the Tauri library for integration with Java detection and launch preparation.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 4 / Module 3: implement mod loader metadata fetching for Fabric, NeoForge, Forge, and Quilt.

## Completed - Phase 4 / Module 3

Scope completed: implement mod loader metadata fetching for Fabric, NeoForge, Forge, and Quilt.

- Confirmed and applied the chosen metadata strategy:
  - `meta.fabricmc.net` for Fabric
  - `meta.quiltmc.org` for Quilt
  - Prism-style open metadata manifests for Forge and NeoForge
- Added the loader metadata module in `src-tauri/src/loader_metadata.rs`.
- Implemented an async `LoaderMetadataClient` that fetches loader metadata for:
  - Fabric
  - Quilt
  - Forge
  - NeoForge
- Implemented endpoint builders for:
  - Fabric loader version listing and profile fetch
  - Quilt loader version listing and profile fetch
  - Prism package index and version-detail manifests
- Implemented selection logic for loader versions:
  - prefer stable Fabric versions
  - use the latest available Quilt version from Quilt meta
  - prefer recommended Prism versions matching the target Minecraft version, with latest-by-release-time fallback
- Implemented common loader metadata normalization into one Rust shape containing:
  - main class
  - library list
  - optional artifact download metadata
  - JVM arguments
  - game arguments
- Exported the loader metadata module from the Tauri library for later launch-command integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 4 / Module 4: implement complete Java launch command construction from runtime, loader metadata, profiler, wrapper, and agent inputs.

## Completed - Phase 4 / Module 4

Scope completed: implement complete Java launch command construction from runtime, loader metadata, profiler, wrapper, and agent inputs.

- Added the launch command builder module in `src-tauri/src/launch_command.rs`.
- Implemented a typed launch-request model that combines:
  - Java runtime path
  - working directory
  - resolved classpath entries
  - loader metadata
  - RAM settings
  - custom JVM args
  - profiler injection
  - Linux wrapper command
  - config attribution Java Agent input
  - additional game arguments
- Implemented complete JVM argument construction including:
  - loader-provided JVM args
  - `-Xms` and `-Xmx`
  - whitespace-split custom JVM args
  - optional profiler `-agentpath:` injection
  - optional config attribution `-javaagent:` injection and properties
- Implemented classpath joining with the correct platform separator.
- Implemented final command assembly with main class and merged game arguments.
- Implemented Linux-only wrapper command prepending while leaving other platforms unaffected.
- Added validation for invalid memory settings and empty classpath input.
- Exported the launch command module from the Tauri library for future process spawning integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 4 / Module 5: implement Java process spawning with piped stdout/stderr and Tauri event streaming hooks.

## Completed - Phase 4 / Module 5

Scope completed: implement Java process spawning with piped stdout/stderr and Tauri event streaming hooks.

- Added the process streaming module in `src-tauri/src/process_streaming.rs`.
- Implemented process spawning from the previously built launch command using:
  - piped stdout
  - piped stderr
  - configured working directory
- Implemented line-by-line stdout/stderr streaming through a generic Rust event sink abstraction.
- Added Tauri event sink integration using `AppHandle.emit(...)`.
- Defined the first process event hooks for frontend integration:
  - `minecraft-log`
  - `minecraft-exit`
- Implemented a managed process handle that waits for process completion and emits a final exit event with success state and exit code.
- Added cross-platform Rust tests that spawn a real subprocess and verify:
  - stdout line streaming
  - stderr line streaming
  - exit event emission
- Exported the process streaming module from the Tauri library for future Play-button integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 5 / Module 1: implement the Microsoft OAuth 2.0 + PKCE flow bootstrap and account schema integration.

## Completed - Phase 5 / Module 1

Scope completed: implement the Microsoft OAuth 2.0 + PKCE flow bootstrap and account schema integration.

- Chosen PKCE dependency stack: `rand + sha2 + base64`.
- Added the Microsoft auth module in `src-tauri/src/microsoft_auth.rs`.
- Implemented Microsoft OAuth bootstrap support with:
  - PKCE verifier generation
  - PKCE S256 code challenge generation
  - authorization URL construction
  - callback URL parsing with state validation
  - default localhost redirect URI helper
- Added an async `MicrosoftOAuthClient` with bootstrap-ready methods for:
  - starting an OAuth session
  - exchanging an authorization code for tokens
  - refreshing an access token
- Implemented account schema integration through a typed SQLite-backed repository for the existing `accounts` table.
- Added repository support for:
  - account upsert
  - active-account switching
  - active-account loading
  - account listing
- Added helper logic for resolving the active account gamertag as the dynamic `author` source for future `rules.json` integration.
- Added helper support for reading a Microsoft client id from a local `.env`-style file for development bootstrap.
- Exported the Microsoft auth module from the Tauri library for later login-flow integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 5 / Module 2: implement encrypted token storage integration for Microsoft account credentials in SQLite.

## Completed - Phase 5 / Module 2

Scope completed: implement encrypted token storage integration for Microsoft account credentials in SQLite.

- Chosen key-management strategy: OS keyring + AES-GCM.
- Added the encrypted token storage module in `src-tauri/src/token_storage.rs`.
- Added the required crypto and key-management dependencies in `src-tauri/Cargo.toml`:
  - `aes-gcm`
  - `keyring`
- Implemented a secret-store abstraction with a production `KeyringSecretStore` backed by the operating system keyring.
- Implemented AES-256-GCM token encryption with:
  - launcher-managed encryption key material
  - key persistence in the OS keyring
  - versioned encrypted payload format
  - per-token random nonce generation
- Implemented token decryption support for stored SQLite BLOB payloads.
- Added an `EncryptedAccountsRepository` that integrates encryption with the existing `accounts` table and `AccountsRepository` logic.
- Added plaintext account helpers so callers can work with decrypted access and refresh tokens without manually handling encryption details.
- Exported the token storage module from the Tauri library for later login-flow and refresh-token integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 5 / Module 3: implement multi-account management flow helpers on top of encrypted storage and active-account switching.

## Completed - Phase 5 / Module 3

Scope completed: implement multi-account management flow helpers on top of encrypted storage and active-account switching.

- Added the account management service module in `src-tauri/src/account_manager.rs`.
- Implemented a higher-level `AccountManager` on top of encrypted account storage for multi-account workflows.
- Added typed account flow models for:
  - managed account profile metadata
  - managed account tokens
  - lightweight account summaries for UI-facing account selection flows
- Implemented helper methods for:
  - saving a completed account login flow
  - switching the active account
  - listing account summaries
  - reading the active account summary
  - resolving the active account gamertag as the current author name
- Extended `EncryptedAccountsRepository` with active-account switching support to complete the encrypted multi-account stack.
- Exported the account manager module from the Tauri library for later UI integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 5 / Module 4: implement offline account mode helpers with deterministic UUID generation and cached profile fallback.

## Completed - Phase 5 / Module 4

Scope completed: implement offline account mode helpers with deterministic UUID generation and cached profile fallback.

- Added the offline account helper module in `src-tauri/src/offline_account.rs`.
- Added the `uuid` dependency in `src-tauri/Cargo.toml` and implemented deterministic offline UUID generation with UUID v5.
- Implemented offline playable account construction from cached account data using:
  - cached profile JSON when available
  - Xbox gamertag fallback when cached profile JSON does not contain a username
- Implemented cached profile username extraction logic for offline fallback from stored JSON profile blobs.
- Implemented an `OfflineAccountService` on top of encrypted account storage for resolving the active offline-playable account.
- Preserved the specification requirement that offline identity generation is deterministic and never random.
- Exported the offline account module from the Tauri library for later launch integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Phase 6 / Module 1: implement the main UI shell with sidebar for Mod-lists and primary content area.

## Completed - Phase 6 / Module 1

Scope completed: implement the main UI shell with sidebar for Mod-lists and primary content area.

- Replaced the temporary bootstrap landing screen with the first real launcher shell in `src/App.tsx`.
- Implemented a persistent left sidebar dedicated to Mod-list browsing.
- Implemented the primary main content area for the selected Mod-list workspace.
- Added a responsive two-column desktop layout that collapses cleanly into a stacked mobile layout.
- Added shell-level UI placeholders for the next editing and launch modules, including:
  - active account card
  - Mod-list cards
  - workspace header
  - quick launch panel
  - session notes panel
- Preserved the established Cubic visual direction with the existing purple + dark gray palette and minimal transitions.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 2: implement the Mod-list editing view top action bar and local list presentation scaffold.

## Completed - Phase 6 / Module 2

Scope completed: implement the Mod-list editing view top action bar and local list presentation scaffold.

- Expanded `src/App.tsx` from the shell-only layout into the first editor-facing Mod-list workspace.
- Added the required top action bar inside the editing view with:
  - `Add Mod`
  - `Create Group`
  - local `Search`
- Implemented local list filtering behavior in the current SolidJS scaffold so search only filters the visible Mod-list entries.
- Added a first structured Mod-list presentation scaffold with:
  - selectable rows
  - group labels
  - functional/manual/status tags
  - local mod and Modrinth visual distinction
  - inline contextual action strip when entries are selected
- Added collapsed-by-default alternatives presentation with inline expand/collapse behavior and indented fallback ordering.
- Preserved the Cubic visual language while making the workspace feel like an actual launcher editor instead of a static landing page.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 3: implement the Add Mod modal shell with Modrinth search surface and local JAR upload entry point.

## Completed - Phase 6 / Module 3

Scope completed: implement the Add Mod modal shell with Modrinth search surface and local JAR upload entry point.

- Added the Add Mod modal shell in `src/App.tsx`.
- Connected the editing view `Add Mod` action to open a dedicated modal surface.
- Implemented a Modrinth search mode with a focused search field and list-style result cards.
- Implemented a local JAR mode with the visual entry point for manual file upload flow.
- Preserved the separation required by the specification:
  - Mod-list search in the editor stays local-only
  - Modrinth search lives inside the Add Mod modal
- Prepared UI hooks for later wiring of real Modrinth API results, file picker integration, and version assignment for local mods.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 4: implement Aesthetic Groups with collapsible accordions and drag-and-drop organization.

## Completed - Phase 6 / Module 4

Scope completed: implement Aesthetic Groups with collapsible accordions and drag-and-drop organization.

- Added `@thisbeyond/solid-dnd` and integrated it into the frontend editing workspace.
- Implemented Aesthetic Groups in `src/App.tsx` as visual-only accordion sections.
- Added collapse/expand behavior for each Aesthetic Group.
- Added inline group renaming for editable visual section names.
- Added creation of new Aesthetic Groups from the editing action bar.
- Implemented drag-and-drop movement of whole mod blocks between Aesthetic Groups.
- Preserved the required semantic boundary: Aesthetic Groups affect organization only and do not change backend resolution logic.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 5: implement Functional Groups as tag-based units driven by multi-selection.

## Completed - Phase 6 / Module 5

Scope completed: implement Functional Groups as tag-based units driven by multi-selection.

- Added tag-based Functional Group state in `src/App.tsx`.
- Implemented the multi-selection flow using the existing row checkboxes.
- Added a dedicated Functional Groups modal opened from the contextual `Add to Group` action.
- Implemented adding selected mods to existing Functional Groups.
- Implemented creation of new Functional Groups with color tone selection.
- Rendered Functional Group tags inline next to the affected mods in the main editor list.
- Preserved the intended distinction between visual Aesthetic Groups and backend-oriented Functional Groups.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 6: implement the Alternatives panel and fallback ordering UI.

## Completed - Phase 6 / Module 6

Scope completed: implement the Alternatives panel and fallback ordering UI.

- Kept alternatives hidden by default in the main list while preserving inline expand/collapse display.
- Added a dedicated Alternatives side panel in `src/App.tsx` for editing fallback order.
- Implemented drag-and-drop reordering of alternatives using the same Solid drag-and-drop toolkit.
- Kept the parent option pinned as priority 1 while alternatives reorder beneath it.
- Synced reordered alternative state back into the main Mod-list tree presentation.
- Wired both the row-level fallback action and the contextual `Alternatives` action to the dedicated panel.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 7: implement the incompatibility editor with DAG cycle detection.

## Completed - Phase 6 / Module 7

Scope completed: implement the incompatibility editor with DAG cycle detection.

- Added an incompatibility editor modal in `src/App.tsx`.
- Implemented per-pair incompatibility configuration with explicit winner selection.
- Modeled incompatibility priority edges as `winner -> loser` rules in frontend state.
- Implemented cycle detection using a topological-sort style DAG validation pass.
- Blocked saving when a cycle is detected and displayed the required message:
  - `Attention, you have created a priority paradox!`
- Added conflict tags to affected mods in the main list after incompatibility rules are saved.
- Preserved the separation between incompatibility priority rules, Functional Groups, and Aesthetic Groups.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 8: implement Minecraft version and mod loader dropdown selection in the launch area.

## Completed - Phase 6 / Module 8

Scope completed: implement Minecraft version and mod loader dropdown selection in the launch area.

- Added Minecraft version selection state to `src/App.tsx`.
- Added mod loader selection state to `src/App.tsx`.
- Implemented dropdown selectors for both values inside the Quick Launch panel.
- Wired the selected version and loader back into the header status cards so the current launch target is visible across the shell.
- Added a launch-pair summary card to show the currently selected `Minecraft version + loader` combination.
- Updated the Play button label to reflect the active launch target selection.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 9: implement the Play button resolution progress state and launch-ready interaction scaffold.

## Completed - Phase 6 / Module 9

Scope completed: implement the Play button resolution progress state and launch-ready interaction scaffold.

- Added launch target state for the selected Minecraft version and loader in `src/App.tsx`.
- Implemented a Play-button interaction scaffold that simulates the Mod-list resolution flow without yet calling backend commands.
- Added a staged resolution progress card in the Quick Launch area with:
  - progress bar
  - active stage label
  - per-stage descriptive text
  - idle / resolving / ready state badge
- Wired the Play button label to the current launch state so the shell now reflects launch readiness instead of staying static.
- Kept the selected version and loader visible in both the header summary cards and Quick Launch area for a consistent launch target display.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 10: implement the log viewer mini-terminal shell and toggle behavior.

## Completed - Phase 6 / Module 10

Scope completed: implement the log viewer mini-terminal shell and toggle behavior.

- Added log viewer UI state in `src/App.tsx`.
- Added a `Show Log` / `Hide Log` toggle to the editing workspace action bar.
- Implemented a mini-terminal style panel that stays hidden by default and expands inline inside the workspace when toggled on.
- Added staged sample log output that reacts to the current launch progress state so the viewer now behaves like a launch-adjacent terminal surface instead of a static placeholder.
- Styled the viewer as a compact scrollable terminal panel suitable for future Tauri IPC log streaming integration.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 11: implement settings screens for global settings and Mod-list override scaffolding.

## Completed - Phase 6 / Module 11

Scope completed: implement settings screens for global settings and Mod-list override scaffolding.

- Added settings modal state and configuration drafts in `src/App.tsx`.
- Added a dedicated `Settings` action to the editing workspace toolbar.
- Implemented a two-section settings surface with:
  - global launcher defaults
  - Mod-list-specific override scaffolding
- Added global settings fields for:
  - min RAM
  - max RAM
  - custom JVM args
  - profiler toggle
  - Linux wrapper command
  - Java path override
- Added Mod-list override scaffolding with per-setting enable/disable inheritance controls for:
  - min RAM
  - max RAM
  - custom JVM args
  - profiler
  - wrapper command
- Added an inheritance summary panel to explain how global settings and Mod-list overrides combine at launch time.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 12: implement account management UI for login, account switch, and offline indicator scaffolding.

## Completed - Phase 6 / Module 12

Scope completed: implement account management UI for login, account switch, and offline indicator scaffolding.

- Added account UI state and sample account summaries in `src/App.tsx`.
- Upgraded the sidebar active-account card to show:
  - active gamertag
  - online/offline state
  - offline cache / OAuth readiness badge
- Added a `Manage` entry point from the active-account card.
- Implemented an account management modal with:
  - Microsoft login entry button scaffold
  - saved account list
  - one-click active account switching
  - active account status summary
  - explicit offline indicator surface
- Added a toggle action to simulate switching the active account between online and offline states so the offline indicator scaffold can be exercised in the UI.
- Updated workspace notes to reflect the new account management surface.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 6 / Module 13: implement instance export UI with checklist-based packaging options.

## Completed - Phase 6 / Module 13

Scope completed: implement instance export UI with checklist-based packaging options.

- Added export UI state in `src/App.tsx`.
- Added an `Export` action to the editing workspace toolbar.
- Implemented an instance export modal with checklist-based packaging options for:
  - `rules.json`
  - mod JAR files
  - configuration files
  - resource packs
  - other files
- Added export summary feedback that distinguishes a lightweight rules-only export from fuller bundled exports.
- Added action scaffolding for future `.zip` export wiring while preserving the user-facing packaging checklist required by the specification.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 7 / Module 1: implement download progress indicators using the existing event-driven frontend shell.

## Completed - Phase 7 / Module 1

Scope completed: implement download progress indicators using the existing event-driven frontend shell.

- Added download progress UI state in `src/App.tsx`.
- Implemented a download progress panel in the Quick Launch area with per-file progress rows.
- Added visual states for queued, downloading, and completed items.
- Added launch-preview integration so the existing resolution scaffold now also drives visible download progress changes.
- Added frontend event-listening scaffolding for Tauri `download-progress` events using `@tauri-apps/api/event`.
- Added event-driven upsert logic so backend progress events can update or create per-file progress entries when real download IPC is wired in.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 7 / Module 2: implement improved user-facing error state surfaces and launcher error messaging scaffolding.

## Completed - Phase 7 / Module 2

Scope completed: implement improved user-facing error state surfaces and launcher error messaging scaffolding.

- Added launcher error UI state in `src/App.tsx`.
- Added user-facing warning and error surfaces directly into the main workspace instead of leaving issue visibility implicit.
- Implemented a top-level attention banner for the latest launcher issue.
- Implemented an `Error Center` modal with:
  - grouped user-facing issue cards
  - readable summary text
  - more detailed explanatory text
  - dismiss actions
- Added a latest-error card in the Quick Launch area so launch-adjacent problems stay visible near the Play flow.
- Added frontend event-listening scaffolding for Tauri `launcher-error` events using the same event-driven approach already used for download progress.
- Added event-driven error upsert logic so backend launch, account, or download failures can be surfaced in the UI when real IPC is connected.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 7 / Module 3: implement manual-mod warning indicators in the editor and launch surfaces.

## Completed - Phase 7 / Module 3

Scope completed: implement manual-mod warning indicators in the editor and launch surfaces.

- Added manual-mod warning summaries in `src/App.tsx`.
- Implemented explicit warning badges for local/manual mods in the main editor list.
- Added warning badges for local/manual alternatives inside expanded fallback trees.
- Added matching manual warning indicators inside Aesthetic Group cards so local JARs stay visible even while reorganizing the list visually.
- Added a launch-facing manual-mod warning card in the Quick Launch area with a count of current manual mod warnings and a short reminder about dependency review.
- Updated workspace notes to reflect the new manual-mod warning behavior.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 7 / Module 4: implement custom icon and notes UI surfaces for per-instance presentation.

## Completed - Phase 7 / Module 4

Scope completed: implement custom icon and notes UI surfaces for per-instance presentation.

- Added per-instance presentation UI state in `src/App.tsx`.
- Added an `Icon & Notes` action to the editing workspace toolbar.
- Implemented an instance presentation preview card in the right-side workspace column.
- Implemented a dedicated modal for editing per-instance presentation details, including:
  - custom icon preview scaffold
  - icon label placeholder editing
  - icon accent/name editing
  - notes editing surface
  - future custom PNG upload entry point
- Kept the UI aligned with the specification requirement that icon and notes live as Mod-list-specific assets.
- Updated workspace notes to reflect the new per-instance presentation surface.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Phase 7 / Module 5: prepare the UI for cross-platform verification notes and final refinement state.

## Completed - Phase 7 / Module 5

Scope completed: prepare the UI for cross-platform verification notes and final refinement state.

- Added verification UI state in `src/App.tsx`.
- Added a `Verify` action to the editing workspace toolbar.
- Implemented a `Cross-Platform Readiness` summary card in the workspace side column covering:
  - Windows symlink checks
  - Linux wrapper expectations
  - macOS path review
- Implemented a dedicated verification modal with platform-specific review notes for Windows, Linux and macOS.
- Added a final refinement summary panel to reflect that the remaining work is validation-focused rather than structural.
- Updated workspace notes to reflect the new verification surface.

## Verification

- `npm run build` - passed
- `cargo check` - passed

## Next Planned Module

Implementation plan completed through Phase 7.

## Completed - Post-Plan Module 1

Scope completed: add the first real Tauri shell-data integration layer for Mod-lists, active account, and settings.

- Added a new backend shell snapshot module in `src-tauri/src/app_shell.rs`.
- Implemented `load_shell_snapshot_command` and registered it in `src-tauri/src/lib.rs`.
- Implemented real backend loading for:
  - Mod-list summaries from `mod-lists/*/rules.json`
  - active account summary from SQLite
  - global settings from `global_settings`
  - selected Mod-list overrides from `modlist_settings`
- Added Rust tests covering empty and populated shell snapshot loading.
- Added a `modlists_dir` accessor in `src-tauri/src/launcher_paths.rs` for command-side path resolution.
- Wired the SolidJS shell in `src/App.tsx` to call the Tauri command on startup.
- Replaced parts of the previous mock shell state with command-backed data for:
  - Mod-list sidebar cards
  - selected workspace Mod-list title
  - active account summary
  - global settings draft state
  - Mod-list override draft state
- Preserved existing mock fallbacks for surfaces that are not yet fully backend-driven.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Post-Plan Module 2: connect the account management UI and Mod-list selection flows to real Tauri-backed data mutations.

## Completed - Post-Plan Module 2

Scope completed: connect the account management UI and settings/Mod-list selection flows to real Tauri-backed data mutations.

- Extended `src-tauri/src/app_shell.rs` with real mutation commands for:
  - switching the active account
  - saving global settings
  - saving Mod-list overrides
  - loading shell snapshots for an explicitly selected Mod-list
- Registered the new commands in `src-tauri/src/lib.rs`.
- Added Rust tests covering settings persistence and explicit Mod-list snapshot selection.
- Updated `src/App.tsx` so the frontend now:
  - reloads a shell snapshot for the selected Mod-list
  - switches the active account through Tauri
  - saves global settings and Mod-list overrides through Tauri
  - refreshes command-backed shell data after successful mutations
- Added UI-side launcher error messages when command-backed mutations fail.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Post-Plan Module 3: connect the launch UI shell to real backend play/log/progress events instead of simulated launch preview state.

## Completed - Post-Plan Module 3

Scope completed: connect the launch UI shell to real backend play/log/progress events instead of relying only on frontend simulation.

- Added a backend-driven launch preview event module in `src-tauri/src/launch_preview.rs`.
- Implemented `start_launch_preview_command` and registered it in `src-tauri/src/lib.rs`.
- Added backend-emitted event flow for:
  - launch progress stages
  - download progress updates
  - Minecraft log stream events
  - launch completion event
- Updated the frontend in `src/App.tsx` so the Play flow now:
  - invokes a Tauri command when running inside Tauri
  - listens for backend-driven launch progress events
  - listens for backend-driven log events
  - listens for backend-driven exit events
  - updates progress cards, log viewer, and download indicators from emitted events
- Kept a browser-safe fallback path so the UI still works outside Tauri during pure frontend builds.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Post-Plan Module 4: replace more mock editor data with persisted Mod-list-backed data and connect editor mutations to backend commands.

## Completed - Post-Plan Module 4

Scope completed: replace more mock editor data with persisted Mod-list-backed data for the selected Mod-list.

- Added a new backend editor data module in `src-tauri/src/editor_data.rs`.
- Implemented `load_modlist_editor_command` and registered it in `src-tauri/src/lib.rs`.
- Added backend mapping from `rules.json` into editor-facing rows, including:
  - primary rule rows
  - fallback alternatives
  - derived local/modrinth kind
  - derived editor notes
  - derived tags such as `Alternative`, `Manual`, `Abort`, and `Conflict Set`
- Added Rust tests covering editor snapshot generation from a real `rules.json` sample.
- Updated `src/App.tsx` so the editor list now loads command-backed Mod-list rows for the selected Mod-list instead of relying only on static mock entries.
- Preserved the existing frontend fallback state for environments where the Tauri command is unavailable.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Post-Plan Module 5: add backend-backed editor mutation commands for selected Mod-list rule editing flows.

## Completed - Post-Plan Module 5

Scope completed: add the first backend-backed editor mutation command for selected Mod-list rule editing flows.

- Extended `src-tauri/src/editor_data.rs` with `save_alternative_order_command`.
- Implemented backend persistence for alternative ordering by rewriting the selected rule's fallback option order inside `rules.json`.
- Added backend input models for alternative ordering mutations.
- Added Rust tests covering persisted alternative reordering and reloading of updated editor state.
- Registered the new mutation command in `src-tauri/src/lib.rs`.
- Updated `src/App.tsx` so drag-and-drop alternative reordering now persists through Tauri when running inside the launcher.
- Added frontend error handling and backend reload behavior around alternative-order saves.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Post-Plan Module 6: add backend-backed editor mutation commands for incompatibility and rule metadata editing flows.

## Completed - Post-Plan Module 6

Scope completed: add backend-backed incompatibility persistence for the current editor flow.

- Extended `src-tauri/src/editor_data.rs` with `save_incompatibilities_command`.
- Implemented backend persistence of incompatibility rules by rewriting primary-option `exclude_if_present` values in `rules.json`.
- Added backend derivation of incompatibility edges back into editor snapshot data so saved conflicts roundtrip into the UI.
- Added Rust tests covering incompatibility persistence and snapshot roundtrip behavior.
- Registered the incompatibility mutation command in `src-tauri/src/lib.rs`.
- Updated `src/App.tsx` so saving incompatibilities now persists through Tauri and then reloads editor-backed state.
- Kept the existing DAG validation on the frontend and added backend reload/error handling around persistence failures.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed

## Next Planned Module

Post-Plan Module 7: add backend-backed mutation commands for additional editor flows such as local rule metadata and grouping persistence.

## Completed - Post-Plan Module 7

Scope completed: add backend-backed mutation commands for additional editor flows beyond alternatives and incompatibilities.

- Extended `src-tauri/src/editor_data.rs` with three new mutation commands:
  - `add_mod_rule_command`: appends a new Rule containing a single Modrinth or local mod to `rules.json`, including validation that local mods carry a `file_name`.
  - `delete_rules_command`: removes a set of rules identified by their editor row IDs from `rules.json`, processing deletions from the end to avoid index shifting.
  - `rename_rule_command`: updates the `rule_name` of a single rule identified by its editor row ID in `rules.json`.
- Added input model types for all three commands: `AddModRuleInput`, `DeleteRulesInput`, `RenameRuleInput`.
- Registered all three new commands in `src-tauri/src/lib.rs`.
- Added Rust unit tests covering:
  - Modrinth rule addition and editor snapshot roundtrip.
  - Local JAR rule addition and editor snapshot roundtrip.
  - Multi-rule deletion with correct index handling.
  - Rule rename and editor snapshot roundtrip.
- Updated `src/App.tsx` with three new async handler functions:
  - `handleAddModrinthRule`: invokes `add_mod_rule_command` on the selected Mod-list, with an optimistic frontend-only fallback for browser preview.
  - `handleDeleteSelectedRules`: invokes `delete_rules_command` for all currently selected row IDs, with an optimistic update before the backend call.
  - `handleRenameRule`: invokes `rename_rule_command` for the targeted rule, with an optimistic name update before persistence.
- Wired `handleAddModrinthRule` to the "Add to List" button in the Add Mod modal for every Modrinth result card.
- Wired `handleDeleteSelectedRules` to the "Delete" button in the contextual actions strip.
- Wired `handleRenameRule` to the "Edit Rule" button via a new rename modal opened by `openRenameRule`.
- Added a rename rule modal with a text input, Enter-key shortcut, and Save/Cancel actions.
- All three backend calls reload the editor snapshot after mutation and push UI errors on failure.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (89 tests)

## Next Planned Module

Post-Plan Module 8: wire the Add Mod local JAR flow to a real file-picker Tauri command and cache placement, and connect the "Create New Mod-list" button to a backend creation command.

## Completed - Post-Plan Module 8

Scope completed: wire the local JAR upload flow to a native file-picker command with cache placement, and connect "Create New Mod-list" to a backend creation command.

- Added `tauri-plugin-dialog = "2"` to `src-tauri/Cargo.toml` and registered it in `lib.rs`.
- Installed `@tauri-apps/plugin-dialog` npm package.
- Added `"dialog:allow-open"` permission to `src-tauri/capabilities/default.json`.
- Added `mods_cache_dir()` public accessor to `src-tauri/src/launcher_paths.rs`.
- Created a new `src-tauri/src/modlist_manager.rs` module with:
  - `CreateModlistInput` + `create_modlist_from_root` + `create_modlist_command`:
    - Creates `mod-lists/<name>/rules.json` with an empty skeleton (name, author, description).
    - Rejects duplicate names.
    - Falls back to `"Author"` when the author field is blank.
  - `CopyLocalJarInput` + `copy_local_jar_from_root` + `copy_local_jar_command`:
    - Validates that the selected file has a `.jar` extension.
    - Copies the JAR to `cache/mods/` with `fs::copy`.
    - Derives the rule name from the filename stem when the caller leaves it blank.
    - Calls `add_mod_rule_from_root` to append a local rule to the selected Mod-list's `rules.json`.
- Registered both new commands in `lib.rs`.
- Added 6 Rust unit tests covering skeleton creation, duplicate rejection, blank-author fallback, JAR copy + rule roundtrip, filename-stem fallback, and non-JAR rejection.
- Updated `src/App.tsx`:
  - Added state signals for the Create Mod-list modal (name, author, description, busy flag) and `localJarRuleName`.
  - Added `handleCreateModlist`: calls `create_modlist_command`, reloads shell + editor snapshots, with an optimistic browser-only fallback.
  - Added `handleUploadLocalJar`: uses `@tauri-apps/plugin-dialog` `open()` to show a native `.jar` file picker, then calls `copy_local_jar_command` and reloads the editor snapshot.
  - Wired the "Create New Mod-list" sidebar button to open the new creation modal.
  - Replaced the static "Upload Local JAR" entry point with a live button that calls `handleUploadLocalJar`, and added an optional rule name input above it.
  - Added a full Create Mod-list modal with name (required), author, and description fields, an Enter-key shortcut, a busy state, and Save/Cancel actions.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (95 tests)

## Next Planned Module

Post-Plan Module 9: wire the full end-to-end Play flow by integrating the resolver, cache checks, download, loader metadata, launch command construction, and process spawning into a single Tauri command path that replaces the current launch preview stub.

## Completed - Post-Plan UI Module 2

Scope completed: close the remaining visible non-auth GUI gaps by wiring the unfinished controls, restoring missing mod-list presentation surfaces, and giving export a real backend path.

- Updated `src/components/Header.tsx` so the custom window chrome now performs real minimize, maximize/unmaximize, and close actions through Tauri window APIs with browser-safe fallback behavior.
- Updated `src/components/LaunchPanel.tsx` and `src/App.tsx` so the bottom account switcher now routes through the existing backend-backed `handleSwitchAccount` flow instead of mutating only local frontend state.
- Updated `src/components/ModRuleItem.tsx` to expose a visible top-level Rename action that opens the existing rename modal instead of leaving the rename flow hidden.
- Added backend-backed mod-list presentation persistence in new module `src-tauri/src/modlist_assets.rs`:
  - `load_modlist_presentation_command`
  - `save_modlist_presentation_command`
  - `export_modlist_command`
- Added a new `modlist-presentation.json` per-mod-list asset file so icon label, accent label, and notes now persist with the mod-list instead of living only in transient UI state.
- Updated `src/App.tsx` to load presentation state whenever the selected mod-list changes, save it through the new backend command, and wire export through a native save dialog plus backend ZIP creation.
- Added `InstancePresentationModal` in `src/components/Modals.tsx` so the existing Notes button now opens a real modal with preview, editable badge label, accent label, and notes field.
- Updated `ExportModal` in `src/components/Modals.tsx` so Export now performs a real async action and shows progress while the archive is being built.
- Implemented ZIP export in `src-tauri/src/modlist_assets.rs` for:
  - `rules.json`
  - saved mod-list presentation metadata
  - optional cached mod JARs
  - optional cached config files
  - optional instance resource packs
  - optional remaining mod-list files
- Added Rust tests for mod-list presentation load/save defaults and ZIP export coverage.
- Hid the unfinished group-creation entry points from `src/components/ActionBar.tsx` so the GUI no longer advertises draft-only group flows that still lack a complete persisted interaction model.

## Verification

- `npm run build` - passed
- `cargo fmt && cargo check` - passed
- `cargo test` - passed (103 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 10

Scope completed: extend visual grouping into the alternatives workflow and refine incompatibility editing so a focused child mod does not offer its own parent as a conflict target.

- Extended aesthetic group persistence with optional `scopeRowId` / `scope_row_id` so visual groups can now belong either to the main top-level mod list or to a specific alternatives list.
- Updated `src/App.tsx` group serialization, deserialization, and ID remapping so scoped alternative groups persist and survive row ID changes.
- Updated `src/store.ts` so top-level editor group sections only render unscoped groups, preserving the main page behavior while allowing scoped groups elsewhere.
- Added alternative-scope visual grouping in `src/components/Modals.tsx`:
  - alternative rows can now be selected in the Alternatives panel
  - `Create Group` now works inside that panel for the currently selected alternatives
  - grouped alternative rows render under their own visual section headers while keeping the existing fallback order model intact
- Updated the incompatibility modal in `src/components/Modals.tsx` so when the focused mod is an alternative/child, its top-level parent is no longer shown as a possible conflict target.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (106 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 9

Scope completed: replace the still-unreliable main-page group drag layer with a simpler manual pointer-driven grouping interaction and add direct group deletion controls.

- Replaced the main mods page visual-group drag implementation in `src/components/ModListEditor.tsx` with a manual pointer-driven drag model instead of the previous `solid-dnd` / native HTML drag hybrid.
- The new flow tracks:
  - dragged row ID
  - hovered drop target
  - pointer position for a floating drag preview
  - global pointer-up completion through window listeners
- This keeps the grouping logic deterministic and avoids the cross-container drag failures that were blocking moves between ungrouped rows and visual groups.
- Added a floating drag preview card during group moves in `src/components/ModListEditor.tsx`.
- Updated `src/components/ModRuleItem.tsx` so the grip icon directly starts the manual group-drag interaction for top-level rows.
- Added `removeAestheticGroup(id)` in `src/store.ts`.
- Added a remove-group `X` control to the group header in `src/components/ModListEditor.tsx`.
- Updated the group list rendering so empty aesthetic groups are no longer shown in the main editor list.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (106 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 8

Scope completed: fix the remaining main-page visual-group drag regression after the earlier nested alternatives and drag overlay work.

- Updated `src/components/ModListEditor.tsx` to use `mostIntersecting` collision detection for the grouped main editor drag surface instead of `closestCenter`, which is more reliable for cross-group container drops.
- Added drag-start, drag-over, and raw drag-end tracing for the main editor visual-group drag path so future regressions can be diagnosed from `debug-trace.txt` without guesswork.
- Increased the effective drop target stability by giving group and ungrouped drop zones a minimum height.
- Restored a larger always-visible top-level drag handle in `src/components/ModRuleItem.tsx` so starting a drag on the main mods page is more reliable than the previous hover-only handle.
- Kept the pointer-events-safe drag overlay behavior from the previous debug pass.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (106 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan Debug Module 1

Scope completed: add persistent trace logging for the currently broken alternatives and visual-group drag/drop flows so live repro steps can be captured to a plain text file and inspected after a user test run.

- Added backend debug trace support in new module `src-tauri/src/debug_trace.rs`.
- Registered two new Tauri commands in `src-tauri/src/lib.rs`:
  - `append_debug_trace_command`
  - `clear_debug_trace_command`
- Trace output now writes to `debug-trace.txt` inside the launcher app-data root.
- Added frontend helper `src/lib/debugTrace.ts` for browser-safe trace writes.
- Instrumented the following frontend flows with trace entries:
  - app boot and initial mod-list load in `src/App.tsx`
  - alternatives add / remove / reorder handlers in `src/App.tsx`
  - alternatives panel open, drag, add, remove actions in `src/components/Modals.tsx`
  - main editor visual-group drag/drop in `src/components/ModListEditor.tsx`
- Instrumented the following backend flows with trace entries in `src-tauri/src/editor_data.rs`:
  - `save_alternative_order_from_root`
  - `add_alternative_from_root`
  - `add_nested_alternative_from_root`
  - `remove_alternative_from_root`
- The trace file is cleared on Tauri app boot and then repopulated for the current test session.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (105 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 7

Scope completed: remove the remaining practical limits in the alternatives editor and restore main-page drag/drop after the recent drag overlay and nested alternative work.

- Updated `src/App.tsx` to make row ID rebuilding, ID remapping, and optimistic alternative reordering recursive instead of only handling one alternative depth.
- Updated `src/App.tsx` add/remove alternative handlers so the open alternatives panel stays locked to the same logical row after live reloads, even when rule indices change.
- Updated `src/components/ModListEditor.tsx` and `src/components/Modals.tsx` drag overlays to ignore pointer events, which restores dropping mods in and out of aesthetic groups on the main editor page.
- Updated `src-tauri/src/editor_data.rs` so `save_alternative_order_from_root` now supports reordering alternatives for nested alternative rows, not only top-level rules.
- The nested alternative model already introduced in `rules.rs` / `editor_data.rs` now behaves consistently in real time at arbitrary depth instead of effectively flattening or desynchronizing after a few levels.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (105 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 6

Scope completed: persist both visual editor groups and functional tag groups across app restarts instead of keeping them only in transient frontend memory.

- Added backend group-layout persistence in `src-tauri/src/modlist_assets.rs` with:
  - `load_modlist_groups_command`
  - `save_modlist_groups_command`
  - a new per-mod-list `modlist-editor-groups.json` asset file
- Registered the new commands in `src-tauri/src/lib.rs`.
- Extended archive export to include the saved group layout file alongside the existing presentation metadata.
- Added Rust coverage for group-layout defaults and roundtrip persistence, bringing the backend suite to 105 tests.
- Updated `src/App.tsx` to:
  - load saved groups whenever a mod-list is opened
  - autosave group changes back to the backend when visual/function groups change
  - remap saved group references after reorder, delete, and rename operations so persisted group membership stays aligned with the current row IDs
- This fixes the restart issue you reported: closing and reopening the app now preserves both aesthetic groups and functional tags for the selected mod-list.

## Verification

- `npm run build` - passed
- `cargo fmt && cargo check` - passed
- `cargo test` - passed (105 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 5

Scope completed: restore the functional group/tag UI that had been accidentally removed while trimming unfinished editor surfaces.

- Re-added the contextual `Add Tag` action in `src/components/ActionBar.tsx` so selected mods can once again be grouped under functional tags.
- Re-wired `setFunctionalGroupModalOpen(true)` from the action bar so the existing functional group creation flow is reachable again.
- Re-added `FunctionalGroupModal` to `src/App.tsx`, restoring the actual modal render path for custom functional tags.
- This keeps the recent duplicate-name validation in place, so functional groups are visible again without reintroducing the duplicate-name bug.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (103 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 4

Scope completed: fix two remaining editor UX issues in the visual/function-group system by restoring a reliable way to drag rows back out of visual groups and preventing duplicate group/tag names.

- Updated `src/components/ModListEditor.tsx` so the ungrouped section now remains visible whenever visual groups exist, even if it currently contains zero rows.
- This restores a stable drop target for grouped rows, which fixes dragging mods back out of aesthetic groups into the ungrouped area.
- Updated `src/store.ts` to reject duplicate visual group names during rename with a warning shown through the existing UI error surface.
- Updated `src/store.ts` to reject duplicate functional group names during creation, again using the existing launcher warning surface instead of silently allowing duplicates.
- Name matching is now case-insensitive and trim-aware, so values like `Performance`, ` performance `, and `PERFORMANCE` are treated as the same group/tag name.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (103 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 3

Scope completed: restore the visual-only aesthetic grouping workflow and fix the drag/drop behavior so mods can actually be placed into and moved out of those groups.

- Re-added the `Create Group` entry point in `src/components/ActionBar.tsx` so users can once again create aesthetic groups for editor organization.
- Updated `src/components/ModListEditor.tsx` to support group-aware drag/drop behavior instead of treating every drop as a flat top-level reorder.
- Added drop zones for:
  - individual visual groups
  - the ungrouped section
- Fixed the grouping flow so users can now:
  - create an empty visual group
  - drag mods into that group
  - reorder mods within a group
  - drag mods back out to the ungrouped section
  - move mods between groups
- Kept aesthetic groups frontend-only, matching their intended UX-only role, while still rebuilding the visible row order and syncing the resulting top-level order through the existing reorder path.
- Added clearer empty drop-target messaging inside empty groups and in the ungrouped area when grouped rows exist.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (103 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan Module 9

Scope completed: replace the fake launch preview with a real backend preparation pipeline that resolves the selected Mod-list, downloads missing Modrinth artifacts, prepares the instance directory, selects Java, builds the launch command, and hands off to real process spawning when the required client JAR is present.

- Reworked `src-tauri/src/launch_preview.rs` from a canned event demo into a real `start_launch_command` flow.
- The new launch command now:
  - loads the selected Mod-list and effective launch settings
  - prefetches compatible Modrinth versions for rule evaluation
  - runs the real resolver against the selected Minecraft version + loader target
  - resolves required Modrinth dependencies through the existing dependency module
  - checks the SQLite-backed mod cache and downloads any missing remote JARs into `cache/mods/`
  - persists downloaded version metadata and dependency links back into SQLite
  - prepares the instance `mods/` directory using links from cache rather than copies
  - fetches loader metadata and materializes loader libraries under the instance directory
  - discovers/persists Java installations and selects a matching runtime (or honors Java Path Override)
  - substitutes common launcher placeholders in loader JVM/game arguments before command construction
  - builds a real Java launch command and spawns it through the existing process streaming module
- Added new launch helper coverage in `src-tauri/src/launch_preview.rs` for loader parsing, settings merge, Maven artifact path derivation, instance path building, and placeholder substitution.
- Updated `src-tauri/src/lib.rs` to register `start_launch_command` instead of the old preview command.
- Added `configs_cache_dir()` and `java_runtimes_dir()` accessors to `src-tauri/src/launcher_paths.rs` so the launch pipeline can reuse managed app-local storage paths.
- Updated `src-tauri/src/process_streaming.rs` so exit events serialize `exitCode` in the format the frontend expects.
- Updated the frontend launch wiring:
  - `src/App.tsx` now invokes `start_launch_command`
  - exit handling returns the launch UI to `idle`
  - the bottom bar treats `running` as an active launch state without leaving the Play button permanently disabled after process exit
  - `src/lib/types.ts`, `src/store.ts`, and `src/components/LaunchPanel.tsx` now understand the `running` state
- Important current limitation: the new launch path now uses real backend orchestration, but it still requires a valid instance-side `minecraft/client.jar`; automatic vanilla client/natives acquisition is not implemented yet, so the launcher now fails honestly instead of simulating success when that prerequisite is missing.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (100 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan UI Module 1

Scope completed: persist incompatibility edits through the real backend command path instead of only copying draft state in the frontend store.

- Updated `src/components/Modals.tsx` so `IncompatibilitiesModal` now accepts an async `onSave` handler from `App.tsx` and shows a saving state while persistence is in flight.
- Updated `src/App.tsx` with `handleSaveIncompatibilities`, which now:
  - reads the current incompatibility draft from store state
  - invokes `save_incompatibilities_command`
  - updates saved incompatibility state only after a successful write
  - closes the modal and reloads the editor snapshot after persistence
  - pushes a real UI error if the backend save fails
- Kept a browser-safe fallback path so the modal still behaves sensibly outside Tauri.
- Removed the old frontend-only `saveIncompatibilities()` helper from `src/store.ts` to avoid future confusion between draft-only and persisted save behavior.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (100 tests)

## Next Planned Module

Post-Plan Module 10: complete the real launch handoff by acquiring/provisioning the vanilla Minecraft client artifacts and natives that the new backend launch pipeline still expects to exist locally.

## Completed - Post-Plan Stability Module 1

Scope completed: move launcher runtime data out of the project working tree and into a Tauri-managed app-local data directory so user-triggered writes no longer look like source changes during development.

- Updated `src-tauri/src/lib.rs` startup to resolve the launcher root from `app.path().app_local_data_dir()` instead of `std::env::current_dir()`.
- Managed the resolved `LauncherPaths` as Tauri application state during setup so commands can reuse the same app-local storage root.
- Added a `root_dir()` accessor to `src-tauri/src/launcher_paths.rs` for command-side reuse.
- Updated command entry points in:
  - `src-tauri/src/app_shell.rs`
  - `src-tauri/src/editor_data.rs`
  - `src-tauri/src/modlist_manager.rs`
  so they now read the managed `LauncherPaths` from Tauri `State` instead of recomputing paths from the current working directory.
- Preserved all existing pure `*_from_root(...)` helpers and tests, so filesystem behavior remains testable while the live app now writes to OS-managed launcher storage.

## Verification

- `npm run build` - passed
- `cargo check` - passed
- `cargo test` - passed (95 tests)

## Next Planned Module

Post-Plan Module 9: wire the full end-to-end Play flow by integrating the resolver, cache checks, download, loader metadata, launch command construction, and process spawning into a single Tauri command path that replaces the current launch preview stub.
