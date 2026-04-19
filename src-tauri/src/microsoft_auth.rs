use std::path::Path;

use anyhow::{anyhow, bail, Context, Result};
use base64::{engine::general_purpose::URL_SAFE_NO_PAD, Engine as _};
use rand::rngs::OsRng;
use rand::RngCore;
use reqwest::Url;
use rusqlite::{params, Connection, OptionalExtension};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

const MICROSOFT_AUTHORIZE_URL: &str = "https://login.live.com/oauth20_authorize.srf";
const MICROSOFT_TOKEN_URL: &str = "https://login.live.com/oauth20_token.srf";

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrosoftOAuthConfig {
    pub client_id: String,
    pub redirect_uri: String,
    pub scopes: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct MicrosoftOAuthSession {
    pub state: String,
    pub code_verifier: String,
    pub code_challenge: String,
    pub authorization_url: Url,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuthorizationCallback {
    pub code: String,
    pub state: String,
}

#[derive(Debug, Clone)]
pub struct MicrosoftOAuthClient {
    http_client: reqwest::Client,
    authorize_url: String,
    token_url: String,
}

impl MicrosoftOAuthClient {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
            authorize_url: MICROSOFT_AUTHORIZE_URL.to_string(),
            token_url: MICROSOFT_TOKEN_URL.to_string(),
        }
    }

    pub fn with_endpoints(authorize_url: impl Into<String>, token_url: impl Into<String>) -> Self {
        Self {
            http_client: reqwest::Client::new(),
            authorize_url: authorize_url.into(),
            token_url: token_url.into(),
        }
    }

    pub fn start_session(&self, config: &MicrosoftOAuthConfig) -> Result<MicrosoftOAuthSession> {
        validate_oauth_config(config)?;

        let state = generate_url_safe_random_bytes(16);
        let code_verifier = generate_url_safe_random_bytes(32);
        let code_challenge = build_pkce_code_challenge(&code_verifier);
        let authorization_url =
            build_authorization_url(&self.authorize_url, config, &state, &code_challenge)?;

        Ok(MicrosoftOAuthSession {
            state,
            code_verifier,
            code_challenge,
            authorization_url,
        })
    }

    pub async fn exchange_authorization_code(
        &self,
        config: &MicrosoftOAuthConfig,
        callback: &AuthorizationCallback,
        code_verifier: &str,
    ) -> Result<MicrosoftTokenResponse> {
        validate_oauth_config(config)?;

        let response = self
            .http_client
            .post(&self.token_url)
            .form(&[
                ("client_id", config.client_id.as_str()),
                ("redirect_uri", config.redirect_uri.as_str()),
                ("grant_type", "authorization_code"),
                ("code", callback.code.as_str()),
                ("code_verifier", code_verifier),
                ("scope", &join_scopes(&config.scopes)),
            ])
            .send()
            .await
            .context("failed to exchange Microsoft authorization code")?
            .error_for_status()
            .context("Microsoft token endpoint returned an error")?;

        response
            .json::<MicrosoftTokenResponse>()
            .await
            .context("failed to deserialize Microsoft token response")
    }

    pub async fn refresh_access_token(
        &self,
        config: &MicrosoftOAuthConfig,
        refresh_token: &str,
    ) -> Result<MicrosoftTokenResponse> {
        validate_oauth_config(config)?;

        let response = self
            .http_client
            .post(&self.token_url)
            .form(&[
                ("client_id", config.client_id.as_str()),
                ("redirect_uri", config.redirect_uri.as_str()),
                ("grant_type", "refresh_token"),
                ("refresh_token", refresh_token),
                ("scope", &join_scopes(&config.scopes)),
            ])
            .send()
            .await
            .context("failed to refresh Microsoft access token")?
            .error_for_status()
            .context("Microsoft refresh token endpoint returned an error")?;

        response
            .json::<MicrosoftTokenResponse>()
            .await
            .context("failed to deserialize Microsoft refresh response")
    }
}

impl Default for MicrosoftOAuthClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct MicrosoftTokenResponse {
    pub token_type: String,
    pub scope: Option<String>,
    pub expires_in: u64,
    pub access_token: String,
    pub refresh_token: Option<String>,
    pub user_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AccountRecord {
    pub microsoft_id: String,
    pub xbox_gamertag: Option<String>,
    pub minecraft_uuid: Option<String>,
    pub access_token_enc: Option<Vec<u8>>,
    pub refresh_token_enc: Option<Vec<u8>>,
    pub profile_data: Option<String>,
    pub is_active: bool,
}

pub struct AccountsRepository<'connection> {
    connection: &'connection Connection,
}

impl<'connection> AccountsRepository<'connection> {
    pub fn new(connection: &'connection Connection) -> Self {
        Self { connection }
    }

    pub fn upsert_account(&self, account: &AccountRecord) -> Result<()> {
        self.connection.execute(
            r#"
            INSERT INTO accounts (
                microsoft_id,
                xbox_gamertag,
                minecraft_uuid,
                access_token_enc,
                refresh_token_enc,
                last_login,
                profile_data,
                is_active
            ) VALUES (?1, ?2, ?3, ?4, ?5, CURRENT_TIMESTAMP, ?6, ?7)
            ON CONFLICT(microsoft_id) DO UPDATE SET
                xbox_gamertag = excluded.xbox_gamertag,
                minecraft_uuid = excluded.minecraft_uuid,
                access_token_enc = excluded.access_token_enc,
                refresh_token_enc = excluded.refresh_token_enc,
                last_login = CURRENT_TIMESTAMP,
                profile_data = excluded.profile_data,
                is_active = excluded.is_active
            "#,
            params![
                &account.microsoft_id,
                &account.xbox_gamertag,
                &account.minecraft_uuid,
                &account.access_token_enc,
                &account.refresh_token_enc,
                &account.profile_data,
                account.is_active,
            ],
        )?;

        if account.is_active {
            self.set_active_account(&account.microsoft_id)?;
        }

        Ok(())
    }

    pub fn set_active_account(&self, microsoft_id: &str) -> Result<()> {
        let transaction = self.connection.unchecked_transaction()?;
        transaction.execute("UPDATE accounts SET is_active = FALSE", [])?;
        let updated_rows = transaction.execute(
            "UPDATE accounts SET is_active = TRUE WHERE microsoft_id = ?1",
            [microsoft_id],
        )?;

        if updated_rows == 0 {
            bail!("account '{}' does not exist", microsoft_id);
        }

        transaction.commit()?;
        Ok(())
    }

    pub fn update_profile_data(&self, microsoft_id: &str, profile_data: &str) -> Result<()> {
        self.connection.execute(
            "UPDATE accounts SET profile_data = ?1 WHERE microsoft_id = ?2",
            params![profile_data, microsoft_id],
        )?;
        Ok(())
    }

    pub fn delete_account(&self, microsoft_id: &str) -> Result<()> {
        let deleted = self.connection.execute(
            "DELETE FROM accounts WHERE microsoft_id = ?1",
            [microsoft_id],
        )?;
        if deleted == 0 {
            bail!("account '{}' does not exist", microsoft_id);
        }
        Ok(())
    }

    pub fn load_active_account(&self) -> Result<Option<AccountRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                microsoft_id,
                xbox_gamertag,
                minecraft_uuid,
                access_token_enc,
                refresh_token_enc,
                profile_data,
                is_active
            FROM accounts
            WHERE is_active = TRUE
            LIMIT 1
            "#,
        )?;

        statement
            .query_row([], |row| {
                Ok(AccountRecord {
                    microsoft_id: row.get(0)?,
                    xbox_gamertag: row.get(1)?,
                    minecraft_uuid: row.get(2)?,
                    access_token_enc: row.get(3)?,
                    refresh_token_enc: row.get(4)?,
                    profile_data: row.get(5)?,
                    is_active: row.get(6)?,
                })
            })
            .optional()
            .map_err(Into::into)
    }

    pub fn list_accounts(&self) -> Result<Vec<AccountRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                microsoft_id,
                xbox_gamertag,
                minecraft_uuid,
                access_token_enc,
                refresh_token_enc,
                profile_data,
                is_active
            FROM accounts
            ORDER BY last_login DESC, id DESC
            "#,
        )?;

        let accounts = statement
            .query_map([], |row| {
                Ok(AccountRecord {
                    microsoft_id: row.get(0)?,
                    xbox_gamertag: row.get(1)?,
                    minecraft_uuid: row.get(2)?,
                    access_token_enc: row.get(3)?,
                    refresh_token_enc: row.get(4)?,
                    profile_data: row.get(5)?,
                    is_active: row.get(6)?,
                })
            })?
            .collect::<rusqlite::Result<Vec<_>>>()
            .map_err(anyhow::Error::from)?;

        Ok(accounts)
    }
}

pub fn build_authorization_url(
    authorize_url: &str,
    config: &MicrosoftOAuthConfig,
    state: &str,
    code_challenge: &str,
) -> Result<Url> {
    validate_oauth_config(config)?;

    let mut url = Url::parse(authorize_url)
        .with_context(|| format!("invalid Microsoft authorize URL '{authorize_url}'"))?;

    url.query_pairs_mut()
        .append_pair("client_id", &config.client_id)
        .append_pair("response_type", "code")
        .append_pair("redirect_uri", &config.redirect_uri)
        .append_pair("scope", &join_scopes(&config.scopes))
        .append_pair("code_challenge", code_challenge)
        .append_pair("code_challenge_method", "S256")
        .append_pair("state", state);

    Ok(url)
}

pub fn parse_authorization_callback(
    callback_url: &str,
    expected_state: &str,
) -> Result<AuthorizationCallback> {
    let url = Url::parse(callback_url)
        .with_context(|| format!("invalid Microsoft callback URL '{callback_url}'"))?;
    let query_pairs = url.query_pairs().collect::<Vec<_>>();

    if let Some(error) = query_pairs
        .iter()
        .find(|(key, _)| key == "error")
        .map(|(_, value)| value.to_string())
    {
        let description = query_pairs
            .iter()
            .find(|(key, _)| key == "error_description")
            .map(|(_, value)| value.to_string())
            .unwrap_or_default();
        let message = format!("Microsoft OAuth error: {} {}", error, description)
            .trim()
            .to_string();
        return Err(anyhow!(message));
    }

    let code = query_pairs
        .iter()
        .find(|(key, _)| key == "code")
        .map(|(_, value)| value.to_string())
        .context("callback is missing authorization code")?;
    let state = query_pairs
        .iter()
        .find(|(key, _)| key == "state")
        .map(|(_, value)| value.to_string())
        .context("callback is missing state")?;

    if state != expected_state {
        bail!("callback state mismatch");
    }

    Ok(AuthorizationCallback { code, state })
}

pub fn build_pkce_code_challenge(code_verifier: &str) -> String {
    let digest = Sha256::digest(code_verifier.as_bytes());
    URL_SAFE_NO_PAD.encode(digest)
}

pub fn generate_url_safe_random_bytes(byte_count: usize) -> String {
    let mut bytes = vec![0_u8; byte_count];
    OsRng.fill_bytes(&mut bytes);
    URL_SAFE_NO_PAD.encode(bytes)
}

pub fn validate_oauth_config(config: &MicrosoftOAuthConfig) -> Result<()> {
    if config.client_id.trim().is_empty() {
        bail!("client_id cannot be empty");
    }

    if config.redirect_uri.trim().is_empty() {
        bail!("redirect_uri cannot be empty");
    }

    if config.scopes.is_empty() {
        bail!("scopes cannot be empty");
    }

    Url::parse(&config.redirect_uri)
        .with_context(|| format!("invalid redirect_uri '{}'", config.redirect_uri))?;

    Ok(())
}

pub fn join_scopes(scopes: &[String]) -> String {
    scopes.join(" ")
}

pub fn default_loopback_redirect_uri(port: u16) -> String {
    format!("http://127.0.0.1:{port}/callback")
}

pub fn default_agent_author_name(connection: &Connection) -> Result<Option<String>> {
    AccountsRepository::new(connection)
        .load_active_account()
        .map(|account| account.and_then(|entry| entry.xbox_gamertag))
}

// ── Xbox Live / XSTS / Minecraft auth chain ─────────────────────────────────

const XBOX_LIVE_AUTH_URL: &str = "https://user.auth.xboxlive.com/user/authenticate";
const XSTS_AUTH_URL: &str = "https://xsts.auth.xboxlive.com/xsts/authorize";
const MINECRAFT_AUTH_URL: &str = "https://api.minecraftservices.com/authentication/login_with_xbox";
const MINECRAFT_PROFILE_URL: &str = "https://api.minecraftservices.com/minecraft/profile";

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct XboxLiveResponse {
    token: String,
    display_claims: XboxDisplayClaims,
}

#[derive(Debug, Clone, Deserialize)]
struct XboxDisplayClaims {
    xui: Vec<XboxUserInfo>,
}

#[derive(Debug, Clone, Deserialize)]
struct XboxUserInfo {
    uhs: String,
    #[serde(default)]
    gtg: Option<String>,
}

#[derive(Debug, Clone, Deserialize)]
struct MinecraftAuthResponse {
    access_token: String,
    expires_in: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinecraftProfile {
    pub id: String,
    pub name: String,
}

/// Full auth chain: Microsoft access token → Xbox Live → XSTS → Minecraft token + profile.
pub struct MinecraftAuthChain {
    http_client: reqwest::Client,
}

#[derive(Debug, Clone)]
pub struct MinecraftLoginResult {
    pub microsoft_id: String,
    pub xbox_gamertag: Option<String>,
    pub minecraft_uuid: String,
    pub minecraft_username: String,
    pub minecraft_access_token: String,
    pub microsoft_refresh_token: Option<String>,
}

impl MinecraftAuthChain {
    pub fn new() -> Self {
        Self {
            http_client: reqwest::Client::new(),
        }
    }

    pub async fn authenticate(
        &self,
        ms_access_token: &str,
        ms_refresh_token: Option<&str>,
        ms_user_id: Option<&str>,
    ) -> Result<MinecraftLoginResult> {
        // Step 1: Xbox Live
        let xbox = self.authenticate_xbox_live(ms_access_token).await?;
        let uhs = xbox
            .display_claims
            .xui
            .first()
            .context("Xbox Live response missing user hash")?;
        let gamertag = uhs.gtg.clone();
        let user_hash = uhs.uhs.clone();

        // Step 2: XSTS
        let xsts = self.authenticate_xsts(&xbox.token).await?;
        let xsts_token = xsts.token;

        // Step 3: Minecraft
        let mc_auth = self.authenticate_minecraft(&user_hash, &xsts_token).await?;

        // Step 4: Profile
        let profile = self.fetch_minecraft_profile(&mc_auth.access_token).await?;

        Ok(MinecraftLoginResult {
            microsoft_id: ms_user_id.unwrap_or(&profile.id).to_string(),
            xbox_gamertag: gamertag,
            minecraft_uuid: profile.id,
            minecraft_username: profile.name,
            minecraft_access_token: mc_auth.access_token,
            microsoft_refresh_token: ms_refresh_token.map(String::from),
        })
    }

    async fn authenticate_xbox_live(&self, ms_access_token: &str) -> Result<XboxLiveResponse> {
        let body = serde_json::json!({
            "Properties": {
                "AuthMethod": "RPS",
                "SiteName": "user.auth.xboxlive.com",
                "RpsTicket": format!("d={ms_access_token}")
            },
            "RelyingParty": "http://auth.xboxlive.com",
            "TokenType": "JWT"
        });

        self.http_client
            .post(XBOX_LIVE_AUTH_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .context("failed to contact Xbox Live")?
            .error_for_status()
            .context("Xbox Live authentication failed")?
            .json()
            .await
            .context("failed to parse Xbox Live response")
    }

    async fn authenticate_xsts(&self, xbox_token: &str) -> Result<XboxLiveResponse> {
        let body = serde_json::json!({
            "Properties": {
                "SandboxId": "RETAIL",
                "UserTokens": [xbox_token]
            },
            "RelyingParty": "rp://api.minecraftservices.com/",
            "TokenType": "JWT"
        });

        self.http_client
            .post(XSTS_AUTH_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .context("failed to contact XSTS")?
            .error_for_status()
            .context("XSTS authentication failed")?
            .json()
            .await
            .context("failed to parse XSTS response")
    }

    async fn authenticate_minecraft(
        &self,
        user_hash: &str,
        xsts_token: &str,
    ) -> Result<MinecraftAuthResponse> {
        let body = serde_json::json!({
            "identityToken": format!("XBL3.0 x={user_hash};{xsts_token}")
        });

        self.http_client
            .post(MINECRAFT_AUTH_URL)
            .header("Content-Type", "application/json")
            .header("Accept", "application/json")
            .json(&body)
            .send()
            .await
            .context("failed to contact Minecraft services")?
            .error_for_status()
            .context("Minecraft authentication failed")?
            .json()
            .await
            .context("failed to parse Minecraft auth response")
    }

    async fn fetch_minecraft_profile(&self, mc_access_token: &str) -> Result<MinecraftProfile> {
        self.http_client
            .get(MINECRAFT_PROFILE_URL)
            .header("Authorization", format!("Bearer {mc_access_token}"))
            .send()
            .await
            .context("failed to fetch Minecraft profile")?
            .error_for_status()
            .context("Minecraft profile request failed")?
            .json()
            .await
            .context("failed to parse Minecraft profile")
    }
}

// ── Login command with local callback server ────────────────────────────────

/// Desktop redirect URI required by the official Minecraft client ID.
pub const DESKTOP_REDIRECT_URI: &str = "https://login.live.com/oauth20_desktop.srf";

/// Start Microsoft login using the desktop redirect flow.
///
/// 1. Opens a local page that redirects to Microsoft login.
/// 2. After login, Microsoft redirects to oauth20_desktop.srf?code=...
/// 3. A JS snippet on the local intermediate page catches the code and
///    POSTs it back to our local server.
///
/// Since the desktop.srf page is on a different origin we can't read its
/// URL from JS. Instead, we open the MS auth URL directly and serve a
/// local "paste your URL" page as fallback. Actually, the cleanest
/// approach: open auth URL directly, start a local server, and after
/// the desktop.srf redirect the browser shows a blank-ish page with the
/// code in the address bar. We show a small local page asking the user
/// to copy-paste it. But even better: we use the Tauri webview to open
/// a window and watch navigation.
///
/// Simplest robust approach: open a Tauri dialog/window is complex.
/// Let's use the approach that many open-source launchers use:
/// Open auth URL directly → user logs in → lands on desktop.srf?code=...
/// → we ask them to paste the URL back.
pub async fn run_microsoft_login(client_id: &str) -> Result<MinecraftLoginResult> {
    use tokio::net::TcpListener;

    let config = MicrosoftOAuthConfig {
        client_id: client_id.to_string(),
        redirect_uri: DESKTOP_REDIRECT_URI.to_string(),
        scopes: vec!["XboxLive.signin".into(), "offline_access".into()],
    };

    let oauth_client = MicrosoftOAuthClient::new();
    let session = oauth_client.start_session(&config)?;

    // Start local server that:
    // 1. Serves a page with JS that opens the MS auth URL in the same tab
    // 2. After MS login, redirect lands on desktop.srf?code=...&state=...
    // 3. The page shows instructions to copy the URL
    // 4. A second local endpoint receives the pasted URL
    let listener = TcpListener::bind("127.0.0.1:0")
        .await
        .context("failed to bind local OAuth server")?;
    let port = listener.local_addr()?.port();

    let auth_url = session.authorization_url.to_string();

    // Open the local helper page in the browser.
    open::that(format!("http://127.0.0.1:{port}/login"))
        .context("failed to open system browser")?;

    // Serve requests until we get the callback URL.
    let callback_url = serve_login_flow(&listener, &auth_url, port).await?;

    // Parse and exchange.
    let callback = parse_authorization_callback(&callback_url, &session.state)?;
    let ms_tokens = oauth_client
        .exchange_authorization_code(&config, &callback, &session.code_verifier)
        .await?;

    // Full auth chain: Xbox Live → XSTS → Minecraft.
    let chain = MinecraftAuthChain::new();
    chain
        .authenticate(
            &ms_tokens.access_token,
            ms_tokens.refresh_token.as_deref(),
            ms_tokens.user_id.as_deref(),
        )
        .await
}

async fn serve_login_flow(
    listener: &tokio::net::TcpListener,
    auth_url: &str,
    port: u16,
) -> Result<String> {
    use tokio::io::{AsyncReadExt, AsyncWriteExt};

    let login_page = format!(
        r#"<!DOCTYPE html><html><head><meta charset="utf-8"><title>Cubic Launcher Login</title></head>
<body style="font-family:system-ui;background:#1e1e2e;color:#cdd6f4;display:flex;align-items:center;justify-content:center;height:100vh;margin:0">
<div style="text-align:center;max-width:500px;padding:20px">
<h2 style="margin-bottom:8px">Cubic Launcher</h2>
<p style="color:#a6adc8;margin-bottom:20px">Click the button below to sign in with Microsoft. After logging in, you'll be redirected to a page — <b>copy the full URL</b> from your browser's address bar and paste it below.</p>
<a href="{auth_url}" style="display:inline-block;background:#89b4fa;color:#1e1e2e;padding:12px 24px;border-radius:8px;text-decoration:none;font-weight:600;margin-bottom:24px">Sign in with Microsoft</a>
<div style="margin-top:16px">
<input id="url" type="text" placeholder="Paste the redirect URL here..." style="width:100%;padding:10px;border-radius:6px;border:1px solid #45475a;background:#313244;color:#cdd6f4;font-size:14px;box-sizing:border-box" />
<button onclick="submit()" style="margin-top:8px;width:100%;padding:10px;border-radius:6px;border:none;background:#a6e3a1;color:#1e1e2e;font-weight:600;font-size:14px;cursor:pointer">Complete Login</button>
<p id="err" style="color:#f38ba8;margin-top:8px;font-size:13px"></p>
</div>
</div>
<script>
function submit(){{
  var u=document.getElementById('url').value.trim();
  if(!u||!u.includes('code=')){{ document.getElementById('err').textContent='Paste the full URL that contains "code=" in it.'; return; }}
  fetch('http://127.0.0.1:{port}/callback',{{method:'POST',body:u}}).then(function(){{
    document.body.innerHTML='<div style="display:flex;align-items:center;justify-content:center;height:100vh"><h2>Login Successful! You can close this tab.</h2></div>';
  }}).catch(function(e){{ document.getElementById('err').textContent='Error: '+e; }});
}}
</script></body></html>"#
    );

    loop {
        let (mut stream, _) =
            tokio::time::timeout(std::time::Duration::from_secs(300), listener.accept())
                .await
                .context("Microsoft login timed out (5 minutes)")?
                .context("failed to accept connection")?;

        let mut buf = vec![0_u8; 16384];
        let n = stream.read(&mut buf).await.unwrap_or(0);
        let request = String::from_utf8_lossy(&buf[..n]);

        let first_line = request.lines().next().unwrap_or("");

        if first_line.starts_with("GET /login") {
            // Serve the login helper page.
            let resp = format!(
                "HTTP/1.1 200 OK\r\nContent-Type: text/html; charset=utf-8\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{}",
                login_page.len(),
                login_page
            );
            let _ = stream.write_all(resp.as_bytes()).await;
        } else if first_line.starts_with("POST /callback") {
            // The body contains the pasted URL.
            let body = request
                .split("\r\n\r\n")
                .nth(1)
                .unwrap_or("")
                .trim()
                .to_string();

            let resp = "HTTP/1.1 200 OK\r\nContent-Length: 2\r\nAccess-Control-Allow-Origin: *\r\nConnection: close\r\n\r\nOK";
            let _ = stream.write_all(resp.as_bytes()).await;

            if body.contains("code=") {
                return Ok(body);
            }
        } else if first_line.starts_with("OPTIONS") {
            // CORS preflight.
            let resp = "HTTP/1.1 200 OK\r\nAccess-Control-Allow-Origin: *\r\nAccess-Control-Allow-Methods: POST\r\nAccess-Control-Allow-Headers: *\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes()).await;
        } else {
            let resp = "HTTP/1.1 404 Not Found\r\nContent-Length: 0\r\nConnection: close\r\n\r\n";
            let _ = stream.write_all(resp.as_bytes()).await;
        }
    }
}

pub fn microsoft_client_id_from_env(env_file_path: &Path) -> Result<Option<String>> {
    if !env_file_path.exists() {
        return Ok(None);
    }

    let contents = std::fs::read_to_string(env_file_path)
        .with_context(|| format!("failed to read {}", env_file_path.display()))?;

    for line in contents.lines() {
        if let Some(value) = line.strip_prefix("MICROSOFT_CLIENT_ID=") {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Ok(Some(trimmed.to_string()));
            }
        }
    }

    Ok(None)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::database::initialize_database;

    use super::{
        build_authorization_url, build_pkce_code_challenge, default_agent_author_name,
        default_loopback_redirect_uri, generate_url_safe_random_bytes, join_scopes,
        microsoft_client_id_from_env, parse_authorization_callback, validate_oauth_config,
        AccountRecord, AccountsRepository, MicrosoftOAuthClient, MicrosoftOAuthConfig,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-microsoft-auth-test-{timestamp}"))
    }

    fn sample_config() -> MicrosoftOAuthConfig {
        MicrosoftOAuthConfig {
            client_id: "test-client-id".into(),
            redirect_uri: "http://127.0.0.1:43821/callback".into(),
            scopes: vec!["XboxLive.signin".into(), "offline_access".into()],
        }
    }

    #[test]
    fn pkce_code_challenge_matches_known_reference() {
        let verifier = "dBjftJeZ4CVP-mB92K27uhbUJU1p1r_wW1gFWFOEjXk";
        let challenge = build_pkce_code_challenge(verifier);

        assert_eq!(challenge, "E9Melhoa2OwvFrEMTJguCHaoeK1t8URWbuGJSstw-cM");
    }

    #[test]
    fn start_session_builds_valid_authorization_url_and_pkce_pair() {
        let client = MicrosoftOAuthClient::new();
        let session = client
            .start_session(&sample_config())
            .expect("session should build");

        assert!(!session.state.is_empty());
        assert!(session.code_verifier.len() >= 43);
        assert_eq!(
            session.code_challenge,
            build_pkce_code_challenge(&session.code_verifier)
        );
        assert_eq!(
            session
                .authorization_url
                .query_pairs()
                .find(|(key, _)| key == "response_type")
                .unwrap()
                .1,
            "code"
        );
    }

    #[test]
    fn builds_authorization_url_with_expected_query_parameters() {
        let url = build_authorization_url(
            "https://login.live.com/oauth20_authorize.srf",
            &sample_config(),
            "state123",
            "challenge456",
        )
        .expect("authorization url should build");

        let query_pairs = url.query_pairs().collect::<Vec<_>>();
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "client_id" && value == "test-client-id"));
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "state" && value == "state123"));
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "code_challenge" && value == "challenge456"));
        assert!(query_pairs
            .iter()
            .any(|(key, value)| key == "scope" && value == "XboxLive.signin offline_access"));
    }

    #[test]
    fn parses_authorization_callback_and_validates_state() {
        let callback = parse_authorization_callback(
            "http://127.0.0.1:43821/callback?code=abc123&state=state123",
            "state123",
        )
        .expect("callback should parse");

        assert_eq!(callback.code, "abc123");
        assert_eq!(callback.state, "state123");
    }

    #[test]
    fn rejects_callback_state_mismatch() {
        let error = parse_authorization_callback(
            "http://127.0.0.1:43821/callback?code=abc123&state=wrong-state",
            "state123",
        )
        .expect_err("callback should fail");

        assert!(error.to_string().contains("callback state mismatch"));
    }

    #[test]
    fn validates_oauth_config() {
        validate_oauth_config(&sample_config()).expect("config should be valid");
        assert_eq!(
            join_scopes(&sample_config().scopes),
            "XboxLive.signin offline_access"
        );
        assert_eq!(
            default_loopback_redirect_uri(43821),
            "http://127.0.0.1:43821/callback"
        );
    }

    #[test]
    fn generated_random_value_is_url_safe() {
        let value = generate_url_safe_random_bytes(32);
        assert!(value.chars().all(|character| {
            character.is_ascii_alphanumeric() || character == '-' || character == '_'
        }));
    }

    #[test]
    fn account_repository_upserts_lists_and_switches_active_account() {
        let root_dir = unique_test_root();
        let database_path = root_dir.join("launcher_data.db");

        initialize_database(&database_path).expect("database should initialize");
        let connection = Connection::open(&database_path).expect("database should open");
        let repository = AccountsRepository::new(&connection);

        repository
            .upsert_account(&AccountRecord {
                microsoft_id: "account-a".into(),
                xbox_gamertag: Some("PlayerA".into()),
                minecraft_uuid: Some("uuid-a".into()),
                access_token_enc: Some(vec![1, 2, 3]),
                refresh_token_enc: Some(vec![4, 5, 6]),
                profile_data: Some("{\"name\":\"PlayerA\"}".into()),
                is_active: true,
            })
            .expect("first account should insert");
        repository
            .upsert_account(&AccountRecord {
                microsoft_id: "account-b".into(),
                xbox_gamertag: Some("PlayerB".into()),
                minecraft_uuid: Some("uuid-b".into()),
                access_token_enc: Some(vec![7, 8, 9]),
                refresh_token_enc: Some(vec![10, 11, 12]),
                profile_data: Some("{\"name\":\"PlayerB\"}".into()),
                is_active: false,
            })
            .expect("second account should insert");

        repository
            .set_active_account("account-b")
            .expect("active account should switch");

        let active_account = repository
            .load_active_account()
            .expect("active account query should succeed")
            .expect("active account should exist");
        let accounts = repository.list_accounts().expect("accounts should list");

        assert_eq!(accounts.len(), 2);
        assert_eq!(active_account.microsoft_id, "account-b");
        assert_eq!(active_account.xbox_gamertag.as_deref(), Some("PlayerB"));
        assert_eq!(
            default_agent_author_name(&connection).expect("author query should succeed"),
            Some("PlayerB".into())
        );

        drop(connection);
        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn loads_client_id_from_env_style_file() {
        let root_dir = unique_test_root();
        let env_path = root_dir.join(".env");

        fs::create_dir_all(&root_dir).expect("root dir should exist");
        fs::write(
            &env_path,
            "MICROSOFT_CLIENT_ID=test-client-id\nOTHER_KEY=value\n",
        )
        .expect("env file should be written");

        let client_id = microsoft_client_id_from_env(&env_path)
            .expect("env file should parse")
            .expect("client id should exist");

        assert_eq!(client_id, "test-client-id");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }
}
