use std::collections::{HashMap, HashSet};
use std::path::PathBuf;

use crate::app_shell::{ShellGlobalSettings, ShellModListOverrides};
use crate::dependencies::DependencyLink;
use crate::modrinth::{DependencyType, ModrinthDependency, ModrinthVersion};
use crate::resolver::{ModLoader, ResolutionTarget};

use super::{
    build_instance_root, build_top_level_owner_map, collect_selected_project_ids,
    embedded_min_java_requirement, fabric_dependency_predicates_match,
    maven_artifact_relative_path, minecraft_version_predicate_matches,
    minimum_java_version_for_predicate, parse_mod_loader, substitute_known_placeholders,
    validate_final_fabric_runtime, validate_selected_parent_dependencies, DependencyLink,
    EffectiveLaunchSettings, EmbeddedFabricModMetadata, EmbeddedFabricRequirementSet,
    EmbeddedFabricRequirements, LaunchPlaceholders, OwnedEmbeddedFabricModMetadata, PlayerIdentity,
};

fn global_settings() -> ShellGlobalSettings {
    ShellGlobalSettings {
        min_ram_mb: 2048,
        max_ram_mb: 4096,
        custom_jvm_args: "-Dglobal=true".into(),
        profiler_enabled: false,
        cache_only_mode: true,
        wrapper_command: "gamemoderun".into(),
        java_path_override: "/custom/java".into(),
    }
}

fn modlist_overrides() -> ShellModListOverrides {
    ShellModListOverrides {
        modlist_name: Some("Pack".into()),
        min_ram_mb: Some(8192),
        max_ram_mb: None,
        custom_jvm_args: Some("-Dmodlist=true".into()),
        profiler_enabled: Some(false),
        wrapper_command: Some("mangohud".into()),
        minecraft_version: None,
        mod_loader: None,
    }
}

#[test]
fn parses_supported_mod_loader_names() {
    assert_eq!(parse_mod_loader("Fabric").unwrap(), ModLoader::Fabric);
    assert_eq!(parse_mod_loader("quilt").unwrap(), ModLoader::Quilt);
    assert_eq!(parse_mod_loader("Forge").unwrap(), ModLoader::Forge);
    assert_eq!(parse_mod_loader("NeoForge").unwrap(), ModLoader::NeoForge);
}

#[test]
fn effective_launch_settings_prefer_modlist_overrides() {
    let settings =
        EffectiveLaunchSettings::from_shell_settings(&global_settings(), &modlist_overrides());

    assert_eq!(settings.min_ram_mb, 8192);
    assert_eq!(settings.max_ram_mb, 4096);
    assert_eq!(settings.custom_jvm_args, "-Dmodlist=true");
    assert!(settings.cache_only_mode);
    assert_eq!(settings.wrapper_command, Some("mangohud".into()));
    assert_eq!(
        settings.java_path_override,
        Some(PathBuf::from("/custom/java"))
    );
}

#[test]
fn maven_coordinates_expand_to_standard_artifact_path() {
    assert_eq!(
        maven_artifact_relative_path("net.fabricmc:fabric-loader:0.16.14").unwrap(),
        PathBuf::from("net/fabricmc/fabric-loader/0.16.14/fabric-loader-0.16.14.jar")
    );
    assert_eq!(
        maven_artifact_relative_path("org.lwjgl:lwjgl:3.3.3:natives-windows").unwrap(),
        PathBuf::from("org/lwjgl/lwjgl/3.3.3/lwjgl-3.3.3-natives-windows.jar")
    );
}

#[test]
fn instance_root_uses_version_and_loader_suffix() {
    let launcher_paths = crate::launcher_paths::LauncherPaths::new("workspace-root");
    let target = ResolutionTarget {
        minecraft_version: "1.21.1".into(),
        mod_loader: ModLoader::Fabric,
    };

    assert_eq!(
        build_instance_root(&launcher_paths, "Pack", &target),
        PathBuf::from("workspace-root")
            .join("mod-lists")
            .join("Pack")
            .join("instances")
            .join("1.21.1-fabric")
    );
}

#[test]
fn known_launch_placeholders_are_substituted() {
    let placeholders = LaunchPlaceholders::new(
        &PlayerIdentity {
            username: "PlayerOne".into(),
            uuid: "uuid-123".into(),
            access_token: "token-abc".into(),
            user_type: "offline".into(),
            version_type: "Cubic".into(),
        },
        "Pack",
        &ResolutionTarget {
            minecraft_version: "1.21.1".into(),
            mod_loader: ModLoader::Fabric,
        },
        PathBuf::from("game-dir").as_path(),
        PathBuf::from("assets-root").as_path(),
        "1.21",
        PathBuf::from("libraries-root").as_path(),
        PathBuf::from("natives-root").as_path(),
    );

    let substituted = substitute_known_placeholders(
        "${auth_player_name}:${auth_uuid}:${game_directory}:${version_name}",
        &placeholders,
    );

    assert_eq!(
        substituted,
        "PlayerOne:uuid-123:game-dir:Pack-1.21.1-fabric"
    );
}

fn sample_version(project_id: &str, version_id: &str) -> ModrinthVersion {
    ModrinthVersion {
        id: version_id.into(),
        project_id: project_id.into(),
        version_number: "1.0.0".into(),
        name: format!("{project_id} {version_id}"),
        game_versions: vec!["1.21.5".into()],
        loaders: vec!["fabric".into()],
        dependencies: Vec::new(),
        files: Vec::new(),
        date_published: "2024-08-15T10:00:00.000Z".into(),
    }
}

fn metadata_entry(
    owner_project_id: &str,
    mod_id: &str,
    version: &str,
) -> OwnedEmbeddedFabricModMetadata {
    OwnedEmbeddedFabricModMetadata {
        owner_project_id: owner_project_id.into(),
        metadata: EmbeddedFabricModMetadata {
            mod_id: mod_id.into(),
            version: version.into(),
            provides: Vec::new(),
            depends: HashMap::new(),
            breaks: HashMap::new(),
        },
    }
}

#[test]
fn java_predicates_extract_minimum_requirement() {
    assert_eq!(minimum_java_version_for_predicate(">=22"), Some(22));
    assert_eq!(minimum_java_version_for_predicate(">21"), Some(22));
    assert_eq!(minimum_java_version_for_predicate("21"), Some(21));
    assert_eq!(
        embedded_min_java_requirement(&EmbeddedFabricRequirements {
            root_entry: None,
            entries: vec![
                EmbeddedFabricRequirementSet {
                    minecraft_predicates: Vec::new(),
                    java_predicates: vec![">=21".into()],
                },
                EmbeddedFabricRequirementSet {
                    minecraft_predicates: Vec::new(),
                    java_predicates: vec![">=22".into(), ">=23".into()],
                },
            ],
        }),
        Some(22)
    );
}

#[test]
fn tilde_minecraft_predicates_match_target_patch_line() {
    assert!(minecraft_version_predicate_matches("~1.21.6", "1.21.6"));
    assert!(minecraft_version_predicate_matches("~1.21.6", "1.21.9"));
    assert!(!minecraft_version_predicate_matches("~1.21.6", "1.22.0"));
}

#[test]
fn fabric_dependency_predicates_match_semver_ranges() {
    assert!(fabric_dependency_predicates_match(
        &["<7.0.0".into()],
        "6.2.9"
    ));
    assert!(fabric_dependency_predicates_match(
        &[">=17.0.6".into()],
        "17.0.6"
    ));
    assert!(!fabric_dependency_predicates_match(
        &["<3.0.0".into()],
        "3.0.0"
    ));
    assert!(fabric_dependency_predicates_match(
        &["<1.8.0".into()],
        "1.8.0-beta.4+mc1.21.1"
    ));
    assert!(!fabric_dependency_predicates_match(
        &[">=1.8.0".into()],
        "1.8.0-beta.4+mc1.21.1"
    ));
}

#[test]
fn exact_parent_dependency_check_uses_project_ids() {
    let mut iris = sample_version("YL57xq9U", "iris-1");
    iris.dependencies.push(ModrinthDependency {
        version_id: Some("sodium-0.6.12".into()),
        project_id: Some("AANobbMI".into()),
        dependency_type: DependencyType::Required,
        file_name: None,
    });
    let sodium = ModrinthVersion {
        id: "sodium-0.6.13".into(),
        ..sample_version("AANobbMI", "sodium-0.6.13")
    };
    let parent_versions = vec![iris.clone(), sodium.clone()];
    let selected_parent_versions = HashMap::from([
        (iris.project_id.clone(), iris),
        (sodium.project_id.clone(), sodium),
    ]);
    let selected_project_ids = collect_selected_project_ids(&parent_versions);

    let excluded = validate_selected_parent_dependencies(
        &parent_versions,
        &selected_parent_versions,
        &selected_project_ids,
    );

    assert_eq!(
        excluded,
        std::collections::HashSet::from(["YL57xq9U".to_string()])
    );
}

#[test]
fn owner_map_propagates_transitive_dependency_owners() {
    let parent_versions = vec![sample_version("top-level", "top-level-1")];
    let owner_map = build_top_level_owner_map(
        &parent_versions,
        &[
            DependencyLink {
                parent_mod_id: "top-level".into(),
                dependency_id: "mid".into(),
                specific_version: None,
                jar_filename: "mid.jar".into(),
            },
            DependencyLink {
                parent_mod_id: "mid".into(),
                dependency_id: "leaf".into(),
                specific_version: None,
                jar_filename: "leaf.jar".into(),
            },
        ],
    );

    assert_eq!(
        owner_map.get("leaf"),
        Some(&HashSet::from(["top-level".to_string()]))
    );
}

#[test]
fn final_fabric_validation_excludes_top_level_on_breaks_conflict() {
    let owner_map = HashMap::from([(
        "puzzle-project".to_string(),
        HashSet::from(["puzzle-project".to_string()]),
    )]);
    let mut puzzle = metadata_entry("puzzle-project", "puzzle", "2.3.0");
    puzzle
        .metadata
        .breaks
        .insert("entity_model_features".into(), vec!["<3.0.0".into()]);
    let emf = metadata_entry("emf-project", "entity_model_features", "2.4.1");

    let issues = validate_final_fabric_runtime(&[puzzle, emf], &owner_map);

    assert_eq!(
        issues.get("puzzle-project").map(|issue| issue.reason_code),
        Some("breaks_conflict")
    );
}

#[test]
fn final_fabric_validation_excludes_top_level_on_prerelease_breaks_conflict() {
    let owner_map = HashMap::from([
        (
            "sodium-project".to_string(),
            HashSet::from(["sodium-project".to_string()]),
        ),
        (
            "reeses-project".to_string(),
            HashSet::from(["sodiumoptionsapi-project".to_string()]),
        ),
    ]);
    let mut sodium = metadata_entry("sodium-project", "sodium", "0.6.13+mc1.21.1");
    sodium
        .metadata
        .breaks
        .insert("reeses-sodium-options".into(), vec!["<1.8.0".into()]);
    let reeses = metadata_entry(
        "reeses-project",
        "reeses-sodium-options",
        "1.8.0-beta.4+mc1.21.1",
    );

    let issues = validate_final_fabric_runtime(&[sodium, reeses], &owner_map);

    assert_eq!(
        issues.get("sodium-project").map(|issue| issue.reason_code),
        Some("breaks_conflict")
    );
}

#[test]
fn final_fabric_validation_excludes_top_level_on_missing_dependency() {
    let owner_map = HashMap::from([
        (
            "sodiumoptionsapi-project".to_string(),
            HashSet::from(["sodiumoptionsapi-project".to_string()]),
        ),
        (
            "embedded-helper-project".to_string(),
            HashSet::from(["sodiumoptionsapi-project".to_string()]),
        ),
    ]);
    let mut sodium_options_api =
        metadata_entry("sodiumoptionsapi-project", "sodiumoptionsapi", "1.0.10");
    sodium_options_api
        .metadata
        .depends
        .insert("reeses-sodium-options".into(), vec!["*".into()]);

    let issues = validate_final_fabric_runtime(&[sodium_options_api], &owner_map);

    assert_eq!(
        issues
            .get("sodiumoptionsapi-project")
            .and_then(|issue| issue.dependency_id.as_deref()),
        Some("reeses-sodium-options")
    );
    assert_eq!(
        issues
            .get("sodiumoptionsapi-project")
            .map(|issue| issue.reason_code),
        Some("missing_dependency")
    );
}
