use std::collections::HashSet;

use anyhow::Result;

use crate::rules::{FallbackStrategy, ModList, ModReference, RuleOption};

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
    let mut active_mods = HashSet::new();
    let mut resolved_rules = Vec::with_capacity(modlist.rules.len());

    for rule in &modlist.rules {
        let mut rule_outcome = RuleOutcome::Unresolved {
            reason: RuleFailureReason::NoOptionsAvailable,
        };

        for (option_index, option) in rule.options.iter().enumerate() {
            if option_is_excluded(option, &active_mods) {
                rule_outcome = RuleOutcome::Unresolved {
                    reason: RuleFailureReason::ExcludedByActiveMods,
                };

                if option.fallback_strategy == FallbackStrategy::Abort {
                    break;
                }

                continue;
            }

            if option_group_is_compatible(option, target, compatibility_checker)? {
                let resolved_mods = option.mods.clone();

                for mod_reference in &resolved_mods {
                    active_mods.insert(mod_reference.id.clone());
                }

                rule_outcome = RuleOutcome::Resolved {
                    option_index,
                    mods: resolved_mods,
                };

                break;
            }

            rule_outcome = RuleOutcome::Unresolved {
                reason: RuleFailureReason::IncompatibleGroup,
            };

            if option.fallback_strategy == FallbackStrategy::Abort {
                break;
            }
        }

        resolved_rules.push(ResolvedRule {
            rule_name: rule.rule_name.clone(),
            outcome: rule_outcome,
        });
    }

    Ok(ResolutionResult {
        active_mods,
        resolved_rules,
    })
}

fn option_is_excluded(option: &RuleOption, active_mods: &HashSet<String>) -> bool {
    option
        .exclude_if_present
        .iter()
        .any(|mod_id| active_mods.contains(mod_id))
}

fn option_group_is_compatible(
    option: &RuleOption,
    target: &ResolutionTarget,
    compatibility_checker: &impl CompatibilityChecker,
) -> Result<bool> {
    for mod_reference in &option.mods {
        if !compatibility_checker.is_compatible(mod_reference, target)? {
            return Ok(false);
        }
    }

    Ok(true)
}

#[cfg(test)]
mod tests {
    use std::collections::{HashMap, HashSet};

    use anyhow::Result;

    use crate::rules::{FallbackStrategy, ModList, ModReference, ModSource, Rule, RuleOption};

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

    #[test]
    fn continues_to_next_option_when_first_option_is_incompatible() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                options: vec![
                    RuleOption {
                        mods: vec![modrinth_mod("sodium")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![modrinth_mod("rubidium")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                ],
            }],
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
    fn aborts_rule_when_excluded_option_uses_abort_strategy() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            rules: vec![
                Rule {
                    rule_name: "Rendering".into(),
                    options: vec![RuleOption {
                        mods: vec![modrinth_mod("sodium")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    }],
                },
                Rule {
                    rule_name: "Conflicting Mod".into(),
                    options: vec![
                        RuleOption {
                            mods: vec![modrinth_mod("mod-b")],
                            exclude_if_present: vec!["sodium".into()],
                            fallback_strategy: FallbackStrategy::Abort,
                            option_name: None,
                            alternatives: vec![],
                        },
                        RuleOption {
                            mods: vec![modrinth_mod("mod-b-alt")],
                            exclude_if_present: vec![],
                            fallback_strategy: FallbackStrategy::Continue,
                            option_name: None,
                            alternatives: vec![],
                        },
                    ],
                },
            ],
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
            RuleOutcome::Unresolved {
                reason: RuleFailureReason::ExcludedByActiveMods,
            }
        );
        assert!(!result.active_mods.contains("mod-b"));
        assert!(!result.active_mods.contains("mod-b-alt"));
    }

    #[test]
    fn group_fails_when_any_member_is_incompatible_and_falls_back() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                options: vec![
                    RuleOption {
                        mods: vec![modrinth_mod("optifine"), modrinth_mod("optifabric")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![modrinth_mod("sodium")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                ],
            }],
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
    fn aborts_rule_when_incompatible_group_uses_abort_strategy() {
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test".into(),
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                options: vec![
                    RuleOption {
                        mods: vec![modrinth_mod("optifine"), modrinth_mod("optifabric")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Abort,
                        option_name: None,
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![modrinth_mod("sodium")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                ],
            }],
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
            rules: vec![
                Rule {
                    rule_name: "Primary Renderer".into(),
                    options: vec![RuleOption {
                        mods: vec![modrinth_mod("sodium")],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    }],
                },
                Rule {
                    rule_name: "Secondary Renderer".into(),
                    options: vec![
                        RuleOption {
                            mods: vec![modrinth_mod("embeddium")],
                            exclude_if_present: vec!["sodium".into()],
                            fallback_strategy: FallbackStrategy::Continue,
                            option_name: None,
                            alternatives: vec![],
                        },
                        RuleOption {
                            mods: vec![modrinth_mod("iris")],
                            exclude_if_present: vec![],
                            fallback_strategy: FallbackStrategy::Continue,
                            option_name: None,
                            alternatives: vec![],
                        },
                    ],
                },
            ],
        };

        let checker =
            FakeCompatibilityChecker::with([("sodium", true), ("embeddium", true), ("iris", true)]);

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
