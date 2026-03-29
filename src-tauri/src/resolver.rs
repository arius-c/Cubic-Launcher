use std::collections::HashSet;

use anyhow::{bail, Result};
use serde::Serialize;
use tauri::State;

use crate::launcher_paths::LauncherPaths;
use crate::rules::{ModList, Rule, VersionRule, VersionRuleKind, RULES_FILENAME};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionTarget {
    pub minecraft_version: String,
    pub mod_loader: ModLoader,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash, Serialize)]
pub enum ModLoader {
    Fabric,
    NeoForge,
    Forge,
    Quilt,
    Vanilla,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionResult {
    pub active_mods: HashSet<String>,
    pub resolved_rules: Vec<ResolvedRule>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolvedRule {
    pub mod_id: String,
    pub outcome: RuleOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleOutcome {
    Resolved {
        /// The mod_id that actually resolved (primary or a nested alternative).
        resolved_id: String,
    },
    Unresolved {
        reason: FailureReason,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum FailureReason {
    ExcludedByActiveMod,
    RequiredModMissing,
    IncompatibleVersion,
    NoOptionAvailable,
}

pub fn resolve_modlist(modlist: &ModList, target: &ResolutionTarget) -> Result<ResolutionResult> {
    // ── Pass 1: sequential resolve to build the initial active set ────────────
    let mut active_mods = HashSet::new();
    let mut resolved_rules = Vec::with_capacity(modlist.rules.len());

    for rule in &modlist.rules {
        let outcome = try_resolve(rule, &active_mods, target);
        if let RuleOutcome::Resolved { ref resolved_id } = outcome {
            active_mods.insert(resolved_id.clone());
        }
        resolved_rules.push(ResolvedRule {
            mod_id: rule.mod_id.clone(),
            outcome,
        });
    }

    // ── Pass 2: re-check each rule against the FULL active set ───────────────
    // Pass 1 is sequential, so a loser that appears before its winner won't see
    // the winner in active_mods.  Pass 2 uses the complete set from pass 1 to
    // retroactively exclude those losers.
    let full_active = active_mods.clone();
    let mut final_active = HashSet::new();

    for (i, rule) in modlist.rules.iter().enumerate() {
        let outcome = try_resolve(rule, &full_active, target);
        if let RuleOutcome::Resolved { ref resolved_id } = outcome {
            final_active.insert(resolved_id.clone());
        }
        resolved_rules[i] = ResolvedRule {
            mod_id: rule.mod_id.clone(),
            outcome,
        };
    }

    Ok(ResolutionResult {
        active_mods: final_active,
        resolved_rules,
    })
}

fn try_resolve(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
) -> RuleOutcome {
    // 1. Check exclude_if
    if rule.exclude_if.iter().any(|id| active_mods.contains(id)) {
        return try_alternatives(rule, active_mods, target, FailureReason::ExcludedByActiveMod);
    }

    // 2. Check requires
    if rule.requires.iter().any(|id| !active_mods.contains(id)) {
        return try_alternatives(rule, active_mods, target, FailureReason::RequiredModMissing);
    }

    // 3. Check version_rules
    if version_rules_conflict(&rule.version_rules, target) {
        return try_alternatives(rule, active_mods, target, FailureReason::IncompatibleVersion);
    }

    RuleOutcome::Resolved {
        resolved_id: rule.mod_id.clone(),
    }
}

fn try_alternatives(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
    reason: FailureReason,
) -> RuleOutcome {
    for alt in &rule.alternatives {
        let outcome = try_resolve(alt, active_mods, target);
        if matches!(outcome, RuleOutcome::Resolved { .. }) {
            return outcome;
        }
    }
    RuleOutcome::Unresolved { reason }
}

/// Returns true if the version rules exclude this mod for the given target.
fn version_rules_conflict(version_rules: &[VersionRule], target: &ResolutionTarget) -> bool {
    for vr in version_rules {
        let version_matches = vr.mc_versions.iter().any(|v| v == &target.minecraft_version);
        let vr_loader = vr.loader.to_ascii_lowercase();
        let loader_matches =
            vr_loader == "any" || vr_loader == target.mod_loader.as_modrinth_loader();

        match vr.kind {
            VersionRuleKind::Only => {
                // The mod is EXCLUDED unless version AND loader match.
                if !(version_matches && loader_matches) {
                    return true;
                }
            }
            VersionRuleKind::Exclude => {
                // The mod is EXCLUDED if version AND loader match.
                if version_matches && loader_matches {
                    return true;
                }
            }
        }
    }
    false
}

/// Look up which Rule was actually resolved for a top-level rule (follows alternatives).
pub fn find_resolved_rule<'a>(top_rule: &'a Rule, outcome: &RuleOutcome) -> Option<&'a Rule> {
    match outcome {
        RuleOutcome::Resolved { resolved_id } => find_rule_by_id(top_rule, resolved_id),
        RuleOutcome::Unresolved { .. } => None,
    }
}

fn find_rule_by_id<'a>(rule: &'a Rule, mod_id: &str) -> Option<&'a Rule> {
    if rule.mod_id == mod_id {
        return Some(rule);
    }
    for alt in &rule.alternatives {
        if let Some(found) = find_rule_by_id(alt, mod_id) {
            return Some(found);
        }
    }
    None
}

// ── Tauri commands ────────────────────────────────────────────────────────────

fn parse_mod_loader(value: &str) -> Result<ModLoader> {
    match value.trim().to_ascii_lowercase().as_str() {
        "fabric" => Ok(ModLoader::Fabric),
        "quilt" => Ok(ModLoader::Quilt),
        "forge" => Ok(ModLoader::Forge),
        "neoforge" => Ok(ModLoader::NeoForge),
        "vanilla" => Ok(ModLoader::Vanilla),
        other => bail!("unsupported mod loader '{other}'"),
    }
}

/// Returns the set of active (resolved) mod IDs for a given modlist + version + loader.
#[tauri::command]
pub fn resolve_modlist_command(
    launcher_paths: State<'_, LauncherPaths>,
    modlist_name: String,
    mc_version: String,
    mod_loader: String,
) -> Result<Vec<String>, String> {
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&modlist_name)
        .join(RULES_FILENAME);

    let modlist = ModList::read_from_file(&rules_path).map_err(|e| e.to_string())?;

    let target = ResolutionTarget {
        minecraft_version: mc_version,
        mod_loader: parse_mod_loader(&mod_loader).map_err(|e| e.to_string())?,
    };

    let result = resolve_modlist(&modlist, &target).map_err(|e| e.to_string())?;

    // For rules whose primary didn't resolve, check ALL alternatives (not just
    // the first winner) so the UI can show every viable fallback as green.
    let mut ids = result.active_mods;
    for (i, rule) in modlist.rules.iter().enumerate() {
        let primary_resolved = matches!(
            &result.resolved_rules[i].outcome,
            RuleOutcome::Resolved { resolved_id } if resolved_id == &rule.mod_id
        );
        // Only expand alternatives when the primary itself was excluded.
        if primary_resolved {
            continue;
        }
        check_all_alts_recursive(rule, &mut ids, &target);
    }

    Ok(ids.into_iter().collect())
}

/// Recursively check every alternative in the tree and add viable ones to `ids`.
/// Only marks an alt green if the alt *itself* passes all checks.
/// Only recurses into an alt's children when the alt itself is NOT viable
/// (its sub-alternatives are only relevant as fallbacks when it fails).
fn check_all_alts_recursive(
    rule: &Rule,
    ids: &mut HashSet<String>,
    target: &ResolutionTarget,
) {
    for alt in &rule.alternatives {
        // Already resolved by the main resolver — it's viable, skip its children.
        if ids.contains(&alt.mod_id) {
            continue;
        }
        if alt_itself_viable(alt, ids, target) {
            ids.insert(alt.mod_id.clone());
            // Alt is viable — its sub-alternatives are irrelevant.
        } else {
            // Alt failed — check its sub-alternatives as fallbacks.
            check_all_alts_recursive(alt, ids, target);
        }
    }
}

/// Returns true only if this specific rule passes all checks (exclude_if, requires,
/// version_rules) — ignores its own alternatives.
fn alt_itself_viable(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
) -> bool {
    if rule.exclude_if.iter().any(|id| active_mods.contains(id)) {
        return false;
    }
    if rule.requires.iter().any(|id| !active_mods.contains(id)) {
        return false;
    }
    !version_rules_conflict(&rule.version_rules, target)
}

#[cfg(test)]
mod tests {
    use crate::rules::{ModSource, Rule, VersionRule, VersionRuleKind};

    use super::*;

    fn target() -> ResolutionTarget {
        ResolutionTarget {
            minecraft_version: "1.21.1".into(),
            mod_loader: ModLoader::Fabric,
        }
    }

    fn simple_rule(mod_id: &str) -> Rule {
        Rule {
            mod_id: mod_id.into(),
            source: ModSource::Modrinth,
            exclude_if: vec![],
            requires: vec![],
            version_rules: vec![],
            custom_configs: vec![],
            alternatives: vec![],
        }
    }

    fn modlist(rules: Vec<Rule>) -> ModList {
        ModList {
            modlist_name: "Test Pack".into(),
            author: "Author".into(),
            description: "Test".into(),
            rules,
        }
    }

    #[test]
    fn basic_resolution() {
        let ml = modlist(vec![simple_rule("sodium"), simple_rule("lithium")]);
        let result = resolve_modlist(&ml, &target()).unwrap();

        assert!(result.active_mods.contains("sodium"));
        assert!(result.active_mods.contains("lithium"));
        assert_eq!(result.resolved_rules.len(), 2);
        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Resolved { resolved_id: "sodium".into() }
        );
    }

    #[test]
    fn exclude_if_with_fallback() {
        let ml = modlist(vec![
            simple_rule("sodium"),
            Rule {
                mod_id: "embeddium".into(),
                source: ModSource::Modrinth,
                exclude_if: vec!["sodium".into()],
                requires: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alternatives: vec![simple_rule("iris")],
            },
        ]);

        let result = resolve_modlist(&ml, &target()).unwrap();

        assert!(result.active_mods.contains("sodium"));
        assert!(!result.active_mods.contains("embeddium"));
        assert!(result.active_mods.contains("iris"));
        assert_eq!(
            result.resolved_rules[1].outcome,
            RuleOutcome::Resolved { resolved_id: "iris".into() }
        );
    }

    #[test]
    fn requires_satisfied() {
        let ml = modlist(vec![
            simple_rule("fabric-api"),
            Rule {
                mod_id: "sodium".into(),
                source: ModSource::Modrinth,
                exclude_if: vec![],
                requires: vec!["fabric-api".into()],
                version_rules: vec![],
                custom_configs: vec![],
                alternatives: vec![],
            },
        ]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(result.active_mods.contains("sodium"));
    }

    #[test]
    fn requires_unsatisfied() {
        let ml = modlist(vec![Rule {
            mod_id: "sodium".into(),
            source: ModSource::Modrinth,
            exclude_if: vec![],
            requires: vec!["fabric-api".into()],
            version_rules: vec![],
            custom_configs: vec![],
            alternatives: vec![],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(!result.active_mods.contains("sodium"));
        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Unresolved {
                reason: FailureReason::RequiredModMissing,
            }
        );
    }

    #[test]
    fn version_rule_exclude() {
        let ml = modlist(vec![Rule {
            mod_id: "sodium".into(),
            source: ModSource::Modrinth,
            exclude_if: vec![],
            requires: vec![],
            version_rules: vec![VersionRule {
                kind: VersionRuleKind::Exclude,
                mc_versions: vec!["1.21.1".into()],
                loader: "fabric".into(),
            }],
            custom_configs: vec![],
            alternatives: vec![],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(!result.active_mods.contains("sodium"));
        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Unresolved {
                reason: FailureReason::IncompatibleVersion,
            }
        );
    }

    #[test]
    fn version_rule_only() {
        // "only" on 1.20.1/forge — should fail on 1.21.1/fabric target
        let ml = modlist(vec![Rule {
            mod_id: "optifine".into(),
            source: ModSource::Local,
            exclude_if: vec![],
            requires: vec![],
            version_rules: vec![VersionRule {
                kind: VersionRuleKind::Only,
                mc_versions: vec!["1.20.1".into()],
                loader: "forge".into(),
            }],
            custom_configs: vec![],
            alternatives: vec![],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(!result.active_mods.contains("optifine"));
    }

    #[test]
    fn version_rule_only_matches() {
        let ml = modlist(vec![Rule {
            mod_id: "optifine".into(),
            source: ModSource::Local,
            exclude_if: vec![],
            requires: vec![],
            version_rules: vec![VersionRule {
                kind: VersionRuleKind::Only,
                mc_versions: vec!["1.21.1".into()],
                loader: "fabric".into(),
            }],
            custom_configs: vec![],
            alternatives: vec![],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(result.active_mods.contains("optifine"));
    }

    #[test]
    fn version_rule_exclude_with_any_loader() {
        let ml = modlist(vec![Rule {
            mod_id: "sodium".into(),
            source: ModSource::Modrinth,
            exclude_if: vec![],
            requires: vec![],
            version_rules: vec![VersionRule {
                kind: VersionRuleKind::Exclude,
                mc_versions: vec!["1.21.1".into()],
                loader: "any".into(),
            }],
            custom_configs: vec![],
            alternatives: vec![],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(!result.active_mods.contains("sodium"));
    }

    #[test]
    fn deep_alternative_recursion() {
        let ml = modlist(vec![Rule {
            mod_id: "a".into(),
            source: ModSource::Modrinth,
            exclude_if: vec![],
            requires: vec!["missing".into()], // fails
            version_rules: vec![],
            custom_configs: vec![],
            alternatives: vec![
                Rule {
                    mod_id: "b".into(),
                    source: ModSource::Modrinth,
                    exclude_if: vec![],
                    requires: vec!["also-missing".into()], // also fails
                    version_rules: vec![],
                    custom_configs: vec![],
                    alternatives: vec![],
                },
                simple_rule("c"), // should succeed
            ],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(result.active_mods.contains("c"));
        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Resolved { resolved_id: "c".into() }
        );
    }

    #[test]
    fn all_alternatives_fail() {
        let ml = modlist(vec![Rule {
            mod_id: "a".into(),
            source: ModSource::Modrinth,
            exclude_if: vec![],
            requires: vec!["missing".into()],
            version_rules: vec![],
            custom_configs: vec![],
            alternatives: vec![Rule {
                mod_id: "b".into(),
                source: ModSource::Modrinth,
                exclude_if: vec![],
                requires: vec!["also-missing".into()],
                version_rules: vec![],
                custom_configs: vec![],
                alternatives: vec![],
            }],
        }]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(result.active_mods.is_empty());
        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Unresolved {
                reason: FailureReason::RequiredModMissing,
            }
        );
    }

    #[test]
    fn exclude_if_no_alternatives_stays_unresolved() {
        let ml = modlist(vec![
            simple_rule("sodium"),
            Rule {
                mod_id: "mod-b".into(),
                source: ModSource::Modrinth,
                exclude_if: vec!["sodium".into()],
                requires: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alternatives: vec![],
            },
        ]);

        let result = resolve_modlist(&ml, &target()).unwrap();
        assert!(!result.active_mods.contains("mod-b"));
        assert_eq!(
            result.resolved_rules[1].outcome,
            RuleOutcome::Unresolved {
                reason: FailureReason::ExcludedByActiveMod,
            }
        );
    }
}
