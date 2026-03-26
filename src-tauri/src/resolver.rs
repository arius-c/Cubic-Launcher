use std::collections::HashSet;

use anyhow::Result;
use serde::Serialize;

use crate::rules::{ModList, Rule, VersionRule, VersionRuleKind};

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
        /// 0 = primary, 1+ = which alternative was used.
        option_index: usize,
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
    let mut active_mods = HashSet::new();
    let mut resolved_rules = Vec::with_capacity(modlist.rules.len());

    for rule in &modlist.rules {
        let outcome = try_resolve(rule, &active_mods, target, 0);
        if let RuleOutcome::Resolved { option_index } = &outcome {
            // Add the actually-resolved mod's id (primary or the chosen alternative).
            let resolved_id = if *option_index == 0 {
                &rule.mod_id
            } else {
                rule.alternatives
                    .get(*option_index - 1)
                    .map(|alt| &alt.mod_id)
                    .unwrap_or(&rule.mod_id)
            };
            active_mods.insert(resolved_id.clone());
        }
        resolved_rules.push(ResolvedRule {
            mod_id: rule.mod_id.clone(),
            outcome,
        });
    }

    Ok(ResolutionResult {
        active_mods,
        resolved_rules,
    })
}

fn try_resolve(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
    depth: usize,
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
        option_index: depth,
    }
}

fn try_alternatives(
    rule: &Rule,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
    reason: FailureReason,
) -> RuleOutcome {
    for (i, alt) in rule.alternatives.iter().enumerate() {
        let outcome = try_resolve(alt, active_mods, target, i + 1);
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
        let loader_matches =
            vr.loader == "any" || vr.loader == target.mod_loader.as_modrinth_loader();

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
        RuleOutcome::Resolved { option_index } => {
            if *option_index == 0 {
                Some(top_rule)
            } else {
                find_alt_by_depth(top_rule, *option_index)
            }
        }
        RuleOutcome::Unresolved { .. } => None,
    }
}

fn find_alt_by_depth(rule: &Rule, target_depth: usize) -> Option<&Rule> {
    for (i, alt) in rule.alternatives.iter().enumerate() {
        if i + 1 == target_depth {
            return Some(alt);
        }
        // Alternatives at this level are tried sequentially, not recursively nested
        // for depth tracking. Each alternative's own alternatives would have their own
        // depth counting from their perspective.
    }
    None
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
            RuleOutcome::Resolved { option_index: 0 }
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
            RuleOutcome::Resolved { option_index: 1 }
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
            RuleOutcome::Resolved { option_index: 2 }
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
