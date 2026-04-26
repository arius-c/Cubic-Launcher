use std::collections::{HashMap, HashSet};

use anyhow::{Context, Result};

use crate::dependencies::{
    DependencyLink, DependencyRequest, DependencyResolution, DependencySelector, ResolvedDependency,
};
use crate::launcher_paths::LauncherPaths;
use crate::mod_cache::ModCacheRecord;
use crate::modrinth::{ModrinthClient, ModrinthVersion};
use crate::resolver::ResolutionTarget;

use super::{load_cached_mod_record_by_version, load_cached_mod_record_for_target, RemoteArtifact};

#[derive(Debug, Clone)]
pub(super) struct DependencyResolutionCandidate {
    parent_mod_id: String,
    selector: DependencySelector,
    resolved_dependency: ResolvedDependency,
    artifact: RemoteArtifact,
}

pub(super) async fn fetch_dependency_versions(
    resolved_dependencies: &[crate::dependencies::ResolvedDependency],
    client: &ModrinthClient,
) -> Result<Vec<ModrinthVersion>> {
    let mut versions = Vec::new();
    let mut seen_version_ids = HashSet::new();

    for dependency in resolved_dependencies {
        let version_id = dependency.version_id.as_str();
        if !seen_version_ids.insert(version_id.to_string()) {
            continue;
        }

        let version = client
            .fetch_version(version_id)
            .await?
            .with_context(|| format!("dependency version '{}' could not be loaded", version_id))?;
        versions.push(version);
    }

    Ok(versions)
}

pub(super) fn deduplicate_versions(
    parent_versions: Vec<ModrinthVersion>,
    dependency_versions: Vec<ModrinthVersion>,
) -> Vec<ModrinthVersion> {
    let mut versions = Vec::new();
    let mut seen = HashSet::new();

    for version in parent_versions.into_iter().chain(dependency_versions) {
        if seen.insert(version.id.clone()) {
            versions.push(version);
        }
    }

    versions
}

pub(super) fn resolved_dependency_from_cache_record(record: &ModCacheRecord) -> ResolvedDependency {
    ResolvedDependency {
        dependency_id: record.modrinth_project_id.clone(),
        version_id: record.modrinth_version_id.clone(),
        jar_filename: record.jar_filename.clone(),
        download_url: record.download_url.clone().unwrap_or_default(),
        file_hash: record.file_hash.clone(),
        date_published: String::new(),
    }
}

pub(super) fn finalize_dependency_candidates(
    candidates: Vec<DependencyResolutionCandidate>,
    mut excluded_parents: HashSet<String>,
) -> Result<(DependencyResolution, Vec<RemoteArtifact>)> {
    loop {
        let valid_candidates = candidates
            .iter()
            .filter(|candidate| !excluded_parents.contains(&candidate.parent_mod_id))
            .cloned()
            .collect::<Vec<_>>();

        let (selected_by_dependency, newly_excluded_parents) =
            select_cached_dependency_candidates(&valid_candidates);

        if newly_excluded_parents.is_empty() {
            return build_cached_dependency_resolution(
                valid_candidates,
                selected_by_dependency,
                excluded_parents,
            );
        }

        excluded_parents.extend(newly_excluded_parents);
    }
}

pub(super) fn select_cached_dependency_candidates(
    candidates: &[DependencyResolutionCandidate],
) -> (
    HashMap<String, DependencyResolutionCandidate>,
    HashSet<String>,
) {
    let mut groups: HashMap<String, Vec<&DependencyResolutionCandidate>> = HashMap::new();
    for candidate in candidates {
        groups
            .entry(candidate.resolved_dependency.dependency_id.clone())
            .or_default()
            .push(candidate);
    }

    let mut selected_by_dependency = HashMap::new();
    let mut excluded_parents = HashSet::new();

    for (dependency_id, group) in groups {
        let exact_candidates = group
            .iter()
            .copied()
            .filter(|candidate| matches!(candidate.selector, DependencySelector::VersionId { .. }))
            .collect::<Vec<_>>();

        let selected = if exact_candidates.is_empty() {
            group.into_iter().max_by(|left, right| {
                left.resolved_dependency
                    .date_published
                    .cmp(&right.resolved_dependency.date_published)
            })
        } else {
            let distinct_exact_versions = exact_candidates
                .iter()
                .map(|candidate| candidate.resolved_dependency.version_id.as_str())
                .collect::<HashSet<_>>();

            if distinct_exact_versions.len() > 1 {
                for candidate in exact_candidates {
                    excluded_parents.insert(candidate.parent_mod_id.clone());
                }
                None
            } else {
                exact_candidates.into_iter().max_by(|left, right| {
                    left.resolved_dependency
                        .date_published
                        .cmp(&right.resolved_dependency.date_published)
                })
            }
        };

        if let Some(selected) = selected {
            selected_by_dependency.insert(dependency_id, selected.clone());
        }
    }

    for candidate in candidates {
        let Some(selected) =
            selected_by_dependency.get(&candidate.resolved_dependency.dependency_id)
        else {
            continue;
        };

        if matches!(candidate.selector, DependencySelector::VersionId { .. })
            && selected.resolved_dependency.version_id != candidate.resolved_dependency.version_id
        {
            excluded_parents.insert(candidate.parent_mod_id.clone());
        }
    }

    (selected_by_dependency, excluded_parents)
}

pub(super) fn build_cached_dependency_resolution(
    candidates: Vec<DependencyResolutionCandidate>,
    selected_by_dependency: HashMap<String, DependencyResolutionCandidate>,
    excluded_parents: HashSet<String>,
) -> Result<(DependencyResolution, Vec<RemoteArtifact>)> {
    let mut resolved_dependencies = selected_by_dependency
        .values()
        .map(|candidate| candidate.resolved_dependency.clone())
        .collect::<Vec<_>>();
    resolved_dependencies.sort_by(|left, right| left.dependency_id.cmp(&right.dependency_id));

    let mut deduplicated_links = HashMap::new();
    for candidate in candidates {
        let Some(selected_candidate) =
            selected_by_dependency.get(&candidate.resolved_dependency.dependency_id)
        else {
            continue;
        };

        if matches!(candidate.selector, DependencySelector::VersionId { .. })
            && selected_candidate.resolved_dependency.version_id
                != candidate.resolved_dependency.version_id
        {
            continue;
        }

        let selected_dependency = &selected_candidate.resolved_dependency;
        let specific_version = match candidate.selector {
            DependencySelector::ProjectId { .. } => None,
            DependencySelector::VersionId { .. } => Some(selected_dependency.version_id.clone()),
        };

        deduplicated_links.insert(
            (
                candidate.parent_mod_id.clone(),
                selected_dependency.dependency_id.clone(),
            ),
            DependencyLink {
                parent_mod_id: candidate.parent_mod_id,
                dependency_id: selected_dependency.dependency_id.clone(),
                specific_version,
                jar_filename: selected_dependency.jar_filename.clone(),
            },
        );
    }

    let mut links = deduplicated_links.into_values().collect::<Vec<_>>();
    links.sort_by(|left, right| {
        left.parent_mod_id
            .cmp(&right.parent_mod_id)
            .then(left.dependency_id.cmp(&right.dependency_id))
    });

    let artifacts = resolved_dependencies
        .iter()
        .filter_map(|dependency| {
            selected_by_dependency
                .get(&dependency.dependency_id)
                .map(|candidate| candidate.artifact.clone())
        })
        .collect::<Vec<_>>();

    Ok((
        DependencyResolution {
            resolved_dependencies,
            links,
            excluded_parents,
        },
        artifacts,
    ))
}

pub(super) async fn resolve_dependency_requests_with_cache_fallback(
    launcher_paths: &LauncherPaths,
    requests: &[DependencyRequest],
    selected_mod_ids: &HashSet<String>,
    target: &ResolutionTarget,
) -> Result<(DependencyResolution, Vec<RemoteArtifact>)> {
    let mut candidates = Vec::with_capacity(requests.len());
    let mut excluded_parents = HashSet::new();

    for request in requests {
        if let DependencySelector::ProjectId { project_id } = &request.selector {
            if selected_mod_ids.contains(project_id) {
                continue;
            }
        }

        if excluded_parents.contains(&request.parent_mod_id) {
            continue;
        }

        let candidate = match &request.selector {
            DependencySelector::ProjectId { project_id } => {
                if let Some(record) =
                    load_cached_mod_record_for_target(launcher_paths, project_id, target)?
                {
                    DependencyResolutionCandidate {
                        parent_mod_id: request.parent_mod_id.clone(),
                        selector: request.selector.clone(),
                        resolved_dependency: resolved_dependency_from_cache_record(&record),
                        artifact: RemoteArtifact::Cached(record),
                    }
                } else {
                    excluded_parents.insert(request.parent_mod_id.clone());
                    continue;
                }
            }
            DependencySelector::VersionId { version_id } => {
                if let Some(record) = load_cached_mod_record_by_version(launcher_paths, version_id)?
                {
                    DependencyResolutionCandidate {
                        parent_mod_id: request.parent_mod_id.clone(),
                        selector: request.selector.clone(),
                        resolved_dependency: resolved_dependency_from_cache_record(&record),
                        artifact: RemoteArtifact::Cached(record),
                    }
                } else {
                    excluded_parents.insert(request.parent_mod_id.clone());
                    continue;
                }
            }
        };

        candidates.push(candidate);
    }

    finalize_dependency_candidates(candidates, excluded_parents)
}
