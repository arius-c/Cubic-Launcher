use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::debug_trace::append_debug_trace_to_root;
use crate::launcher_paths::LauncherPaths;
use crate::rules::{AltGroupMeta, ModList, ModReference, ModSource, Rule, RuleConfigFile, RuleCustomConfig, RuleGroupMeta, RuleVersionFilter, RULES_FILENAME};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorSnapshot {
    pub modlist_name: String,
    pub rows: Vec<EditorRow>,
    pub incompatibilities: Vec<EditorIncompatibilityRule>,
    /// Rule group definitions from `rules.json` (structural containers).
    pub groups: Vec<EditorGroupInfo>,
}

/// Group definition — structural container from `rules.json`.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorGroupInfo {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    /// Row IDs of the rules that belong to this group.
    pub block_ids: Vec<String>,
}

/// Visual group inside a rule's alternatives panel (stored in `Rule.alt_groups`).
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorAltGroupInfo {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    /// Row IDs of the alternative rows belonging to this visual group.
    pub block_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorRow {
    pub id: String,
    pub name: String,
    /// Primary Modrinth project slug/id for icon fetching — None for local mods.
    pub modrinth_id: Option<String>,
    /// The first mod's ID regardless of source (used for link target lookup on the frontend).
    #[serde(rename = "primaryModId")]
    pub primary_mod_id: Option<String>,
    pub kind: String,
    pub area: String,
    pub note: String,
    pub tags: Vec<String>,
    pub alternatives: Vec<EditorRow>,
    /// Primary mod IDs of linked rules (as stored in rules.json).
    pub links: Vec<String>,
    #[serde(rename = "customConfigs")]
    pub custom_configs: Vec<EditorCustomConfig>,
    #[serde(rename = "versionRules")]
    pub version_rules: Vec<EditorVersionRule>,
    /// Visual groups for this row's alternatives panel (stored in `Rule.alt_groups`).
    #[serde(rename = "altGroups")]
    pub alt_groups: Vec<EditorAltGroupInfo>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorVersionRule {
    pub id: String,
    pub kind: String,
    pub mc_versions: Vec<String>,
    pub loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorCustomConfig {
    pub id: String,
    pub files: Vec<EditorConfigFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorConfigFile {
    pub source_path: String,
    pub target_path: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorIncompatibilityRule {
    pub winner_id: String,
    pub loser_id: String,
}

// ── Input structs ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveAlternativeOrderInput {
    pub modlist_name: String,
    pub parent_row_id: String,
    pub ordered_alternative_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveIncompatibilitiesInput {
    pub modlist_name: String,
    pub rules: Vec<EditorIncompatibilityRuleInput>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EditorIncompatibilityRuleInput {
    pub winner_id: String,
    pub loser_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddModRuleInput {
    pub modlist_name: String,
    pub rule_name: String,
    pub mod_id: String,
    /// Either `"modrinth"` or `"local"`.
    pub mod_source: String,
    pub file_name: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeleteRulesInput {
    pub modlist_name: String,
    pub row_ids: Vec<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RenameRuleInput {
    pub modlist_name: String,
    pub row_id: String,
    pub new_name: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMetadataLinkInput {
    /// Row ID of the "from" rule.
    pub from_id: String,
    /// Row ID of the "to" rule.
    pub to_id: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMetadataConfigInput {
    /// Row ID of the rule owning this config.
    pub mod_id: String,
    pub id: String,
    pub files: Vec<RuleConfigFile>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RuleMetadataVersionRuleInput {
    /// Row ID of the rule this version filter belongs to.
    pub mod_id: String,
    pub id: String,
    pub kind: String,
    pub mc_versions: Vec<String>,
    pub loader: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRuleMetadataInput {
    pub modlist_name: String,
    pub links: Vec<RuleMetadataLinkInput>,
    pub custom_configs: Vec<RuleMetadataConfigInput>,
    pub version_rules: Vec<RuleMetadataVersionRuleInput>,
}

// ── Save rule groups ──────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRuleGroupItem {
    pub id: String,
    pub name: String,
    pub collapsed: bool,
    /// Row IDs of rules belonging to this group (e.g. "rule-0-sodium").
    pub row_ids: Vec<String>,
    /// `None` for top-level structural groups; `Some(parent_row_id)` for
    /// alternative-panel visual groups.
    pub scope_row_id: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRuleGroupsInput {
    pub modlist_name: String,
    pub groups: Vec<SaveRuleGroupItem>,
}

// ── Tauri commands ────────────────────────────────────────────────────────────

#[tauri::command]
pub fn load_modlist_editor_command(
    launcher_paths: State<'_, LauncherPaths>,
    selected_modlist_name: String,
) -> Result<EditorSnapshot, String> {
    load_editor_snapshot_from_root(launcher_paths.root_dir(), &selected_modlist_name)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_alternative_order_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveAlternativeOrderInput,
) -> Result<(), String> {
    save_alternative_order_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_incompatibilities_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveIncompatibilitiesInput,
) -> Result<(), String> {
    save_incompatibilities_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn add_mod_rule_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: AddModRuleInput,
) -> Result<(), String> {
    add_mod_rule_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn delete_rules_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: DeleteRulesInput,
) -> Result<(), String> {
    delete_rules_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn rename_rule_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: RenameRuleInput,
) -> Result<(), String> {
    rename_rule_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_rule_metadata_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveRuleMetadataInput,
) -> Result<(), String> {
    save_rule_metadata_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_rule_groups_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveRuleGroupsInput,
) -> Result<(), String> {
    save_rule_groups_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

#[tauri::command]
pub fn save_rule_links_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: SaveRuleLinksInput,
) -> Result<(), String> {
    save_rule_links_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

// ── Load snapshot ─────────────────────────────────────────────────────────────

pub fn load_editor_snapshot_from_root(
    root_dir: &Path,
    modlist_name: &str,
) -> Result<EditorSnapshot> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(modlist_name)
        .join(RULES_FILENAME);
    let modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load editor data for modlist '{}' from {}",
            modlist_name,
            rules_path.display()
        )
    })?;

    let rows: Vec<EditorRow> = modlist
        .rules
        .iter()
        .enumerate()
        .map(|(rule_index, rule)| build_editor_row(rule_index, rule))
        .collect();

    // Build rule_name → row_id map for group block_ids.
    let name_to_row_id: std::collections::HashMap<&str, &str> = modlist
        .rules
        .iter()
        .zip(rows.iter())
        .map(|(rule, row)| (rule.rule_name.as_str(), row.id.as_str()))
        .collect();

    let groups: Vec<EditorGroupInfo> = modlist
        .groups_meta
        .iter()
        .map(|gm| {
            let block_ids = gm
                .rule_names
                .iter()
                .filter_map(|name| name_to_row_id.get(name.as_str()).map(|id| id.to_string()))
                .collect();
            EditorGroupInfo {
                id: gm.id.clone(),
                name: gm.name.clone(),
                collapsed: gm.collapsed,
                block_ids,
            }
        })
        .collect();

    Ok(EditorSnapshot {
        modlist_name: modlist.modlist_name.clone(),
        rows,
        incompatibilities: derive_editor_incompatibilities(&modlist.rules),
        groups,
    })
}

// ── Save alternative order ────────────────────────────────────────────────────

pub fn save_alternative_order_from_root(
    root_dir: &Path,
    input: &SaveAlternativeOrderInput,
) -> Result<()> {
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.reorder.backend] start modlist={} parent_row_id={} ordered_ids={:?}",
            input.modlist_name, input.parent_row_id, input.ordered_alternative_ids
        ),
    );
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);
    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for alternative reordering from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    let option_path = parse_option_path_from_row_id(&input.parent_row_id)?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.reorder.backend] parsed option_path={:?}",
            option_path
        ),
    );
    let rule_index = option_path[0];
    let parent_path_str = option_path_suffix(&option_path[1..]);

    // Phase 1: collect current alternatives with their IDs (immutable).
    let is_top_level = option_path.len() == 1;
    let alternative_rules: Vec<(String, Rule)> = {
        let parent_rule = get_rule_ref(&modlist.rules, &option_path)
            .with_context(|| format!("could not find rule/alt for path {:?}", option_path))?;
        if parent_rule.alternatives.is_empty() {
            return Ok(());
        }
        parent_rule
            .alternatives
            .iter()
            .enumerate()
            .map(|(i, alt)| {
                let id = if is_top_level {
                    build_alternative_row(rule_index, i + 1, alt, "").id
                } else {
                    build_alternative_row(rule_index, i + 1, alt, &parent_path_str).id
                };
                (id, alt.clone())
            })
            .collect()
    };

    if input.ordered_alternative_ids.len() != alternative_rules.len() {
        anyhow::bail!(
            "alternative ordering size mismatch: expected {}, got {}",
            alternative_rules.len(),
            input.ordered_alternative_ids.len()
        );
    }

    let reordered = input
        .ordered_alternative_ids
        .iter()
        .map(|alt_id| {
            alternative_rules
                .iter()
                .find(|(candidate_id, _)| candidate_id == alt_id)
                .map(|(_, rule)| rule.clone())
                .with_context(|| format!("unknown alternative id '{}'", alt_id))
        })
        .collect::<Result<Vec<_>>>()?;

    // Phase 2: navigate (mutable) and assign.
    let parent_rule = navigate_to_rule_mut(&mut modlist.rules, &option_path)
        .with_context(|| format!("could not find rule/alt for path {:?}", option_path))?;
    parent_rule.alternatives = reordered;

    modlist.write_to_file(&rules_path)?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.reorder.backend] saved modlist={} parent_row_id={} is_top_level={}",
            input.modlist_name, input.parent_row_id, is_top_level
        ),
    );
    Ok(())
}

// ── Save incompatibilities ────────────────────────────────────────────────────

pub fn save_incompatibilities_from_root(
    root_dir: &Path,
    input: &SaveIncompatibilitiesInput,
) -> Result<()> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);
    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for incompatibility editing from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    // Build a comprehensive map of every row ID (at any depth) → its primary mod IDs.
    let row_id_to_mod_ids: std::collections::HashMap<String, Vec<String>> =
        collect_all_row_ids(&modlist.rules).into_iter().collect();

    // Accumulate per-loser exclusion mod IDs from the winner's primary mods.
    let mut exclusions_by_loser: std::collections::HashMap<String, Vec<String>> =
        std::collections::HashMap::new();
    for rule in &input.rules {
        if let Some(winner_mod_ids) = row_id_to_mod_ids.get(&rule.winner_id) {
            exclusions_by_loser
                .entry(rule.loser_id.clone())
                .or_default()
                .extend(winner_mod_ids.iter().cloned());
        }
    }

    // Clear all existing exclude_if_present at every depth before re-applying.
    for rule in modlist.rules.iter_mut() {
        clear_exclusions_recursive(rule);
    }

    // Apply exclusions to the correct rule at any depth using the path encoded
    // in the row ID (rule-N for top-level, rule-N-alternative-M-… for alternatives).
    for (loser_id, mut exclusions) in exclusions_by_loser {
        let path = match parse_option_path_from_row_id(&loser_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        if path[0] >= modlist.rules.len() {
            continue;
        }
        match navigate_to_rule_mut(&mut modlist.rules, &path) {
            Ok(rule) => {
                exclusions.sort();
                exclusions.dedup();
                rule.exclude_if_present = exclusions;
            }
            Err(_) => continue,
        }
    }

    modlist.write_to_file(&rules_path)
}

// ── Add mod rule ──────────────────────────────────────────────────────────────

pub fn add_mod_rule_from_root(root_dir: &Path, input: &AddModRuleInput) -> Result<()> {
    let rule_name = input.rule_name.trim().to_string();
    anyhow::ensure!(!rule_name.is_empty(), "rule_name cannot be empty");

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for adding a new rule from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    let source = match input.mod_source.to_lowercase().as_str() {
        "local" => ModSource::Local,
        _ => ModSource::Modrinth,
    };

    if source == ModSource::Local && input.file_name.is_none() {
        anyhow::bail!("local mod requires a file_name");
    }

    let mod_reference = ModReference {
        id: input.mod_id.clone(),
        source,
        file_name: input.file_name.clone(),
    };

    let new_rule = Rule {
        rule_name,
        mods: vec![mod_reference],
        exclude_if_present: vec![],
        alternatives: vec![],
        links: vec![],
        version_rules: vec![],
        custom_configs: vec![],
        alt_groups: vec![],
    };

    modlist.rules.push(new_rule);
    modlist.write_to_file(&rules_path)
}

// ── Delete rules ──────────────────────────────────────────────────────────────

pub fn delete_rules_from_root(root_dir: &Path, input: &DeleteRulesInput) -> Result<()> {
    if input.row_ids.is_empty() {
        return Ok(());
    }

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for rule deletion from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    // Separate the requested IDs into whole-rule deletions and alternative deletions.
    let mut rule_indices_to_remove: Vec<usize> = Vec::new();
    // Maps rule_index → set of alternative indices (1-based) to remove.
    let mut alternatives_to_remove: std::collections::HashMap<
        usize,
        std::collections::BTreeSet<usize>,
    > = std::collections::HashMap::new();

    for row_id in &input.row_ids {
        if let Some((rule_index, alt_index)) = parse_alternative_indices_from_row_id(row_id) {
            // This is an alternative (alt index ≥ 1 by definition).
            if rule_index < modlist.rules.len() && alt_index >= 1 {
                alternatives_to_remove
                    .entry(rule_index)
                    .or_default()
                    .insert(alt_index);
            }
        } else if let Ok(rule_index) = parse_rule_index_from_row_id(row_id) {
            if rule_index < modlist.rules.len() {
                rule_indices_to_remove.push(rule_index);
            }
        }
    }

    // Step 1: remove specific alternatives from their parent rules.
    for (rule_index, alt_set) in &alternatives_to_remove {
        // Skip if the whole rule is being removed anyway.
        if rule_indices_to_remove.contains(rule_index) {
            continue;
        }
        let rule = &mut modlist.rules[*rule_index];
        // Remove from the end so lower indices stay valid (convert 1-based to 0-based).
        for &alt_index in alt_set.iter().rev() {
            let alt_idx_0 = alt_index - 1;
            if alt_idx_0 < rule.alternatives.len() {
                rule.alternatives.remove(alt_idx_0);
            }
        }
        // Note: unlike old code, we do NOT delete the whole rule when alternatives
        // become empty — the rule's primary mods are still valid.
    }

    // Step 2: remove whole rules (from the end to preserve index validity).
    rule_indices_to_remove.sort_unstable();
    rule_indices_to_remove.dedup();

    // Collect names of rules being deleted so we can clean up groups_meta.
    let deleted_names: std::collections::HashSet<String> = rule_indices_to_remove
        .iter()
        .filter_map(|&i| modlist.rules.get(i).map(|r| r.rule_name.clone()))
        .collect();

    for index in rule_indices_to_remove.into_iter().rev() {
        modlist.rules.remove(index);
    }

    // Remove deleted rule names from groups_meta; drop empty groups.
    if !deleted_names.is_empty() {
        for gm in modlist.groups_meta.iter_mut() {
            gm.rule_names.retain(|name| !deleted_names.contains(name));
        }
        modlist.groups_meta.retain(|gm| !gm.rule_names.is_empty());
    }

    modlist.write_to_file(&rules_path)
}

// ── Rename rule ───────────────────────────────────────────────────────────────

pub fn rename_rule_from_root(root_dir: &Path, input: &RenameRuleInput) -> Result<()> {
    let new_name = input.new_name.trim().to_string();
    anyhow::ensure!(!new_name.is_empty(), "new rule name cannot be empty");

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for rule rename from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    let rule_index = parse_rule_index_from_row_id(&input.row_id)?;
    let old_name = modlist
        .rules
        .get(rule_index)
        .map(|r| r.rule_name.clone())
        .with_context(|| format!("rule index {} does not exist", rule_index))?;

    modlist.rules[rule_index].rule_name = new_name.clone();

    // Update groups_meta so the renamed rule stays in its group.
    for gm in modlist.groups_meta.iter_mut() {
        for rn in gm.rule_names.iter_mut() {
            if *rn == old_name {
                *rn = new_name.clone();
            }
        }
    }

    modlist.write_to_file(&rules_path)
}

// ── Save rule metadata (links + custom configs → rules.json) ──────────────────

pub fn save_rule_metadata_from_root(
    root_dir: &Path,
    input: &SaveRuleMetadataInput,
) -> Result<()> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for rule metadata save from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    // Build row_id → first primary mod ID map (used for link target resolution).
    let row_to_primary_mod: std::collections::HashMap<String, String> =
        collect_all_row_ids(&modlist.rules)
            .into_iter()
            .filter_map(|(row_id, mod_ids)| mod_ids.into_iter().next().map(|m| (row_id, m)))
            .collect();

    // Clear all existing metadata recursively.
    fn clear_metadata(rule: &mut Rule) {
        rule.links.clear();
        rule.custom_configs.clear();
        rule.version_rules.clear();
        for alt in rule.alternatives.iter_mut() {
            clear_metadata(alt);
        }
    }
    for rule in modlist.rules.iter_mut() {
        clear_metadata(rule);
    }

    // Apply links: store the primary mod ID of the "to" rule on the "from" rule.
    for link in &input.links {
        let from_path = match parse_option_path_from_row_id(&link.from_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let to_mod_id = match row_to_primary_mod.get(&link.to_id) {
            Some(id) => id.clone(),
            None => continue,
        };
        match navigate_to_rule_mut(&mut modlist.rules, &from_path) {
            Ok(from_rule) => {
                from_rule.links.push(to_mod_id);
            }
            Err(_) => continue,
        }
    }

    // Apply custom configs: each config is attached to the rule identified by mod_id (row ID).
    for config in &input.custom_configs {
        let path = match parse_option_path_from_row_id(&config.mod_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        match navigate_to_rule_mut(&mut modlist.rules, &path) {
            Ok(rule) => {
                rule.custom_configs.push(RuleCustomConfig {
                    id: config.id.clone(),
                    files: config.files.clone(),
                });
            }
            Err(_) => continue,
        }
    }

    // Apply version rules: each filter is attached to the rule identified by mod_id (row ID).
    for vr in &input.version_rules {
        let path = match parse_option_path_from_row_id(&vr.mod_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        match navigate_to_rule_mut(&mut modlist.rules, &path) {
            Ok(rule) => {
                rule.version_rules.push(RuleVersionFilter {
                    id: vr.id.clone(),
                    kind: vr.kind.clone(),
                    mc_versions: vr.mc_versions.clone(),
                    loader: vr.loader.clone(),
                });
            }
            Err(_) => continue,
        }
    }

    modlist.write_to_file(&rules_path)
}

// ── Save rule groups ──────────────────────────────────────────────────────────

pub fn save_rule_groups_from_root(root_dir: &Path, input: &SaveRuleGroupsInput) -> Result<()> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for rule groups save from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    // Separate top-level groups (scope_row_id == None) from alt-scoped groups.
    let top_level: Vec<&SaveRuleGroupItem> =
        input.groups.iter().filter(|g| g.scope_row_id.is_none()).collect();
    let alt_scoped: Vec<&SaveRuleGroupItem> =
        input.groups.iter().filter(|g| g.scope_row_id.is_some()).collect();

    // ── Top-level structural groups ───────────────────────────────────────────
    // Build row_id → rule_name map for top-level rules.
    let id_to_name: std::collections::HashMap<String, String> = modlist
        .rules
        .iter()
        .enumerate()
        .map(|(i, r)| (build_editor_row(i, r).id, r.rule_name.clone()))
        .collect();

    modlist.groups_meta = top_level
        .iter()
        .map(|g| RuleGroupMeta {
            id: g.id.clone(),
            name: g.name.clone(),
            collapsed: g.collapsed,
            rule_names: g
                .row_ids
                .iter()
                .filter_map(|rid| id_to_name.get(rid))
                .cloned()
                .collect(),
        })
        .collect();

    // ── Alt-scoped visual groups ──────────────────────────────────────────────
    // Clear all existing alt_groups on all rules before re-applying.
    fn clear_alt_groups(rule: &mut Rule) {
        rule.alt_groups.clear();
        for alt in rule.alternatives.iter_mut() {
            clear_alt_groups(alt);
        }
    }
    for rule in modlist.rules.iter_mut() {
        clear_alt_groups(rule);
    }

    // Group by scope_row_id, then update each parent rule's alt_groups.
    let mut groups_by_scope: std::collections::HashMap<&str, Vec<&SaveRuleGroupItem>> =
        std::collections::HashMap::new();
    for g in &alt_scoped {
        if let Some(ref sid) = g.scope_row_id {
            groups_by_scope.entry(sid.as_str()).or_default().push(g);
        }
    }

    for (scope_row_id, groups) in groups_by_scope {
        let path = match parse_option_path_from_row_id(scope_row_id) {
            Ok(p) => p,
            Err(_) => continue,
        };

        // Build alt name → row_id map for the scoped rule's direct alternatives.
        let alt_id_to_name: std::collections::HashMap<String, String> = {
            let rule_idx = path[0];
            let parent_path_str = if path.len() == 1 {
                String::new()
            } else {
                option_path_suffix(&path[1..])
            };
            match get_rule_ref(&modlist.rules, &path) {
                None => continue,
                Some(scoped_rule) => scoped_rule
                    .alternatives
                    .iter()
                    .enumerate()
                    .map(|(i, alt)| {
                        let alt_id =
                            build_alternative_row(rule_idx, i + 1, alt, &parent_path_str).id;
                        (alt_id, alt.rule_name.clone())
                    })
                    .collect(),
            }
        };

        let new_alt_groups: Vec<AltGroupMeta> = groups
            .iter()
            .map(|g| AltGroupMeta {
                id: g.id.clone(),
                name: g.name.clone(),
                collapsed: g.collapsed,
                block_names: g
                    .row_ids
                    .iter()
                    .filter_map(|rid| alt_id_to_name.get(rid))
                    .cloned()
                    .collect(),
            })
            .collect();

        match navigate_to_rule_mut(&mut modlist.rules, &path) {
            Ok(rule) => rule.alt_groups = new_alt_groups,
            Err(_) => continue,
        }
    }

    modlist.write_to_file(&rules_path)
}

// ── Save rule links ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveRuleLinksInput {
    pub modlist_name: String,
    pub links: Vec<RuleMetadataLinkInput>,
}

pub fn save_rule_links_from_root(root_dir: &Path, input: &SaveRuleLinksInput) -> Result<()> {
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for rule links save from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    // Build row_id → first primary mod ID map (needed for link target resolution).
    let row_to_primary_mod: std::collections::HashMap<String, String> =
        collect_all_row_ids(&modlist.rules)
            .into_iter()
            .filter_map(|(row_id, mod_ids)| mod_ids.into_iter().next().map(|m| (row_id, m)))
            .collect();

    // Clear all links recursively.
    fn clear_links(rule: &mut Rule) {
        rule.links.clear();
        for alt in rule.alternatives.iter_mut() {
            clear_links(alt);
        }
    }
    for rule in modlist.rules.iter_mut() {
        clear_links(rule);
    }

    // Re-apply links from input.
    for link in &input.links {
        let from_path = match parse_option_path_from_row_id(&link.from_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let to_mod_id = match row_to_primary_mod.get(&link.to_id) {
            Some(id) => id.clone(),
            None => continue,
        };
        match navigate_to_rule_mut(&mut modlist.rules, &from_path) {
            Ok(from_rule) => from_rule.links.push(to_mod_id),
            Err(_) => continue,
        }
    }

    modlist.write_to_file(&rules_path)
}

// ── Add alternative ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAlternativeInput {
    pub modlist_name: String,
    /// The row that will receive a new fallback alternative.
    pub parent_row_id: String,
    /// The standalone rule being converted into an alternative of the parent.
    pub alternative_row_id: String,
}

#[tauri::command]
pub fn add_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: AddAlternativeInput,
) -> Result<(), String> {
    add_alternative_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

pub fn add_alternative_from_root(root_dir: &Path, input: &AddAlternativeInput) -> Result<()> {
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.add.backend] start modlist={} parent_row_id={} alternative_row_id={}",
            input.modlist_name, input.parent_row_id, input.alternative_row_id
        ),
    );
    let parent_idx = parse_rule_index_from_row_id(&input.parent_row_id)
        .with_context(|| format!("invalid parent row id '{}'", input.parent_row_id))?;
    let alt_idx = parse_rule_index_from_row_id(&input.alternative_row_id)
        .with_context(|| format!("invalid alternative row id '{}'", input.alternative_row_id))?;

    anyhow::ensure!(
        parent_idx != alt_idx,
        "a rule cannot be made an alternative of itself"
    );

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' to add an alternative",
            input.modlist_name
        )
    })?;

    anyhow::ensure!(
        parent_idx < modlist.rules.len(),
        "parent rule index out of bounds"
    );
    anyhow::ensure!(
        alt_idx < modlist.rules.len(),
        "alternative rule index out of bounds"
    );

    // Clone the alt rule and push it as a new alternative of the parent.
    // Its own alternatives and metadata are preserved as-is.
    let alt_rule = modlist.rules[alt_idx].clone();
    let alt_name = alt_rule.rule_name.clone();
    modlist.rules[parent_idx].alternatives.push(alt_rule);

    // Remove the now-merged standalone rule.
    modlist.rules.remove(alt_idx);

    // Clean up groups_meta: remove the merged rule from any group.
    for gm in modlist.groups_meta.iter_mut() {
        gm.rule_names.retain(|name| *name != alt_name);
    }
    modlist.groups_meta.retain(|gm| !gm.rule_names.is_empty());

    modlist.write_to_file(&rules_path)?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.add.backend] saved modlist={} parent_idx={} alt_idx={}",
            input.modlist_name, parent_idx, alt_idx
        ),
    );
    Ok(())
}

// ── Add nested alternative (alternative-of-alternative) ───────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddNestedAlternativeInput {
    pub modlist_name: String,
    /// The alternative row that should receive a new sub-alternative.
    /// Format: "rule-N-alternative-M-…" or deeper.
    pub parent_alt_row_id: String,
    /// The standalone top-level rule being converted into a sub-alternative.
    pub alternative_row_id: String,
}

#[tauri::command]
pub fn add_nested_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: AddNestedAlternativeInput,
) -> Result<(), String> {
    add_nested_alternative_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

pub fn add_nested_alternative_from_root(
    root_dir: &Path,
    input: &AddNestedAlternativeInput,
) -> Result<()> {
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.add_nested.backend] start modlist={} parent_alt_row_id={} alternative_row_id={}",
            input.modlist_name, input.parent_alt_row_id, input.alternative_row_id
        ),
    );
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path)?;

    // Parse the parent alternative path: [rule_idx, alt_idx, …]
    let parent_path =
        parse_option_path_from_row_id(&input.parent_alt_row_id).with_context(|| {
            format!(
                "invalid parent alternative row id '{}'",
                input.parent_alt_row_id
            )
        })?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.add_nested.backend] parsed parent_path={:?}",
            parent_path
        ),
    );
    anyhow::ensure!(
        parent_path.len() >= 2,
        "parent_alt_row_id must refer to an alternative, not a top-level rule"
    );

    // Parse the source rule index.
    let src_rule_idx =
        parse_rule_index_from_row_id(&input.alternative_row_id).with_context(|| {
            format!(
                "invalid source alternative row id '{}'",
                input.alternative_row_id
            )
        })?;

    anyhow::ensure!(
        parent_path[0] != src_rule_idx,
        "cannot add a sub-alternative from the same rule"
    );
    anyhow::ensure!(
        src_rule_idx < modlist.rules.len(),
        "source rule index out of bounds"
    );

    // Clone the src rule before mutably borrowing modlist.rules.
    let src_rule = modlist.rules[src_rule_idx].clone();
    let src_name = src_rule.rule_name.clone();

    // Navigate to the parent alternative and push the new sub-alternative.
    let parent_rule = navigate_to_rule_mut(&mut modlist.rules, &parent_path).with_context(|| {
        format!(
            "could not find parent alternative for path {:?}",
            parent_path
        )
    })?;
    parent_rule.alternatives.push(src_rule);

    // Remove the now-nested source rule.
    modlist.rules.remove(src_rule_idx);

    // Clean up groups_meta: remove the nested rule from any group.
    for gm in modlist.groups_meta.iter_mut() {
        gm.rule_names.retain(|name| *name != src_name);
    }
    modlist.groups_meta.retain(|gm| !gm.rule_names.is_empty());

    modlist.write_to_file(&rules_path)?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.add_nested.backend] saved modlist={} parent_path={:?} src_rule_idx={}",
            input.modlist_name, parent_path, src_rule_idx
        ),
    );
    Ok(())
}

// ── Move alternative to another parent ───────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct MoveAltToAlternativeInput {
    pub modlist_name: String,
    /// The alternative row being moved (must be non-top-level: contains "-alternative-").
    pub source_row_id: String,
    /// The row that will become the new parent of the source (any depth).
    pub target_parent_row_id: String,
}

#[tauri::command]
pub fn move_alt_to_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: MoveAltToAlternativeInput,
) -> Result<(), String> {
    move_alt_to_alternative_from_root(launcher_paths.root_dir(), &input)
        .map_err(|e| e.to_string())
}

pub fn move_alt_to_alternative_from_root(
    root_dir: &Path,
    input: &MoveAltToAlternativeInput,
) -> Result<()> {
    let source_path = parse_option_path_from_row_id(&input.source_row_id)
        .with_context(|| format!("invalid source_row_id '{}'", input.source_row_id))?;
    let target_path = parse_option_path_from_row_id(&input.target_parent_row_id)
        .with_context(|| format!("invalid target_parent_row_id '{}'", input.target_parent_row_id))?;

    anyhow::ensure!(
        source_path.len() >= 2,
        "source_row_id must refer to an alternative (non-top-level), got '{}'",
        input.source_row_id
    );
    anyhow::ensure!(
        source_path[0] != target_path[0],
        "source and target parent must belong to different top-level rule chains"
    );

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!("failed to load modlist '{}' for move_alt_to_alternative", input.modlist_name)
    })?;

    // Clone the source rule before mutably modifying the modlist.
    let source_rule = {
        let src = navigate_to_rule_mut(&mut modlist.rules, &source_path)
            .with_context(|| format!("failed to navigate to source {:?}", source_path))?;
        src.clone()
    };

    // Remove source from its current parent's alternatives.
    {
        let parent_path = &source_path[..source_path.len() - 1];
        // path indices after position 0 are 1-based; convert last index to 0-based.
        let alt_idx_1based = source_path[source_path.len() - 1];
        let alt_idx = alt_idx_1based
            .checked_sub(1)
            .context("alternative index must be >= 1")?;
        let parent = navigate_to_rule_mut(&mut modlist.rules, parent_path)
            .with_context(|| format!("failed to navigate to source parent {:?}", parent_path))?;
        anyhow::ensure!(
            alt_idx < parent.alternatives.len(),
            "source alt index {} out of bounds (len={})",
            alt_idx,
            parent.alternatives.len()
        );
        parent.alternatives.remove(alt_idx);
    }

    // Append source to the target parent's alternatives.
    // target_path is in a different top-level rule so indices are unaffected by the removal above.
    {
        let target = navigate_to_rule_mut(&mut modlist.rules, &target_path)
            .with_context(|| format!("failed to navigate to target parent {:?}", target_path))?;
        target.alternatives.push(source_rule);
    }

    modlist.write_to_file(&rules_path)
}

// ── Remove alternative ────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RemoveAlternativeInput {
    pub modlist_name: String,
    /// The alternative row ID to detach and restore as a top-level rule.
    pub alternative_row_id: String,
}

#[tauri::command]
pub fn remove_alternative_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: RemoveAlternativeInput,
) -> Result<(), String> {
    remove_alternative_from_root(launcher_paths.root_dir(), &input)
        .map_err(|error| error.to_string())
}

pub fn remove_alternative_from_root(root_dir: &Path, input: &RemoveAlternativeInput) -> Result<()> {
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.remove.backend] start modlist={} alternative_row_id={}",
            input.modlist_name, input.alternative_row_id
        ),
    );
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path)?;

    let path = parse_option_path_from_row_id(&input.alternative_row_id)
        .with_context(|| format!("invalid alternative row id '{}'", input.alternative_row_id))?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!("[alts.remove.backend] parsed path={:?}", path),
    );
    anyhow::ensure!(
        path.len() >= 2,
        "alternative_row_id must refer to an alternative, not a top-level rule"
    );

    let rule_idx = path[0];
    anyhow::ensure!(rule_idx < modlist.rules.len(), "rule index out of bounds");

    let opt_idx = *path.last().unwrap();
    let alt_idx_0 = opt_idx
        .checked_sub(1)
        .with_context(|| "alternative index 0 is invalid")?;

    let detached_rule = if path.len() == 2 {
        // Direct child of the top-level rule.
        anyhow::ensure!(
            alt_idx_0 < modlist.rules[rule_idx].alternatives.len(),
            "alternative index out of bounds"
        );
        modlist.rules[rule_idx].alternatives.remove(alt_idx_0)
    } else {
        // Nested alternative — remove from parent alternative's list.
        let parent_path = &path[..path.len() - 1];
        let parent_rule = navigate_to_rule_mut(&mut modlist.rules, parent_path)
            .with_context(|| "could not find parent alternative for detachment")?;
        anyhow::ensure!(
            alt_idx_0 < parent_rule.alternatives.len(),
            "nested alternative index out of bounds"
        );
        parent_rule.alternatives.remove(alt_idx_0)
    };

    // Re-insert the detached rule as a new top-level rule.
    modlist.rules.push(detached_rule);

    modlist.write_to_file(&rules_path)?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.remove.backend] saved modlist={} alternative_row_id={} detached_path={:?}",
            input.modlist_name, input.alternative_row_id, path
        ),
    );
    Ok(())
}

// ── Reorder rules ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRulesInput {
    pub modlist_name: String,
    /// Parent-row IDs in the desired new order.
    pub ordered_row_ids: Vec<String>,
}

#[tauri::command]
pub fn reorder_rules_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: ReorderRulesInput,
) -> Result<(), String> {
    reorder_rules_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

pub fn reorder_rules_from_root(root_dir: &Path, input: &ReorderRulesInput) -> Result<()> {
    if input.ordered_row_ids.is_empty() {
        return Ok(());
    }

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);

    let mut modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for rule reordering from {}",
            input.modlist_name,
            rules_path.display()
        )
    })?;

    // Build a pre-reorder (rule_id → rule) lookup using current indices.
    let id_to_rule: Vec<(String, Rule)> = modlist
        .rules
        .iter()
        .enumerate()
        .map(|(i, rule)| (build_editor_row(i, rule).id, rule.clone()))
        .collect();

    // Assemble rules in the requested order.
    let reordered: Vec<Rule> = input
        .ordered_row_ids
        .iter()
        .filter_map(|id| {
            id_to_rule
                .iter()
                .find(|(row_id, _)| row_id == id)
                .map(|(_, rule)| rule.clone())
        })
        .collect();

    anyhow::ensure!(
        reordered.len() == modlist.rules.len(),
        "reorder size mismatch: expected {}, got {} (unknown row id in ordered list)",
        modlist.rules.len(),
        reordered.len(),
    );

    modlist.rules = reordered;
    modlist.write_to_file(&rules_path)
}

// ── Row-building helpers ──────────────────────────────────────────────────────

fn build_editor_row(rule_index: usize, rule: &Rule) -> EditorRow {
    // Build alt rule_name → row_id map for converting alt_groups block_names.
    let alt_name_to_id: std::collections::HashMap<&str, String> = rule
        .alternatives
        .iter()
        .enumerate()
        .map(|(i, alt)| (alt.rule_name.as_str(), build_alternative_row(rule_index, i + 1, alt, "").id))
        .collect();
    let alt_groups = build_editor_alt_groups(&rule.alt_groups, &alt_name_to_id);

    EditorRow {
        id: format!(
            "rule-{}-{}",
            rule_index,
            normalize_identifier(&rule.rule_name)
        ),
        name: rule.rule_name.clone(),
        modrinth_id: rule
            .mods
            .iter()
            .find(|m| m.source == ModSource::Modrinth)
            .map(|m| m.id.clone()),
        primary_mod_id: rule.mods.first().map(|m| m.id.clone()),
        kind: mods_kind(&rule.mods),
        area: "Rule".to_string(),
        note: build_note(&rule.mods, &rule.exclude_if_present, false),
        tags: build_tags(&rule.mods, &rule.exclude_if_present, false),
        alternatives: rule
            .alternatives
            .iter()
            .enumerate()
            .map(|(i, alt)| build_alternative_row(rule_index, i + 1, alt, ""))
            .collect(),
        links: rule.links.clone(),
        custom_configs: rule.custom_configs.iter().map(to_editor_custom_config).collect(),
        version_rules: rule.version_rules.iter().map(to_editor_version_rule).collect(),
        alt_groups,
    }
}

fn build_alternative_row(
    rule_index: usize,
    alt_index: usize,
    alt: &Rule,
    parent_path: &str,
) -> EditorRow {
    let id = format!(
        "rule-{}{}-alternative-{}-{}",
        rule_index,
        parent_path,
        alt_index,
        normalize_identifier(&alt.rule_name)
    );
    let child_path = format!("{parent_path}-alternative-{alt_index}");

    // Build sub-alt name → row_id map for converting alt_groups block_names.
    let sub_alt_name_to_id: std::collections::HashMap<&str, String> = alt
        .alternatives
        .iter()
        .enumerate()
        .map(|(i, sub)| (sub.rule_name.as_str(), build_alternative_row(rule_index, i + 1, sub, &child_path).id))
        .collect();
    let alt_groups = build_editor_alt_groups(&alt.alt_groups, &sub_alt_name_to_id);

    EditorRow {
        id,
        name: alt.rule_name.clone(),
        modrinth_id: alt
            .mods
            .iter()
            .find(|m| m.source == ModSource::Modrinth)
            .map(|m| m.id.clone()),
        primary_mod_id: alt.mods.first().map(|m| m.id.clone()),
        kind: mods_kind(&alt.mods),
        area: "Rule".to_string(),
        note: build_note(&alt.mods, &alt.exclude_if_present, true),
        tags: build_tags(&alt.mods, &alt.exclude_if_present, true),
        alternatives: alt
            .alternatives
            .iter()
            .enumerate()
            .map(|(i, sub)| build_alternative_row(rule_index, i + 1, sub, &child_path))
            .collect(),
        links: alt.links.clone(),
        custom_configs: alt.custom_configs.iter().map(to_editor_custom_config).collect(),
        version_rules: alt.version_rules.iter().map(to_editor_version_rule).collect(),
        alt_groups,
    }
}

fn build_editor_alt_groups(
    alt_groups: &[AltGroupMeta],
    name_to_id: &std::collections::HashMap<&str, String>,
) -> Vec<EditorAltGroupInfo> {
    alt_groups
        .iter()
        .map(|ag| EditorAltGroupInfo {
            id: ag.id.clone(),
            name: ag.name.clone(),
            collapsed: ag.collapsed,
            block_ids: ag
                .block_names
                .iter()
                .filter_map(|name| name_to_id.get(name.as_str()).cloned())
                .collect(),
        })
        .collect()
}

fn to_editor_version_rule(vr: &RuleVersionFilter) -> EditorVersionRule {
    EditorVersionRule {
        id: vr.id.clone(),
        kind: vr.kind.clone(),
        mc_versions: vr.mc_versions.clone(),
        loader: vr.loader.clone(),
    }
}

fn to_editor_custom_config(c: &RuleCustomConfig) -> EditorCustomConfig {
    EditorCustomConfig {
        id: c.id.clone(),
        files: c.files.iter().map(|f| EditorConfigFile {
            source_path: f.source_path.clone(),
            target_path: f.target_path.clone(),
        }).collect(),
    }
}

fn mods_kind(mods: &[ModReference]) -> String {
    if mods.iter().any(|m| m.source == ModSource::Local) {
        "local".to_string()
    } else {
        "modrinth".to_string()
    }
}

fn build_note(mods: &[ModReference], exclude_if_present: &[String], is_alternative: bool) -> String {
    let mod_count = mods.len();
    let local_count = mods.iter().filter(|m| m.source == ModSource::Local).count();
    let exclude_count = exclude_if_present.len();

    let kind = if is_alternative { "Fallback option" } else { "Primary option" };

    format!(
        "{} with {} mod{}{}{}.",
        kind,
        mod_count,
        if mod_count == 1 { "" } else { "s" },
        if local_count > 0 {
            format!(
                ", including {} local JAR{}",
                local_count,
                if local_count == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        },
        if exclude_count > 0 {
            format!(
                ", excluded by {} higher-priority mod{}",
                exclude_count,
                if exclude_count == 1 { "" } else { "s" }
            )
        } else {
            String::new()
        }
    )
}

fn build_tags(mods: &[ModReference], exclude_if_present: &[String], is_alternative: bool) -> Vec<String> {
    let mut tags = Vec::new();
    if mods.len() > 1 {
        tags.push(format!("{} Mods", mods.len()));
    }
    if mods.iter().any(|m| m.source == ModSource::Local) {
        tags.push("Manual".to_string());
    }
    if !exclude_if_present.is_empty() {
        tags.push("Conflict Set".to_string());
    }
    if is_alternative {
        tags.push("Alternative".to_string());
    }
    tags
}

// ── Incompatibility helpers ───────────────────────────────────────────────────

fn rule_mod_ids(rule: &Rule) -> Vec<String> {
    rule.mods.iter().map(|m| m.id.clone()).collect()
}

fn collect_all_row_ids(rules: &[Rule]) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    for (rule_idx, rule) in rules.iter().enumerate() {
        result.push((build_editor_row(rule_idx, rule).id, rule_mod_ids(rule)));
        for (alt_offset, alt) in rule.alternatives.iter().enumerate() {
            collect_alternative_row_ids_into(rule_idx, alt_offset + 1, alt, "", &mut result);
        }
    }
    result
}

fn collect_alternative_row_ids_into(
    rule_idx: usize,
    alt_idx: usize,
    alt: &Rule,
    parent_path: &str,
    result: &mut Vec<(String, Vec<String>)>,
) {
    let row = build_alternative_row(rule_idx, alt_idx, alt, parent_path);
    result.push((row.id, rule_mod_ids(alt)));
    let child_path = format!("{parent_path}-alternative-{alt_idx}");
    for (sub_idx, sub_alt) in alt.alternatives.iter().enumerate() {
        collect_alternative_row_ids_into(rule_idx, sub_idx + 1, sub_alt, &child_path, result);
    }
}

fn push_incompatibilities_for_rule(
    rule: &Rule,
    loser_row_id: &str,
    row_lookup: &[(String, Vec<String>)],
    incompatibilities: &mut Vec<EditorIncompatibilityRule>,
) {
    for excluded_mod_id in &rule.exclude_if_present {
        if let Some((winner_row_id, _)) = row_lookup
            .iter()
            .find(|(_, mods)| mods.contains(excluded_mod_id))
        {
            // Skip within-chain exclusions (winner and loser belong to the same rule).
            if rule_idx_from_row_id(winner_row_id) == rule_idx_from_row_id(loser_row_id) {
                continue;
            }
            let candidate = EditorIncompatibilityRule {
                winner_id: winner_row_id.clone(),
                loser_id: loser_row_id.to_string(),
            };
            if !incompatibilities.contains(&candidate) {
                incompatibilities.push(candidate);
            }
        }
    }
}

fn collect_alternative_incompatibilities(
    rule_idx: usize,
    alt_idx: usize,
    alt: &Rule,
    parent_path: &str,
    row_lookup: &[(String, Vec<String>)],
    incompatibilities: &mut Vec<EditorIncompatibilityRule>,
) {
    let row = build_alternative_row(rule_idx, alt_idx, alt, parent_path);
    push_incompatibilities_for_rule(alt, &row.id, row_lookup, incompatibilities);
    let child_path = format!("{parent_path}-alternative-{alt_idx}");
    for (sub_idx, sub_alt) in alt.alternatives.iter().enumerate() {
        collect_alternative_incompatibilities(
            rule_idx,
            sub_idx + 1,
            sub_alt,
            &child_path,
            row_lookup,
            incompatibilities,
        );
    }
}

fn clear_exclusions_recursive(rule: &mut Rule) {
    rule.exclude_if_present.clear();
    for alt in rule.alternatives.iter_mut() {
        clear_exclusions_recursive(alt);
    }
}

fn derive_editor_incompatibilities(rules: &[Rule]) -> Vec<EditorIncompatibilityRule> {
    let row_lookup = collect_all_row_ids(rules);
    let mut incompatibilities = Vec::new();

    for (rule_index, rule) in rules.iter().enumerate() {
        let loser_row_id = build_editor_row(rule_index, rule).id;
        push_incompatibilities_for_rule(rule, &loser_row_id, &row_lookup, &mut incompatibilities);
        for (alt_offset, alt) in rule.alternatives.iter().enumerate() {
            collect_alternative_incompatibilities(
                rule_index,
                alt_offset + 1,
                alt,
                "",
                &row_lookup,
                &mut incompatibilities,
            );
        }
    }

    incompatibilities
}

// ── Row ID parsing ────────────────────────────────────────────────────────────

fn parse_rule_index_from_row_id(row_id: &str) -> Result<usize> {
    let mut parts = row_id.split('-');
    let prefix = parts.next().unwrap_or_default();
    let rule_index = parts.next().unwrap_or_default();

    if prefix != "rule" {
        anyhow::bail!("row id '{}' does not start with a rule prefix", row_id);
    }

    rule_index
        .parse::<usize>()
        .with_context(|| format!("row id '{}' does not contain a valid rule index", row_id))
}

/// Returns `Some((rule_index, alt_index))` if the row_id belongs to an
/// alternative (i.e. the format is `"rule-N-alternative-M-…"`), or `None`
/// if it is a top-level rule id.
fn parse_alternative_indices_from_row_id(row_id: &str) -> Option<(usize, usize)> {
    let parts: Vec<&str> = row_id.splitn(5, '-').collect();
    // Expected: ["rule", rule_index, "alternative", alt_index, ...]
    if parts.len() < 4 || parts[0] != "rule" || parts[2] != "alternative" {
        return None;
    }
    let rule_index = parts[1].parse::<usize>().ok()?;
    let alt_index = parts[3].parse::<usize>().ok()?;
    Some((rule_index, alt_index))
}

/// Parse a row ID into a path of indices: [rule_idx, alt_idx1, alt_idx2, …].
/// For top-level rules returns a single-element vec.
/// For "rule-N-alternative-M-…" returns [N, M].
/// For "rule-N-alternative-M-alternative-K-…" returns [N, M, K], etc.
fn parse_option_path_from_row_id(row_id: &str) -> Result<Vec<usize>> {
    let after_rule = row_id
        .strip_prefix("rule-")
        .ok_or_else(|| anyhow::anyhow!("row id '{}' does not start with 'rule-'", row_id))?;

    let mut path = Vec::new();
    let mut remaining = after_rule;

    // First segment is always the rule index.
    let (rule_idx_str, rest) = remaining
        .split_once('-')
        .ok_or_else(|| anyhow::anyhow!("row id '{}' has no content after rule prefix", row_id))?;
    let rule_idx = rule_idx_str
        .parse::<usize>()
        .with_context(|| format!("invalid rule index in row id '{}'", row_id))?;
    path.push(rule_idx);
    remaining = rest;

    // Scan for "-alternative-{N}-" patterns.
    const ALT_MARKER: &str = "alternative-";
    loop {
        if let Some(alt_rest) = remaining.strip_prefix(ALT_MARKER) {
            let (idx_str, after_idx) = alt_rest
                .split_once('-')
                .ok_or_else(|| anyhow::anyhow!("malformed alternative segment in '{}'", row_id))?;
            let opt_idx = idx_str
                .parse::<usize>()
                .with_context(|| format!("invalid option index in row id '{}'", row_id))?;
            path.push(opt_idx);
            remaining = after_idx;
        } else {
            break;
        }
    }

    Ok(path)
}

fn option_path_suffix(path: &[usize]) -> String {
    path.iter()
        .map(|index| format!("-alternative-{index}"))
        .collect::<String>()
}

/// Navigate to a rule using a path where:
/// - `path[0]` is a **0-based** index into `rules`
/// - `path[1..]` are **1-based** indices into each successive `.alternatives`
fn navigate_to_rule_mut<'a>(rules: &'a mut Vec<Rule>, path: &[usize]) -> Result<&'a mut Rule> {
    anyhow::ensure!(!path.is_empty(), "path cannot be empty");
    let rule_idx = path[0];
    anyhow::ensure!(
        rule_idx < rules.len(),
        "rule index {} out of bounds (len={})",
        rule_idx,
        rules.len()
    );
    if path.len() == 1 {
        return Ok(&mut rules[rule_idx]);
    }
    navigate_through_alternatives_mut(&mut rules[rule_idx], &path[1..])
        .with_context(|| format!("navigating alternatives path {:?}", &path[1..]))
}

/// Navigate through `.alternatives` where every element of `path` is a **1-based** index.
fn navigate_through_alternatives_mut<'a>(
    rule: &'a mut Rule,
    path: &[usize],
) -> Result<&'a mut Rule> {
    anyhow::ensure!(!path.is_empty(), "alternative path cannot be empty");
    let idx_1based = path[0];
    let idx = idx_1based
        .checked_sub(1)
        .with_context(|| format!("alternative index {idx_1based} is invalid (must be ≥ 1)"))?;
    anyhow::ensure!(
        idx < rule.alternatives.len(),
        "alternative index {} out of bounds (len={})",
        idx_1based,
        rule.alternatives.len()
    );
    if path.len() == 1 {
        return Ok(&mut rule.alternatives[idx]);
    }
    navigate_through_alternatives_mut(&mut rule.alternatives[idx], &path[1..])
        .with_context(|| format!("navigating sub-path {:?}", &path[1..]))
}

/// Immutable navigation — returns `None` if any index is out of bounds.
fn get_rule_ref<'a>(rules: &'a [Rule], path: &[usize]) -> Option<&'a Rule> {
    let rule = rules.get(path[0])?;
    if path.len() == 1 {
        return Some(rule);
    }
    get_alternative_ref(rule, &path[1..])
}

fn get_alternative_ref<'a>(rule: &'a Rule, path: &[usize]) -> Option<&'a Rule> {
    let idx = path[0].checked_sub(1)?;
    let alt = rule.alternatives.get(idx)?;
    if path.len() == 1 {
        return Some(alt);
    }
    get_alternative_ref(alt, &path[1..])
}

/// Returns the rule index encoded in a row ID ("rule-N-…" → N).
fn rule_idx_from_row_id(row_id: &str) -> Option<usize> {
    row_id
        .strip_prefix("rule-")?
        .split('-')
        .next()?
        .parse::<usize>()
        .ok()
}

fn normalize_identifier(value: &str) -> String {
    value
        .chars()
        .map(|character| {
            if character.is_ascii_alphanumeric() {
                character.to_ascii_lowercase()
            } else {
                '-'
            }
        })
        .collect::<String>()
        .trim_matches('-')
        .to_string()
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::rules::{ModList, ModReference, ModSource, Rule};

    use super::{
        add_alternative_from_root, add_mod_rule_from_root, delete_rules_from_root,
        load_editor_snapshot_from_root, navigate_to_rule_mut, parse_option_path_from_row_id,
        rename_rule_from_root, save_alternative_order_from_root, save_incompatibilities_from_root,
        AddAlternativeInput, AddModRuleInput, DeleteRulesInput, EditorIncompatibilityRuleInput,
        RenameRuleInput, SaveAlternativeOrderInput, SaveIncompatibilitiesInput,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-editor-data-test-{timestamp}"))
    }

    fn modrinth_ref(id: &str) -> ModReference {
        ModReference {
            id: id.into(),
            source: ModSource::Modrinth,
            file_name: None,
        }
    }

    fn simple_rule(name: &str, mod_id: &str) -> Rule {
        Rule {
            rule_name: name.into(),
            mods: vec![modrinth_ref(mod_id)],
            exclude_if_present: vec![],
            alternatives: vec![],
            links: vec![],
            version_rules: vec![],
            custom_configs: vec![],
            alt_groups: vec![],
        }
    }

    fn write_modlist(modlist: &ModList, root_dir: &PathBuf) {
        let modlist_root = root_dir.join("mod-lists").join(&modlist.modlist_name);
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");
        let rules_path = modlist_root.join("rules.json");
        modlist.write_to_file(&rules_path).expect("should write");
    }

    #[test]
    fn load_editor_snapshot_maps_rules_into_primary_rows_and_alternatives() {
        let root_dir = unique_test_root();

        let modlist = ModList {
            modlist_name: "Visual Pack".into(),
            author: "PlayerLine".into(),
            description: "Visual test pack".into(),
            rules: vec![Rule {
                rule_name: "Rendering Engine".into(),
                mods: vec![modrinth_ref("sodium")],
                exclude_if_present: vec![],
                alternatives: vec![simple_rule("Rubidium", "rubidium")],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Visual Pack").expect("snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        let row = &snapshot.rows[0];
        assert_eq!(row.name, "Rendering Engine");
        assert_eq!(row.modrinth_id.as_deref(), Some("sodium"));
        assert_eq!(row.alternatives.len(), 1);
        assert_eq!(row.alternatives[0].name, "Rubidium");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn add_mod_rule_appends_new_rule() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![simple_rule("Sodium", "sodium")],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        add_mod_rule_from_root(
            &root_dir,
            &AddModRuleInput {
                modlist_name: "Test Pack".into(),
                rule_name: "Iris".into(),
                mod_id: "iris".into(),
                mod_source: "modrinth".into(),
                file_name: None,
            },
        )
        .expect("add_mod_rule should succeed");

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        assert_eq!(snapshot.rows.len(), 2);
        assert_eq!(snapshot.rows[1].name, "Iris");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn rename_rule_updates_name() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![simple_rule("Old Name", "sodium")],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        let row_id = snapshot.rows[0].id.clone();

        rename_rule_from_root(
            &root_dir,
            &RenameRuleInput {
                modlist_name: "Test Pack".into(),
                row_id,
                new_name: "New Name".into(),
            },
        )
        .expect("rename should succeed");

        let updated =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        assert_eq!(updated.rows[0].name, "New Name");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn delete_rules_removes_whole_rule() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![simple_rule("Sodium", "sodium"), simple_rule("Iris", "iris")],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        let sodium_id = snapshot.rows[0].id.clone();

        delete_rules_from_root(
            &root_dir,
            &DeleteRulesInput {
                modlist_name: "Test Pack".into(),
                row_ids: vec![sodium_id],
            },
        )
        .expect("delete should succeed");

        let updated =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        assert_eq!(updated.rows.len(), 1);
        assert_eq!(updated.rows[0].name, "Iris");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn delete_rules_removes_alternative_but_keeps_primary() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                mods: vec![modrinth_ref("sodium")],
                exclude_if_present: vec![],
                alternatives: vec![simple_rule("Rubidium", "rubidium")],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        let alt_id = snapshot.rows[0].alternatives[0].id.clone();

        delete_rules_from_root(
            &root_dir,
            &DeleteRulesInput {
                modlist_name: "Test Pack".into(),
                row_ids: vec![alt_id],
            },
        )
        .expect("delete alternative should succeed");

        let updated =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        assert_eq!(updated.rows.len(), 1, "rule should still exist");
        assert!(updated.rows[0].alternatives.is_empty(), "alternative should be removed");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn save_incompatibilities_sets_exclude_if_present() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![simple_rule("Sodium", "sodium"), simple_rule("Embeddium", "embeddium")],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        let sodium_id = snapshot.rows[0].id.clone();
        let embeddium_id = snapshot.rows[1].id.clone();

        save_incompatibilities_from_root(
            &root_dir,
            &SaveIncompatibilitiesInput {
                modlist_name: "Test Pack".into(),
                rules: vec![EditorIncompatibilityRuleInput {
                    winner_id: sodium_id,
                    loser_id: embeddium_id,
                }],
            },
        )
        .expect("save incompatibilities should succeed");

        let rules_path = root_dir.join("mod-lists").join("Test Pack").join("rules.json");
        let loaded = ModList::read_from_file(&rules_path).expect("should load");
        assert!(loaded.rules[1].exclude_if_present.contains(&"sodium".to_string()));

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn add_alternative_merges_rule_into_parent_alternatives() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![simple_rule("Sodium", "sodium"), simple_rule("Rubidium", "rubidium")],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        let sodium_id = snapshot.rows[0].id.clone();
        let rubidium_id = snapshot.rows[1].id.clone();

        add_alternative_from_root(
            &root_dir,
            &AddAlternativeInput {
                modlist_name: "Test Pack".into(),
                parent_row_id: sodium_id,
                alternative_row_id: rubidium_id,
            },
        )
        .expect("add alternative should succeed");

        let updated =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        assert_eq!(updated.rows.len(), 1, "rubidium rule should be removed as top-level");
        assert_eq!(updated.rows[0].alternatives.len(), 1);
        assert_eq!(updated.rows[0].alternatives[0].name, "Rubidium");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn save_alternative_order_reorders_top_level_alternatives() {
        let root_dir = unique_test_root();
        let modlist = ModList {
            modlist_name: "Test Pack".into(),
            author: "Tester".into(),
            description: "".into(),
            rules: vec![Rule {
                rule_name: "Rendering".into(),
                mods: vec![modrinth_ref("sodium")],
                exclude_if_present: vec![],
                alternatives: vec![
                    simple_rule("Rubidium", "rubidium"),
                    simple_rule("Embeddium", "embeddium"),
                ],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            groups_meta: vec![],
            presentation: None,
        };
        write_modlist(&modlist, &root_dir);

        let snapshot =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        let parent_id = snapshot.rows[0].id.clone();
        let rubidium_alt_id = snapshot.rows[0].alternatives[0].id.clone();
        let embeddium_alt_id = snapshot.rows[0].alternatives[1].id.clone();

        save_alternative_order_from_root(
            &root_dir,
            &SaveAlternativeOrderInput {
                modlist_name: "Test Pack".into(),
                parent_row_id: parent_id,
                // Reverse order: embeddium first, rubidium second
                ordered_alternative_ids: vec![embeddium_alt_id, rubidium_alt_id],
            },
        )
        .expect("reorder should succeed");

        let updated =
            load_editor_snapshot_from_root(&root_dir, "Test Pack").expect("snapshot should load");
        assert_eq!(updated.rows[0].alternatives[0].name, "Embeddium");
        assert_eq!(updated.rows[0].alternatives[1].name, "Rubidium");

        fs::remove_dir_all(&root_dir).ok();
    }

    #[test]
    fn parse_option_path_from_row_id_parses_top_level() {
        let path = parse_option_path_from_row_id("rule-2-sodium").expect("should parse");
        assert_eq!(path, vec![2]);
    }

    #[test]
    fn parse_option_path_from_row_id_parses_alternative() {
        let path =
            parse_option_path_from_row_id("rule-0-alternative-1-rubidium").expect("should parse");
        assert_eq!(path, vec![0, 1]);
    }

    #[test]
    fn parse_option_path_from_row_id_parses_nested_alternative() {
        let path =
            parse_option_path_from_row_id("rule-0-alternative-1-alternative-2-embeddium")
                .expect("should parse");
        assert_eq!(path, vec![0, 1, 2]);
    }

    #[test]
    fn navigate_to_rule_mut_reaches_nested_alternative() {
        let mut rules = vec![Rule {
            rule_name: "Primary".into(),
            mods: vec![ModReference {
                id: "sodium".into(),
                source: ModSource::Modrinth,
                file_name: None,
            }],
            exclude_if_present: vec![],
            alternatives: vec![Rule {
                rule_name: "First Alt".into(),
                mods: vec![ModReference {
                    id: "rubidium".into(),
                    source: ModSource::Modrinth,
                    file_name: None,
                }],
                exclude_if_present: vec![],
                alternatives: vec![simple_rule("Second Alt", "embeddium")],
                links: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alt_groups: vec![],
            }],
            links: vec![],
            version_rules: vec![],
            custom_configs: vec![],
            alt_groups: vec![],
        }];

        // Navigate to rules[0].alternatives[0]: path = [0 (rule idx), 1 (1-based alt idx)]
        let first_alt = navigate_to_rule_mut(&mut rules, &[0, 1])
            .expect("should navigate to first alt");
        assert_eq!(first_alt.rule_name, "First Alt");

        // Navigate to rules[0].alternatives[0].alternatives[0]: path = [0, 1, 1]
        let second_alt = navigate_to_rule_mut(&mut rules, &[0, 1, 1])
            .expect("should navigate to second alt");
        assert_eq!(second_alt.rule_name, "Second Alt");
    }
}
