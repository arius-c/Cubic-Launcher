use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::launcher_paths::LauncherPaths;
use crate::rules::{CustomConfig, ModList, ModSource, Rule, VersionRule, VersionRuleKind, RULES_FILENAME};

// ── Snapshot types (sent to frontend) ────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorSnapshot {
    pub modlist_name: String,
    pub rows: Vec<EditorRow>,
    pub incompatibilities: Vec<IncompatibilityEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorRow {
    pub mod_id: String,
    pub name: String,
    pub source: String,
    pub exclude_if: Vec<String>,
    pub requires: Vec<String>,
    pub version_rules: Vec<EditorVersionRule>,
    pub custom_configs: Vec<EditorCustomConfig>,
    pub alternatives: Vec<EditorRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorVersionRule {
    pub kind: String,
    pub mc_versions: Vec<String>,
    pub loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCustomConfig {
    pub mc_versions: Vec<String>,
    pub loader: String,
    pub target_path: String,
    pub files: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompatibilityEntry {
    pub winner_id: String,
    pub loser_id: String,
}

// ── Input structs ────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddModRuleInput {
    pub modlist_name: String,
    pub mod_id: String,
    pub source: String,
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRulesInput {
    pub modlist_name: String,
    pub mod_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRuleInput {
    pub modlist_name: String,
    pub mod_id: String,
    pub new_mod_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRulesInput {
    pub modlist_name: String,
    pub ordered_mod_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAlternativeOrderInput {
    pub modlist_name: String,
    pub parent_mod_id: String,
    pub ordered_alt_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAlternativeInput {
    pub modlist_name: String,
    pub parent_mod_id: String,
    pub mod_id: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNestedAlternativeInput {
    pub modlist_name: String,
    pub parent_mod_id: String,
    pub mod_id: String,
    pub source: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAlternativeInput {
    pub modlist_name: String,
    pub parent_mod_id: String,
    pub alt_mod_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveIncompatibilitiesInput {
    pub modlist_name: String,
    pub rules: Vec<IncompatibilityRuleInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct IncompatibilityRuleInput {
    pub winner_id: String,
    pub loser_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRuleAdvancedInput {
    pub modlist_name: String,
    pub mod_id: String,
    pub exclude_if: Vec<String>,
    pub requires: Vec<String>,
    pub version_rules: Vec<SaveVersionRuleInput>,
    pub custom_configs: Vec<SaveCustomConfigInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAdvancedBatchInput {
    pub modlist_name: String,
    pub requires_entries: Vec<RequiresEntry>,
    pub version_rules_entries: Vec<VersionRulesEntry>,
    pub custom_configs_entries: Vec<CustomConfigsEntry>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RequiresEntry {
    pub mod_id: String,
    pub requires: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct VersionRulesEntry {
    pub mod_id: String,
    pub version_rules: Vec<SaveVersionRuleInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CustomConfigsEntry {
    pub mod_id: String,
    pub custom_configs: Vec<SaveCustomConfigInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveVersionRuleInput {
    pub kind: String,
    pub mc_versions: Vec<String>,
    pub loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveCustomConfigInput {
    pub mc_versions: Vec<String>,
    pub loader: String,
    pub target_path: String,
    pub files: Vec<String>,
}

// ── Tauri commands ───────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_modlist_editor_command(
    launcher_paths: State<'_, LauncherPaths>,
    selected_modlist_name: String,
) -> Result<EditorSnapshot, String> {
    load_editor_snapshot_from_root(launcher_paths.root_dir(), &selected_modlist_name)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_mod_rule_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: AddModRuleInput,
) -> Result<(), String> {
    add_mod_rule_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn delete_rules_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: DeleteRulesInput,
) -> Result<(), String> {
    delete_rules_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn rename_rule_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: RenameRuleInput,
) -> Result<(), String> {
    rename_rule_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn reorder_rules_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: ReorderRulesInput,
) -> Result<(), String> {
    reorder_rules_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_alternative_order_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveAlternativeOrderInput,
) -> Result<(), String> {
    save_alternative_order_from_root(launcher_paths.root_dir(), &input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: AddAlternativeInput,
) -> Result<(), String> {
    add_alternative_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn add_nested_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: AddNestedAlternativeInput,
) -> Result<(), String> {
    add_nested_alternative_from_root(launcher_paths.root_dir(), &input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn remove_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: RemoveAlternativeInput,
) -> Result<(), String> {
    remove_alternative_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_incompatibilities_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveIncompatibilitiesInput,
) -> Result<(), String> {
    save_incompatibilities_from_root(launcher_paths.root_dir(), &input)
        .map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_rule_advanced_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveRuleAdvancedInput,
) -> Result<(), String> {
    save_rule_advanced_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

#[tauri::command]
pub fn save_advanced_batch_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveAdvancedBatchInput,
) -> Result<(), String> {
    save_advanced_batch_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
}

// ── Worker functions ─────────────────────────────────────────────────────────

fn load_modlist(root_dir: &Path, modlist_name: &str) -> Result<ModList> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join(RULES_FILENAME);
    ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' from {}",
            modlist_name,
            rules_path.display()
        )
    })
}

fn save_modlist(root_dir: &Path, modlist_name: &str, modlist: &ModList) -> Result<()> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join(RULES_FILENAME);
    modlist.write_to_file(&rules_path)
}

pub fn load_editor_snapshot_from_root(
    root_dir: &Path,
    modlist_name: &str,
) -> Result<EditorSnapshot> {
    let modlist = load_modlist(root_dir, modlist_name)?;

    let rows: Vec<EditorRow> = modlist.rules.iter().map(build_editor_row).collect();
    let incompatibilities = derive_incompatibilities(&modlist.rules);

    Ok(EditorSnapshot {
        modlist_name: modlist.modlist_name.clone(),
        rows,
        incompatibilities,
    })
}

pub fn add_mod_rule_from_root(root_dir: &Path, input: &AddModRuleInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    if modlist.contains_mod_id(&input.mod_id) {
        bail!(
            "a rule with mod_id '{}' already exists in modlist '{}'",
            input.mod_id,
            input.modlist_name
        );
    }

    let source = parse_mod_source(&input.source)?;

    // For local source with a file_name, copy the JAR to local-jars/
    if source == ModSource::Local {
        if let Some(file_name) = &input.file_name {
            let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
            let local_jars_dir = launcher_paths
                .modlists_dir()
                .join(&input.modlist_name)
                .join("local-jars");
            std::fs::create_dir_all(&local_jars_dir).with_context(|| {
                format!("failed to create local-jars directory at {}", local_jars_dir.display())
            })?;
            let source_path = Path::new(file_name);
            if source_path.exists() {
                let dest = local_jars_dir.join(format!("{}.jar", input.mod_id));
                std::fs::copy(source_path, &dest).with_context(|| {
                    format!("failed to copy JAR to {}", dest.display())
                })?;
            }
        }
    }

    modlist.rules.push(Rule {
        mod_id: input.mod_id.clone(),
        source,
        exclude_if: vec![],
        requires: vec![],
        version_rules: vec![],
        custom_configs: vec![],
        alternatives: vec![],
    });

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn delete_rules_from_root(root_dir: &Path, input: &DeleteRulesInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    let ids_to_remove: std::collections::HashSet<&str> =
        input.mod_ids.iter().map(|s| s.as_str()).collect();

    // Remove from top-level
    modlist.rules.retain(|r| !ids_to_remove.contains(r.mod_id.as_str()));

    // Remove from alternatives recursively
    for rule in &mut modlist.rules {
        remove_from_alternatives(rule, &ids_to_remove);
    }

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

fn remove_from_alternatives(rule: &mut Rule, ids: &std::collections::HashSet<&str>) {
    rule.alternatives.retain(|alt| !ids.contains(alt.mod_id.as_str()));
    for alt in &mut rule.alternatives {
        remove_from_alternatives(alt, ids);
    }
}

pub fn rename_rule_from_root(root_dir: &Path, input: &RenameRuleInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    if input.new_mod_id.trim().is_empty() {
        bail!("new mod_id cannot be empty");
    }

    if input.mod_id != input.new_mod_id && modlist.contains_mod_id(&input.new_mod_id) {
        bail!(
            "a rule with mod_id '{}' already exists",
            input.new_mod_id
        );
    }

    // Update the rule's own mod_id
    let rule = modlist
        .find_rule_mut(&input.mod_id)
        .with_context(|| format!("rule '{}' not found", input.mod_id))?;
    rule.mod_id = input.new_mod_id.clone();

    // Update all references in exclude_if and requires across the whole tree
    let old_id = input.mod_id.clone();
    let new_id = input.new_mod_id.clone();
    for rule in &mut modlist.rules {
        rename_references_in_tree(rule, &old_id, &new_id);
    }

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

fn rename_references_in_tree(rule: &mut Rule, old_id: &str, new_id: &str) {
    for entry in &mut rule.exclude_if {
        if entry == old_id {
            *entry = new_id.to_string();
        }
    }
    for entry in &mut rule.requires {
        if entry == old_id {
            *entry = new_id.to_string();
        }
    }
    for alt in &mut rule.alternatives {
        rename_references_in_tree(alt, old_id, new_id);
    }
}

pub fn reorder_rules_from_root(root_dir: &Path, input: &ReorderRulesInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    let mut reordered = Vec::with_capacity(input.ordered_mod_ids.len());
    for mod_id in &input.ordered_mod_ids {
        let pos = modlist
            .rules
            .iter()
            .position(|r| r.mod_id == *mod_id)
            .with_context(|| format!("rule '{}' not found in top-level rules", mod_id))?;
        reordered.push(modlist.rules.remove(pos));
    }

    // Append any rules not mentioned in the order (shouldn't happen, but be safe)
    reordered.append(&mut modlist.rules);
    modlist.rules = reordered;

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn save_alternative_order_from_root(
    root_dir: &Path,
    input: &SaveAlternativeOrderInput,
) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    let parent = modlist
        .find_rule_mut(&input.parent_mod_id)
        .with_context(|| format!("parent rule '{}' not found", input.parent_mod_id))?;

    let mut reordered = Vec::with_capacity(input.ordered_alt_ids.len());
    for alt_id in &input.ordered_alt_ids {
        let pos = parent
            .alternatives
            .iter()
            .position(|a| a.mod_id == *alt_id)
            .with_context(|| {
                format!(
                    "alternative '{}' not found under parent '{}'",
                    alt_id, input.parent_mod_id
                )
            })?;
        reordered.push(parent.alternatives.remove(pos));
    }
    reordered.append(&mut parent.alternatives);
    parent.alternatives = reordered;

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn add_alternative_from_root(root_dir: &Path, input: &AddAlternativeInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    if modlist.contains_mod_id(&input.mod_id) {
        bail!("a rule with mod_id '{}' already exists", input.mod_id);
    }

    let source = parse_mod_source(&input.source)?;

    // Find top-level rule
    let parent = modlist
        .rules
        .iter_mut()
        .find(|r| r.mod_id == input.parent_mod_id)
        .with_context(|| {
            format!(
                "top-level rule '{}' not found",
                input.parent_mod_id
            )
        })?;

    parent.alternatives.push(Rule {
        mod_id: input.mod_id.clone(),
        source,
        exclude_if: vec![],
        requires: vec![],
        version_rules: vec![],
        custom_configs: vec![],
        alternatives: vec![],
    });

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn add_nested_alternative_from_root(
    root_dir: &Path,
    input: &AddNestedAlternativeInput,
) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    if modlist.contains_mod_id(&input.mod_id) {
        bail!("a rule with mod_id '{}' already exists", input.mod_id);
    }

    let source = parse_mod_source(&input.source)?;

    let parent = modlist
        .find_rule_mut(&input.parent_mod_id)
        .with_context(|| format!("rule '{}' not found", input.parent_mod_id))?;

    parent.alternatives.push(Rule {
        mod_id: input.mod_id.clone(),
        source,
        exclude_if: vec![],
        requires: vec![],
        version_rules: vec![],
        custom_configs: vec![],
        alternatives: vec![],
    });

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn remove_alternative_from_root(root_dir: &Path, input: &RemoveAlternativeInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    let parent = modlist
        .find_rule_mut(&input.parent_mod_id)
        .with_context(|| format!("rule '{}' not found", input.parent_mod_id))?;

    let initial_len = parent.alternatives.len();
    parent
        .alternatives
        .retain(|alt| alt.mod_id != input.alt_mod_id);

    if parent.alternatives.len() == initial_len {
        bail!(
            "alternative '{}' not found under rule '{}'",
            input.alt_mod_id,
            input.parent_mod_id
        );
    }

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn save_incompatibilities_from_root(
    root_dir: &Path,
    input: &SaveIncompatibilitiesInput,
) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    // Clear all exclude_if arrays across the entire tree
    for rule in &mut modlist.rules {
        clear_exclude_if_tree(rule);
    }

    // Rebuild exclude_if from the incompatibility list.
    // Incompatibility: winner_id's presence causes loser_id to be excluded.
    // So we add winner_id to loser_id's exclude_if.
    for incompat in &input.rules {
        if let Some(loser_rule) = modlist.find_rule_mut(&incompat.loser_id) {
            if !loser_rule.exclude_if.contains(&incompat.winner_id) {
                loser_rule.exclude_if.push(incompat.winner_id.clone());
            }
        }
    }

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

fn clear_exclude_if_tree(rule: &mut Rule) {
    rule.exclude_if.clear();
    for alt in &mut rule.alternatives {
        clear_exclude_if_tree(alt);
    }
}

pub fn save_rule_advanced_from_root(root_dir: &Path, input: &SaveRuleAdvancedInput) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    let rule = modlist
        .find_rule_mut(&input.mod_id)
        .with_context(|| format!("rule '{}' not found", input.mod_id))?;

    // Note: exclude_if is NOT set here — it is managed exclusively by save_incompatibilities_command.
    rule.requires = input.requires.clone();
    rule.version_rules = input
        .version_rules
        .iter()
        .map(|vr| VersionRule {
            kind: match vr.kind.as_str() {
                "only" => VersionRuleKind::Only,
                _ => VersionRuleKind::Exclude,
            },
            mc_versions: vr.mc_versions.clone(),
            loader: vr.loader.clone(),
        })
        .collect();
    rule.custom_configs = input
        .custom_configs
        .iter()
        .map(|cc| CustomConfig {
            mc_versions: cc.mc_versions.clone(),
            loader: cc.loader.clone(),
            target_path: cc.target_path.clone(),
            files: cc.files.clone(),
        })
        .collect();

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

pub fn save_advanced_batch_from_root(
    root_dir: &Path,
    input: &SaveAdvancedBatchInput,
) -> Result<()> {
    let mut modlist = load_modlist(root_dir, &input.modlist_name)?;

    // Clear all requires, version_rules, and custom_configs across the entire tree.
    fn clear_advanced(rule: &mut Rule) {
        rule.requires.clear();
        rule.version_rules.clear();
        rule.custom_configs.clear();
        for alt in &mut rule.alternatives {
            clear_advanced(alt);
        }
    }
    for rule in &mut modlist.rules {
        clear_advanced(rule);
    }

    // Set new requires
    for entry in &input.requires_entries {
        if let Some(rule) = modlist.find_rule_mut(&entry.mod_id) {
            rule.requires = entry.requires.clone();
        }
    }

    // Set new version rules
    for entry in &input.version_rules_entries {
        if let Some(rule) = modlist.find_rule_mut(&entry.mod_id) {
            rule.version_rules = entry
                .version_rules
                .iter()
                .map(|vr| VersionRule {
                    kind: match vr.kind.as_str() {
                        "only" => VersionRuleKind::Only,
                        _ => VersionRuleKind::Exclude,
                    },
                    mc_versions: vr.mc_versions.clone(),
                    loader: vr.loader.clone(),
                })
                .collect();
        }
    }

    // Set new custom configs
    for entry in &input.custom_configs_entries {
        if let Some(rule) = modlist.find_rule_mut(&entry.mod_id) {
            rule.custom_configs = entry
                .custom_configs
                .iter()
                .map(|cc| CustomConfig {
                    mc_versions: cc.mc_versions.clone(),
                    loader: cc.loader.clone(),
                    target_path: cc.target_path.clone(),
                    files: cc.files.clone(),
                })
                .collect();
        }
    }

    save_modlist(root_dir, &input.modlist_name, &modlist)
}

// ── Helpers ──────────────────────────────────────────────────────────────────

fn build_editor_row(rule: &Rule) -> EditorRow {
    EditorRow {
        mod_id: rule.mod_id.clone(),
        name: rule.mod_id.clone(),
        source: match &rule.source {
            ModSource::Modrinth => "modrinth".into(),
            ModSource::Local => "local".into(),
        },
        exclude_if: rule.exclude_if.clone(),
        requires: rule.requires.clone(),
        version_rules: rule
            .version_rules
            .iter()
            .map(|vr| EditorVersionRule {
                kind: match vr.kind {
                    VersionRuleKind::Exclude => "exclude".into(),
                    VersionRuleKind::Only => "only".into(),
                },
                mc_versions: vr.mc_versions.clone(),
                loader: vr.loader.clone(),
            })
            .collect(),
        custom_configs: rule
            .custom_configs
            .iter()
            .map(|cc| EditorCustomConfig {
                mc_versions: cc.mc_versions.clone(),
                loader: cc.loader.clone(),
                target_path: cc.target_path.clone(),
                files: cc.files.clone(),
            })
            .collect(),
        alternatives: rule.alternatives.iter().map(build_editor_row).collect(),
    }
}

fn derive_incompatibilities(rules: &[Rule]) -> Vec<IncompatibilityEntry> {
    let mut entries = Vec::new();
    for rule in rules {
        collect_incompatibilities_from_tree(rule, &mut entries);
    }
    entries
}

fn collect_incompatibilities_from_tree(rule: &Rule, entries: &mut Vec<IncompatibilityEntry>) {
    for excl_id in &rule.exclude_if {
        entries.push(IncompatibilityEntry {
            winner_id: excl_id.clone(),
            loser_id: rule.mod_id.clone(),
        });
    }
    for alt in &rule.alternatives {
        collect_incompatibilities_from_tree(alt, entries);
    }
}

fn parse_mod_source(s: &str) -> Result<ModSource> {
    match s {
        "modrinth" => Ok(ModSource::Modrinth),
        "local" => Ok(ModSource::Local),
        other => bail!("unknown mod source '{}'", other),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::rules::{ModList, ModSource, Rule};

    use super::*;

    fn unique_test_root() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("cubic-editor-test-{ts}"))
    }

    fn setup_modlist(root_dir: &Path, name: &str, rules: Vec<Rule>) {
        let modlist_dir = root_dir.join("mod-lists").join(name);
        fs::create_dir_all(&modlist_dir).unwrap();

        ModList {
            modlist_name: name.into(),
            author: "Author".into(),
            description: "".into(),
            rules,
        }
        .write_to_file(&modlist_dir.join("rules.json"))
        .unwrap();
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

    #[test]
    fn load_editor_snapshot_returns_rows_and_incompatibilities() {
        let root = unique_test_root();
        setup_modlist(
            &root,
            "Test Pack",
            vec![
                Rule {
                    mod_id: "sodium".into(),
                    source: ModSource::Modrinth,
                    exclude_if: vec![],
                    requires: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alternatives: vec![simple_rule("rubidium")],
                },
                Rule {
                    mod_id: "embeddium".into(),
                    source: ModSource::Modrinth,
                    exclude_if: vec!["sodium".into()],
                    requires: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alternatives: vec![],
                },
            ],
        );

        let snapshot = load_editor_snapshot_from_root(&root, "Test Pack").unwrap();

        assert_eq!(snapshot.modlist_name, "Test Pack");
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(snapshot.rows[0].mod_id, "sodium");
        assert_eq!(snapshot.rows[0].alternatives.len(), 1);
        assert_eq!(snapshot.rows[0].alternatives[0].mod_id, "rubidium");
        assert_eq!(snapshot.incompatibilities.len(), 1);
        assert_eq!(snapshot.incompatibilities[0].winner_id, "sodium");
        assert_eq!(snapshot.incompatibilities[0].loser_id, "embeddium");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn add_mod_rule_appends_rule() {
        let root = unique_test_root();
        setup_modlist(&root, "Pack", vec![simple_rule("sodium")]);

        add_mod_rule_from_root(
            &root,
            &AddModRuleInput {
                modlist_name: "Pack".into(),
                mod_id: "lithium".into(),
                source: "modrinth".into(),
                file_name: None,
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(snapshot.rows[1].mod_id, "lithium");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn add_mod_rule_rejects_duplicate() {
        let root = unique_test_root();
        setup_modlist(&root, "Pack", vec![simple_rule("sodium")]);

        let result = add_mod_rule_from_root(
            &root,
            &AddModRuleInput {
                modlist_name: "Pack".into(),
                mod_id: "sodium".into(),
                source: "modrinth".into(),
                file_name: None,
            },
        );
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn delete_rules_removes_from_tree() {
        let root = unique_test_root();
        setup_modlist(
            &root,
            "Pack",
            vec![
                Rule {
                    mod_id: "sodium".into(),
                    source: ModSource::Modrinth,
                    exclude_if: vec![],
                    requires: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alternatives: vec![simple_rule("rubidium")],
                },
                simple_rule("lithium"),
            ],
        );

        delete_rules_from_root(
            &root,
            &DeleteRulesInput {
                modlist_name: "Pack".into(),
                mod_ids: vec!["rubidium".into(), "lithium".into()],
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].mod_id, "sodium");
        assert!(snapshot.rows[0].alternatives.is_empty());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn rename_rule_updates_all_references() {
        let root = unique_test_root();
        setup_modlist(
            &root,
            "Pack",
            vec![
                simple_rule("sodium"),
                Rule {
                    mod_id: "embeddium".into(),
                    source: ModSource::Modrinth,
                    exclude_if: vec!["sodium".into()],
                    requires: vec![],
                    version_rules: vec![],
                    custom_configs: vec![],
                    alternatives: vec![],
                },
            ],
        );

        rename_rule_from_root(
            &root,
            &RenameRuleInput {
                modlist_name: "Pack".into(),
                mod_id: "sodium".into(),
                new_mod_id: "sodium-extra".into(),
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert_eq!(snapshot.rows[0].mod_id, "sodium-extra");
        assert_eq!(snapshot.rows[1].exclude_if, vec!["sodium-extra"]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn reorder_rules_changes_order() {
        let root = unique_test_root();
        setup_modlist(
            &root,
            "Pack",
            vec![simple_rule("a"), simple_rule("b"), simple_rule("c")],
        );

        reorder_rules_from_root(
            &root,
            &ReorderRulesInput {
                modlist_name: "Pack".into(),
                ordered_mod_ids: vec!["c".into(), "a".into(), "b".into()],
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        let ids: Vec<&str> = snapshot.rows.iter().map(|r| r.mod_id.as_str()).collect();
        assert_eq!(ids, vec!["c", "a", "b"]);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn add_and_remove_alternative() {
        let root = unique_test_root();
        setup_modlist(&root, "Pack", vec![simple_rule("sodium")]);

        add_alternative_from_root(
            &root,
            &AddAlternativeInput {
                modlist_name: "Pack".into(),
                parent_mod_id: "sodium".into(),
                mod_id: "rubidium".into(),
                source: "modrinth".into(),
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert_eq!(snapshot.rows[0].alternatives.len(), 1);

        remove_alternative_from_root(
            &root,
            &RemoveAlternativeInput {
                modlist_name: "Pack".into(),
                parent_mod_id: "sodium".into(),
                alt_mod_id: "rubidium".into(),
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert!(snapshot.rows[0].alternatives.is_empty());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn save_incompatibilities_rewrites_exclude_if() {
        let root = unique_test_root();
        setup_modlist(
            &root,
            "Pack",
            vec![simple_rule("sodium"), simple_rule("embeddium")],
        );

        save_incompatibilities_from_root(
            &root,
            &SaveIncompatibilitiesInput {
                modlist_name: "Pack".into(),
                rules: vec![IncompatibilityRuleInput {
                    winner_id: "sodium".into(),
                    loser_id: "embeddium".into(),
                }],
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert_eq!(snapshot.rows[1].exclude_if, vec!["sodium"]);
        assert_eq!(snapshot.incompatibilities.len(), 1);

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn save_rule_advanced_updates_fields() {
        let root = unique_test_root();
        setup_modlist(&root, "Pack", vec![simple_rule("sodium")]);

        save_rule_advanced_from_root(
            &root,
            &SaveRuleAdvancedInput {
                modlist_name: "Pack".into(),
                mod_id: "sodium".into(),
                exclude_if: vec!["optifine".into()],
                requires: vec!["fabric-api".into()],
                version_rules: vec![SaveVersionRuleInput {
                    kind: "exclude".into(),
                    mc_versions: vec!["1.18.2".into()],
                    loader: "forge".into(),
                }],
                custom_configs: vec![SaveCustomConfigInput {
                    mc_versions: vec!["1.21.1".into()],
                    loader: "fabric".into(),
                    target_path: "config/sodium.json".into(),
                    files: vec!["sodium.json".into()],
                }],
            },
        )
        .unwrap();

        let snapshot = load_editor_snapshot_from_root(&root, "Pack").unwrap();
        assert_eq!(snapshot.rows[0].exclude_if, vec!["optifine"]);
        assert_eq!(snapshot.rows[0].requires, vec!["fabric-api"]);
        assert_eq!(snapshot.rows[0].version_rules.len(), 1);
        assert_eq!(snapshot.rows[0].custom_configs.len(), 1);

        fs::remove_dir_all(&root).unwrap();
    }
}
