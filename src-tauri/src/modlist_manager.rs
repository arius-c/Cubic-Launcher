use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use tauri::State;

use crate::editor_data::{add_mod_rule_from_root, AddModRuleInput};
use crate::launcher_paths::LauncherPaths;
use crate::rules::{ModList, RULES_FILENAME};

// ---------------------------------------------------------------------------
// Create Mod-list
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModlistInput {
    pub name: String,
    pub author: String,
    pub description: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
#[serde(rename_all = "camelCase")]
pub struct CreateModlistResult {
    pub name: String,
    pub author: String,
    pub description: String,
}

#[tauri::command]
pub fn create_modlist_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: CreateModlistInput,
) -> Result<CreateModlistResult, String> {
    create_modlist_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

pub fn create_modlist_from_root(
    root_dir: &Path,
    input: &CreateModlistInput,
) -> Result<CreateModlistResult> {
    let name = input.name.trim().to_string();
    if name.is_empty() {
        bail!("mod-list name cannot be empty");
    }

    let author = if input.author.trim().is_empty() {
        "Author".to_string()
    } else {
        input.author.trim().to_string()
    };

    let description = input.description.trim().to_string();

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&name)
        .join(RULES_FILENAME);

    if rules_path.exists() {
        bail!("a mod-list named '{}' already exists", name);
    }

    let modlist = ModList {
        modlist_name: name.clone(),
        author: author.clone(),
        description: description.clone(),
        rules: vec![],
        groups_meta: vec![],
    };

    modlist.write_to_file(&rules_path).with_context(|| {
        format!(
            "failed to create mod-list '{}' at {}",
            name,
            rules_path.display()
        )
    })?;

    Ok(CreateModlistResult {
        name,
        author,
        description,
    })
}

// ---------------------------------------------------------------------------
// Delete Mod-list
// ---------------------------------------------------------------------------

#[tauri::command]
pub fn delete_modlist_command(
    launcher_paths: State<'_, LauncherPaths>,
    modlist_name: String,
) -> Result<(), String> {
    delete_modlist_from_root(launcher_paths.root_dir(), &modlist_name)
        .map_err(|error| error.to_string())
}

pub fn delete_modlist_from_root(root_dir: &Path, modlist_name: &str) -> Result<()> {
    let name = modlist_name.trim();
    if name.is_empty() {
        bail!("mod-list name cannot be empty");
    }

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let modlist_dir = launcher_paths.modlists_dir().join(name);

    if !modlist_dir.exists() {
        bail!("mod-list '{}' does not exist", name);
    }

    std::fs::remove_dir_all(&modlist_dir).with_context(|| {
        format!(
            "failed to delete mod-list '{}' at {}",
            name,
            modlist_dir.display()
        )
    })?;

    Ok(())
}

// ---------------------------------------------------------------------------
// Copy Local JAR
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct CopyLocalJarInput {
    /// Absolute path of the source JAR on the user's filesystem.
    pub source_path: String,
    /// Human-readable rule name to assign (falls back to filename stem if empty).
    pub rule_name: String,
    /// Name of the mod-list to which the rule will be appended.
    pub modlist_name: String,
}

#[tauri::command]
pub fn copy_local_jar_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: CopyLocalJarInput,
) -> Result<(), String> {
    copy_local_jar_from_root(launcher_paths.root_dir(), &input).map_err(|error| error.to_string())
}

pub fn copy_local_jar_from_root(root_dir: &Path, input: &CopyLocalJarInput) -> Result<()> {
    let source_path = Path::new(&input.source_path);

    let file_name = source_path
        .file_name()
        .and_then(|name| name.to_str())
        .with_context(|| format!("source path '{}' has no valid filename", input.source_path))?;

    if !file_name.to_ascii_lowercase().ends_with(".jar") {
        bail!(
            "only .jar files are accepted for local mod upload, got '{}'",
            file_name
        );
    }

    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let dest_path = launcher_paths.mods_cache_dir().join(file_name);

    // Ensure cache/mods/ directory exists before copying.
    if let Some(parent) = dest_path.parent() {
        std::fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create cache/mods/ directory at {}",
                parent.display()
            )
        })?;
    }

    std::fs::copy(source_path, &dest_path).with_context(|| {
        format!(
            "failed to copy '{}' to '{}'",
            source_path.display(),
            dest_path.display()
        )
    })?;

    // Derive the rule name: prefer the user-provided name, fall back to filename stem.
    let rule_name = {
        let candidate = input.rule_name.trim().to_string();
        if candidate.is_empty() {
            file_name
                .trim_end_matches(".jar")
                .trim_end_matches(".JAR")
                .to_string()
        } else {
            candidate
        }
    };

    // Derive the mod_id from the filename stem (without extension).
    let mod_id = file_name
        .trim_end_matches(".jar")
        .trim_end_matches(".JAR")
        .to_string();

    add_mod_rule_from_root(
        root_dir,
        &AddModRuleInput {
            modlist_name: input.modlist_name.clone(),
            rule_name,
            mod_id,
            mod_source: "local".into(),
            file_name: Some(file_name.to_string()),
        },
    )
}

// ---------------------------------------------------------------------------
// Tests
// ---------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use crate::editor_data::load_editor_snapshot_from_root;
    use crate::rules::{ModList, ModReference, ModSource, Rule};

    use super::{
        copy_local_jar_from_root, create_modlist_from_root, CopyLocalJarInput, CreateModlistInput,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-modlist-manager-test-{timestamp}"))
    }

    #[test]
    fn create_modlist_writes_skeleton_rules_json() {
        let root_dir = unique_test_root();
        fs::create_dir_all(root_dir.join("mod-lists")).expect("mod-lists directory should exist");

        create_modlist_from_root(
            &root_dir,
            &CreateModlistInput {
                name: "My New Pack".into(),
                author: "PlayerLine".into(),
                description: "A fresh mod-list".into(),
            },
        )
        .expect("create modlist should succeed");

        let rules_path = root_dir
            .join("mod-lists")
            .join("My New Pack")
            .join("rules.json");
        assert!(rules_path.exists(), "rules.json should have been created");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "My New Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.modlist_name, "My New Pack");
        assert!(
            snapshot.rows.is_empty(),
            "new mod-list should have no rules"
        );

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn create_modlist_uses_default_author_when_blank() {
        let root_dir = unique_test_root();
        fs::create_dir_all(root_dir.join("mod-lists")).expect("mod-lists directory should exist");

        let result = create_modlist_from_root(
            &root_dir,
            &CreateModlistInput {
                name: "Blank Author Pack".into(),
                author: "   ".into(),
                description: String::new(),
            },
        )
        .expect("create modlist should succeed");

        assert_eq!(result.author, "Author");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn create_modlist_rejects_duplicate_name() {
        let root_dir = unique_test_root();
        fs::create_dir_all(root_dir.join("mod-lists")).expect("mod-lists directory should exist");

        create_modlist_from_root(
            &root_dir,
            &CreateModlistInput {
                name: "Duplicate Pack".into(),
                author: "PlayerLine".into(),
                description: String::new(),
            },
        )
        .expect("first create should succeed");

        let second = create_modlist_from_root(
            &root_dir,
            &CreateModlistInput {
                name: "Duplicate Pack".into(),
                author: "PlayerLine".into(),
                description: String::new(),
            },
        );

        assert!(second.is_err(), "second create with same name should fail");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn copy_local_jar_places_file_in_cache_and_adds_rule() {
        let root_dir = unique_test_root();
        let modlist_dir = root_dir.join("mod-lists").join("Test Pack");
        let mods_cache_dir = root_dir.join("cache").join("mods");
        fs::create_dir_all(&modlist_dir).expect("modlist directory should exist");
        fs::create_dir_all(&mods_cache_dir).expect("cache mods directory should exist");

        ModList {
            modlist_name: "Test Pack".into(),
            author: "PlayerLine".into(),
            description: String::new(),
            rules: vec![],
            groups_meta: vec![],
        }
        .write_to_file(&modlist_dir.join("rules.json"))
        .expect("rules should write");

        // Create a fake JAR source file.
        let source_dir = root_dir.join("source");
        fs::create_dir_all(&source_dir).expect("source dir should exist");
        let source_jar = source_dir.join("custom-patch-1.0.jar");
        fs::write(&source_jar, b"fake jar content").expect("fake jar should write");

        copy_local_jar_from_root(
            &root_dir,
            &CopyLocalJarInput {
                source_path: source_jar.to_string_lossy().into_owned(),
                rule_name: "Custom Patch".into(),
                modlist_name: "Test Pack".into(),
            },
        )
        .expect("copy local jar should succeed");

        assert!(
            mods_cache_dir.join("custom-patch-1.0.jar").exists(),
            "JAR should have been copied to cache/mods/"
        );

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Test Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "Custom Patch");
        assert_eq!(snapshot.rows[0].kind, "local");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn copy_local_jar_uses_filename_stem_as_rule_name_when_rule_name_is_blank() {
        let root_dir = unique_test_root();
        let modlist_dir = root_dir.join("mod-lists").join("Stem Pack");
        let mods_cache_dir = root_dir.join("cache").join("mods");
        fs::create_dir_all(&modlist_dir).expect("modlist directory should exist");
        fs::create_dir_all(&mods_cache_dir).expect("cache mods directory should exist");

        ModList {
            modlist_name: "Stem Pack".into(),
            author: "PlayerLine".into(),
            description: String::new(),
            rules: vec![],
            groups_meta: vec![],
        }
        .write_to_file(&modlist_dir.join("rules.json"))
        .expect("rules should write");

        let source_dir = root_dir.join("source");
        fs::create_dir_all(&source_dir).expect("source dir should exist");
        let source_jar = source_dir.join("sodium-fabric-0.6.0.jar");
        fs::write(&source_jar, b"fake jar content").expect("fake jar should write");

        copy_local_jar_from_root(
            &root_dir,
            &CopyLocalJarInput {
                source_path: source_jar.to_string_lossy().into_owned(),
                rule_name: "".into(),
                modlist_name: "Stem Pack".into(),
            },
        )
        .expect("copy should succeed with blank rule name");

        let snapshot = load_editor_snapshot_from_root(&root_dir, "Stem Pack")
            .expect("editor snapshot should load");

        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].name, "sodium-fabric-0.6.0");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn copy_local_jar_rejects_non_jar_files() {
        let root_dir = unique_test_root();
        fs::create_dir_all(root_dir.join("mod-lists").join("Any Pack"))
            .expect("modlist dir should exist");

        let result = copy_local_jar_from_root(
            &root_dir,
            &CopyLocalJarInput {
                source_path: "/tmp/not-a-jar.zip".into(),
                rule_name: "Bad File".into(),
                modlist_name: "Any Pack".into(),
            },
        );

        assert!(result.is_err(), "non-JAR files should be rejected");

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[allow(dead_code)]
    fn _uses_rule_types() {
        let _ = Rule {
            rule_name: "x".into(),
            mods: vec![ModReference {
                id: "x".into(),
                source: ModSource::Local,
                file_name: Some("x.jar".into()),
            }],
            exclude_if_present: vec![],
            alternatives: vec![],
            links: vec![],
            version_rules: vec![],
            custom_configs: vec![],
        };
    }
}
