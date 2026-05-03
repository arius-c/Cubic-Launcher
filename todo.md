# Cubic Launcher — TODO

## Completed

- **Modrinth User-Agent** — added `cubic-launcher/0.1.0 (https://github.com/arius-c/Cubic-Launcher)` in `modrinth.rs` `build_http_client`
- **Limit Modrinth API concurrency** — `tokio::sync::Semaphore` with 10 permits in `resolver.rs` around the `JoinSet` fetch loop
- **429 rate-limit handling** — `send_with_retry` helper in `modrinth.rs` parses `Retry-After` (default 2s, cap 10s), retries once; applied to all three fetch methods
- **User-visible errors for skipped mods** — `prefetch_compatible_versions_for_selected` in `launch_preview.rs` now takes `app_handle` and emits skip errors via `emit_log(..., Stderr, ...)` instead of `eprintln!`
- **Parallel mod downloads** — `download_pending_artifacts` in `launch_preview.rs` now runs downloads concurrently via `JoinSet` + `Semaphore(10)`
- **SHA1 verification of mod downloads** — `DownloadArtifact` now carries `file_hash: Option<String>`; downloads with a hash reuse `minecraft_downloader::download_file_verified` (delete + error on mismatch)
- **Granular download progress** — mod downloads now stream chunks to disk and update `launch-progress` by aggregate downloaded bytes from 42→58 while preserving SHA1 verification
- **300+ mods launch warning** — the launch pipeline now emits a frontend warning before large launches; it suggests enabling cache-only mode when appropriate
- **Cache-only mode** — added global setting `cache_only_mode`; when enabled the launcher prefers compatible cached mod/dependency artifacts and stored dependency links before calling Modrinth, with API fallback only on cache misses

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
