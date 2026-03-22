use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const RULES_FILENAME: &str = "rules.json";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ModList {
    pub modlist_name: String,
    pub author: String,
    pub description: String,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

impl ModList {
    pub fn read_from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read rules file at {}", path.display()))?;
        let modlist = serde_json::from_str::<Self>(&contents)
            .with_context(|| format!("failed to deserialize rules file at {}", path.display()))?;

        modlist.validate()?;

        Ok(modlist)
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        self.validate()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directories for {}", path.display())
            })?;
        }

        let json = serde_json::to_string_pretty(self)
            .with_context(|| format!("failed to serialize rules file for {}", path.display()))?;

        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("failed to write rules file at {}", path.display()))?;

        Ok(())
    }

    pub fn validate(&self) -> Result<()> {
        if self.modlist_name.trim().is_empty() {
            bail!("modlist_name cannot be empty");
        }

        if self.author.trim().is_empty() {
            bail!("author cannot be empty");
        }

        for (rule_index, rule) in self.rules.iter().enumerate() {
            if rule.rule_name.trim().is_empty() {
                bail!("rule at index {rule_index} has an empty rule_name");
            }

            for (option_index, option) in rule.options.iter().enumerate() {
                validate_option(option, &rule.rule_name, option_index, "option")?;
            }
        }

        Ok(())
    }
}

fn validate_option(
    option: &RuleOption,
    rule_name: &str,
    option_index: usize,
    label: &str,
) -> Result<()> {
    if option.mods.is_empty() {
        bail!(
            "rule '{}' {} at index {} must contain at least one mod",
            rule_name,
            label,
            option_index
        );
    }

    for mod_reference in &option.mods {
        mod_reference.validate().with_context(|| {
            format!(
                "invalid mod entry '{}' in rule '{}' {} {}",
                mod_reference.id, rule_name, label, option_index
            )
        })?;
    }

    for (alt_index, alt) in option.alternatives.iter().enumerate() {
        validate_option(alt, rule_name, alt_index, "nested alternative")?;
    }

    Ok(())
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub rule_name: String,
    #[serde(default)]
    pub options: Vec<RuleOption>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
pub struct RuleOption {
    pub mods: Vec<ModReference>,
    #[serde(default)]
    pub exclude_if_present: Vec<String>,
    #[serde(default)]
    pub fallback_strategy: FallbackStrategy,
    /// Optional display name for this option. Used when a standalone rule is
    /// converted into an alternative — preserves the original rule_name so the
    /// UI can show a human-readable label instead of just the mod ID.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub option_name: Option<String>,
    /// Nested fallback options — tried if this option is not compatible.
    /// These are the "alternatives of this alternative" in the editor UI.
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<RuleOption>,
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

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "lowercase")]
pub enum FallbackStrategy {
    #[default]
    Continue,
    Abort,
}

#[derive(Debug, Copy, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum ModSource {
    Modrinth,
    Local,
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{
        FallbackStrategy, ModList, ModReference, ModSource, Rule, RuleOption, RULES_FILENAME,
    };

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
                options: vec![RuleOption {
                    mods: vec![ModReference {
                        id: "sodium".into(),
                        source: ModSource::Modrinth,
                        file_name: None,
                    }],
                    exclude_if_present: vec![],
                    fallback_strategy: FallbackStrategy::Continue,
                    option_name: None,
                    alternatives: vec![],
                }],
            }],
        }
    }

    #[test]
    fn deserialize_applies_option_defaults() {
        let json = r#"
        {
          "modlist_name": "My Pack",
          "author": "ActiveGamertag",
          "description": "Test",
          "rules": [
            {
              "rule_name": "Rendering Engine",
              "options": [
                {
                  "mods": [
                    { "id": "sodium", "source": "modrinth" }
                  ]
                }
              ]
            }
          ]
        }
        "#;

        let modlist: ModList = serde_json::from_str(json).expect("json should deserialize");
        let option = &modlist.rules[0].options[0];

        assert!(option.exclude_if_present.is_empty());
        assert_eq!(option.fallback_strategy, FallbackStrategy::Continue);
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
                options: vec![RuleOption {
                    mods: vec![ModReference {
                        id: "optifine".into(),
                        source: ModSource::Local,
                        file_name: None,
                    }],
                    exclude_if_present: vec![],
                    fallback_strategy: FallbackStrategy::Continue,
                    option_name: None,
                    alternatives: vec![],
                }],
            }],
        };

        let error = modlist.validate().expect_err("validation should fail");

        assert!(error.chain().any(|cause| cause
            .to_string()
            .contains("local entries must define file_name")));
    }

    #[test]
    fn validation_rejects_option_without_mods() {
        let modlist = ModList {
            modlist_name: "My Pack".into(),
            author: "ActiveGamertag".into(),
            description: "Test description".into(),
            rules: vec![Rule {
                rule_name: "Broken Rule".into(),
                options: vec![RuleOption {
                    mods: vec![],
                    exclude_if_present: vec![],
                    fallback_strategy: FallbackStrategy::Continue,
                    option_name: None,
                    alternatives: vec![],
                }],
            }],
        };

        let error = modlist.validate().expect_err("validation should fail");

        assert!(error.to_string().contains("must contain at least one mod"));
    }
}
