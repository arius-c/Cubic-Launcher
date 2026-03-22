use std::fs;
use std::path::PathBuf;

use anyhow::Result;

use crate::database::DATABASE_FILENAME;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct LauncherPaths {
    root_dir: PathBuf,
    cache_dir: PathBuf,
    mods_cache_dir: PathBuf,
    configs_cache_dir: PathBuf,
    modlists_dir: PathBuf,
    java_runtimes_dir: PathBuf,
    database_path: PathBuf,
}

impl LauncherPaths {
    pub fn new(root_dir: impl Into<PathBuf>) -> Self {
        let root_dir = root_dir.into();
        let cache_dir = root_dir.join("cache");
        let mods_cache_dir = cache_dir.join("mods");
        let configs_cache_dir = cache_dir.join("configs");
        let modlists_dir = root_dir.join("mod-lists");
        let java_runtimes_dir = root_dir.join("java-runtimes");
        let database_path = root_dir.join(DATABASE_FILENAME);

        Self {
            root_dir,
            cache_dir,
            mods_cache_dir,
            configs_cache_dir,
            modlists_dir,
            java_runtimes_dir,
            database_path,
        }
    }

    pub fn root_dir(&self) -> &std::path::Path {
        &self.root_dir
    }

    pub fn database_path(&self) -> &std::path::Path {
        &self.database_path
    }

    pub fn modlists_dir(&self) -> &std::path::Path {
        &self.modlists_dir
    }

    pub fn mods_cache_dir(&self) -> &std::path::Path {
        &self.mods_cache_dir
    }

    pub fn configs_cache_dir(&self) -> &std::path::Path {
        &self.configs_cache_dir
    }

    pub fn java_runtimes_dir(&self) -> &std::path::Path {
        &self.java_runtimes_dir
    }

    pub fn create_required_directories(&self) -> Result<()> {
        for directory in [
            &self.root_dir,
            &self.cache_dir,
            &self.mods_cache_dir,
            &self.configs_cache_dir,
            &self.modlists_dir,
            &self.java_runtimes_dir,
        ] {
            fs::create_dir_all(directory)?;
        }

        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use super::LauncherPaths;

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-paths-test-{timestamp}"))
    }

    #[test]
    fn create_required_directories_builds_expected_structure() {
        let root_dir = unique_test_root();
        let paths = LauncherPaths::new(&root_dir);

        paths
            .create_required_directories()
            .expect("directory initialization should succeed");

        assert!(paths.root_dir.exists(), "root directory should exist");
        assert!(
            paths.mods_cache_dir.exists(),
            "mods cache directory should exist"
        );
        assert!(
            paths.configs_cache_dir.exists(),
            "configs cache directory should exist"
        );
        assert!(
            paths.modlists_dir.exists(),
            "mod-lists directory should exist"
        );
        assert!(
            paths.java_runtimes_dir.exists(),
            "java runtimes directory should exist"
        );

        fs::remove_dir_all(&root_dir).expect("temporary root directory should be removable");
    }

    #[test]
    fn create_required_directories_is_idempotent() {
        let root_dir = unique_test_root();
        let paths = LauncherPaths::new(&root_dir);

        paths
            .create_required_directories()
            .expect("first directory initialization should succeed");
        paths
            .create_required_directories()
            .expect("second directory initialization should also succeed");

        fs::remove_dir_all(&root_dir).expect("temporary root directory should be removable");
    }

    #[test]
    fn database_path_points_to_root_database_file() {
        let root_dir = unique_test_root();
        let paths = LauncherPaths::new(&root_dir);

        assert_eq!(paths.database_path(), root_dir.join("launcher_data.db"));
    }
}
