use std::fs;
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};

pub const RULES_FILENAME: &str = "rules.json";

// ── Public presentation type (shared with modlist_assets) ─────────────────────

/// Modlist cosmetic data stored inline in `rules.json` (schema v3+).
/// Kept as `Option` in `ModList`; `None` means "not yet saved to rules.json —
/// fall back to the legacy `modlist-presentation.json` file".
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ModlistPresentation {
    pub icon_label: String,
    pub icon_accent: String,
    pub notes: String,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub icon_image: Option<String>,
}

// ── Public in-memory group types ──────────────────────────────────────────────

/// Top-level structural group (persisted in `rules.json`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleGroup {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    #[serde(default)]
    pub rules: Vec<Rule>,
}

/// Internal metadata for a top-level structural group.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuleGroupMeta {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    pub rule_names: Vec<String>,
}

/// Internal metadata for a visual alternative group (stored inside a `Rule`,
/// not persisted directly — serialised inline in the alternatives array).
#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AltGroupMeta {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    /// Rule names of the alternatives that belong to this visual group.
    #[serde(default)]
    pub block_names: Vec<String>,
}

// ── Public in-memory modlist ──────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ModList {
    pub modlist_name: String,
    pub author: String,
    pub description: String,
    /// All rules in a flat ordered list.
    pub rules: Vec<Rule>,
    /// Top-level group structure (not serialised directly).
    pub groups_meta: Vec<RuleGroupMeta>,
    /// Cosmetic data; `None` when not yet stored in rules.json.
    pub presentation: Option<ModlistPresentation>,
}

// ── v3 file-format types (private) ───────────────────────────────────────────
//
// Schema version 3 stores alternative groups inline in the alternatives array
// (same polymorphic `type` tag as top-level items).  `alt_groups` no longer
// appears as a sibling field.

/// A single rule as it appears in the v3 file (no `alt_groups` field).
#[derive(Serialize, Deserialize)]
struct RuleFileData {
    rule_name: String,
    mods: Vec<ModReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    exclude_if_present: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    alternatives: Vec<AltTopLevelItem>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    links: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    version_rules: Vec<RuleVersionFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    custom_configs: Vec<RuleCustomConfig>,
}

/// Polymorphic item inside a rule's alternatives list (v3).
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum AltTopLevelItem {
    Group(AltGroupFileData),
    SingleRule(RuleFileData),
}

/// A visual group of alternatives as stored in the v3 file.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct AltGroupFileData {
    id: String,
    name: String,
    collapsed: bool,
    #[serde(default)]
    rules: Vec<RuleFileData>,
}

/// Top-level structural group in v3.
#[derive(Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
struct RuleGroupFileData {
    id: String,
    name: String,
    collapsed: bool,
    #[serde(default)]
    rules: Vec<RuleFileData>,
}

/// Polymorphic top-level item in the v3 file.
#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TopLevelItemFile {
    Group(RuleGroupFileData),
    SingleRule(RuleFileData),
}

/// v3 file root.
#[derive(Serialize, Deserialize)]
struct ModListFileV3 {
    modlist_name: String,
    author: String,
    description: String,
    schema_version: u32, // always 3 when written
    #[serde(default, skip_serializing_if = "Option::is_none")]
    presentation: Option<ModlistPresentation>,
    #[serde(default)]
    rules: Vec<TopLevelItemFile>,
}

// ── v2 file-format types (private, read-only) ─────────────────────────────────
//
// The old format stored `alternatives: Vec<Rule>` flat and had a sibling
// `alt_groups` field.  We keep these types so existing files are read without
// data loss.

#[derive(Serialize, Deserialize)]
#[serde(tag = "type", rename_all = "snake_case")]
enum TopLevelItem {
    Group(RuleGroup),
    SingleRule(Rule),
}

/// v2 file root (schema_version: 2).
#[derive(Serialize, Deserialize)]
struct ModListFile {
    modlist_name: String,
    author: String,
    description: String,
    schema_version: u32,
    #[serde(default)]
    rules: Vec<TopLevelItem>,
}

/// Pre-v2 format: separate `groups` array + flat `rules` array.
#[derive(Deserialize)]
struct LegacyGroupsModListFile {
    modlist_name: String,
    author: String,
    description: String,
    #[serde(default)]
    groups: Vec<RuleGroup>,
    #[serde(default)]
    rules: Vec<Rule>,
}

// ── Conversion between in-memory Rule and v3 RuleFileData ────────────────────

fn rule_to_file_data(rule: &Rule) -> RuleFileData {
    use std::collections::{HashMap, HashSet};

    // Build name → group-index map from alt_groups.
    let mut name_to_group: HashMap<&str, usize> = HashMap::new();
    for (gi, ag) in rule.alt_groups.iter().enumerate() {
        for name in &ag.block_names {
            name_to_group.insert(name.as_str(), gi);
        }
    }

    let mut items: Vec<AltTopLevelItem> = Vec::new();
    let mut emitted: HashSet<usize> = HashSet::new();

    for alt in &rule.alternatives {
        if let Some(&gi) = name_to_group.get(alt.rule_name.as_str()) {
            if !emitted.contains(&gi) {
                emitted.insert(gi);
                let ag = &rule.alt_groups[gi];
                let group_rules: Vec<RuleFileData> = ag
                    .block_names
                    .iter()
                    .filter_map(|name| {
                        rule.alternatives.iter().find(|a| a.rule_name == *name)
                    })
                    .map(rule_to_file_data)
                    .collect();
                items.push(AltTopLevelItem::Group(AltGroupFileData {
                    id: ag.id.clone(),
                    name: ag.name.clone(),
                    collapsed: ag.collapsed,
                    rules: group_rules,
                }));
            }
        } else {
            items.push(AltTopLevelItem::SingleRule(rule_to_file_data(alt)));
        }
    }

    RuleFileData {
        rule_name: rule.rule_name.clone(),
        mods: rule.mods.clone(),
        exclude_if_present: rule.exclude_if_present.clone(),
        alternatives: items,
        links: rule.links.clone(),
        version_rules: rule.version_rules.clone(),
        custom_configs: rule.custom_configs.clone(),
    }
}

fn file_data_to_rule(data: RuleFileData) -> Rule {
    let mut alternatives: Vec<Rule> = Vec::new();
    let mut alt_groups: Vec<AltGroupMeta> = Vec::new();

    for item in data.alternatives {
        match item {
            AltTopLevelItem::Group(g) => {
                let rule_names: Vec<String> =
                    g.rules.iter().map(|r| r.rule_name.clone()).collect();
                for r in g.rules {
                    alternatives.push(file_data_to_rule(r));
                }
                alt_groups.push(AltGroupMeta {
                    id: g.id,
                    name: g.name,
                    collapsed: g.collapsed,
                    block_names: rule_names,
                });
            }
            AltTopLevelItem::SingleRule(r) => {
                alternatives.push(file_data_to_rule(r));
            }
        }
    }

    Rule {
        rule_name: data.rule_name,
        mods: data.mods,
        exclude_if_present: data.exclude_if_present,
        alternatives,
        links: data.links,
        version_rules: data.version_rules,
        custom_configs: data.custom_configs,
        alt_groups,
    }
}

// ── ModList impl ──────────────────────────────────────────────────────────────

impl ModList {
    pub fn read_from_file(path: &Path) -> Result<Self> {
        let contents = fs::read_to_string(path)
            .with_context(|| format!("failed to read rules file at {}", path.display()))?;

        // 1. Try v3 (schema_version: 3, polymorphic alternatives).
        if let Ok(file) = serde_json::from_str::<ModListFileV3>(&contents) {
            if file.schema_version == 3 {
                let modlist = ModList::from_file_format_v3(file);
                modlist.validate()?;
                return Ok(modlist);
            }
        }

        // 2. Try v2 (schema_version: 2, flat alternatives + alt_groups sibling).
        if let Ok(file) = serde_json::from_str::<ModListFile>(&contents) {
            let modlist = ModList::from_file_format_v2(file);
            if modlist.validate().is_ok() {
                return Ok(modlist);
            }
        }

        // 3. Pre-v2 format (separate `groups` array + flat `rules` array).
        if let Ok(file) = serde_json::from_str::<LegacyGroupsModListFile>(&contents) {
            let modlist = ModList::from_grouped_file_format(file);
            if modlist.validate().is_ok() {
                return Ok(modlist);
            }
        }

        // 4. Very-legacy format (rules had `options: Vec<RuleOption>`).
        let legacy = serde_json::from_str::<legacy::LegacyModList>(&contents)
            .with_context(|| format!("failed to deserialize rules file at {}", path.display()))?;
        let migrated = legacy.into_current();
        migrated.validate()?;
        let _ = migrated.write_to_file(path);
        Ok(migrated)
    }

    fn from_file_format_v3(file: ModListFileV3) -> Self {
        let mut all_rules: Vec<Rule> = Vec::new();
        let mut groups_meta: Vec<RuleGroupMeta> = Vec::new();

        for item in file.rules {
            match item {
                TopLevelItemFile::Group(g) => {
                    let mut rule_names = Vec::new();
                    for rfd in g.rules {
                        let rule = file_data_to_rule(rfd);
                        rule_names.push(rule.rule_name.clone());
                        all_rules.push(rule);
                    }
                    groups_meta.push(RuleGroupMeta {
                        id: g.id,
                        name: g.name,
                        collapsed: g.collapsed,
                        rule_names,
                    });
                }
                TopLevelItemFile::SingleRule(rfd) => {
                    all_rules.push(file_data_to_rule(rfd));
                }
            }
        }

        ModList {
            modlist_name: file.modlist_name,
            author: file.author,
            description: file.description,
            rules: all_rules,
            groups_meta,
            presentation: file.presentation,
        }
    }

    fn from_file_format_v2(file: ModListFile) -> Self {
        let mut all_rules: Vec<Rule> = Vec::new();
        let mut groups_meta: Vec<RuleGroupMeta> = Vec::new();

        for item in file.rules {
            match item {
                TopLevelItem::Group(group) => {
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
                TopLevelItem::SingleRule(rule) => {
                    all_rules.push(rule);
                }
            }
        }

        ModList {
            modlist_name: file.modlist_name,
            author: file.author,
            description: file.description,
            rules: all_rules,
            groups_meta,
            presentation: None,
        }
    }

    fn from_grouped_file_format(file: LegacyGroupsModListFile) -> Self {
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
        all_rules.extend(file.rules);

        ModList {
            modlist_name: file.modlist_name,
            author: file.author,
            description: file.description,
            rules: all_rules,
            groups_meta,
            presentation: None,
        }
    }

    pub fn write_to_file(&self, path: &Path) -> Result<()> {
        self.validate()?;

        if let Some(parent) = path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!("failed to create parent directories for {}", path.display())
            })?;
        }

        let file = self.to_file_format_v3();
        let json = serde_json::to_string_pretty(&file)
            .with_context(|| format!("failed to serialize rules file for {}", path.display()))?;

        fs::write(path, format!("{json}\n"))
            .with_context(|| format!("failed to write rules file at {}", path.display()))?;

        Ok(())
    }

    fn to_file_format_v3(&self) -> ModListFileV3 {
        use std::collections::{HashMap, HashSet};

        let rule_map: HashMap<&str, &Rule> =
            self.rules.iter().map(|r| (r.rule_name.as_str(), r)).collect();

        let mut rule_to_group: HashMap<&str, usize> = HashMap::new();
        for (gm_idx, gm) in self.groups_meta.iter().enumerate() {
            for name in &gm.rule_names {
                rule_to_group.insert(name.as_str(), gm_idx);
            }
        }

        let mut items: Vec<TopLevelItemFile> = Vec::new();
        let mut emitted_groups: HashSet<usize> = HashSet::new();

        for rule in &self.rules {
            if let Some(&gm_idx) = rule_to_group.get(rule.rule_name.as_str()) {
                if !emitted_groups.contains(&gm_idx) {
                    emitted_groups.insert(gm_idx);
                    let gm = &self.groups_meta[gm_idx];
                    let group_rules: Vec<RuleFileData> = gm
                        .rule_names
                        .iter()
                        .filter_map(|name| rule_map.get(name.as_str()).copied())
                        .map(rule_to_file_data)
                        .collect();
                    if !group_rules.is_empty() {
                        items.push(TopLevelItemFile::Group(RuleGroupFileData {
                            id: gm.id.clone(),
                            name: gm.name.clone(),
                            collapsed: gm.collapsed,
                            rules: group_rules,
                        }));
                    }
                }
            } else {
                items.push(TopLevelItemFile::SingleRule(rule_to_file_data(rule)));
            }
        }

        ModListFileV3 {
            modlist_name: self.modlist_name.clone(),
            author: self.author.clone(),
            description: self.description.clone(),
            schema_version: 3,
            presentation: self.presentation.clone(),
            rules: items,
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

// ── Rule (public in-memory type) ──────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct Rule {
    pub rule_name: String,
    pub mods: Vec<ModReference>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub exclude_if_present: Vec<String>,
    /// Flat list of fallback rules (groups tracked separately in `alt_groups`).
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub alternatives: Vec<Rule>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub links: Vec<String>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub version_rules: Vec<RuleVersionFilter>,
    #[serde(default, skip_serializing_if = "Vec::is_empty")]
    pub custom_configs: Vec<RuleCustomConfig>,
    /// Visual group metadata for this rule's alternatives panel.
    /// Not emitted to JSON directly — stored inline via v3 conversion.
    /// Kept for reading v2 files that still have the old `altGroups` field.
    #[serde(default, rename = "altGroups", skip_serializing_if = "Vec::is_empty")]
    pub alt_groups: Vec<AltGroupMeta>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleVersionFilter {
    pub id: String,
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

// ── Legacy format migration ───────────────────────────────────────────────────

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
                presentation: None,
            }
        }
    }

    impl LegacyRule {
        fn into_current(self) -> Rule {
            let mut options = self.options.into_iter();
            let primary = match options.next() {
                Some(p) => p,
                None => {
                    return Rule {
                        rule_name: self.rule_name,
                        mods: vec![],
                        exclude_if_present: vec![],
                        alternatives: vec![],
                        links: vec![],
                        version_rules: vec![],
                        custom_configs: vec![],
                        alt_groups: vec![],
                    };
                }
            };

            let mut alternatives: Vec<Rule> = primary
                .alternatives
                .into_iter()
                .enumerate()
                .map(|(i, opt)| option_to_rule(opt, i))
                .collect();

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
                alt_groups: vec![],
            }
        }
    }

    fn option_to_rule(opt: LegacyRuleOption, index: usize) -> Rule {
        let rule_name =
            opt.option_name.filter(|s| !s.trim().is_empty()).unwrap_or_else(|| {
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
            alt_groups: vec![],
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{AltGroupMeta, ModList, ModReference, ModSource, Rule, RULES_FILENAME};

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
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        }
    }

    #[test]
    fn deserialize_applies_rule_defaults() {
        let json = r#"
        {
          "modlist_name": "My Pack",
          "author": "ActiveGamertag",
          "description": "Test",
          "schema_version": 3,
          "rules": [
            {
              "type": "single_rule",
              "rule_name": "Rendering Engine",
              "mods": [
                { "id": "sodium", "source": "modrinth" }
              ]
            }
          ]
        }
        "#;

        let modlist = ModList::read_from_file(&{
            let dir = unique_test_root();
            let p = dir.join("mod-lists").join("My Pack").join(RULES_FILENAME);
            fs::create_dir_all(p.parent().unwrap()).unwrap();
            fs::write(&p, json).unwrap();
            p
        })
        .expect("should deserialize");

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

        modlist.write_to_file(&rules_path).expect("rules should be written successfully");
        let reloaded = ModList::read_from_file(&rules_path).expect("rules should load back");

        assert_eq!(reloaded.modlist_name, modlist.modlist_name);
        assert_eq!(reloaded.author, modlist.author);
        assert_eq!(reloaded.rules.len(), modlist.rules.len());
        assert_eq!(reloaded.rules[0].rule_name, modlist.rules[0].rule_name);

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
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        };

        let error = modlist.validate().expect_err("validation should fail");
        assert!(error.chain().any(|c| c.to_string().contains("local entries must define file_name")));
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
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        };

        let error = modlist.validate().expect_err("validation should fail");
        assert!(error.to_string().contains("must have at least one mod"));
    }

    #[test]
    fn alternatives_are_serialized_at_rule_level_v3() {
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
                mods: vec![ModReference { id: "sodium".into(), source: ModSource::Modrinth, file_name: None }],
                exclude_if_present: vec![],
                alternatives: vec![Rule {
                    rule_name: "Rubidium".into(),
                    mods: vec![ModReference { id: "rubidium".into(), source: ModSource::Modrinth, file_name: None }],
                    exclude_if_present: vec![],
                    alternatives: vec![],
                    links: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alt_groups: vec![],
                }],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        };

        modlist.write_to_file(&rules_path).expect("should serialize");
        let json = fs::read_to_string(&rules_path).expect("should read");
        assert!(json.contains("\"alternatives\""));
        assert!(!json.contains("\"options\""));
        assert!(!json.contains("\"altGroups\""));
        assert!(!json.contains("\"alt_groups\""));

        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");
        assert_eq!(parsed["schema_version"], 3);
        assert_eq!(parsed["rules"][0]["type"], "single_rule");

        let alt = &parsed["rules"][0]["alternatives"][0];
        assert_eq!(alt["type"], "single_rule");
        assert_eq!(alt["rule_name"], "Rubidium");
        assert_eq!(alt["mods"][0]["id"], "rubidium");

        // Roundtrip
        let reloaded = ModList::read_from_file(&rules_path).expect("should reload");
        assert_eq!(reloaded.rules[0].alternatives[0].rule_name, "Rubidium");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn alt_groups_are_stored_inline_in_v3() {
        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);

        let modlist = ModList {
            modlist_name: "My Pack".into(),
            author: "TestAuthor".into(),
            description: "".into(),
            rules: vec![Rule {
                rule_name: "Rendering Engine".into(),
                mods: vec![ModReference { id: "sodium".into(), source: ModSource::Modrinth, file_name: None }],
                exclude_if_present: vec![],
                alternatives: vec![
                    Rule {
                        rule_name: "Rubidium".into(),
                        mods: vec![ModReference { id: "rubidium".into(), source: ModSource::Modrinth, file_name: None }],
                        exclude_if_present: vec![], alternatives: vec![], links: vec![],
                        version_rules: vec![], custom_configs: vec![], alt_groups: vec![],
                    },
                    Rule {
                        rule_name: "OptiFine".into(),
                        mods: vec![ModReference { id: "optifine".into(), source: ModSource::Local, file_name: Some("OptiFine.jar".into()) }],
                        exclude_if_present: vec![], alternatives: vec![], links: vec![],
                        version_rules: vec![], custom_configs: vec![], alt_groups: vec![],
                    },
                ],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![AltGroupMeta {
                    id: "ag-1".into(),
                    name: "Legacy Options".into(),
                    collapsed: false,
                    block_names: vec!["Rubidium".into(), "OptiFine".into()],
                }],
            }],
            groups_meta: vec![],
            presentation: None,
        };

        modlist.write_to_file(&rules_path).expect("should write");
        let json = fs::read_to_string(&rules_path).expect("should read");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

        // Alternatives should now be a group, not a flat list
        let alts = &parsed["rules"][0]["alternatives"];
        assert_eq!(alts[0]["type"], "group");
        assert_eq!(alts[0]["name"], "Legacy Options");
        assert_eq!(alts[0]["rules"][0]["rule_name"], "Rubidium");
        assert_eq!(alts[0]["rules"][1]["rule_name"], "OptiFine");
        // No alt_groups sibling field
        assert!(parsed["rules"][0].get("altGroups").is_none());
        assert!(parsed["rules"][0].get("alt_groups").is_none());

        // Roundtrip must reconstruct alt_groups
        let reloaded = ModList::read_from_file(&rules_path).expect("should reload");
        let reloaded_rule = &reloaded.rules[0];
        assert_eq!(reloaded_rule.alternatives.len(), 2);
        assert_eq!(reloaded_rule.alt_groups.len(), 1);
        assert_eq!(reloaded_rule.alt_groups[0].block_names, vec!["Rubidium", "OptiFine"]);

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
                    exclude_if_present: vec![], alternatives: vec![], links: vec![], version_rules: vec![], custom_configs: vec![], alt_groups: vec![],
                },
                Rule {
                    rule_name: "Iris".into(),
                    mods: vec![ModReference { id: "iris".into(), source: ModSource::Modrinth, file_name: None }],
                    exclude_if_present: vec![], alternatives: vec![], links: vec![], version_rules: vec![], custom_configs: vec![], alt_groups: vec![],
                },
            ],
            groups_meta: vec![RuleGroupMeta {
                id: "g1".into(),
                name: "Performance".into(),
                collapsed: false,
                rule_names: vec!["Sodium".into()],
            }],
            presentation: None,
        };

        modlist.write_to_file(&rules_path).expect("should write");

        let json = fs::read_to_string(&rules_path).expect("should read");
        let parsed: serde_json::Value = serde_json::from_str(&json).expect("should parse");

        assert_eq!(parsed["schema_version"], 3);
        assert_eq!(parsed["rules"][0]["type"], "group");
        assert_eq!(parsed["rules"][0]["id"], "g1");
        assert_eq!(parsed["rules"][0]["rules"][0]["rule_name"], "Sodium");
        assert_eq!(parsed["rules"][1]["type"], "single_rule");
        assert_eq!(parsed["rules"][1]["rule_name"], "Iris");
        assert!(parsed.get("groups").is_none());

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

        std::fs::create_dir_all(rules_path.parent().unwrap()).expect("should create parent dirs");
        std::fs::write(&rules_path, legacy_json).expect("should write legacy json");

        let modlist = ModList::read_from_file(&rules_path)
            .expect("legacy format should be migrated and loaded");

        assert_eq!(modlist.modlist_name, "My Pack");
        assert_eq!(modlist.rules.len(), 1);
        let rule = &modlist.rules[0];
        assert_eq!(rule.rule_name, "Rendering Engine");
        assert_eq!(rule.mods[0].id, "sodium");
        assert_eq!(rule.alternatives.len(), 1);
        assert_eq!(rule.alternatives[0].rule_name, "Rubidium");
        assert_eq!(rule.alternatives[0].mods[0].id, "rubidium");

        fs::remove_dir_all(&root_dir).expect("temp dir should be removable");
    }

    #[test]
    fn read_v2_format_still_works() {
        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);

        // Write a file in the old v2 format (flat alternatives, altGroups sibling)
        let v2_json = r#"{
  "modlist_name": "My Pack",
  "author": "Tester",
  "description": "V2 pack",
  "schema_version": 2,
  "rules": [
    {
      "type": "single_rule",
      "rule_name": "Rendering Engine",
      "mods": [{ "id": "sodium", "source": "modrinth" }],
      "alternatives": [
        { "rule_name": "Rubidium", "mods": [{ "id": "rubidium", "source": "modrinth" }] }
      ],
      "altGroups": [
        { "id": "ag-1", "name": "Forge", "collapsed": false, "blockNames": ["Rubidium"] }
      ]
    }
  ]
}
"#;

        fs::create_dir_all(rules_path.parent().unwrap()).unwrap();
        fs::write(&rules_path, v2_json).unwrap();

        let modlist = ModList::read_from_file(&rules_path).expect("v2 format should load");
        assert_eq!(modlist.rules[0].rule_name, "Rendering Engine");
        assert_eq!(modlist.rules[0].alternatives[0].rule_name, "Rubidium");
        // alt_groups should be populated from the old altGroups field
        assert_eq!(modlist.rules[0].alt_groups.len(), 1);
        assert_eq!(modlist.rules[0].alt_groups[0].block_names, vec!["Rubidium"]);

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn presentation_roundtrips_in_v3() {
        use super::ModlistPresentation;

        let root_dir = unique_test_root();
        let rules_path = root_dir
            .join("mod-lists")
            .join("My Pack")
            .join(RULES_FILENAME);

        let mut modlist = sample_modlist();
        modlist.presentation = Some(ModlistPresentation {
            icon_label: "MP".into(),
            icon_accent: "blue".into(),
            notes: "My notes".into(),
            icon_image: None,
        });

        modlist.write_to_file(&rules_path).expect("should write");
        let reloaded = ModList::read_from_file(&rules_path).expect("should reload");

        assert_eq!(reloaded.presentation, modlist.presentation);

        fs::remove_dir_all(&root_dir).ok();
    }
}
