use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const RULES_FILENAME: &str = "rules.json";

/// A named group of rules stored as a structural container in `rules.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleGroup {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// In-memory metadata for a rule group (not written to file directly).
/// Rules are referenced by `rule_name` and stored in the flat `ModList.rules` list.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleGroupMeta {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    /// Rule names (in order) for rules belonging to this group.
    pub rule_names: Vec<String>,
}

/// Flat internal representation of a mod-list.
/// The `rules` field holds ALL rules (grouped and ungrouped).
/// Group structure is tracked in `groups_meta` and written to file via `write_to_file`.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModList {
    pub modlist_name: String,
    pub author: String,
    pub description: String,
    /// All rules in a flat ordered list (grouped rules appear first, in group order).
    pub rules: Vec<Rule>,
    /// Group structure (not serialized directly; reconstructed during `read_from_file`).
    pub groups_meta: Vec<RuleGroupMeta>,
}

// ── File-format types (private) ───────────────────────────────────────────────

#[derive(Serialize, Deserialize)]
struct ModListFile {
    modlist_name: String,
    author: String,
    description: String,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    groups: Vec<RuleGroup>,
    #[serde(default)]
    rules: Vec<Rule>,
}

impl ModList {
    pub fn read_from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read rules file at {}", path.display()))?;

        // Try the current format first (ModListFile with groups).
        match serde_json::from_str::<ModListFile>(&contents) {
            Ok(file) => {
                let modlist = ModList::from_file_format(file);
                modlist.validate()?;
                return Ok(modlist);
            }
            Err(_) => {}
        }

        // Fall back to the legacy format (pre-options→mods rename).
        let legacy = serde_json::from_str::<legacy::LegacyModList>(&contents)
            .with_context(|| format!("failed to deserialize rules file at {}", path.display()))?;
        let migrated = legacy.into_current();
        migrated.validate()?;

        // Overwrite the file with the migrated format so we only migrate once.
        if let Err(write_err) = migrated.write_to_file(path) {
            let _ = write_err;
        }

        Ok(migrated)
    }

    fn from_file_format(file: ModListFile) -> Self {
        let mut all_rules: Vec<Rule> = Vec::new();
        let mut groups_meta: Vec<RuleGroupMeta> = Vec::new();

        for group in file.groups {
            let mut rule_names = Vec::new();
            for rule in group.rules {
                rule_names.push(rule.rule_name.clone());
                all_rules.push(rule);
            }
            groups_meta.push(RuleGroupMeta {
                id: group.id,
                name: group.name,
                collapsed: group.collapsed,
                rule_names,
            });
        }

        // Ungrouped rules follow.
        all_rules.extend(file.rules);

        ModList {
            modlist_name: file.modlist_name,
            author: file.author,
            description: file.description,
            rules: all_rules,
            groups_meta,
        }
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        self.validate()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directories for {}", path.display())
            })?;
        }

        let file = self.to_file_format();
        let json = serde_json::to_string_pretty(&file)
            .with_context(|| format!("failed to serialize rules file for {}", path.display()))?;

        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("failed to write rules file at {}", path.display()))?;

        Ok(())
    }

    fn to_file_format(&self) -> ModListFile {
        use std::collections::{HashMap, HashSet};

        // Build name → Rule map.
        let rule_map: HashMap<&str, &Rule> =
            self.rules.iter().map(|r| (r.rule_name.as_str(), r)).collect();

        // Build groups, tracking which rule names are grouped.
        let mut grouped_names: HashSet<&str> = HashSet::new();
        let groups: Vec<RuleGroup> = self
            .groups_meta
            .iter()
            .filter_map(|gm| {
                let rules: Vec<Rule> = gm
                    .rule_names
                    .iter()
                    .filter_map(|name| rule_map.get(name.as_str()).copied().cloned())
                    .collect();
                if rules.is_empty() {
                    return None;
                }
                for name in &gm.rule_names {
                    grouped_names.insert(name.as_str());
                }
                Some(RuleGroup {
                    id: gm.id.clone(),
                    name: gm.name.clone(),
                    collapsed: gm.collapsed,
                    rules,
                })
            })
            .collect();

        // Ungrouped rules — preserve order from self.rules.
        let ungrouped_rules: Vec<Rule> = self
            .rules
            .iter()
            .filter(|r| !grouped_names.contains(r.rule_name.as_str()))
            .cloned()
            .collect();

        ModListFile {
            modlist_name: self.modlist_name.clone(),
            author: self.author.clone(),
            description: self.description.clone(),
            groups,
            rules: ungrouped_rules,
        }
    }

    pub fn validate(&self) -> Result<()> {
        if self.modlist_name.trim().is_empty() {
            bail!("modlist_name cannot be empty");
        }

        if self.author.trim().is_empty() {
            bail!("author cannot be empty");
        }

        for (rule_index, rule) in self.rules.iter().enumerate() {
            validate_rule(rule, rule_index, "rule")?;
        }

        Ok(())
    }
}

fn validate_rule(rule: &Rule, index: usize, label: &str) -> Result<()> {
    if rule.rule_name.trim().is_empty() {
        bail!("{label} at index {index} has an empty rule_name");
    }

    if rule.mods.is_empty() {
        bail!(
            "{label} '{}' at index {index} must have at least one mod",
            rule.rule_name
        );
    }

    for mod_reference in &rule.mods {
        mod_reference.validate().with_context(|| {
            format!(
                "invalid mod entry '{}' in {label} '{}'",
                mod_reference.id, rule.rule_name
            )
        })?;
    }

    for (alt_index, alt) in rule.alternatives.iter().enumerate() {
        validate_rule(alt, alt_index, "alternative")?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub rule_name: String,
    pub mods: Vec<ModReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_if_present: Vec<String>,
    /// Fallback rules tried in order if the primary mods are excluded or incompatible.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<Rule>,
    /// Primary mod IDs of linked rules (stable across rule reorder).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub version_rules: Vec<RuleVersionFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_configs: Vec<RuleCustomConfig>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleVersionFilter {
    pub id: String,
    /// `"exclude"` or `"only"`
    pub kind: String,
    pub mc_versions: Vec<String>,
    pub loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleCustomConfig {
    pub id: String,
    pub files: Vec<RuleConfigFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleConfigFile {
    pub source_path: String,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModReference {
    pub id: String,
    pub source: ModSource,
    #[serde(default)]
    pub file_name: Option<String>,
}

impl ModReference {
    fn validate(&self) -> Result<()> {
        if self.id.trim().is_empty() {
            bail!("mod id cannot be empty");
        }

        match self.source {
            ModSource::Modrinth => {
                if self.file_name.is_some() {
                    bail!("modrinth entries must not define file_name");
                }
            }
            ModSource::Local => {
                if self
                    .file_name
                    .as_deref()
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                    .is_none()
                {
                    bail!("local entries must define file_name");
                }
            }
        }

        Ok(())
    }
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModSource {
    Modrinth,
    Local,
}

// ── Legacy format migration (pre-options→mods rename) ────────────────────────
//
// Before the schema was simplified, `Rule` stored its mods inside an
// `options: Vec<RuleOption>` array where `options[0]` was the primary group
// and `options[1..]` were top-level alternatives.  This module converts those
// old on-disk files transparently so users don't lose their data.

mod legacy {
    use serde::Deserialize;

    use super::{ModReference, ModSource, Rule};

    #[derive(Debug, Clone, Deserialize)]
    pub struct LegacyModList {
        pub modlist_name: String,
        pub author: String,
        pub description: String,
        #[serde(default)]
        pub rules: Vec<LegacyRule>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct LegacyRule {
        pub rule_name: String,
        #[serde(default)]
        pub options: Vec<LegacyRuleOption>,
    }

    #[derive(Debug, Clone, Deserialize)]
    pub struct LegacyRuleOption {
        pub mods: Vec<ModReference>,
        #[serde(default)]
        pub exclude_if_present: Vec<String>,
        #[serde(default)]
        pub option_name: Option<String>,
        #[serde(default)]
        pub alternatives: Vec<LegacyRuleOption>,
    }

    impl LegacyModList {
        pub fn into_current(self) -> super::ModList {
            super::ModList {
                modlist_name: self.modlist_name,
                author: self.author,
                description: self.description,
                rules: self.rules.into_iter().map(LegacyRule::into_current).collect(),
                groups_meta: vec![],
            }
        }
    }

    impl LegacyRule {
        fn into_current(self) -> Rule {
            let mut options = self.options.into_iter();
            let primary = match options.next() {
                Some(p) => p,
                None => {
                    // Empty options — produce an empty (will fail validate) rule.
                    return Rule {
                        rule_name: self.rule_name,
                        mods: vec![],
                        exclude_if_present: vec![],
                        alternatives: vec![],
                        links: vec![],
                        version_rules: vec![],
                        custom_configs: vec![],
                    };
                }
            };

            // Nested alternatives that were inside options[0].alternatives become
            // the first set of top-level alternatives in the new format.
            let mut alternatives: Vec<Rule> = primary
                .alternatives
                .into_iter()
                .enumerate()
                .map(|(i, opt)| option_to_rule(opt, i))
                .collect();

            // options[1..] also become top-level alternatives.
            for (i, opt) in options.enumerate() {
                alternatives.push(option_to_rule(opt, alternatives.len() + i));
            }

            Rule {
                rule_name: self.rule_name,
                mods: primary.mods,
                exclude_if_present: primary.exclude_if_present,
                alternatives,
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
            }
        }
    }

    fn option_to_rule(opt: LegacyRuleOption, index: usize) -> Rule {
        let rule_name = opt.option_name.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
            opt.mods
                .first()
                .map(|m| {
                    if m.source == ModSource::Local {
                        m.file_name.clone().unwrap_or_else(|| m.id.clone())
                    } else {
                        m.id.clone()
                    }
                })
                .unwrap_or_else(|| format!("Option {}", index + 1))
        });

        let alternatives = opt
            .alternatives
            .into_iter()
            .enumerate()
            .map(|(i, sub)| option_to_rule(sub, i))
            .collect();

        Rule {
            rule_name,
            mods: opt.mods,
            exclude_if_present: opt.exclude_if_present,
            alternatives,
            links: vec![],
            version_rules: vec![],
            custom_configs: vec![],
        }
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{ModList, ModReference, ModSource, Rule, RULES_FILENAME};

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-rules-test-{timestamp}"))
    }

    fn sample_modlist() -> ModList {
        ModList {
            modlist_name: "My Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test description".into(),
            rules: vec![Rule {
                rule_name: "Rendering Engine".into(),
                mods: vec![ModReference {
                    id: "sodium".into(),
                    source: ModSource::Modrinth,
                    file_name: None,
                }],
                exclude_if_present: vec![],
                alternatives: vec![],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
            }],
            groups_meta: vec![],
        }
    }

    #[test]
    fn deserialize_applies_rule_defaults() {
        let json = r#"
        {
          "modlist_name": "My Pack",
          "author": "ActiveGamertag",
          "description": "Test",
          "rules": [
            {
              "rule_name": "Rendering Engine",
              "mods": [
                { "id": "sodium", "source": "modrinth" }
              ]
            }
          ]
        }
        "#;

        let modlist = ModList::from_file_format(
            serde_json::from_str(json).expect("json should deserialize")
        );
        let rule = &modlist.rules[0];

        assert!(rule.exclude_if_present.is_empty());
        assert!(rule.alternatives.is_empty());
        assert!(rule.links.is_empty());
    }

    #[test]
    fn write_and_read_roundtrip_preserves_modlist() {
        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);
        let modlist = sample_modlist();

        modlist
            .write_to_file(&rules_path)
            .expect("rules should be written successfully");

        let reloaded = ModList::read_from_file(&rules_path).expect("rules should load back");

        assert_eq!(reloaded, modlist);

        fs::remove_dir_all(&root_dir).expect("temporary directory should be removable");
    }

    #[test]
    fn validation_rejects_local_mod_without_file_name() {
        let modlist = ModList {
            modlist_name: "My Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test description".into(),
            rules: vec![Rule {
                rule_name: "Manual Mods".into(),
                mods: vec![ModReference {
                    id: "optifine".into(),
                    source: ModSource::Local,
                    file_name: None,
                }],
                exclude_if_present: vec![],
                alternatives: vec![],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
            }],
            groups_meta: vec![],
        };

        let error = modlist.validate().expect_err("validation should fail");

        assert!(error.chain().any(|cause| cause
            .to_string()
            .contains("local entries must define file_name")));
    }

    #[test]
    fn validation_rejects_rule_without_mods() {
        let modlist = ModList {
            modlist_name: "My Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test description".into(),
            rules: vec![Rule {
                rule_name: "Broken Rule".into(),
                mods: vec![],
                exclude_if_present: vec![],
                alternatives: vec![],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
            }],
            groups_meta: vec![],
        };

        let error = modlist.validate().expect_err("validation should fail");

        assert!(error.to_string().contains("must have at least one mod"));
    }

    #[test]
    fn alternatives_are_serialized_at_rule_level() {
        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);

        let modlist = ModList {
            modlist_name: "My Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test description".into(),
            rules: vec![Rule {
                rule_name: "Rendering Engine".into(),
                mods: vec![ModReference {
                    id: "sodium".into(),
                    source: ModSource::Modrinth,
                    file_name: None,
                }],
                exclude_if_present: vec![],
                alternatives: vec![Rule {
                    rule_name: "Rubidium".into(),
                    mods: vec![ModReference {
                        id: "rubidium".into(),
                        source: ModSource::Modrinth,
                        file_name: None,
                    }],
                    exclude_if_present: vec![],
                    alternatives: vec![],
                    links: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                }],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
            }],
            groups_meta: vec![],
        };

        modlist.write_to_file(&rules_path).expect("should serialize");
        let json = fs::read_to_string(&rules_path).expect("should read");
        assert!(json.contains("\"alternatives\""));
        // Ensure primary mods are at rule level, not inside options
        assert!(!json.contains("\"options\""));
        // Alternative should be a child of the rule, not a sibling
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");
        let alt = &parsed["rules"][0]["alternatives"][0];
        assert_eq!(alt["rule_name"], "Rubidium");
        assert_eq!(alt["mods"][0]["id"], "rubidium");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn groups_are_written_as_containers_in_rules_json() {
        use super::RuleGroupMeta;

        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);

        let modlist = ModList {
            modlist_name: "My Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test description".into(),
            rules: vec![
                Rule {
                    rule_name: "Sodium".into(),
                    mods: vec![ModReference { id: "sodium".into(), source: ModSource::Modrinth, file_name: None }],
                    exclude_if_present: vec![], alternatives: vec![], links: vec![], version_rules: vec![], custom_configs: vec![],
                },
                Rule {
                    rule_name: "Iris".into(),
                    mods: vec![ModReference { id: "iris".into(), source: ModSource::Modrinth, file_name: None }],
                    exclude_if_present: vec![], alternatives: vec![], links: vec![], version_rules: vec![], custom_configs: vec![],
                },
            ],
            groups_meta: vec![RuleGroupMeta {
                id: "g1".into(),
                name: "Performance".into(),
                collapsed: false,
                rule_names: vec!["Sodium".into()],
            }],
        };

        modlist.write_to_file(&rules_path).expect("should write");

        let json = fs::read_to_string(&rules_path).expect("should read");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

        // Sodium should be inside the group
        assert_eq!(parsed["groups"][0]["id"], "g1");
        assert_eq!(parsed["groups"][0]["rules"][0]["rule_name"], "Sodium");
        // Iris should be ungrouped
        assert_eq!(parsed["rules"][0]["rule_name"], "Iris");

        // Roundtrip should preserve structure
        let reloaded = ModList::read_from_file(&rules_path).expect("should reload");
        assert_eq!(reloaded.rules.len(), 2);
        assert_eq!(reloaded.groups_meta.len(), 1);
        assert_eq!(reloaded.groups_meta[0].rule_names, vec!["Sodium"]);

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn read_from_file_migrates_legacy_options_format() {
        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);

        // Write a file in the OLD format (Rule had `options: Vec<RuleOption>`).
        let legacy_json = r#"{
  "modlist_name": "My Pack",
  "author": "Tester",
  "description": "Legacy pack",
  "rules": [
    {
      "rule_name": "Rendering Engine",
      "options": [
        {
          "mods": [{ "id": "sodium", "source": "modrinth" }],
          "exclude_if_present": [],
          "fallback_strategy": "continue",
          "alternatives": [
            {
              "mods": [{ "id": "rubidium", "source": "modrinth" }],
              "option_name": "Rubidium",
              "exclude_if_present": [],
              "fallback_strategy": "continue",
              "alternatives": []
            }
          ]
        }
      ]
    }
  ]
}
"#;

        std::fs::create_dir_all(rules_path.parent().unwrap())
            .expect("should create parent dirs");
        std::fs::write(&rules_path, legacy_json).expect("should write legacy json");

        let modlist = ModList::read_from_file(&rules_path)
            .expect("legacy format should be migrated and loaded");

        assert_eq!(modlist.modlist_name, "My Pack");
        assert_eq!(modlist.rules.len(), 1);
        let rule = &modlist.rules[0];
        assert_eq!(rule.rule_name, "Rendering Engine");
        assert_eq!(rule.mods.len(), 1);
        assert_eq!(rule.mods[0].id, "sodium");
        assert_eq!(rule.alternatives.len(), 1);
        assert_eq!(rule.alternatives[0].rule_name, "Rubidium");
        assert_eq!(rule.alternatives[0].mods[0].id, "rubidium");

        // The file should have been rewritten in the new format.
        let reloaded = ModList::read_from_file(&rules_path)
            .expect("migrated file should reload in new format");
        assert_eq!(reloaded.rules[0].mods[0].id, "sodium");

        fs::remove_dir_all(&root_dir).expect("temp dir should be removable");
    }
}
