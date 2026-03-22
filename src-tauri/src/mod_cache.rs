use std::collections::HashSet;
use std::path::PathBuf;

use anyhow::{Context, Result};
use rusqlite::{params, Connection, OptionalExtension};

use crate::modrinth::ModrinthVersion;
use crate::resolver::ResolutionTarget;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModCacheRecord {
    pub modrinth_project_id: String,
    pub modrinth_version_id: String,
    pub jar_filename: String,
    pub mc_version: String,
    pub mod_loader: String,
    pub file_hash: Option<String>,
    pub download_url: Option<String>,
    pub is_local: bool,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PendingDownload {
    pub modrinth_project_id: String,
    pub modrinth_version_id: String,
    pub jar_filename: String,
    pub mc_version: String,
    pub mod_loader: String,
    pub file_hash: Option<String>,
    pub download_url: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModAcquisitionPlan {
    pub cached: Vec<ModCacheRecord>,
    pub to_download: Vec<PendingDownload>,
}

pub trait ModCacheLookup {
    fn find_by_version_id(&self, version_id: &str) -> Result<Option<ModCacheRecord>>;
}

pub struct SqliteModCacheRepository<'connection> {
    connection: &'connection Connection,
    mods_cache_dir: PathBuf,
}

impl<'connection> SqliteModCacheRepository<'connection> {
    pub fn new(connection: &'connection Connection, mods_cache_dir: impl Into<PathBuf>) -> Self {
        Self {
            connection,
            mods_cache_dir: mods_cache_dir.into(),
        }
    }

    pub fn upsert_modrinth_version(
        &self,
        version: &ModrinthVersion,
        target: &ResolutionTarget,
    ) -> Result<ModCacheRecord> {
        let record = cache_record_from_version(version, target)?;

        self.connection.execute(
            r#"
            INSERT INTO mod_cache (
                modrinth_project_id,
                modrinth_version_id,
                jar_filename,
                mc_version,
                mod_loader,
                file_hash,
                download_url,
                is_local
            ) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)
            ON CONFLICT(modrinth_version_id) DO UPDATE SET
                modrinth_project_id = excluded.modrinth_project_id,
                jar_filename = excluded.jar_filename,
                mc_version = excluded.mc_version,
                mod_loader = excluded.mod_loader,
                file_hash = excluded.file_hash,
                download_url = excluded.download_url,
                is_local = excluded.is_local
            "#,
            params![
                &record.modrinth_project_id,
                &record.modrinth_version_id,
                &record.jar_filename,
                &record.mc_version,
                &record.mod_loader,
                &record.file_hash,
                &record.download_url,
                record.is_local,
            ],
        )?;

        Ok(record)
    }
}

impl ModCacheLookup for SqliteModCacheRepository<'_> {
    fn find_by_version_id(&self, version_id: &str) -> Result<Option<ModCacheRecord>> {
        let mut statement = self.connection.prepare(
            r#"
            SELECT
                modrinth_project_id,
                modrinth_version_id,
                jar_filename,
                mc_version,
                mod_loader,
                file_hash,
                download_url,
                is_local
            FROM mod_cache
            WHERE modrinth_version_id = ?1
            "#,
        )?;

        let record = statement
            .query_row([version_id], |row| {
                Ok(ModCacheRecord {
                    modrinth_project_id: row.get(0)?,
                    modrinth_version_id: row.get(1)?,
                    jar_filename: row.get(2)?,
                    mc_version: row.get(3)?,
                    mod_loader: row.get(4)?,
                    file_hash: row.get(5)?,
                    download_url: row.get(6)?,
                    is_local: row.get(7)?,
                })
            })
            .optional()?;

        match record {
            Some(record) if self.mods_cache_dir.join(&record.jar_filename).exists() => {
                Ok(Some(record))
            }
            Some(_) => Ok(None),
            None => Ok(None),
        }
    }
}

pub fn build_mod_acquisition_plan(
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
    cache_lookup: &impl ModCacheLookup,
) -> Result<ModAcquisitionPlan> {
    let mut seen_version_ids = HashSet::new();
    let mut cached = Vec::new();
    let mut to_download = Vec::new();

    for version in versions {
        if !seen_version_ids.insert(version.id.clone()) {
            continue;
        }

        match cache_lookup.find_by_version_id(&version.id)? {
            Some(record) => cached.push(record),
            None => to_download.push(pending_download_from_version(version, target)?),
        }
    }

    Ok(ModAcquisitionPlan {
        cached,
        to_download,
    })
}

pub fn cache_record_from_version(
    version: &ModrinthVersion,
    target: &ResolutionTarget,
) -> Result<ModCacheRecord> {
    let primary_file = version.primary_file().with_context(|| {
        format!(
            "version '{}' for project '{}' does not expose any downloadable file",
            version.id, version.project_id
        )
    })?;

    Ok(ModCacheRecord {
        modrinth_project_id: version.project_id.clone(),
        modrinth_version_id: version.id.clone(),
        jar_filename: primary_file.filename.clone(),
        mc_version: target.minecraft_version.clone(),
        mod_loader: target.mod_loader.as_modrinth_loader().to_string(),
        file_hash: primary_file.hashes.get("sha1").cloned(),
        download_url: Some(primary_file.url.clone()),
        is_local: false,
    })
}

pub fn pending_download_from_version(
    version: &ModrinthVersion,
    target: &ResolutionTarget,
) -> Result<PendingDownload> {
    let primary_file = version.primary_file().with_context(|| {
        format!(
            "version '{}' for project '{}' does not expose any downloadable file",
            version.id, version.project_id
        )
    })?;

    Ok(PendingDownload {
        modrinth_project_id: version.project_id.clone(),
        modrinth_version_id: version.id.clone(),
        jar_filename: primary_file.filename.clone(),
        mc_version: target.minecraft_version.clone(),
        mod_loader: target.mod_loader.as_modrinth_loader().to_string(),
        file_hash: primary_file.hashes.get("sha1").cloned(),
        download_url: primary_file.url.clone(),
    })
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use anyhow::Result;
    use rusqlite::Connection;

    use crate::database::initialize_database;
    use crate::modrinth::ModrinthVersion;
    use crate::resolver::{ModLoader, ResolutionTarget};

    use super::{
        build_mod_acquisition_plan, cache_record_from_version, pending_download_from_version,
        ModCacheLookup, SqliteModCacheRepository,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-mod-cache-test-{timestamp}"))
    }

    fn target() -> ResolutionTarget {
        ResolutionTarget {
            minecraft_version: "1.21.1".into(),
            mod_loader: ModLoader::Fabric,
        }
    }

    fn version(project_id: &str, version_id: &str, filename: &str) -> ModrinthVersion {
        serde_json::from_str(&format!(
            r#"{{
              "id": "{version_id}",
              "project_id": "{project_id}",
              "version_number": "1.0.0",
              "name": "{project_id}",
              "game_versions": ["1.21.1"],
              "loaders": ["fabric"],
              "date_published": "2024-08-01T10:00:00.000Z",
              "dependencies": [],
              "files": [
                {{
                  "hashes": {{ "sha1": "{version_id}-sha1" }},
                  "url": "https://cdn.modrinth.com/data/{project_id}/{filename}",
                  "filename": "{filename}",
                  "primary": true,
                  "size": 100
                }}
              ]
            }}"#
        ))
        .expect("version json should deserialize")
    }

    struct InMemoryLookup {
        records: Vec<super::ModCacheRecord>,
    }

    impl ModCacheLookup for InMemoryLookup {
        fn find_by_version_id(&self, version_id: &str) -> Result<Option<super::ModCacheRecord>> {
            Ok(self
                .records
                .iter()
                .find(|record| record.modrinth_version_id == version_id)
                .cloned())
        }
    }

    #[test]
    fn cache_record_and_pending_download_use_primary_file_metadata() {
        let version = version("sodium", "version-1", "sodium.jar");

        let record = cache_record_from_version(&version, &target()).expect("record should build");
        let pending =
            pending_download_from_version(&version, &target()).expect("download should build");

        assert_eq!(record.jar_filename, "sodium.jar");
        assert_eq!(record.file_hash.as_deref(), Some("version-1-sha1"));
        assert_eq!(
            pending.download_url,
            "https://cdn.modrinth.com/data/sodium/sodium.jar"
        );
    }

    #[test]
    fn repository_returns_cache_hit_only_when_file_exists() {
        let root_dir = unique_test_root();
        let database_path = root_dir.join("launcher_data.db");
        let mods_cache_dir = root_dir.join("cache").join("mods");

        fs::create_dir_all(&mods_cache_dir).expect("mods cache directory should be created");
        initialize_database(&database_path).expect("database should initialize");

        let connection = Connection::open(&database_path).expect("database should open");
        let repository = SqliteModCacheRepository::new(&connection, &mods_cache_dir);
        let version = version("sodium", "version-1", "sodium.jar");

        repository
            .upsert_modrinth_version(&version, &target())
            .expect("cache record should insert");

        assert!(repository
            .find_by_version_id("version-1")
            .expect("lookup should succeed")
            .is_none());

        fs::write(mods_cache_dir.join("sodium.jar"), b"jar").expect("jar should be written");

        let record = repository
            .find_by_version_id("version-1")
            .expect("lookup should succeed")
            .expect("record should exist once file exists");

        assert_eq!(record.modrinth_project_id, "sodium");
        assert_eq!(record.jar_filename, "sodium.jar");

        drop(connection);
        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn acquisition_plan_splits_cached_and_missing_versions_and_deduplicates() {
        let cached_version = version("sodium", "version-cached", "sodium.jar");
        let missing_version = version("fabric-api", "version-missing", "fabric-api.jar");

        let lookup = InMemoryLookup {
            records: vec![cache_record_from_version(&cached_version, &target())
                .expect("cache record should build")],
        };

        let plan = build_mod_acquisition_plan(
            &[
                cached_version.clone(),
                missing_version.clone(),
                missing_version.clone(),
            ],
            &target(),
            &lookup,
        )
        .expect("plan should build");

        assert_eq!(plan.cached.len(), 1);
        assert_eq!(plan.cached[0].modrinth_version_id, "version-cached");
        assert_eq!(plan.to_download.len(), 1);
        assert_eq!(plan.to_download[0].modrinth_version_id, "version-missing");
    }
}
