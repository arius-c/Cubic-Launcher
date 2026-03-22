use std::path::Path;

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::debug_trace::append_debug_trace_to_root;
use crate::launcher_paths::LauncherPaths;
use crate::rules::{ModList, ModReference, ModSource, Rule, RuleOption, RULES_FILENAME};

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorSnapshot {
    pub modlist_name: String,
    pub rows: Vec<EditorRow>,
    pub incompatibilities: Vec<EditorIncompatibilityRule>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorRow {
    pub id: String,
    pub name: String,
    /// Primary Modrinth project slug/id for icon fetching — None for local mods or groups.
    pub modrinth_id: Option<String>,
    pub kind: String,
    pub area: String,
    pub note: String,
    pub tags: Vec<String>,
    pub alternatives: Vec<EditorRow>,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct EditorIncompatibilityRule {
    pub winner_id: String,
    pub loser_id: String,
}

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

    Ok(EditorSnapshot {
        modlist_name: modlist.modlist_name.clone(),
        rows: modlist
            .rules
            .iter()
            .enumerate()
            .map(|(rule_index, rule)| build_editor_row(rule_index, rule))
            .collect(),
        incompatibilities: derive_editor_incompatibilities(&modlist.rules),
    })
}

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
    let rule = modlist
        .rules
        .get_mut(rule_index)
        .with_context(|| format!("rule index {} does not exist", rule_index))?;

    let (alternative_options, replace_top_level) = if option_path.len() == 1 {
        if rule.options.len() <= 1 {
            return Ok(());
        }

        let alternatives = rule
            .options
            .iter()
            .enumerate()
            .skip(1)
            .map(|(option_index, option)| {
                (
                    build_alternative_row(rule_index, option_index, option, "").id,
                    option.clone(),
                )
            })
            .collect::<Vec<_>>();
        (alternatives, true)
    } else {
        let parent_option = navigate_to_option_mut(&mut rule.options, &option_path[1..])
            .with_context(|| format!("could not find parent option for {:?}", option_path))?;
        if parent_option.alternatives.is_empty() {
            return Ok(());
        }
        let parent_path = option_path_suffix(&option_path[1..]);
        let alternatives = parent_option
            .alternatives
            .iter()
            .enumerate()
            .map(|(option_index, option)| {
                (
                    build_alternative_row(rule_index, option_index + 1, option, &parent_path).id,
                    option.clone(),
                )
            })
            .collect::<Vec<_>>();
        (alternatives, false)
    };

    if input.ordered_alternative_ids.len() != alternative_options.len() {
        anyhow::bail!(
            "alternative ordering size mismatch: expected {}, got {}",
            alternative_options.len(),
            input.ordered_alternative_ids.len()
        );
    }

    let reordered_alternatives = input
        .ordered_alternative_ids
        .iter()
        .map(|alternative_id| {
            alternative_options
                .iter()
                .find(|(candidate_id, _)| candidate_id == alternative_id)
                .map(|(_, option)| option.clone())
                .with_context(|| format!("unknown alternative id '{}'", alternative_id))
        })
        .collect::<Result<Vec<_>>>()?;

    if replace_top_level {
        let primary_option = rule.options[0].clone();
        rule.options = std::iter::once(primary_option)
            .chain(reordered_alternatives)
            .collect();
    } else {
        let parent_option = navigate_to_option_mut(&mut rule.options, &option_path[1..])
            .with_context(|| format!("could not find parent option for {:?}", option_path))?;
        parent_option.alternatives = reordered_alternatives;
    }

    modlist.write_to_file(&rules_path)?;
    let _ = append_debug_trace_to_root(
        root_dir,
        &format!(
            "[alts.reorder.backend] saved modlist={} parent_row_id={} replace_top_level={}",
            input.modlist_name, input.parent_row_id, replace_top_level
        ),
    );
    Ok(())
}

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
        for opt in rule.options.iter_mut() {
            clear_exclusions_recursive(opt);
        }
    }

    // Apply exclusions to the correct option at any depth using the path encoded
    // in the row ID (rule-N for top-level, rule-N-alternative-M-… for alternatives).
    for (loser_id, mut exclusions) in exclusions_by_loser {
        let path = match parse_option_path_from_row_id(&loser_id) {
            Ok(p) => p,
            Err(_) => continue,
        };
        let rule_idx = path[0];
        if rule_idx >= modlist.rules.len() {
            continue;
        }
        // Top-level loser → target the primary option (index 0).
        // Alternative loser → target the specific option via path[1..].
        let nav_path: Vec<usize> = if path.len() == 1 {
            vec![0]
        } else {
            path[1..].to_vec()
        };
        if let Ok(option) =
            navigate_to_option_mut(&mut modlist.rules[rule_idx].options, &nav_path)
        {
            exclusions.sort();
            exclusions.dedup();
            option.exclude_if_present = exclusions;
        }
    }

    modlist.write_to_file(&rules_path)
}

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
        options: vec![RuleOption {
            mods: vec![mod_reference],
            exclude_if_present: vec![],
            fallback_strategy: crate::rules::FallbackStrategy::Continue,
            option_name: None,
            alternatives: vec![],
        }],
    };

    modlist.rules.push(new_rule);
    modlist.write_to_file(&rules_path)
}

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

    // Separate the requested IDs into whole-rule deletions and single-option
    // (alternative) deletions.
    let mut rule_indices_to_remove: Vec<usize> = Vec::new();
    // Maps rule_index → set of option_indices to remove from that rule.
    let mut options_to_remove: std::collections::HashMap<usize, std::collections::BTreeSet<usize>> =
        std::collections::HashMap::new();

    for row_id in &input.row_ids {
        if let Some((rule_index, option_index)) = parse_alternative_indices_from_row_id(row_id) {
            // This is an alternative (option index ≥ 1 by definition).
            if rule_index < modlist.rules.len() && option_index >= 1 {
                options_to_remove
                    .entry(rule_index)
                    .or_default()
                    .insert(option_index);
            }
        } else if let Ok(rule_index) = parse_rule_index_from_row_id(row_id) {
            if rule_index < modlist.rules.len() {
                rule_indices_to_remove.push(rule_index);
            }
        }
    }

    // Step 1: remove specific options (alternatives) from their parent rules.
    for (rule_index, option_set) in &options_to_remove {
        // Skip if the whole rule is being removed anyway.
        if rule_indices_to_remove.contains(rule_index) {
            continue;
        }
        let rule = &mut modlist.rules[*rule_index];
        // Remove from the end so lower indices stay valid.
        for &option_index in option_set.iter().rev() {
            if option_index < rule.options.len() {
                rule.options.remove(option_index);
            }
        }
        // If all options were removed, schedule the whole rule for deletion.
        if rule.options.is_empty() {
            rule_indices_to_remove.push(*rule_index);
        }
    }

    // Step 2: remove whole rules (from the end to preserve index validity).
    rule_indices_to_remove.sort_unstable();
    rule_indices_to_remove.dedup();
    for index in rule_indices_to_remove.into_iter().rev() {
        modlist.rules.remove(index);
    }

    modlist.write_to_file(&rules_path)
}

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
    let rule = modlist
        .rules
        .get_mut(rule_index)
        .with_context(|| format!("rule index {} does not exist", rule_index))?;

    rule.rule_name = new_name;
    modlist.write_to_file(&rules_path)
}

fn build_editor_row(rule_index: usize, rule: &Rule) -> EditorRow {
    let primary_option = rule.options.first();

    EditorRow {
        id: format!(
            "rule-{}-{}",
            rule_index,
            normalize_identifier(&rule.rule_name)
        ),
        name: rule.rule_name.clone(),
        modrinth_id: primary_option.and_then(|opt| {
            opt.mods
                .iter()
                .find(|m| m.source == ModSource::Modrinth)
                .map(|m| m.id.clone())
        }),
        kind: option_kind(primary_option),
        area: "Rule".to_string(),
        note: option_note(primary_option, false),
        tags: option_tags(primary_option, false),
        alternatives: rule
            .options
            .iter()
            .skip(1)
            .enumerate()
            .map(|(option_offset, option)| {
                build_alternative_row(rule_index, option_offset + 1, option, "")
            })
            .collect(),
    }
}

fn build_alternative_row(
    rule_index: usize,
    option_index: usize,
    option: &RuleOption,
    parent_path: &str,
) -> EditorRow {
    let display_name = option
        .option_name
        .as_ref()
        .cloned()
        .unwrap_or_else(|| option_label(option));

    let id_label = option_label(option);
    // Build a fully-qualified path ID so nested alternatives have unique stable IDs.
    // Format: rule-{rule_index}{parent_path}-alternative-{option_index}-{name}
    let id = format!(
        "rule-{}{}-alternative-{}-{}",
        rule_index,
        parent_path,
        option_index,
        normalize_identifier(&id_label)
    );

    // Build the path segment for children of this option.
    let child_path = format!("{parent_path}-alternative-{option_index}");

    EditorRow {
        id,
        name: display_name,
        modrinth_id: option
            .mods
            .iter()
            .find(|m| m.source == ModSource::Modrinth)
            .map(|m| m.id.clone()),
        kind: option_kind(Some(option)),
        area: "Rule".to_string(),
        note: option_note(Some(option), true),
        tags: option_tags(Some(option), true),
        alternatives: option
            .alternatives
            .iter()
            .enumerate()
            .map(|(sub_idx, sub_opt)| {
                build_alternative_row(rule_index, sub_idx + 1, sub_opt, &child_path)
            })
            .collect(),
    }
}

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

/// Returns `Some((rule_index, option_index))` if the row_id belongs to an
/// alternative (i.e. the format is `"rule-N-alternative-M-…"`), or `None`
/// if it is a top-level rule id.
fn parse_alternative_indices_from_row_id(row_id: &str) -> Option<(usize, usize)> {
    // Split into at most 5 parts so trailing hyphens in the name are kept together.
    let parts: Vec<&str> = row_id.splitn(5, '-').collect();
    // Expected: ["rule", rule_index, "alternative", option_index, ...]
    if parts.len() < 4 || parts[0] != "rule" || parts[2] != "alternative" {
        return None;
    }
    let rule_index = parts[1].parse::<usize>().ok()?;
    let option_index = parts[3].parse::<usize>().ok()?;
    Some((rule_index, option_index))
}

// ── Add alternative ───────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AddAlternativeInput {
    pub modlist_name: String,
    /// The row that will receive a new fallback option.
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

    // Take the alternative rule's primary option and append it to the parent.
    // Any sub-alternatives the alt rule had (options[1..]) are preserved as nested
    // alternatives inside the primary option, so they move with it rather than
    // becoming flat siblings.
    let alt_rule_name = modlist.rules[alt_idx].rule_name.clone();
    anyhow::ensure!(
        !modlist.rules[alt_idx].options.is_empty(),
        "alternative rule '{}' has no options",
        alt_rule_name
    );
    let mut primary_opt = modlist.rules[alt_idx].options[0].clone();
    if primary_opt.option_name.is_none() {
        primary_opt.option_name = Some(alt_rule_name.clone());
    }
    // Move any existing sub-alternatives (options[1..]) into the primary option's
    // nested alternatives list.
    let sub_alts: Vec<_> = modlist.rules[alt_idx].options[1..]
        .iter()
        .map(|opt| {
            let mut cloned = opt.clone();
            if cloned.option_name.is_none() {
                cloned.option_name = Some(alt_rule_name.clone());
            }
            cloned
        })
        .collect();
    primary_opt.alternatives.extend(sub_alts);
    modlist.rules[parent_idx].options.push(primary_opt);

    // Remove the now-merged standalone rule (from highest index first).
    modlist.rules.remove(alt_idx);

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

    // Parse the parent alternative path: [rule_idx, opt_idx, opt_idx, …]
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

    // Take the source rule's primary option and convert it to a nested alternative.
    // Any sub-alternatives the source rule had (options[1..]) are preserved as nested
    // alternatives inside the primary option, so they move with it.
    let src_rule_name = modlist.rules[src_rule_idx].rule_name.clone();
    anyhow::ensure!(
        !modlist.rules[src_rule_idx].options.is_empty(),
        "source rule '{}' has no options",
        src_rule_name
    );
    let mut primary_opt = modlist.rules[src_rule_idx].options[0].clone();
    if primary_opt.option_name.is_none() {
        primary_opt.option_name = Some(src_rule_name.clone());
    }
    let sub_alts: Vec<RuleOption> = modlist.rules[src_rule_idx].options[1..]
        .iter()
        .map(|opt| {
            let mut cloned = opt.clone();
            if cloned.option_name.is_none() {
                cloned.option_name = Some(src_rule_name.clone());
            }
            cloned
        })
        .collect();
    primary_opt.alternatives.extend(sub_alts);

    // Navigate to the parent option and append the new alternative there.
    let rule_idx = parent_path[0];
    anyhow::ensure!(rule_idx < modlist.rules.len(), "rule index out of bounds");

    let parent_option =
        navigate_to_option_mut(&mut modlist.rules[rule_idx].options, &parent_path[1..])
            .with_context(|| {
                format!(
                    "could not find parent alternative for path {:?}",
                    parent_path
                )
            })?;

    parent_option.alternatives.push(primary_opt);

    // Remove the now-nested source rule.
    modlist.rules.remove(src_rule_idx);

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

    // Navigate to the parent option list that contains the target option.
    let opt_idx = *path.last().unwrap();
    let detached_option = if path.len() == 2 {
        // Direct child of the top-level rule — remove from rule.options.
        anyhow::ensure!(
            opt_idx < modlist.rules[rule_idx].options.len(),
            "alternative option index out of bounds"
        );
        modlist.rules[rule_idx].options.remove(opt_idx)
    } else {
        // Nested alternative — remove from parent option's alternatives list.
        let parent_path = &path[1..path.len() - 1];
        let parent_option =
            navigate_to_option_mut(&mut modlist.rules[rule_idx].options, parent_path)
                .with_context(|| "could not find parent option for detachment")?;
        anyhow::ensure!(
            opt_idx - 1 < parent_option.alternatives.len(),
            "nested alternative index out of bounds"
        );
        parent_option.alternatives.remove(opt_idx - 1)
    };

    // Re-insert the detached option as a new top-level rule at the end.
    let new_rule_name = detached_option
        .option_name
        .clone()
        .unwrap_or_else(|| option_label(&detached_option));
    let mut new_option = detached_option;
    // Promote sub-alternatives to rule-level options so they remain visible.
    let sub_alternatives = std::mem::take(&mut new_option.alternatives);
    new_option.option_name = None;

    let mut new_options = vec![new_option];
    new_options.extend(sub_alternatives);

    modlist.rules.push(Rule {
        rule_name: new_rule_name,
        options: new_options,
    });

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

/// Parse a row ID into a path of indices: [rule_idx, opt_idx, opt_idx, …].
/// For top-level rules (no "-alternative-") returns a single-element vec.
/// For "rule-N-alternative-M-…" returns [N, M].
/// For "rule-N-alternative-M-alternative-K-…" returns [N, M+1, K+1], etc.
/// The option_index stored in the ID is 1-based for alternatives so we keep
/// that as-is — callers use it to index into `.options` or `.alternatives`.
fn parse_option_path_from_row_id(row_id: &str) -> Result<Vec<usize>> {
    // Split by "-alternative-" segments.
    // row-id format: "rule-{N}(-alternative-{M})*-{name}"
    // We detect by scanning for the literal "-alternative-" separators.
    let after_rule = row_id
        .strip_prefix("rule-")
        .ok_or_else(|| anyhow::anyhow!("row id '{}' does not start with 'rule-'", row_id))?;

    // Split into [rule_index_and_rest] by finding "-alternative-" tokens.
    // We process character by character to respect that the name can contain hyphens.
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

/// Navigate through nested `.options` / `.alternatives` using an index path
/// and return a mutable reference to the target option.
/// The path contains the option indices at each depth level.
/// - depth 0 → index into `options`
/// - deeper depths → 1-based indices into nested `alternatives`
fn navigate_to_option_mut<'a>(
    options: &'a mut Vec<RuleOption>,
    path: &[usize],
) -> Result<&'a mut RuleOption> {
    navigate_to_option_mut_at_depth(options, path, 0)
}

fn navigate_to_option_mut_at_depth<'a>(
    options: &'a mut Vec<RuleOption>,
    path: &[usize],
    depth: usize,
) -> Result<&'a mut RuleOption> {
    anyhow::ensure!(!path.is_empty(), "option path cannot be empty");
    let raw_index = path[0];
    let normalized_index = if depth == 0 {
        raw_index
    } else {
        raw_index
            .checked_sub(1)
            .with_context(|| format!("nested alternative index {raw_index} is invalid"))?
    };
    anyhow::ensure!(
        normalized_index < options.len(),
        "option index {} out of bounds at depth {}",
        raw_index,
        depth
    );
    if path.len() == 1 {
        return Ok(&mut options[normalized_index]);
    }
    navigate_to_option_mut_at_depth(
        &mut options[normalized_index].alternatives,
        &path[1..],
        depth + 1,
    )
    .with_context(|| format!("navigating sub-path {:?}", &path[1..]))
}

// ── Reorder rules ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ReorderRulesInput {
    pub modlist_name: String,
    /// Parent-row IDs in the desired new order.  These are the IDs that were
    /// valid before the reorder (based on current indices in rules.json).
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

fn primary_mod_ids(option: &RuleOption) -> Vec<String> {
    option
        .mods
        .iter()
        .map(|mod_reference| mod_reference.id.clone())
        .collect()
}

/// Build a flat list of (row_id, primary_mod_ids) for every option at every depth.
fn collect_all_row_ids(rules: &[Rule]) -> Vec<(String, Vec<String>)> {
    let mut result = Vec::new();
    for (rule_idx, rule) in rules.iter().enumerate() {
        if let Some(opt) = rule.options.first() {
            result.push((build_editor_row(rule_idx, rule).id, primary_mod_ids(opt)));
        }
        for (opt_offset, opt) in rule.options.iter().skip(1).enumerate() {
            collect_option_row_ids_into(rule_idx, opt_offset + 1, opt, "", &mut result);
        }
    }
    result
}

fn collect_option_row_ids_into(
    rule_idx: usize,
    opt_idx: usize,
    opt: &RuleOption,
    parent_path: &str,
    result: &mut Vec<(String, Vec<String>)>,
) {
    let row = build_alternative_row(rule_idx, opt_idx, opt, parent_path);
    result.push((row.id, primary_mod_ids(opt)));
    let child_path = format!("{parent_path}-alternative-{opt_idx}");
    for (sub_idx, sub_opt) in opt.alternatives.iter().enumerate() {
        collect_option_row_ids_into(rule_idx, sub_idx + 1, sub_opt, &child_path, result);
    }
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

fn push_incompatibilities_for_option(
    option: &RuleOption,
    loser_row_id: &str,
    row_lookup: &[(String, Vec<String>)],
    incompatibilities: &mut Vec<EditorIncompatibilityRule>,
) {
    for excluded_mod_id in &option.exclude_if_present {
        if let Some((winner_row_id, _)) = row_lookup
            .iter()
            .find(|(_, mods)| mods.contains(excluded_mod_id))
        {
            // Skip within-chain exclusions (winner and loser belong to the same rule).
            // These are resolver hints (e.g. "skip this alternative if primary is active"),
            // not cross-rule incompatibilities that belong in the UI editor.
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

fn collect_option_incompatibilities(
    rule_idx: usize,
    opt_idx: usize,
    opt: &RuleOption,
    parent_path: &str,
    row_lookup: &[(String, Vec<String>)],
    incompatibilities: &mut Vec<EditorIncompatibilityRule>,
) {
    let row = build_alternative_row(rule_idx, opt_idx, opt, parent_path);
    push_incompatibilities_for_option(opt, &row.id, row_lookup, incompatibilities);
    let child_path = format!("{parent_path}-alternative-{opt_idx}");
    for (sub_idx, sub_opt) in opt.alternatives.iter().enumerate() {
        collect_option_incompatibilities(
            rule_idx,
            sub_idx + 1,
            sub_opt,
            &child_path,
            row_lookup,
            incompatibilities,
        );
    }
}

fn clear_exclusions_recursive(opt: &mut RuleOption) {
    opt.exclude_if_present.clear();
    for alt in opt.alternatives.iter_mut() {
        clear_exclusions_recursive(alt);
    }
}

fn derive_editor_incompatibilities(rules: &[Rule]) -> Vec<EditorIncompatibilityRule> {
    let row_lookup = collect_all_row_ids(rules);
    let mut incompatibilities = Vec::new();

    for (rule_index, rule) in rules.iter().enumerate() {
        if let Some(primary_option) = rule.options.first() {
            let loser_row_id = build_editor_row(rule_index, rule).id;
            push_incompatibilities_for_option(
                primary_option,
                &loser_row_id,
                &row_lookup,
                &mut incompatibilities,
            );
        }
        for (opt_offset, opt) in rule.options.iter().skip(1).enumerate() {
            collect_option_incompatibilities(
                rule_index,
                opt_offset + 1,
                opt,
                "",
                &row_lookup,
                &mut incompatibilities,
            );
        }
    }

    incompatibilities
}

fn option_kind(option: Option<&RuleOption>) -> String {
    if option
        .map(|option| {
            option
                .mods
                .iter()
                .any(|mod_reference| mod_reference.source == ModSource::Local)
        })
        .unwrap_or(false)
    {
        "local".to_string()
    } else {
        "modrinth".to_string()
    }
}

fn option_note(option: Option<&RuleOption>, is_alternative: bool) -> String {
    let Some(option) = option else {
        return if is_alternative {
            "Fallback option placeholder for the selected rule.".to_string()
        } else {
            "Rule block without options yet.".to_string()
        };
    };

    let mod_count = option.mods.len();
    let local_count = option
        .mods
        .iter()
        .filter(|mod_reference| mod_reference.source == ModSource::Local)
        .count();
    let exclude_count = option.exclude_if_present.len();

    if is_alternative {
        format!(
            "Fallback option with {} mod{}{}{}.",
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
    } else {
        format!(
            "Primary option with {} mod{}{}{}.",
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
}

fn option_tags(option: Option<&RuleOption>, is_alternative: bool) -> Vec<String> {
    let Some(option) = option else {
        return Vec::new();
    };

    let mut tags = Vec::new();
    if option.mods.len() > 1 {
        tags.push(format!("{} Mods", option.mods.len()));
    }
    if option
        .mods
        .iter()
        .any(|mod_reference| mod_reference.source == ModSource::Local)
    {
        tags.push("Manual".to_string());
    }
    if !option.exclude_if_present.is_empty() {
        tags.push("Conflict Set".to_string());
    }
    if matches!(
        option.fallback_strategy,
        crate::rules::FallbackStrategy::Abort
    ) {
        tags.push("Abort".to_string());
    }
    if is_alternative {
        tags.push("Alternative".to_string());
    }

    tags
}

fn option_label(option: &RuleOption) -> String {
    option
        .mods
        .iter()
        .map(mod_reference_label)
        .collect::<Vec<_>>()
        .join(" + ")
}

fn mod_reference_label(mod_reference: &ModReference) -> String {
    match mod_reference.source {
        ModSource::Local => mod_reference
            .file_name
            .clone()
            .unwrap_or_else(|| mod_reference.id.clone()),
        ModSource::Modrinth => mod_reference.id.clone(),
    }
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

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::rules::{FallbackStrategy, ModList, ModReference, ModSource, Rule, RuleOption};

    use super::{
        add_mod_rule_from_root, delete_rules_from_root, load_editor_snapshot_from_root,
        navigate_to_option_mut, parse_option_path_from_row_id, rename_rule_from_root,
        save_alternative_order_from_root, save_incompatibilities_from_root, AddModRuleInput,
        DeleteRulesInput, EditorIncompatibilityRuleInput, RenameRuleInput,
        SaveAlternativeOrderInput, SaveIncompatibilitiesInput,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-editor-data-test-{timestamp}"))
    }

    #[test]
    fn load_editor_snapshot_maps_rules_into_primary_rows_and_alternatives() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Visual Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Visual Pack".into(),
            author: "PlayerLine".into(),
            description: "Visual test pack".into(),
            rules: vec![Rule {
                rule_name: "Rendering Engine".into(),
                options: vec![
                    RuleOption {
                        mods: vec![ModReference {
                            id: "sodium".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![
                            ModReference {
                                id: "optifine".into(),
                                source: ModSource::Local,
                                file_name: Some("optifine-manuale.jar".into()),
                            },
                            ModReference {
                                id: "optifabric".into(),
                                source: ModSource::Modrinth,
                                file_name: None,
                            },
                        ],
                        exclude_if_present: vec!["sodium".into()],
                        fallback_strategy: FallbackStrategy::Abort,
                        option_name: None,
                        alternatives: vec![],
                    },
                ],
            }],
        }
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Visual Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.modlist_name, "Visual Pack");
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "Rendering Engine");
        assert_eq!(snapshot.rows[0].kind, "modrinth");
        assert_eq!(snapshot.rows[0].alternatives.len(), 1);
        assert_eq!(
            snapshot.rows[0].alternatives[0].name,
            "optifine-manuale.jar + optifabric"
        );
        assert_eq!(snapshot.rows[0].alternatives[0].kind, "local");
        assert!(snapshot.rows[0].alternatives[0]
            .tags
            .contains(&"Alternative".to_string()));
        assert!(snapshot.rows[0].alternatives[0]
            .tags
            .contains(&"Manual".to_string()));
        assert!(snapshot.rows[0].alternatives[0]
            .tags
            .contains(&"Abort".to_string()));
        assert!(snapshot.rows[0].alternatives[0]
            .tags
            .contains(&"Conflict Set".to_string()));
        assert!(snapshot.incompatibilities.is_empty());

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn navigate_to_option_mut_resolves_deep_nested_alternative_paths() {
        let mut options = vec![
            RuleOption {
                mods: vec![ModReference {
                    id: "primary".into(),
                    source: ModSource::Modrinth,
                    file_name: None,
                }],
                exclude_if_present: vec![],
                fallback_strategy: FallbackStrategy::Continue,
                option_name: Some("Primary".into()),
                alternatives: vec![],
            },
            RuleOption {
                mods: vec![ModReference {
                    id: "alt-a".into(),
                    source: ModSource::Modrinth,
                    file_name: None,
                }],
                exclude_if_present: vec![],
                fallback_strategy: FallbackStrategy::Continue,
                option_name: Some("Alt A".into()),
                alternatives: vec![
                    RuleOption {
                        mods: vec![ModReference {
                            id: "alt-a-1".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: Some("Alt A1".into()),
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![ModReference {
                            id: "alt-a-2".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: Some("Alt A2".into()),
                        alternatives: vec![RuleOption {
                            mods: vec![ModReference {
                                id: "alt-a-2-i".into(),
                                source: ModSource::Modrinth,
                                file_name: None,
                            }],
                            exclude_if_present: vec![],
                            fallback_strategy: FallbackStrategy::Continue,
                            option_name: Some("Alt A2-I".into()),
                            alternatives: vec![],
                        }],
                    },
                ],
            },
        ];

        let path = parse_option_path_from_row_id("rule-0-alternative-1-alternative-2-alt-a-2")
            .expect("path should parse");
        let nested = navigate_to_option_mut(&mut options, &path[1..]).expect("path should resolve");

        assert_eq!(nested.option_name.as_deref(), Some("Alt A2"));
        assert_eq!(nested.alternatives.len(), 1);
        assert_eq!(
            nested.alternatives[0].option_name.as_deref(),
            Some("Alt A2-I")
        );
    }

    #[test]
    fn save_alternative_order_rewrites_rule_option_order() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Visual Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Visual Pack".into(),
            author: "PlayerLine".into(),
            description: "Visual test pack".into(),
            rules: vec![Rule {
                rule_name: "Rendering Engine".into(),
                options: vec![
                    RuleOption {
                        mods: vec![ModReference {
                            id: "sodium".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![ModReference {
                            id: "rubidium".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                    RuleOption {
                        mods: vec![
                            ModReference {
                                id: "optifine".into(),
                                source: ModSource::Local,
                                file_name: Some("optifine-manuale.jar".into()),
                            },
                            ModReference {
                                id: "optifabric".into(),
                                source: ModSource::Modrinth,
                                file_name: None,
                            },
                        ],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    },
                ],
            }],
        }
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        save_alternative_order_from_root(
            &root_dir,
            &SaveAlternativeOrderInput {
                modlist_name: "Visual Pack".into(),
                parent_row_id: "rule-0-rendering-engine".into(),
                ordered_alternative_ids: vec![
                    "rule-0-alternative-2-optifine-manuale-jar---optifabric".into(),
                    "rule-0-alternative-1-rubidium".into(),
                ],
            },
        )
        .expect("alternative order should save");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Visual Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows[0].alternatives.len(), 2);
        assert_eq!(
            snapshot.rows[0].alternatives[0].name,
            "optifine-manuale.jar + optifabric"
        );
        assert_eq!(snapshot.rows[0].alternatives[1].name, "rubidium");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn save_incompatibilities_rewrites_primary_exclusions_and_roundtrips() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Conflict Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Conflict Pack".into(),
            author: "PlayerLine".into(),
            description: "Conflict test pack".into(),
            rules: vec![
                Rule {
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
                },
                Rule {
                    rule_name: "Minimap Suite".into(),
                    options: vec![RuleOption {
                        mods: vec![ModReference {
                            id: "xaeros-minimap".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    }],
                },
            ],
        }
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        save_incompatibilities_from_root(
            &root_dir,
            &SaveIncompatibilitiesInput {
                modlist_name: "Conflict Pack".into(),
                rules: vec![EditorIncompatibilityRuleInput {
                    winner_id: "rule-0-rendering-engine".into(),
                    loser_id: "rule-1-minimap-suite".into(),
                }],
            },
        )
        .expect("incompatibilities should save");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Conflict Pack")
            .expect("snapshot should load");

        assert_eq!(snapshot.incompatibilities.len(), 1);
        assert_eq!(
            snapshot.incompatibilities[0].winner_id,
            "rule-0-rendering-engine"
        );
        assert_eq!(
            snapshot.incompatibilities[0].loser_id,
            "rule-1-minimap-suite"
        );
        assert!(snapshot.rows[1].tags.contains(&"Conflict Set".to_string()));

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn add_mod_rule_appends_a_modrinth_rule_and_roundtrips() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Feature Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Feature Pack".into(),
            author: "PlayerLine".into(),
            description: "Test pack".into(),
            rules: vec![],
        }
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        add_mod_rule_from_root(
            &root_dir,
            &AddModRuleInput {
                modlist_name: "Feature Pack".into(),
                rule_name: "Minimap".into(),
                mod_id: "xaeros-minimap".into(),
                mod_source: "modrinth".into(),
                file_name: None,
            },
        )
        .expect("add mod rule should succeed");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Feature Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "Minimap");
        assert_eq!(snapshot.rows[0].kind, "modrinth");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn add_mod_rule_appends_a_local_rule_and_roundtrips() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Local Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Local Pack".into(),
            author: "PlayerLine".into(),
            description: "Test pack".into(),
            rules: vec![],
        }
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        add_mod_rule_from_root(
            &root_dir,
            &AddModRuleInput {
                modlist_name: "Local Pack".into(),
                rule_name: "Custom Patch".into(),
                mod_id: "custom-patch".into(),
                mod_source: "local".into(),
                file_name: Some("custom-patch-1.0.jar".into()),
            },
        )
        .expect("add local mod rule should succeed");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Local Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "Custom Patch");
        assert_eq!(snapshot.rows[0].kind, "local");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn delete_rules_removes_matching_rules_and_roundtrips() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Delete Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Delete Pack".into(),
            author: "PlayerLine".into(),
            description: "Test pack".into(),
            rules: vec![
                Rule {
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
                },
                Rule {
                    rule_name: "Minimap".into(),
                    options: vec![RuleOption {
                        mods: vec![ModReference {
                            id: "xaeros-minimap".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    }],
                },
                Rule {
                    rule_name: "Performance Kit".into(),
                    options: vec![RuleOption {
                        mods: vec![ModReference {
                            id: "lithium".into(),
                            source: ModSource::Modrinth,
                            file_name: None,
                        }],
                        exclude_if_present: vec![],
                        fallback_strategy: FallbackStrategy::Continue,
                        option_name: None,
                        alternatives: vec![],
                    }],
                },
            ],
        }
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        delete_rules_from_root(
            &root_dir,
            &DeleteRulesInput {
                modlist_name: "Delete Pack".into(),
                row_ids: vec![
                    "rule-0-rendering-engine".into(),
                    "rule-2-performance-kit".into(),
                ],
            },
        )
        .expect("delete rules should succeed");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Delete Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "Minimap");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn rename_rule_updates_rule_name_and_roundtrips() {
        let root_dir = unique_test_root();
        let modlist_root = root_dir.join("mod-lists").join("Rename Pack");
        fs::create_dir_all(&modlist_root).expect("modlist directory should exist");

        ModList {
            modlist_name: "Rename Pack".into(),
            author: "PlayerLine".into(),
            description: "Test pack".into(),
            rules: vec![Rule {
                rule_name: "Old Name".into(),
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
        .write_to_file(&modlist_root.join("rules.json"))
        .expect("rules should write");

        rename_rule_from_root(
            &root_dir,
            &RenameRuleInput {
                modlist_name: "Rename Pack".into(),
                row_id: "rule-0-old-name".into(),
                new_name: "New Name".into(),
            },
        )
        .expect("rename rule should succeed");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Rename Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "New Name");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }
}
