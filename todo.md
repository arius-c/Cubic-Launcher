# Cubic Launcher — TODO

## Completed

- **Modrinth User-Agent** — added `cubic-launcher/0.1.0 (https://github.com/arius-c/Cubic-Launcher)` in `modrinth.rs` `build_http_client`
- **Limit Modrinth API concurrency** — `tokio::sync::Semaphore` with 10 permits in `resolver.rs` around the `JoinSet` fetch loop
- **429 rate-limit handling** — `send_with_retry` helper in `modrinth.rs` parses `Retry-After` (default 2s, cap 10s), retries once; applied to all three fetch methods
- **User-visible errors for skipped mods** — `prefetch_compatible_versions_for_selected` in `launch_preview.rs` now takes `app_handle` and emits skip errors via `emit_log(..., Stderr, ...)` instead of `eprintln!`
- **Parallel mod downloads** — `download_pending_artifacts` in `launch_preview.rs` now runs downloads concurrently via `JoinSet` + `Semaphore(10)`
- **SHA1 verification of mod downloads** — `DownloadArtifact` now carries `file_hash: Option<String>`; downloads with a hash reuse `minecraft_downloader::download_file_verified` (delete + error on mismatch)
- **Aggregate download progress** — each completed download bumps `launch-progress` linearly from 42→58 with `"Downloading Mods"` / `"Downloaded N of M mods."` (per-file `download-progress` event removed — nothing listened to it)

---

## To do (medium priority)

### Microsoft Auth — switch to Authorization Code Flow
- Currently: Device Code Flow (the user has to manually copy a link and a code)
- Target: Authorization Code Flow with PKCE (opens system browser → redirect to localhost → captures token)
- Cost: zero. Just configure the redirect URI in the Azure AD app
- Much better UX: one click on "Sign in with Microsoft" instead of copy-paste
- Check in `microsoft_auth.rs` how the current flow is implemented

### Public GitHub repo
- Consider privacy: the username `MattiasMalavolti` is a real name → evaluate creating a pseudonym/organization
- Audit for secrets in the git history before publishing (`gitleaks detect --source . -v`)
- Add a license: **GPL-3.0 recommended** (standard for Minecraft launchers, protects against malicious forks)
  - Create a `LICENSE` file at the root with the GPL-3.0 text
  - Cost: zero, it's just a text file
- Add a basic README (what it does, screenshots, how to build)
- Don't publish in a rush just for the User-Agent — do it when ready
- Once public: update the User-Agent with the repo link

### Windows SmartScreen signing
- Without signing: "Unknown publisher" warning for users
- SignPath.io offers free code signing for open source (requires a public repo)
- To be done after the repo is public

---

## Suggested improvements (from Claude browser, not yet implemented)

### 1. Parallel mod downloads
- Refactor `download_pending_artifacts` to use `tokio::task::JoinSet`
- Download all pending JARs in parallel instead of sequentially
- Do NOT touch the Modrinth API calls (version resolution) — only the download phase

### 2. SHA1 verification of downloads
- After each JAR download, verify the SHA1 hash against `PendingDownload.file_hash`
- If the hash doesn't match: delete the file and return a clear error
- Follow the `download_file_verified` pattern already present in `minecraft_downloader.rs`

### 3. Granular download progress
- Replace the current 0%/100% progress with real byte-level progress
- Use `response.bytes_stream()` and `tokio::io`
- Emit progress events as chunks get written
- The frontend receives continuous updates during each download

### 4. Warning for 300+ mods
- Before starting the launch, count the mods in the resolved modlist
- If > 300: emit a warning to the frontend like:
  "You have more than 300 mods. The first launch may take 1-2 minutes due to API rate limits. You can enable 'Use local cache only' in settings to skip API checks on future launches."

### 5. Cache-only mode (do last)
- Add a boolean setting `cache_only_mode` (follow the existing naming convention)
- **Default: OFF** — current behavior unchanged for anyone who doesn't touch settings
- When enabled: skip Modrinth API calls for mods that already have a valid cache entry (version_id present in mod_cache AND file exists on disk)
- Fall back to the API only for mods without a cache entry
- Wire it into the existing settings system and expose it to the frontend

---

## Decisions made (do not implement)

### Incremental check of the mods folder — NO
- Currently: full wipe + relink every launch (`instance_mods.rs:29`)
- Considered adding a diff/check of already-present mods
- Decision: leave as is. Too many edge cases (broken links, modified jars, manual files) for minimal gain (a few ms on SSD)

### Modrinth calls API every launch for version prefetch — known, not urgent
- `prefetch_compatible_versions_for_selected` (`launch_preview.rs:662`) calls Modrinth for every mod, every launch, even if the JAR is already in cache
- The cache (`mod_cache`) is indexed by `modrinth_version_id`, not by `(project_id, mc, loader)` → the API is needed to know WHICH version_id to look up
- Point 5 (cache-only mode) would solve this for users who want fast/offline launches

---

## Context notes

### How the mod system currently works
- A `Rule` has a `mod_id` (Modrinth slug, e.g. "sodium") and a `source` (Modrinth or Local)
- The launcher calls `GET /project/{mod_id}/version?loaders=[...]&game_versions=[...]`
- Modrinth returns the canonical JAR filename → the launcher downloads it or uses it from the cache
- The launcher does NOT parse internal JAR metadata — it fully trusts Modrinth
- For Local mods: the mod_id → JAR association is static, provided by the user

### Symlink vs hardlink
- `instance_mods.rs:101`: tries symlink first, hardlink as fallback
- On Windows without dev mode/admin: symlink fails → uses hardlink
- On Linux/Mac: uses symlink
- Performance: equivalent for practical use

### Platform costs
- GitHub (repo, releases, CI for public repos): free
- Open source license: free (text file)
- Modrinth API: free up to rate limits
- User-Agent: free (HTTP string)
- Apple Developer (for macOS signing): ~90 EUR/year — only needed if distributing on macOS
- Windows signing: free with SignPath.io for open source, otherwise ~150-300 EUR/year
- Azure AD for Microsoft auth: free
