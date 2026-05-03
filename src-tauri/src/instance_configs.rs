use std::fs;
use std::path::{Path, PathBuf};

use anyhow::{bail, Context, Result};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CachedConfigPlacement {
    pub cache_subdir: String,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct PreparedInstanceConfigs {
    pub materialized_files: Vec<PathBuf>,
}

pub fn prepare_instance_config_directory(
    configs_cache_dir: &Path,
    instance_config_dir: &Path,
    placements: &[CachedConfigPlacement],
) -> Result<PreparedInstanceConfigs> {
    fs::create_dir_all(instance_config_dir).with_context(|| {
        format!(
            "failed to create instance config directory at {}",
            instance_config_dir.display()
        )
    })?;

    let mut materialized_files = Vec::new();

    for placement in placements {
        let source_dir = configs_cache_dir.join(&placement.cache_subdir);

        if !source_dir.exists() {
            bail!(
                "required config cache directory '{}' is missing from {}",
                placement.cache_subdir,
                configs_cache_dir.display()
            );
        }

        if !source_dir.is_dir() {
            bail!(
                "config cache entry '{}' is not a directory in {}",
                placement.cache_subdir,
                configs_cache_dir.display()
            );
        }

        collect_and_materialize_config_files(
            &source_dir,
            &source_dir,
            instance_config_dir,
            &mut materialized_files,
        )?;
    }

    Ok(PreparedInstanceConfigs { materialized_files })
}

fn collect_and_materialize_config_files(
    source_root: &Path,
    current_dir: &Path,
    instance_config_dir: &Path,
    materialized_files: &mut Vec<PathBuf>,
) -> Result<()> {
    for entry in fs::read_dir(current_dir).with_context(|| {
        format!(
            "failed to read config cache directory {}",
            current_dir.display()
        )
    })? {
        let entry = entry.with_context(|| {
            format!(
                "failed to inspect config cache entry in {}",
                current_dir.display()
            )
        })?;
        let path = entry.path();
        let metadata = fs::symlink_metadata(&path)
            .with_context(|| format!("failed to read metadata for {}", path.display()))?;

        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            collect_and_materialize_config_files(
                source_root,
                &path,
                instance_config_dir,
                materialized_files,
            )?;
            continue;
        }

        let relative_path = path.strip_prefix(source_root).with_context(|| {
            format!(
                "failed to determine relative config path for {} from {}",
                path.display(),
                source_root.display()
            )
        })?;
        let target_path = instance_config_dir.join(relative_path);

        if let Some(parent) = target_path.parent() {
            fs::create_dir_all(parent).with_context(|| {
                format!(
                    "failed to create config target directory {}",
                    parent.display()
                )
            })?;
        }

        link_or_copy_file(&path, &target_path)?;
        materialized_files.push(target_path);
    }

    Ok(())
}

fn link_or_copy_file(source_path: &Path, target_path: &Path) -> Result<()> {
    if let Ok(metadata) = fs::symlink_metadata(target_path) {
        if metadata.file_type().is_dir() && !metadata.file_type().is_symlink() {
            fs::remove_dir_all(target_path).with_context(|| {
                format!(
                    "failed to remove directory target {}",
                    target_path.display()
                )
            })?;
        } else {
            fs::remove_file(target_path).with_context(|| {
                format!("failed to remove file target {}", target_path.display())
            })?;
        }
    }

    if create_symlink_file(source_path, target_path).is_ok() {
        return Ok(());
    }

    if fs::hard_link(source_path, target_path).is_ok() {
        return Ok(());
    }

    fs::copy(source_path, target_path)
        .map(|_| ())
        .with_context(|| {
            format!(
                "failed to materialize config file from {} to {}",
                source_path.display(),
                target_path.display()
            )
        })
}

#[cfg(target_family = "unix")]
fn create_symlink_file(source_path: &Path, target_path: &Path) -> Result<()> {
    std::os::unix::fs::symlink(source_path, target_path).map_err(Into::into)
}

#[cfg(target_family = "windows")]
fn create_symlink_file(source_path: &Path, target_path: &Path) -> Result<()> {
    std::os::windows::fs::symlink_file(source_path, target_path).map_err(Into::into)
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::{prepare_instance_config_directory, CachedConfigPlacement};

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-instance-configs-test-{timestamp}"))
    }

    #[test]
    fn prepares_nested_config_files_without_removing_unrelated_entries() {
        let root_dir = unique_test_root();
        let configs_cache_dir = root_dir.join("cache").join("configs");
        let cache_subdir = configs_cache_dir.join("hash_sodium_fabric_084");
        let instance_config_dir = root_dir.join("instance").join("config");

        fs::create_dir_all(cache_subdir.join("cloth-config"))
            .expect("cache subdirectories should be created");
        fs::create_dir_all(&instance_config_dir).expect("instance config dir should be created");

        fs::write(cache_subdir.join("sodium-options.json"), b"new sodium")
            .expect("sodium config should be written");
        fs::write(
            cache_subdir.join("cloth-config").join("client.toml"),
            b"cloth settings",
        )
        .expect("cloth config should be written");
        fs::write(instance_config_dir.join("generated.cfg"), b"keep me")
            .expect("unrelated config should be written");

        let prepared = prepare_instance_config_directory(
            &configs_cache_dir,
            &instance_config_dir,
            &[CachedConfigPlacement {
                cache_subdir: "hash_sodium_fabric_084".into(),
            }],
        )
        .expect("config preparation should succeed");

        assert_eq!(prepared.materialized_files.len(), 2);
        assert_eq!(
            fs::read(instance_config_dir.join("sodium-options.json"))
                .expect("sodium config should read"),
            b"new sodium"
        );
        assert_eq!(
            fs::read(instance_config_dir.join("cloth-config").join("client.toml"))
                .expect("nested config should read"),
            b"cloth settings"
        );
        assert_eq!(
            fs::read(instance_config_dir.join("generated.cfg"))
                .expect("unrelated config should remain"),
            b"keep me"
        );

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn later_config_placements_override_earlier_files() {
        let root_dir = unique_test_root();
        let configs_cache_dir = root_dir.join("cache").join("configs");
        let first_cache_subdir = configs_cache_dir.join("hash_first");
        let second_cache_subdir = configs_cache_dir.join("hash_second");
        let instance_config_dir = root_dir.join("instance").join("config");

        fs::create_dir_all(&first_cache_subdir).expect("first cache dir should be created");
        fs::create_dir_all(&second_cache_subdir).expect("second cache dir should be created");
        fs::write(first_cache_subdir.join("shared.toml"), b"first")
            .expect("first config should be written");
        fs::write(second_cache_subdir.join("shared.toml"), b"second")
            .expect("second config should be written");

        prepare_instance_config_directory(
            &configs_cache_dir,
            &instance_config_dir,
            &[
                CachedConfigPlacement {
                    cache_subdir: "hash_first".into(),
                },
                CachedConfigPlacement {
                    cache_subdir: "hash_second".into(),
                },
            ],
        )
        .expect("config preparation should succeed");

        assert_eq!(
            fs::read(instance_config_dir.join("shared.toml")).expect("shared config should read"),
            b"second"
        );

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn errors_when_required_config_cache_directory_is_missing() {
        let root_dir = unique_test_root();
        let configs_cache_dir = root_dir.join("cache").join("configs");
        let instance_config_dir = root_dir.join("instance").join("config");

        fs::create_dir_all(&configs_cache_dir).expect("configs cache dir should be created");

        let error = prepare_instance_config_directory(
            &configs_cache_dir,
            &instance_config_dir,
            &[CachedConfigPlacement {
                cache_subdir: "missing_config_set".into(),
            }],
        )
        .expect_err("config preparation should fail");

        assert!(error
            .to_string()
            .contains("required config cache directory 'missing_config_set' is missing"));

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }
}
