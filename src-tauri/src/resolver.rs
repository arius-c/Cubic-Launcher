use std::collections::HashSet;

use anyhow::Result;

use crate::rules::{ModList, ModReference, Rule};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ResolutionTarget {
    pub minecraft_version: String,
    pub mod_loader: ModLoader,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Hash)]
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
    pub rule_name: String,
    pub outcome: RuleOutcome,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub enum RuleOutcome {
    Resolved {
        /// 0 = primary mods were used, 1+ = Nth alternative was used.
        option_index: usize,
        mods: Vec<ModReference>,
    },
    Unresolved {
        reason: RuleFailureReason,
    },
}

#[derive(Debug, Copy, Clone, PartialEq, Eq)]
pub enum RuleFailureReason {
    NoOptionsAvailable,
    ExcludedByActiveMods,
    IncompatibleGroup,
}

pub trait CompatibilityChecker {
    fn is_compatible(
        &self,
        mod_reference: &ModReference,
        target: &ResolutionTarget,
    ) -> Result<bool>;
}

pub fn resolve_modlist(
    modlist: &ModList,
    target: &ResolutionTarget,
    compatibility_checker: &impl CompatibilityChecker,
) -> Result<ResolutionResult> {
    // Pass 1: resolve rules in order, building active_mods incrementally.
    // Incompatibilities only apply to losers processed AFTER their winners here.
    let mut active_mods = HashSet::new();
    let mut resolved_rules = Vec::with_capacity(modlist.rules.len());

    for rule in &modlist.rules {
        let outcome = try_resolve_rule(rule, 0, &active_mods, target, compatibility_checker)?;
        if let RuleOutcome::Resolved { ref mods, .. } = outcome {
            for m in mods {
                active_mods.insert(m.id.clone());
            }
        }
        resolved_rules.push(ResolvedRule {
            rule_name: rule.rule_name.clone(),
            outcome,
        });
    }

    // Pass 2: re-resolve every rule using the full post-pass-1 active_mods.
    // This correctly excludes losers that were resolved before their winners in pass 1.
    // Winners never have the loser's mods in their exclude_if_present, so this is safe.
    let pass1_active = active_mods.clone();
    let mut final_active = HashSet::new();

    for (i, rule) in modlist.rules.iter().enumerate() {
        let outcome = try_resolve_rule(rule, 0, &pass1_active, target, compatibility_checker)?;
        if let RuleOutcome::Resolved { ref mods, .. } = outcome {
            for m in mods {
                final_active.insert(m.id.clone());
            }
        }
        resolved_rules[i] = ResolvedRule {
            rule_name: rule.rule_name.clone(),
            outcome,
        };
    }

    Ok(ResolutionResult {
        active_mods: final_active,
        resolved_rules,
    })
}

/// Try to resolve a rule. `option_index` tracks which position this rule occupies
/// in the fallback chain: 0 = primary, 1 = first alternative, 2 = second, etc.
fn try_resolve_rule(
    rule: &Rule,
    option_index: usize,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
    checker: &impl CompatibilityChecker,
) -> Result<RuleOutcome> {
    // 1. Is this rule excluded by an already-active mod?
    if rule_is_excluded(rule, active_mods) {
        return try_alternatives(
            rule,
            option_index,
            active_mods,
            target,
            checker,
            RuleFailureReason::ExcludedByActiveMods,
        );
    }

    // 2. Are all mods in this rule compatible with the target?
    if !all_mods_compatible(&rule.mods, target, checker)? {
        return try_alternatives(
            rule,
            option_index,
            active_mods,
            target,
            checker,
            RuleFailureReason::IncompatibleGroup,
        );
    }

    // 3. Resolved — primary mods are active.
    Ok(RuleOutcome::Resolved {
        option_index,
        mods: rule.mods.clone(),
    })
}

/// Iterate alternatives in order, returning the first that resolves successfully.
fn try_alternatives(
    rule: &Rule,
    base_index: usize,
    active_mods: &HashSet<String>,
    target: &ResolutionTarget,
    checker: &impl CompatibilityChecker,
    fallback_reason: RuleFailureReason,
) -> Result<RuleOutcome> {
    for (i, alt) in rule.alternatives.iter().enumerate() {
        let outcome = try_resolve_rule(alt, base_index + i + 1, active_mods, target, checker)?;
        if matches!(outcome, RuleOutcome::Resolved { .. }) {
            return Ok(outcome);
        }
    }
    Ok(RuleOutcome::Unresolved {
        reason: fallback_reason,
    })
}

fn rule_is_excluded(rule: &Rule, active_mods: &HashSet<String>) -> bool {
    rule.exclude_if_present
        .iter()
        .any(|mod_id| active_mods.contains(mod_id))
}

fn all_mods_compatible(
    mods: &[ModReference],
    target: &ResolutionTarget,
    checker: &impl CompatibilityChecker,
) -> Result<bool> {
    for mod_reference in mods {
        if !checker.is_compatible(mod_reference, target)? {
            return Ok(false);
        }
    }
    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use anyhow::Result;

    use crate::rules::{ModList, ModReference, ModSource, Rule};

    use super::{
        resolve_modlist, CompatibilityChecker, ModLoader, ResolutionTarget, RuleFailureReason,
        RuleOutcome,
    };

    #[derive(Default)]
    struct FakeCompatibilityChecker {
        compatibility: HashMap<String, bool>,
    }

    impl FakeCompatibilityChecker {
        fn with(entries: impl IntoIterator<Item = (&'static str, bool)>) -> Self {
            Self {
                compatibility: entries
                    .into_iter()
                    .map(|(mod_id, is_compatible)| (mod_id.to_string(), is_compatible))
                    .collect(),
            }
        }
    }

    impl CompatibilityChecker for FakeCompatibilityChecker {
        fn is_compatible(
            &self,
            mod_reference: &ModReference,
            _target: &ResolutionTarget,
        ) -> Result<bool> {
            Ok(*self.compatibility.get(&mod_reference.id).unwrap_or(&false))
        }
    }

    fn target() -> ResolutionTarget {
        ResolutionTarget {
            minecraft_version: "1.21.1".into(),
            mod_loader: ModLoader::Fabric,
        }
    }

    fn modrinth_mod(id: &str) -> ModReference {
        ModReference {
            id: id.into(),
            source: ModSource::Modrinth,
            file_name: None,
        }
    }

    fn simple_rule(name: &str, mod_id: &str) -> Rule {
        Rule {
            rule_name: name.into(),
            mods: vec![modrinth_mod(mod_id)],
            exclude_if_present: vec![],
            alternatives: vec![],
            links: vec![],
            version_rules: vec![],
            custom_configs: vec![],
            alt_groups: vec![],
        }
    }

    #[test]
    fn continues_to_alternative_when_primary_is_incompatible() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            groups_meta: vec![],
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                mods: vec![modrinth_mod("sodium")],
                exclude_if_present: vec![],
                alternatives: vec![simple_rule("Rubidium", "rubidium")],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            presentation: None,
        };

        let checker = FakeCompatibilityChecker::with([("sodium", false), ("rubidium", true)]);

        let result =
            resolve_modlist(&modlist, &target(), &checker).expect("resolution should work");

        assert!(result.active_mods.contains("rubidium"));
        assert!(!result.active_mods.contains("sodium"));
        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Resolved {
                option_index: 1,
                mods: vec![modrinth_mod("rubidium")],
            }
        );
    }

    #[test]
    fn rule_stays_unresolved_when_excluded_and_no_alternatives() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            groups_meta: vec![],
            rules: vec![
                simple_rule("Primary Renderer", "sodium"),
                Rule {
                    rule_name: "Conflicting Mod".into(),
                    mods: vec![modrinth_mod("mod-b")],
                    exclude_if_present: vec!["sodium".into()],
                    alternatives: vec![],
                    links: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alt_groups: vec![],
                },
            ],
            presentation: None,
        };

        let checker =
            FakeCompatibilityChecker::with([("sodium", true), ("mod-b", true)]);

        let result =
            resolve_modlist(&modlist, &target(), &checker).expect("resolution should work");

        assert_eq!(
            result.resolved_rules[1].outcome,
            RuleOutcome::Unresolved {
                reason: RuleFailureReason::ExcludedByActiveMods,
            }
        );
        assert!(!result.active_mods.contains("mod-b"));
    }

    #[test]
    fn falls_back_to_alternative_when_excluded() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            groups_meta: vec![],
            rules: vec![
                simple_rule("Primary Renderer", "sodium"),
                Rule {
                    rule_name: "Conflicting Mod".into(),
                    mods: vec![modrinth_mod("mod-b")],
                    exclude_if_present: vec!["sodium".into()],
                    alternatives: vec![simple_rule("Alt Mod", "mod-b-alt")],
                    links: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alt_groups: vec![],
                },
            ],
            presentation: None,
        };

        let checker = FakeCompatibilityChecker::with([
            ("sodium", true),
            ("mod-b", true),
            ("mod-b-alt", true),
        ]);

        let result =
            resolve_modlist(&modlist, &target(), &checker).expect("resolution should work");

        assert_eq!(
            result.resolved_rules[1].outcome,
            RuleOutcome::Resolved {
                option_index: 1,
                mods: vec![modrinth_mod("mod-b-alt")],
            }
        );
        assert!(result.active_mods.contains("mod-b-alt"));
        assert!(!result.active_mods.contains("mod-b"));
    }

    #[test]
    fn group_fails_when_any_member_is_incompatible_and_falls_back() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            groups_meta: vec![],
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                mods: vec![modrinth_mod("optifine"), modrinth_mod("optifabric")],
                exclude_if_present: vec![],
                alternatives: vec![simple_rule("Sodium", "sodium")],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            presentation: None,
        };

        let checker = FakeCompatibilityChecker::with([
            ("optifine", true),
            ("optifabric", false),
            ("sodium", true),
        ]);

        let result =
            resolve_modlist(&modlist, &target(), &checker).expect("resolution should work");

        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Resolved {
                option_index: 1,
                mods: vec![modrinth_mod("sodium")],
            }
        );
        assert_eq!(result.active_mods, HashSet::from([String::from("sodium")]));
    }

    #[test]
    fn rule_stays_unresolved_when_incompatible_and_no_alternatives() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            groups_meta: vec![],
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                mods: vec![modrinth_mod("optifine"), modrinth_mod("optifabric")],
                exclude_if_present: vec![],
                alternatives: vec![],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            presentation: None,
        };

        let checker = FakeCompatibilityChecker::with([
            ("optifine", true),
            ("optifabric", false),
        ]);

        let result =
            resolve_modlist(&modlist, &target(), &checker).expect("resolution should work");

        assert_eq!(
            result.resolved_rules[0].outcome,
            RuleOutcome::Unresolved {
                reason: RuleFailureReason::IncompatibleGroup,
            }
        );
        assert!(result.active_mods.is_empty());
    }

    #[test]
    fn earlier_rules_populate_active_mods_for_later_exclusions() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            groups_meta: vec![],
            rules: vec![
                simple_rule("Primary Renderer", "sodium"),
                Rule {
                    rule_name: "Secondary Renderer".into(),
                    mods: vec![modrinth_mod("embeddium")],
                    exclude_if_present: vec!["sodium".into()],
                    alternatives: vec![simple_rule("Iris", "iris")],
                    links: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alt_groups: vec![],
                },
            ],
            presentation: None,
        };

        let checker = FakeCompatibilityChecker::with([
            ("sodium", true),
            ("embeddium", true),
            ("iris", true),
        ]);

        let result =
            resolve_modlist(&modlist, &target(), &checker).expect("resolution should work");

        assert_eq!(
            result.active_mods,
            HashSet::from([String::from("sodium"), String::from("iris")])
        );
        assert_eq!(
            result.resolved_rules[1].outcome,
            RuleOutcome::Resolved {
                option_index: 1,
                mods: vec![modrinth_mod("iris")],
            }
        );
    }
}
