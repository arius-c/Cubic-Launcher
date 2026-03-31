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
    create_modlist_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
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
    let modlist_dir = launcher_paths.modlists_dir().join(&name);
    let rules_path = modlist_dir.join(RULES_FILENAME);

    if rules_path.exists() {
        bail!("a mod-list named '{}' already exists", name);
    }

    // Create subdirectories
    std::fs::create_dir_all(modlist_dir.join("local-jars")).with_context(|| {
        format!("failed to create local-jars directory for modlist '{}'", name)
    })?;
    std::fs::create_dir_all(modlist_dir.join("custom_configs")).with_context(|| {
        format!(
            "failed to create custom_configs directory for modlist '{}'",
            name
        )
    })?;

    let modlist = ModList {
        modlist_name: name.clone(),
        author: author.clone(),
        description: description.clone(),
        rules: vec![],
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
        .map_err(|e| e.to_string())
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
    pub source_path: String,
    pub modlist_name: String,
}

#[tauri::command]
pub fn import_modlist_command(
    launcher_paths: State<'_, LauncherPaths>,
    modlist_name: String,
    source_path: String,
) -> Result<(), String> {
    let dest = launcher_paths
        .modlists_dir()
        .join(&modlist_name)
        .join(crate::rules::RULES_FILENAME);
    // Validate the source is valid rules.json
    crate::rules::ModList::read_from_file(std::path::Path::new(&source_path))
        .map_err(|e| format!("Invalid rules file: {e}"))?;
    std::fs::copy(&source_path, &dest)
        .map_err(|e| format!("Failed to copy: {e}"))?;
    Ok(())
}

#[tauri::command]
pub fn copy_local_jar_command(
    launcher_paths: State<'_, LauncherPaths>,
    input: CopyLocalJarInput,
) -> Result<(), String> {
    copy_local_jar_from_root(launcher_paths.root_dir(), &input).map_err(|e| e.to_string())
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

    // Derive mod_id from the filename stem (without .jar)
    let mod_id = file_name
        .trim_end_matches(".jar")
        .trim_end_matches(".JAR")
        .to_string();

    if mod_id.is_empty() {
        bail!("JAR filename has no stem: '{}'", file_name);
    }

    // Check if a rule with this mod_id already exists
    let launcher_paths = LauncherPaths::new(root_dir.to_path_buf());
    let rules_path = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join(RULES_FILENAME);
    let modlist = ModList::read_from_file(&rules_path).with_context(|| {
        format!(
            "failed to load modlist '{}' for local JAR copy",
            input.modlist_name
        )
    })?;

    if modlist.contains_mod_id(&mod_id) {
        bail!(
            "a rule with mod_id '{}' already exists in modlist '{}'",
            mod_id,
            input.modlist_name
        );
    }

    // Copy the JAR to local-jars/
    let local_jars_dir = launcher_paths
        .modlists_dir()
        .join(&input.modlist_name)
        .join("local-jars");
    std::fs::create_dir_all(&local_jars_dir).with_context(|| {
        format!(
            "failed to create local-jars directory at {}",
            local_jars_dir.display()
        )
    })?;

    let dest_path = local_jars_dir.join(format!("{}.jar", mod_id));
    std::fs::copy(source_path, &dest_path).with_context(|| {
        format!(
            "failed to copy '{}' to '{}'",
            source_path.display(),
            dest_path.display()
        )
    })?;

    // Append a new top-level rule
    add_mod_rule_from_root(
        root_dir,
        &AddModRuleInput {
            modlist_name: input.modlist_name.clone(),
            mod_id,
            source: "local".into(),
            file_name: None,
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
    use crate::rules::ModList;

    use super::{
        copy_local_jar_from_root, create_modlist_from_root, CopyLocalJarInput, CreateModlistInput,
    };

    fn unique_test_root() -> PathBuf {
        let ts = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .unwrap()
            .as_nanos();
        env::temp_dir().join(format!("cubic-modlist-mgr-test-{ts}"))
    }

    #[test]
    fn create_modlist_writes_skeleton_rules_json() {
        let root = unique_test_root();
        fs::create_dir_all(root.join("mod-lists")).unwrap();

        create_modlist_from_root(
            &root,
            &CreateModlistInput {
                name: "My New Pack".into(),
                author: "PlayerLine".into(),
                description: "A fresh mod-list".into(),
            },
        )
        .unwrap();

        let rules_path = root.join("mod-lists").join("My New Pack").join("rules.json");
        assert!(rules_path.exists());

        // local-jars and custom_configs directories should exist
        assert!(root.join("mod-lists").join("My New Pack").join("local-jars").exists());
        assert!(root
            .join("mod-lists")
            .join("My New Pack")
            .join("custom_configs")
            .exists());

        let snapshot = load_editor_snapshot_from_root(&root, "My New Pack").unwrap();
        assert_eq!(snapshot.modlist_name, "My New Pack");
        assert!(snapshot.rows.is_empty());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_modlist_uses_default_author_when_blank() {
        let root = unique_test_root();
        fs::create_dir_all(root.join("mod-lists")).unwrap();

        let result = create_modlist_from_root(
            &root,
            &CreateModlistInput {
                name: "Blank Author Pack".into(),
                author: "   ".into(),
                description: String::new(),
            },
        )
        .unwrap();

        assert_eq!(result.author, "Author");
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn create_modlist_rejects_duplicate_name() {
        let root = unique_test_root();
        fs::create_dir_all(root.join("mod-lists")).unwrap();

        create_modlist_from_root(
            &root,
            &CreateModlistInput {
                name: "Dup Pack".into(),
                author: "Author".into(),
                description: String::new(),
            },
        )
        .unwrap();

        let result = create_modlist_from_root(
            &root,
            &CreateModlistInput {
                name: "Dup Pack".into(),
                author: "Author".into(),
                description: String::new(),
            },
        );
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_local_jar_places_file_and_adds_rule() {
        let root = unique_test_root();
        let modlist_dir = root.join("mod-lists").join("Test Pack");
        fs::create_dir_all(modlist_dir.join("local-jars")).unwrap();

        ModList {
            modlist_name: "Test Pack".into(),
            author: "Author".into(),
            description: String::new(),
            rules: vec![],
        }
        .write_to_file(&modlist_dir.join("rules.json"))
        .unwrap();

        // Create a fake JAR source file
        let source_dir = root.join("source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_jar = source_dir.join("custom-patch-1.0.jar");
        fs::write(&source_jar, b"fake jar content").unwrap();

        copy_local_jar_from_root(
            &root,
            &CopyLocalJarInput {
                source_path: source_jar.to_string_lossy().into_owned(),
                modlist_name: "Test Pack".into(),
            },
        )
        .unwrap();

        assert!(modlist_dir.join("local-jars").join("custom-patch-1.0.jar").exists());

        let snapshot = load_editor_snapshot_from_root(&root, "Test Pack").unwrap();
        assert_eq!(snapshot.rows.len(), 1);
        assert_eq!(snapshot.rows[0].mod_id, "custom-patch-1.0");
        assert_eq!(snapshot.rows[0].source, "local");

        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_local_jar_rejects_non_jar_files() {
        let root = unique_test_root();
        fs::create_dir_all(root.join("mod-lists").join("Any Pack")).unwrap();

        let result = copy_local_jar_from_root(
            &root,
            &CopyLocalJarInput {
                source_path: "/tmp/not-a-jar.zip".into(),
                modlist_name: "Any Pack".into(),
            },
        );

        assert!(result.is_err());
        fs::remove_dir_all(&root).unwrap();
    }

    #[test]
    fn copy_local_jar_rejects_duplicate_mod_id() {
        let root = unique_test_root();
        let modlist_dir = root.join("mod-lists").join("Test Pack");
        fs::create_dir_all(modlist_dir.join("local-jars")).unwrap();

        use crate::rules::{ModSource, Rule};
        ModList {
            modlist_name: "Test Pack".into(),
            author: "Author".into(),
            description: String::new(),
            rules: vec![Rule {
                mod_id: "existing-mod".into(),
                source: ModSource::Local,
                exclude_if: vec![],
                requires: vec![],
                version_rules: vec![],
                custom_configs: vec![],
                alternatives: vec![],
            }],
        }
        .write_to_file(&modlist_dir.join("rules.json"))
        .unwrap();

        let source_dir = root.join("source");
        fs::create_dir_all(&source_dir).unwrap();
        let source_jar = source_dir.join("existing-mod.jar");
        fs::write(&source_jar, b"fake").unwrap();

        let result = copy_local_jar_from_root(
            &root,
            &CopyLocalJarInput {
                source_path: source_jar.to_string_lossy().into_owned(),
                modlist_name: "Test Pack".into(),
            },
        );
        assert!(result.is_err());

        fs::remove_dir_all(&root).unwrap();
    }
}
