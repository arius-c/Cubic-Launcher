use std::collections::HashMap;
use std::time::Duration;

use anyhow::{bail, Context, Result};
use reqwest::Url;
use serde::Deserialize;

use crate::resolver::{ModLoader, ResolutionTarget};

const MODRINTH_API_BASE_URL: &str = "https://api.modrinth.com/v2";

/// Check whether a game-version string from Modrinth matches a concrete MC
/// version.  Handles wildcard patterns such as `"1.21.x"` / `"1.21.X"` where
/// the last segment is a case-insensitive `x` meaning "any patch".
pub fn mc_version_matches(pattern: &str, concrete: &str) -> bool {
    if pattern == concrete {
        return true;
    }
    // Check for trailing `.x` / `.X` wildcard
    let Some(prefix) = pattern
        .strip_suffix(".x")
        .or_else(|| pattern.strip_suffix(".X"))
    else {
        return false;
    };
    // `concrete` must start with the prefix followed by a dot and at least one
    // more character (the actual patch number).
    // e.g. pattern "1.21.x" → prefix "1.21", concrete "1.21.1" ✓
    concrete.starts_with(prefix) && concrete.as_bytes().get(prefix.len()) == Some(&b'.')
}

const REQUEST_TIMEOUT: Duration = Duration::from_secs(10);

fn build_http_client() -> reqwest::Client {
    reqwest::Client::builder()
        .user_agent("cubic-launcher/0.1.0 (https://github.com/arius-c/Cubic-Launcher)")
        .timeout(REQUEST_TIMEOUT)
        .build()
        .unwrap_or_default()
}

// Retry once on HTTP 429 to be polite to Modrinth's rate limiter.
async fn send_with_retry(
    http_client: &reqwest::Client,
    url: reqwest::Url,
) -> reqwest::Result<reqwest::Response> {
    let first = http_client.get(url.clone()).send().await?;
    if first.status() != reqwest::StatusCode::TOO_MANY_REQUESTS {
        return Ok(first);
    }
    let retry_after_secs = first
        .headers()
        .get(reqwest::header::RETRY_AFTER)
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<u64>().ok())
        .unwrap_or(2)
        .min(10);
    tokio::time::sleep(std::time::Duration::from_secs(retry_after_secs)).await;
    http_client.get(url).send().await
}

#[derive(Debug, Clone)]
pub struct ModrinthClient {
    http_client: reqwest::Client,
    base_url: String,
}

impl ModrinthClient {
    pub fn new() -> Self {
        Self {
            http_client: build_http_client(),
            base_url: MODRINTH_API_BASE_URL.to_string(),
        }
    }

    pub fn with_base_url(base_url: impl Into<String>) -> Self {
        Self {
            http_client: build_http_client(),
            base_url: base_url.into(),
        }
    }

    pub async fn fetch_project_versions(
        &self,
        project_id: &str,
        target: &ResolutionTarget,
    ) -> Result<Vec<ModrinthVersion>> {
        if project_id.trim().is_empty() {
            bail!("project_id cannot be empty");
        }

        let url = build_project_versions_url(&self.base_url, project_id, target)?;
        let response = send_with_retry(&self.http_client, url)
            .await
            .with_context(|| {
                format!("failed to query Modrinth versions for project '{project_id}'")
            })?
            .error_for_status()
            .with_context(|| {
                format!("Modrinth returned an error for project '{project_id}' version lookup")
            })?;

        let versions = response
            .json::<Vec<ModrinthVersion>>()
            .await
            .with_context(|| {
                format!("failed to deserialize Modrinth versions for project '{project_id}'")
            })?;

        Ok(filter_compatible_versions(&versions, target))
    }

    pub async fn fetch_latest_compatible_version(
        &self,
        project_id: &str,
        target: &ResolutionTarget,
    ) -> Result<Option<ModrinthVersion>> {
        let versions = self.fetch_project_versions(project_id, target).await?;
        Ok(select_latest_compatible_version(&versions, target))
    }

    /// Fetch versions for a content pack (resource pack, data pack, shader).
    /// Only filters by game version, not by loader.
    pub async fn fetch_content_pack_versions(
        &self,
        project_id: &str,
        minecraft_version: &str,
    ) -> Result<Vec<ModrinthVersion>> {
        if project_id.trim().is_empty() {
            bail!("project_id cannot be empty");
        }
        let sanitized = self.base_url.trim_end_matches('/');
        let mut url = Url::parse(&format!("{sanitized}/project/{project_id}/version"))
            .with_context(|| format!("invalid Modrinth base URL '{}'", self.base_url))?;
        let game_versions_json = serde_json::to_string(&vec![minecraft_version])?;
        url.query_pairs_mut()
            .append_pair("game_versions", &game_versions_json);
        let response = send_with_retry(&self.http_client, url).await
            .with_context(|| format!("failed to query Modrinth versions for content pack '{project_id}'"))?
            .error_for_status()
            .with_context(|| format!("Modrinth returned an error for content pack '{project_id}'"))?;
        let versions = response.json::<Vec<ModrinthVersion>>().await
            .with_context(|| format!("failed to deserialize Modrinth versions for content pack '{project_id}'"))?;
        // Filter to only versions matching the MC version
        Ok(versions.into_iter().filter(|v| v.game_versions.iter().any(|gv| mc_version_matches(gv, minecraft_version))).collect())
    }

    pub async fn fetch_version(&self, version_id: &str) -> Result<Option<ModrinthVersion>> {
        if version_id.trim().is_empty() {
            bail!("version_id cannot be empty");
        }

        let url = build_version_url(&self.base_url, version_id)?;
        let response = send_with_retry(&self.http_client, url)
            .await
            .with_context(|| format!("failed to query Modrinth version '{version_id}'"))?;

        if response.status() == reqwest::StatusCode::NOT_FOUND {
            return Ok(None);
        }

        let response = response.error_for_status().with_context(|| {
            format!("Modrinth returned an error for version '{version_id}' lookup")
        })?;

        let version = response
            .json::<ModrinthVersion>()
            .await
            .with_context(|| format!("failed to deserialize Modrinth version '{version_id}'"))?;

        Ok(Some(version))
    }
}

impl Default for ModrinthClient {
    fn default() -> Self {
        Self::new()
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModrinthVersion {
    pub id: String,
    pub project_id: String,
    pub version_number: String,
    pub name: String,
    pub game_versions: Vec<String>,
    pub loaders: Vec<String>,
    #[serde(default)]
    pub dependencies: Vec<ModrinthDependency>,
    #[serde(default)]
    pub files: Vec<ModrinthFile>,
    pub date_published: String,
}

impl ModrinthVersion {
    pub fn primary_file(&self) -> Option<&ModrinthFile> {
        self.files
            .iter()
            .find(|file| file.primary)
            .or_else(|| self.files.first())
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModrinthDependency {
    pub version_id: Option<String>,
    pub project_id: Option<String>,
    #[serde(rename = "dependency_type")]
    pub dependency_type: DependencyType,
    #[serde(default)]
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct ModrinthFile {
    pub hashes: HashMap<String, String>,
    pub url: String,
    pub filename: String,
    pub primary: bool,
    pub size: u64,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum DependencyType {
    Required,
    Optional,
    Incompatible,
    Embedded,
}

pub fn build_project_versions_url(
    base_url: &str,
    project_id: &str,
    target: &ResolutionTarget,
) -> Result<Url> {
    let sanitized_base_url = base_url.trim_end_matches('/');
    let mut url = Url::parse(&format!(
        "{sanitized_base_url}/project/{project_id}/version"
    ))
    .with_context(|| format!("invalid Modrinth base URL '{base_url}'"))?;

    let loaders_json = serde_json::to_string(&vec![target.mod_loader.as_modrinth_loader()])?;
    let game_versions_json = serde_json::to_string(&vec![target.minecraft_version.clone()])?;

    url.query_pairs_mut()
        .append_pair("loaders", &loaders_json)
        .append_pair("game_versions", &game_versions_json);

    Ok(url)
}

pub fn build_version_url(base_url: &str, version_id: &str) -> Result<Url> {
    let sanitized_base_url = base_url.trim_end_matches('/');
    Url::parse(&format!("{sanitized_base_url}/version/{version_id}"))
        .with_context(|| format!("invalid Modrinth base URL '{base_url}'"))
}

pub fn filter_compatible_versions(
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Vec<ModrinthVersion> {
    versions
        .iter()
        .filter(|version| is_version_compatible(version, target))
        .cloned()
        .collect()
}

pub fn is_version_compatible(version: &ModrinthVersion, target: &ResolutionTarget) -> bool {
    version
        .game_versions
        .iter()
        .any(|game_version| mc_version_matches(game_version, &target.minecraft_version))
        && version
            .loaders
            .iter()
            .any(|loader| loader == target.mod_loader.as_modrinth_loader())
}

pub fn select_latest_compatible_version(
    versions: &[ModrinthVersion],
    target: &ResolutionTarget,
) -> Option<ModrinthVersion> {
    filter_compatible_versions(versions, target)
        .into_iter()
        .max_by(|left, right| left.date_published.cmp(&right.date_published))
}

impl ModLoader {
    pub fn as_modrinth_loader(self) -> &'static str {
        match self {
            ModLoader::Fabric => "fabric",
            ModLoader::NeoForge => "neoforge",
            ModLoader::Forge => "forge",
            ModLoader::Quilt => "quilt",
            ModLoader::Vanilla => "vanilla",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::{
        build_project_versions_url, build_version_url, filter_compatible_versions,
        select_latest_compatible_version, DependencyType, ModrinthVersion,
    };
    use crate::resolver::{ModLoader, ResolutionTarget};

    fn target() -> ResolutionTarget {
        ResolutionTarget {
            minecraft_version: "1.21.1".into(),
            mod_loader: ModLoader::Fabric,
        }
    }

    fn sample_versions_json() -> &'static str {
        r#"[
          {
            "id": "version-old",
            "project_id": "sodium",
            "version_number": "0.5.9",
            "name": "Sodium 0.5.9",
            "game_versions": ["1.21.1"],
            "loaders": ["fabric"],
            "date_published": "2024-06-01T10:00:00.000Z",
            "dependencies": [
              {
                "version_id": null,
                "project_id": "fabric-api",
                "dependency_type": "required",
                "file_name": null
              }
            ],
            "files": [
              {
                "hashes": { "sha1": "abc" },
                "url": "https://cdn.modrinth.com/data/sodium/version-old.jar",
                "filename": "sodium-old.jar",
                "primary": true,
                "size": 12345
              }
            ]
          },
          {
            "id": "version-new",
            "project_id": "sodium",
            "version_number": "0.6.0",
            "name": "Sodium 0.6.0",
            "game_versions": ["1.21.1"],
            "loaders": ["fabric"],
            "date_published": "2024-08-01T10:00:00.000Z",
            "dependencies": [],
            "files": [
              {
                "hashes": { "sha1": "def" },
                "url": "https://cdn.modrinth.com/data/sodium/version-new.jar",
                "filename": "sodium-new.jar",
                "primary": false,
                "size": 67890
              },
              {
                "hashes": { "sha1": "ghi" },
                "url": "https://cdn.modrinth.com/data/sodium/version-new-primary.jar",
                "filename": "sodium-new-primary.jar",
                "primary": true,
                "size": 67900
              }
            ]
          },
          {
            "id": "version-wrong-loader",
            "project_id": "sodium",
            "version_number": "0.6.0-neoforge",
            "name": "Sodium NeoForge",
            "game_versions": ["1.21.1"],
            "loaders": ["neoforge"],
            "date_published": "2024-09-01T10:00:00.000Z",
            "dependencies": [],
            "files": []
          }
        ]"#
    }

    #[test]
    fn builds_modrinth_versions_url_with_expected_filters() {
        let url = build_project_versions_url("https://api.modrinth.com/v2", "sodium", &target())
            .expect("url should build");

        assert_eq!(
            url.as_str(),
            "https://api.modrinth.com/v2/project/sodium/version?loaders=%5B%22fabric%22%5D&game_versions=%5B%221.21.1%22%5D"
        );
    }

    #[test]
    fn builds_modrinth_single_version_url() {
        let url =
            build_version_url("https://api.modrinth.com/v2", "abc123").expect("url should build");

        assert_eq!(url.as_str(), "https://api.modrinth.com/v2/version/abc123");
    }

    #[test]
    fn deserializes_version_payload_with_dependencies_and_files() {
        let versions: Vec<ModrinthVersion> =
            serde_json::from_str(sample_versions_json()).expect("json should deserialize");

        assert_eq!(versions.len(), 3);
        assert_eq!(versions[0].dependencies.len(), 1);
        assert_eq!(
            versions[0].dependencies[0].dependency_type,
            DependencyType::Required
        );
        assert_eq!(versions[0].files[0].filename, "sodium-old.jar");
    }

    #[test]
    fn filters_versions_by_target_loader_and_game_version() {
        let versions: Vec<ModrinthVersion> =
            serde_json::from_str(sample_versions_json()).expect("json should deserialize");

        let compatible = filter_compatible_versions(&versions, &target());

        assert_eq!(compatible.len(), 2);
        assert!(compatible
            .iter()
            .all(|version| version.loaders.contains(&"fabric".into())));
    }

    #[test]
    fn selects_most_recent_compatible_version() {
        let versions: Vec<ModrinthVersion> =
            serde_json::from_str(sample_versions_json()).expect("json should deserialize");

        let selected =
            select_latest_compatible_version(&versions, &target()).expect("version should exist");

        assert_eq!(selected.id, "version-new");
        assert_eq!(
            selected
                .primary_file()
                .expect("primary file should exist")
                .filename,
            "sodium-new-primary.jar"
        );
    }
}
