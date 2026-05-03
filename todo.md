# Cubic Launcher — TODO

## Completed

- **Public GitHub repo cleanup** — repo history was squashed to a single public `Beta` commit, local databases/tooling/agent files were removed from tracking, `.gitignore` was expanded, and GPL-3.0 `LICENSE` + README notes were added
- **Modrinth User-Agent** — added `cubic-launcher/0.1.0 (https://github.com/arius-c/Cubic-Launcher)` in `modrinth.rs` `build_http_client`
- **Limit Modrinth API concurrency** — `tokio::sync::Semaphore` with 10 permits in `resolver.rs` around the `JoinSet` fetch loop
- **429 rate-limit handling** — `send_with_retry` helper in `modrinth.rs` parses `Retry-After` (default 2s, cap 10s), retries once; applied to all three fetch methods
- **User-visible errors for skipped mods** — `prefetch_compatible_versions_for_selected` in `launch_preview.rs` now takes `app_handle` and emits skip errors via `emit_log(..., Stderr, ...)` instead of `eprintln!`
- **Parallel mod downloads** — `download_pending_artifacts` in `launch_preview.rs` now runs downloads concurrently via `JoinSet` + `Semaphore(10)`
- **SHA1 verification of mod downloads** — `DownloadArtifact` now carries `file_hash: Option<String>`; downloads with a hash reuse `minecraft_downloader::download_file_verified` (delete + error on mismatch)
- **Granular download progress** — mod downloads now stream chunks to disk and update `launch-progress` by aggregate downloaded bytes from 42→58 while preserving SHA1 verification
- **300+ mods launch warning** — the launch pipeline now emits a frontend warning before large launches; it suggests enabling cache-only mode when appropriate
- **Cache-only mode** — added global setting `cache_only_mode`; when enabled the launcher prefers compatible cached mod/dependency artifacts and stored dependency links before calling Modrinth, with API fallback only on cache misses
- **Microsoft Auth Authorization Code + PKCE** — browser login now redirects to `http://localhost/callback`; the Entra AppID exists and has been submitted for Minecraft Services AppID review. Current blocker is Minecraft Services approval (`Invalid app registration`)

---

## To do (medium priority)

### Microsoft Auth AppID review
- Wait for Minecraft Services AppID approval for `7dce88aa-79b0-4b77-9666-0fdb8addd50c`
- After approval, retest Microsoft login and launch online mode
- If approval is rejected or delayed, add a user-facing fallback path that explains the AppID approval requirement

### Windows SmartScreen signing
- Without signing, users can see "Unknown publisher" / "Windows protected your PC"
- Apply for free open-source signing through SignPath Foundation now that the repo is public
- Add a release workflow that builds the Tauri Windows bundle from a clean GitHub Actions environment
- Sign release artifacts and attach them to GitHub Releases
- Expect SmartScreen reputation to build over time; signing improves publisher identity but may not remove warnings immediately

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
- Windows signing: free options may be available for open source through SignPath Foundation; otherwise paid OV/EV or Microsoft Trusted Signing are typical options
- Azure AD for Microsoft auth: free
