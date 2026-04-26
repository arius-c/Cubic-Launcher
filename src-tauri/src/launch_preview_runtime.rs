use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};
use rusqlite::Connection;

use crate::adoptium::{
    host_adoptium_os, normalize_adoptium_architecture, plan_runtime_download, AdoptiumClient,
};
use crate::java_runtime::{
    discover_java_installations, persist_java_installations, select_java_for_requirement,
    CommandJavaBinaryInspector, JavaBinaryInspector,
};
use crate::launcher_paths::LauncherPaths;
use crate::loader_metadata::{LoaderLibrary, LoaderMetadata};
use crate::offline_account::deterministic_offline_uuid;
use crate::process_streaming::ProcessLogStream;
use crate::resolver::ResolutionTarget;

use super::{
    download_file, emit_log, emit_progress, EffectiveLaunchSettings, LaunchPlaceholders,
    PlayerIdentity,
};

pub(super) async fn materialize_loader_libraries(
    http_client: &reqwest::Client,
    library_root: &Path,
    loader_metadata: &LoaderMetadata,
) -> Result<Vec<PathBuf>> {
    let mut paths = Vec::new();

    for library in &loader_metadata.libraries {
        let relative_path = relative_loader_library_path(library)?;
        let destination_path = library_root.join(&relative_path);

        if !destination_path.exists() {
            // Determine the download URL: prefer explicit download artifact,
            // fall back to constructing from Maven base URL + coordinates.
            let download_url = if let Some(download) = &library.download {
                download.url.clone()
            } else if let Some(base_url) = &library.url {
                let base = base_url.trim_end_matches('/');
                format!(
                    "{base}/{}",
                    relative_path.to_string_lossy().replace('\\', "/")
                )
            } else {
                continue; // no way to obtain this library
            };

            download_file(http_client, &download_url, &destination_path).await?;
        }

        paths.push(destination_path);
    }

    Ok(paths)
}

pub(super) fn relative_loader_library_path(library: &LoaderLibrary) -> Result<PathBuf> {
    if let Some(download) = &library.download {
        if let Some(path) = &download.path {
            return Ok(PathBuf::from(path));
        }
    }

    maven_artifact_relative_path(&library.name)
}

pub(super) fn maven_artifact_relative_path(coordinates: &str) -> Result<PathBuf> {
    let parts = coordinates.split(':').collect::<Vec<_>>();
    if parts.len() < 3 {
        bail!("invalid Maven coordinates '{coordinates}'");
    }

    let group = parts[0].replace('.', "/");
    let artifact = parts[1];
    let version = parts[2];
    let classifier = parts.get(3).copied();
    let file_name = match classifier {
        Some(classifier) if !classifier.trim().is_empty() => {
            format!("{artifact}-{version}-{classifier}.jar")
        }
        _ => format!("{artifact}-{version}.jar"),
    };

    Ok(PathBuf::from(group)
        .join(artifact)
        .join(version)
        .join(file_name))
}

pub(super) async fn load_player_identity(launcher_paths: &LauncherPaths) -> Result<PlayerIdentity> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;

    // Read the raw active account; no decryption is needed because we use profile_data.
    // Extract all data from DB BEFORE any async work (Connection is not Send).
    let raw_account = {
        use crate::microsoft_auth::AccountsRepository as RawRepo;
        RawRepo::new(&connection)
            .load_active_account()
            .ok()
            .flatten()
    };
    let db_path = launcher_paths.database_path().to_path_buf();
    drop(connection); // Release connection before async work.

    if let Some(raw) = raw_account {
        let profile = raw
            .profile_data
            .as_deref()
            .and_then(|pd| serde_json::from_str::<serde_json::Value>(pd).ok());

        let username = profile
            .as_ref()
            .and_then(|v| v.get("username").and_then(|u| u.as_str()).map(String::from))
            .or(raw.xbox_gamertag.clone())
            .unwrap_or_else(|| "CubicPlayer".to_string());

        let uuid = format_uuid_with_dashes(
            &raw.minecraft_uuid
                .unwrap_or_else(|| deterministic_offline_uuid(&username).to_string()),
        );

        let microsoft_id = raw.microsoft_id.clone();

        // Try to refresh the Minecraft access token using the stored MS refresh token.
        let ms_refresh = profile.as_ref().and_then(|v| {
            v.get("ms_refresh_token")
                .and_then(|t| t.as_str())
                .map(String::from)
        });

        if let Some(refresh_token) = ms_refresh {
            if !refresh_token.is_empty() {
                eprintln!(
                    "[Auth] Has refresh token (len={}), attempting refresh...",
                    refresh_token.len()
                );

                const DEFAULT_CLIENT_ID: &str = "00000000402b5328";
                let env_path = launcher_paths.root_dir().join(".env");
                let client_id = crate::microsoft_auth::microsoft_client_id_from_env(&env_path)
                    .ok()
                    .flatten()
                    .unwrap_or_else(|| DEFAULT_CLIENT_ID.to_string());

                let config = crate::microsoft_auth::MicrosoftOAuthConfig {
                    client_id,
                    redirect_uri: crate::microsoft_auth::DESKTOP_REDIRECT_URI.to_string(),
                    scopes: vec!["XboxLive.signin".into(), "offline_access".into()],
                };
                let oauth_client = crate::microsoft_auth::MicrosoftOAuthClient::new();

                match oauth_client
                    .refresh_access_token(&config, &refresh_token)
                    .await
                {
                    Ok(ms_tokens) => {
                        eprintln!("[Auth] MS token refresh OK, authenticating with Xbox/MC...");
                        let chain = crate::microsoft_auth::MinecraftAuthChain::new();
                        match chain
                            .authenticate(
                                &ms_tokens.access_token,
                                ms_tokens.refresh_token.as_deref(),
                                ms_tokens.user_id.as_deref(),
                            )
                            .await
                        {
                            Ok(login) => {
                                eprintln!(
                                    "[Auth] Full auth chain OK: username={}",
                                    login.minecraft_username
                                );
                                // Update stored tokens in profile_data.
                                let new_profile = serde_json::json!({
                                    "username": login.minecraft_username,
                                    "uuid": login.minecraft_uuid,
                                    "mc_access_token": login.minecraft_access_token,
                                    "ms_refresh_token": ms_tokens.refresh_token.as_deref()
                                        .unwrap_or(&refresh_token),
                                });
                                if let Ok(conn) = Connection::open(&db_path) {
                                    use crate::microsoft_auth::AccountsRepository as RawRepo;
                                    let _ = RawRepo::new(&conn).update_profile_data(
                                        &microsoft_id,
                                        &new_profile.to_string(),
                                    );
                                }

                                return Ok(PlayerIdentity {
                                    username: login.minecraft_username,
                                    uuid: format_uuid_with_dashes(&login.minecraft_uuid),
                                    access_token: login.minecraft_access_token,
                                    user_type: "msa".to_string(),
                                    version_type: "Cubic".to_string(),
                                });
                            }
                            Err(e) => eprintln!("[Auth] MC auth chain failed: {e:#}"),
                        }
                    }
                    Err(e) => eprintln!("[Auth] MS token refresh failed: {e:#}"),
                }
            }
        }

        eprintln!("[Auth] Falling back to offline mode");
        // No refresh token or refresh failed; use offline mode with the correct name.
        return Ok(PlayerIdentity {
            username,
            uuid,
            access_token: "0".to_string(),
            user_type: "offline".to_string(),
            version_type: "Cubic".to_string(),
        });
    }

    // No active account at all; use fully offline mode.
    Ok(PlayerIdentity {
        username: "CubicPlayer".to_string(),
        uuid: deterministic_offline_uuid("CubicPlayer").to_string(),
        access_token: "0".to_string(),
        user_type: "offline".to_string(),
        version_type: "Cubic".to_string(),
    })
}

pub(super) async fn select_or_download_java(
    app_handle: &tauri::AppHandle,
    launcher_paths: &LauncherPaths,
    settings: &EffectiveLaunchSettings,
    target: &ResolutionTarget,
    required_version: u32,
) -> Result<PathBuf> {
    if let Some(path) = &settings.java_path_override {
        let inspector = CommandJavaBinaryInspector;
        let probe = inspector
            .inspect(path)
            .with_context(|| format!("failed to inspect Java override at {}", path.display()))?
            .with_context(|| format!("Java override '{}' is not runnable", path.display()))?;
        if probe.version < required_version {
            bail!(
                "Java override '{}' reports version {}, but launch requires Java {} or higher for Minecraft {}",
                path.display(),
                probe.version,
                required_version,
                target.minecraft_version
            );
        }

        return Ok(path.clone());
    }

    let installations = discover_java_installations(launcher_paths.java_runtimes_dir())?;
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    persist_java_installations(&connection, &installations)?;

    if let Some(installation) = select_java_for_requirement(&installations, required_version) {
        return Ok(installation.path);
    }

    // No suitable Java found; auto-download via Adoptium.
    emit_log(
        app_handle,
        ProcessLogStream::Stdout,
        format!(
            "[Java] No Java {} found, downloading from Adoptium...",
            required_version
        ),
    )?;
    emit_progress(
        app_handle,
        "resolving",
        85,
        "Downloading Java",
        &format!("Fetching Java {} runtime from Adoptium.", required_version),
    )?;

    let adoptium = AdoptiumClient::new();
    let os = host_adoptium_os();
    let arch = normalize_adoptium_architecture(std::env::consts::ARCH);

    let package = adoptium
        .fetch_latest_jre_package(required_version, os, arch)
        .await?
        .with_context(|| {
            format!(
                "Adoptium has no JRE {} for {}/{}",
                required_version, os, arch
            )
        })?;

    let plan = plan_runtime_download(
        launcher_paths.java_runtimes_dir(),
        required_version,
        package,
        os,
        arch,
    );

    // Download the archive.
    adoptium
        .download_package(&plan.package, &plan.archive_path)
        .await?;

    // Extract the archive into the install directory.
    emit_log(
        app_handle,
        ProcessLogStream::Stdout,
        format!("[Java] Extracting to {}", plan.install_dir.display()),
    )?;
    let archive_path = plan.archive_path.clone();
    let install_dir = plan.install_dir.clone();
    tokio::task::spawn_blocking(move || -> Result<()> {
        let file = std::fs::File::open(&archive_path)
            .with_context(|| format!("failed to open Java archive {}", archive_path.display()))?;
        let mut archive = zip::ZipArchive::new(file)
            .with_context(|| format!("failed to read Java archive {}", archive_path.display()))?;
        archive.extract(&install_dir).with_context(|| {
            format!(
                "failed to extract Java archive to {}",
                install_dir.display()
            )
        })?;
        // Clean up archive file.
        let _ = std::fs::remove_file(&archive_path);
        Ok(())
    })
    .await
    .context("Java extraction task panicked")??;

    // Re-scan and select.
    let installations = discover_java_installations(launcher_paths.java_runtimes_dir())?;
    persist_java_installations(&connection, &installations)?;

    select_java_for_requirement(&installations, required_version)
        .map(|installation| installation.path)
        .with_context(|| {
            format!(
                "Java {} was downloaded but could not be found after extraction. Check {}",
                required_version,
                launcher_paths.java_runtimes_dir().display()
            )
        })
}

pub(super) fn format_uuid_with_dashes(uuid: &str) -> String {
    let clean = uuid.replace('-', "");
    if clean.len() == 32 {
        format!(
            "{}-{}-{}-{}-{}",
            &clean[0..8],
            &clean[8..12],
            &clean[12..16],
            &clean[16..20],
            &clean[20..32]
        )
    } else {
        uuid.to_string()
    }
}

pub(super) fn substitute_loader_placeholders(
    loader_metadata: &mut LoaderMetadata,
    placeholders: &LaunchPlaceholders,
) {
    loader_metadata.jvm_arguments = loader_metadata
        .jvm_arguments
        .iter()
        .map(|argument| substitute_known_placeholders(argument, placeholders))
        .collect();
    loader_metadata.game_arguments = loader_metadata
        .game_arguments
        .iter()
        .map(|argument| substitute_known_placeholders(argument, placeholders))
        .collect();
}

pub(super) fn substitute_known_placeholders(
    argument: &str,
    placeholders: &LaunchPlaceholders,
) -> String {
    [
        (
            "${auth_player_name}",
            placeholders.auth_player_name.as_str(),
        ),
        ("${version_name}", placeholders.version_name.as_str()),
        ("${game_directory}", placeholders.game_directory.as_str()),
        ("${assets_root}", placeholders.assets_root.as_str()),
        (
            "${assets_index_name}",
            placeholders.assets_index_name.as_str(),
        ),
        ("${auth_uuid}", placeholders.auth_uuid.as_str()),
        (
            "${auth_access_token}",
            placeholders.auth_access_token.as_str(),
        ),
        ("${user_type}", placeholders.user_type.as_str()),
        ("${version_type}", placeholders.version_type.as_str()),
        (
            "${library_directory}",
            placeholders.library_directory.as_str(),
        ),
        (
            "${natives_directory}",
            placeholders.natives_directory.as_str(),
        ),
        ("${launcher_name}", placeholders.launcher_name.as_str()),
        (
            "${launcher_version}",
            placeholders.launcher_version.as_str(),
        ),
        (
            "${classpath_separator}",
            placeholders.classpath_separator.as_str(),
        ),
    ]
    .into_iter()
    .fold(argument.to_string(), |current, (needle, replacement)| {
        current.replace(needle, replacement)
    })
}
