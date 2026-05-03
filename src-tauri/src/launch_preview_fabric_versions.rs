#![allow(dead_code)]

pub(super) fn compare_fabric_dependency_versions(
    left: &str,
    right: &str,
) -> Option<std::cmp::Ordering> {
    if let Some(ordering) = compare_semver_like_versions(left, right) {
        return Some(ordering);
    }

    let left = parse_fabric_dependency_version(left)?;
    let right = parse_fabric_dependency_version(right)?;
    let max_len = left.core.len().max(right.core.len());

    for index in 0..max_len {
        let left_part = left.core.get(index).copied().unwrap_or(0);
        let right_part = right.core.get(index).copied().unwrap_or(0);
        match left_part.cmp(&right_part) {
            std::cmp::Ordering::Equal => {}
            ordering => return Some(ordering),
        }
    }

    match (&left.prerelease, &right.prerelease) {
        (None, None) => Some(std::cmp::Ordering::Equal),
        (Some(_), None) => Some(std::cmp::Ordering::Less),
        (None, Some(_)) => Some(std::cmp::Ordering::Greater),
        (Some(left_pre), Some(right_pre)) => {
            Some(compare_prerelease_identifiers(left_pre, right_pre))
        }
    }
}

fn compare_semver_like_versions(left: &str, right: &str) -> Option<std::cmp::Ordering> {
    let left = normalize_semver_like_version(left)?;
    let right = normalize_semver_like_version(right)?;
    Some(left.cmp(&right))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct ParsedFabricDependencyVersion {
    core: Vec<u64>,
    prerelease: Option<Vec<String>>,
}

fn parse_fabric_dependency_version(value: &str) -> Option<ParsedFabricDependencyVersion> {
    let mut main_and_build = value.trim().splitn(2, '+');
    let main = main_and_build.next()?.trim();
    if main.is_empty() {
        return None;
    }

    let mut core_and_pre = main.splitn(2, '-');
    let core = core_and_pre
        .next()?
        .split('.')
        .map(|segment| segment.trim().parse::<u64>().ok())
        .collect::<Option<Vec<_>>>()?;
    if core.is_empty() {
        return None;
    }

    let prerelease = core_and_pre.next().map(|value| {
        value
            .split('.')
            .map(|segment| segment.trim().to_ascii_lowercase())
            .filter(|segment| !segment.is_empty())
            .collect::<Vec<_>>()
    });

    Some(ParsedFabricDependencyVersion { core, prerelease })
}

fn normalize_semver_like_version(value: &str) -> Option<semver::Version> {
    let value = value.trim();
    if value.is_empty() {
        return None;
    }

    let (core_and_pre, build) = match value.split_once('+') {
        Some((left, right)) => (left.trim(), Some(right.trim())),
        None => (value, None),
    };
    let (core, prerelease) = match core_and_pre.split_once('-') {
        Some((left, right)) => (left.trim(), Some(right.trim())),
        None => (core_and_pre, None),
    };

    let mut core_parts = core
        .split('.')
        .map(str::trim)
        .filter(|segment| !segment.is_empty())
        .collect::<Vec<_>>();
    if core_parts.is_empty() || core_parts.len() > 3 {
        return None;
    }
    if core_parts
        .iter()
        .any(|segment| segment.parse::<u64>().is_err())
    {
        return None;
    }
    while core_parts.len() < 3 {
        core_parts.push("0");
    }

    let mut normalized = core_parts.join(".");
    if let Some(prerelease) = prerelease {
        if prerelease.is_empty() {
            return None;
        }
        normalized.push('-');
        normalized.push_str(prerelease);
    }
    if let Some(build) = build {
        if !build.is_empty() {
            normalized.push('+');
            normalized.push_str(build);
        }
    }

    semver::Version::parse(&normalized).ok()
}

fn compare_prerelease_identifiers(left: &[String], right: &[String]) -> std::cmp::Ordering {
    for index in 0..left.len().max(right.len()) {
        let Some(left_part) = left.get(index) else {
            return std::cmp::Ordering::Less;
        };
        let Some(right_part) = right.get(index) else {
            return std::cmp::Ordering::Greater;
        };

        let left_numeric = left_part.parse::<u64>().ok();
        let right_numeric = right_part.parse::<u64>().ok();
        let ordering = match (left_numeric, right_numeric) {
            (Some(left_numeric), Some(right_numeric)) => left_numeric.cmp(&right_numeric),
            (Some(_), None) => std::cmp::Ordering::Less,
            (None, Some(_)) => std::cmp::Ordering::Greater,
            (None, None) => left_part.cmp(right_part),
        };
        if ordering != std::cmp::Ordering::Equal {
            return ordering;
        }
    }

    std::cmp::Ordering::Equal
}
