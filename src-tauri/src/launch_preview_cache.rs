#![allow(dead_code)]

use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::Arc;

use anyhow::{Context, Result};
use rusqlite::{params, Connection};

use crate::dependencies::{DependencyLink, DependencyRequest, DependencySelector};
use crate::instance_mods::CachedModJar;
use crate::launcher_paths::LauncherPaths;
use crate::mod_cache::{
    build_mod_acquisition_plan, cache_record_from_version, cached_artifact_path_for_record,
    cached_local_artifact_path, pending_download_from_version, ModAcquisitionPlan, ModCacheLookup,
    ModCacheRecord, SqliteModCacheRepository,
};
use crate::modrinth::ModrinthVersion;
use crate::process_streaming::ProcessLogStream;
use crate::resolver::ResolutionTarget;
use crate::rules::ModSource;

use super::{emit_log, emit_progress, jar_metadata_allows_target, DownloadArtifact, SelectedMod};

pub(super) async fn ensure_remote_version_cached(
    http_client: &reqwest::Client,
    launcher_paths: &LauncherPaths,
    version: &ModrinthVersion,
    target: &ResolutionTarget,
) -> Result<PathBuf> {
    let record = cache_record_from_version(version, target)?;
    let destination_path =
        cached_artifact_path_for_record(launcher_paths.mods_cache_dir(), &record);
    if destination_path.exists() {
        return Ok(destination_path);
    }

    let file = version.primary_file().with_context(|| {
        format!(
            "version '{}' for project '{}' does not expose a primary file",
            version.id, version.project_id
        )
    })?;
    match file.hashes.get("sha1").map(String::as_str) {
        Some(hash) => {
            crate::minecraft_downloader::download_file_verified(
                http_client,
                &file.url,
                &destination_path,
                hash,
            )
            .await?
        }
        None => download_file(http_client, &file.url, &destination_path).await?,
    }

    Ok(destination_path)
}

pub(super) fn load_cached_mod_record_for_target(
    launcher_paths: &LauncherPaths,
    project_id: &str,
    target: &ResolutionTarget,
) -> Result<Option<ModCacheRecord>> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());
    repository.find_compatible_by_project(project_id, target)
}

pub(super) fn load_cached_mod_record_by_version(
    launcher_paths: &LauncherPaths,
    version_id: &str,
) -> Result<Option<ModCacheRecord>> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());
    repository.find_by_version_id(version_id)
}

pub(super) fn load_cached_dependency_requests(
    launcher_paths: &LauncherPaths,
    parent_mod_ids: &[String],
) -> Result<Vec<DependencyRequest>> {
    if parent_mod_ids.is_empty() {
        return Ok(Vec::new());
    }

    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;

    let mut requests = Vec::new();
    let mut statement = connection.prepare(
        r#"
        SELECT dependency_id, specific_version
        FROM dependencies
        WHERE mod_parent_id = ?1
          AND dep_type = 'required'
        ORDER BY dependency_id ASC
        "#,
    )?;

    for parent_mod_id in parent_mod_ids {
        let rows = statement.query_map([parent_mod_id.as_str()], |row| {
            Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
        })?;

        for row in rows {
            let (dependency_id, specific_version) = row?;
            let selector = match specific_version {
                Some(version_id) if !version_id.trim().is_empty() => {
                    DependencySelector::VersionId { version_id }
                }
                _ => DependencySelector::ProjectId {
                    project_id: dependency_id.clone(),
                },
            };

            requests.push(DependencyRequest {
                parent_mod_id: parent_mod_id.clone(),
                selector,
            });
        }
    }

    Ok(requests)
}

pub(super) fn build_remote_acquisition_plan_from_artifacts(
    launcher_paths: &LauncherPaths,
    live_versions: &[ModrinthVersion],
    cached_records: &[ModCacheRecord],
    target: &ResolutionTarget,
) -> Result<ModAcquisitionPlan> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());

    let mut seen_version_ids = HashSet::new();
    let mut cached = Vec::new();
    let mut to_download = Vec::new();

    for record in cached_records {
        if seen_version_ids.insert(record.modrinth_version_id.clone()) {
            cached.push(record.clone());
        }
    }

    for version in live_versions {
        if !seen_version_ids.insert(version.id.clone()) {
            continue;
        }

        match repository.find_by_version_id(&version.id)? {
            Some(record) => cached.push(record),
            None => to_download.push(pending_download_from_version(version, target)?),
        }
    }

    Ok(ModAcquisitionPlan {
        cached,
        to_download,
    })
}

pub(super) fn build_remote_acquisition_plan(
    launcher_paths: &LauncherPaths,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Result<crate::mod_cache::ModAcquisitionPlan> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());

    build_mod_acquisition_plan(versions, target, &repository)
}

pub(super) async fn download_pending_artifacts(
    app_handle: &tauri::AppHandle,
    http_client: &reqwest::Client,
    default_directory: &Path,
    artifacts: &[DownloadArtifact],
) -> Result<()> {
    let total = artifacts.len();
    if total == 0 {
        return Ok(());
    }

    let semaphore = Arc::new(tokio::sync::Semaphore::new(10));
    let mut tasks: tokio::task::JoinSet<Result<DownloadArtifact>> = tokio::task::JoinSet::new();

    for artifact in artifacts {
        let artifact = artifact.clone();
        let http_client = http_client.clone();
        let permit_source = semaphore.clone();
        tasks.spawn(async move {
            // Permit is held through the await so concurrency stays bounded.
            let _permit = permit_source
                .acquire_owned()
                .await
                .map_err(|error| anyhow::anyhow!("failed to acquire download permit: {error}"))?;
            match &artifact.file_hash {
                Some(hash) => crate::minecraft_downloader::download_file_verified(
                    &http_client,
                    &artifact.url,
                    &artifact.destination_path,
                    hash,
                )
                .await
                .with_context(|| {
                    format!(
                        "failed to download '{}' to {}",
                        artifact.url,
                        artifact.destination_path.display()
                    )
                })?,
                None => download_file(&http_client, &artifact.url, &artifact.destination_path)
                    .await
                    .with_context(|| {
                        format!(
                            "failed to download '{}' to {}",
                            artifact.url,
                            artifact.destination_path.display()
                        )
                    })?,
            }
            Ok(artifact)
        });
    }

    let mut completed: usize = 0;
    while let Some(join_result) = tasks.join_next().await {
        let artifact =
            join_result.map_err(|error| anyhow::anyhow!("download task panicked: {error}"))??;
        completed += 1;

        let progress = 42u8 + ((16usize * completed) / total) as u8;
        emit_progress(
            app_handle,
            "resolving",
            progress,
            "Downloading Mods",
            &format!("Downloaded {completed} of {total} mods."),
        )?;

        emit_log(
            app_handle,
            ProcessLogStream::Stdout,
            format!(
                "[Download] Saved {} to {}",
                artifact.filename,
                artifact
                    .destination_path
                    .strip_prefix(default_directory)
                    .unwrap_or(&artifact.destination_path)
                    .display()
            ),
        )?;
    }

    Ok(())
}

pub(super) async fn download_file(
    http_client: &reqwest::Client,
    url: &str,
    destination_path: &Path,
) -> Result<()> {
    let response = http_client
        .get(url)
        .send()
        .await
        .with_context(|| format!("failed to request {url}"))?
        .error_for_status()
        .with_context(|| format!("download request failed for {url}"))?;
    let bytes = response
        .bytes()
        .await
        .with_context(|| format!("failed to read downloaded bytes from {url}"))?;

    if let Some(parent) = destination_path.parent() {
        tokio::fs::create_dir_all(parent)
            .await
            .with_context(|| format!("failed to create {}", parent.display()))?;
    }

    tokio::fs::write(destination_path, bytes)
        .await
        .with_context(|| format!("failed to write {}", destination_path.display()))?;

    Ok(())
}

pub(super) fn persist_remote_versions_and_dependencies(
    launcher_paths: &LauncherPaths,
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
    dependency_links: &[DependencyLink],
) -> Result<()> {
    let connection = Connection::open(launcher_paths.database_path()).with_context(|| {
        format!(
            "failed to open launcher database at {}",
            launcher_paths.database_path().display()
        )
    })?;
    let repository = SqliteModCacheRepository::new(&connection, launcher_paths.mods_cache_dir());

    for version in versions {
        repository.upsert_modrinth_version(version, target)?;
    }

    persist_dependency_links(&connection, dependency_links)
}

pub(super) fn persist_dependency_links(
    connection: &Connection,
    links: &[DependencyLink],
) -> Result<()> {
    let transaction = connection.unchecked_transaction()?;

    for link in links {
        transaction.execute(
            r#"
            INSERT INTO dependencies (
                mod_parent_id,
                dependency_id,
                dep_type,
                specific_version,
                jar_filename
            ) VALUES (?1, ?2, 'required', ?3, ?4)
            ON CONFLICT(mod_parent_id, dependency_id) DO UPDATE SET
                dep_type = excluded.dep_type,
                specific_version = excluded.specific_version,
                jar_filename = excluded.jar_filename
            "#,
            params![
                &link.parent_mod_id,
                &link.dependency_id,
                &link.specific_version,
                &link.jar_filename,
            ],
        )?;
    }

    transaction.commit()?;
    Ok(())
}

pub(super) fn build_cached_mod_jars(
    app_handle: &tauri::AppHandle,
    selected_mods: &[SelectedMod],
    versions: &[ModrinthVersion],
    cached_records: &[ModCacheRecord],
    target: &ResolutionTarget,
    launcher_paths: &LauncherPaths,
    modlist_name: &str,
) -> Result<Vec<CachedModJar>> {
    let mut jars = Vec::new();
    let mut seen = HashSet::new();
    let local_jars_dir = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join("local-jars");
    let mod_loader = target.mod_loader.as_modrinth_loader();

    // Local mods: JAR lives at local-jars/{mod_id}.jar; copy to cache/mods/.
    for selected in selected_mods {
        if !matches!(selected.source, ModSource::Local) {
            continue;
        }

        let file_name = format!("{}.jar", selected.mod_id);
        if seen.insert(file_name.clone()) {
            let source = local_jars_dir.join(&file_name);
            let dest =
                cached_local_artifact_path(launcher_paths.mods_cache_dir(), mod_loader, &file_name);
            if source.exists() && !jar_metadata_allows_target(&source, target)? {
                emit_log(
                    app_handle,
                    ProcessLogStream::Stdout,
                    format!(
                        "[Launch] skipping local mod '{}': embedded metadata is incompatible with {} / {}",
                        selected.mod_id,
                        target.minecraft_version,
                        target.mod_loader.as_modrinth_loader()
                    ),
                )?;
                continue;
            }
            if source.exists() && !dest.exists() {
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent).ok();
                }
                std::fs::copy(&source, &dest).with_context(|| {
                    format!("failed to copy local JAR '{}' to mod cache", file_name)
                })?;
            }
            jars.push(CachedModJar {
                jar_filename: file_name,
                cache_path: dest,
            });
        }
    }

    for record in cached_records {
        if seen.insert(record.jar_filename.clone()) {
            jars.push(CachedModJar {
                jar_filename: record.jar_filename.clone(),
                cache_path: cached_artifact_path_for_record(
                    launcher_paths.mods_cache_dir(),
                    record,
                ),
            });
        }
    }

    for version in versions {
        let record = cache_record_from_version(version, target)?;
        let jar_filename = record.jar_filename.clone();
        if seen.insert(jar_filename.clone()) {
            jars.push(CachedModJar {
                jar_filename,
                cache_path: cached_artifact_path_for_record(
                    launcher_paths.mods_cache_dir(),
                    &record,
                ),
            });
        }
    }

    Ok(jars)
}
