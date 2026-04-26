use std::fs;
use std::path::Path;

use anyhow::Result;
use rusqlite::Connection;

pub const DATABASE_FILENAME: &str = "launcher_data.db";

const SCHEMA_SQL: &str = r#"
CREATE TABLE IF NOT EXISTS accounts (
    id                  INTEGER PRIMARY KEY AUTOINCREMENT,
    microsoft_id        TEXT NOT NULL UNIQUE,
    xbox_gamertag       TEXT,
    minecraft_uuid      TEXT,
    access_token_enc    BLOB,
    refresh_token_enc   BLOB,
    last_login          DATETIME,
    profile_data        TEXT,
    is_active           BOOLEAN DEFAULT FALSE
);

CREATE TABLE IF NOT EXISTS java_installations (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    path            TEXT NOT NULL UNIQUE,
    version         INTEGER NOT NULL,
    auto_detected   BOOLEAN DEFAULT TRUE,
    architecture    TEXT DEFAULT 'x64'
);

CREATE TABLE IF NOT EXISTS mod_cache (
    modrinth_project_id TEXT,
    modrinth_version_id TEXT,
    jar_filename        TEXT NOT NULL,
    mc_version          TEXT NOT NULL,
    mod_loader          TEXT NOT NULL,
    file_hash           TEXT,
    download_url        TEXT,
    is_local            BOOLEAN DEFAULT FALSE,
    PRIMARY KEY (modrinth_version_id)
);

CREATE TABLE IF NOT EXISTS modrinth_project_aliases (
    alias                TEXT PRIMARY KEY,
    canonical_project_id TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS dependencies (
    mod_parent_id    TEXT NOT NULL,
    dependency_id    TEXT NOT NULL,
    dep_type         TEXT NOT NULL DEFAULT 'required',
    specific_version TEXT,
    jar_filename     TEXT NOT NULL,
    PRIMARY KEY (mod_parent_id, dependency_id)
);

CREATE TABLE IF NOT EXISTS config_attribution (
    config_path     TEXT NOT NULL,
    jar_filename    TEXT NOT NULL,
    source_class    TEXT,
    timestamp       DATETIME DEFAULT CURRENT_TIMESTAMP,
    PRIMARY KEY (config_path)
);

CREATE TABLE IF NOT EXISTS global_settings (
    key     TEXT PRIMARY KEY,
    value   TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS modlist_settings (
    modlist_name TEXT NOT NULL,
    key          TEXT NOT NULL,
    value        TEXT NOT NULL,
    PRIMARY KEY (modlist_name, key)
);

CREATE TABLE IF NOT EXISTS modrinth_availability (
    project_id  TEXT NOT NULL,
    mc_version  TEXT NOT NULL,
    mod_loader  TEXT NOT NULL,
    available   BOOLEAN NOT NULL,
    PRIMARY KEY (project_id, mc_version, mod_loader)
);
"#;

pub fn initialize_database(database_path: &Path) -> Result<()> {
    if let Some(parent) = database_path.parent() {
        fs::create_dir_all(parent)?;
    }

    let connection = Connection::open(database_path)?;
    connection.execute_batch(SCHEMA_SQL)?;
    migrate_mod_cache_schema(&connection)?;

    Ok(())
}

fn migrate_mod_cache_schema(connection: &Connection) -> Result<()> {
    let table_sql: Option<String> = connection
        .query_row(
            "SELECT sql FROM sqlite_master WHERE type = 'table' AND name = 'mod_cache'",
            [],
            |row| row.get(0),
        )
        .ok();

    let Some(table_sql) = table_sql else {
        return Ok(());
    };

    if !table_sql
        .to_ascii_lowercase()
        .contains("jar_filename        text not null unique")
        && !table_sql
            .to_ascii_lowercase()
            .contains("jar_filename text not null unique")
    {
        return Ok(());
    }

    let transaction = connection.unchecked_transaction()?;
    transaction.execute("ALTER TABLE mod_cache RENAME TO mod_cache_old", [])?;
    transaction.execute_batch(
        r#"
        CREATE TABLE mod_cache (
            modrinth_project_id TEXT,
            modrinth_version_id TEXT,
            jar_filename        TEXT NOT NULL,
            mc_version          TEXT NOT NULL,
            mod_loader          TEXT NOT NULL,
            file_hash           TEXT,
            download_url        TEXT,
            is_local            BOOLEAN DEFAULT FALSE,
            PRIMARY KEY (modrinth_version_id)
        );

        INSERT INTO mod_cache (
            modrinth_project_id,
            modrinth_version_id,
            jar_filename,
            mc_version,
            mod_loader,
            file_hash,
            download_url,
            is_local
        )
        SELECT
            modrinth_project_id,
            modrinth_version_id,
            jar_filename,
            mc_version,
            mod_loader,
            file_hash,
            download_url,
            is_local
        FROM mod_cache_old;

        DROP TABLE mod_cache_old;
        "#,
    )?;
    transaction.commit()?;

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::{params, Connection};

    use super::{initialize_database, DATABASE_FILENAME};

    fn unique_test_database_path() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir()
            .join(format!("cubic-launcher-db-test-{timestamp}"))
            .join(DATABASE_FILENAME)
    }

    fn fetch_table_names(connection: &Connection) -> Vec<String> {
        let mut statement = connection
            .prepare("SELECT name FROM sqlite_master WHERE type = 'table'")
            .expect("failed to prepare table query");

        statement
            .query_map([], |row| row.get::<_, String>(0))
            .expect("failed to query table names")
            .collect::<rusqlite::Result<Vec<_>>>()
            .expect("failed to collect table names")
    }

    #[test]
    fn initialize_database_creates_all_required_tables() {
        let database_path = unique_test_database_path();

        initialize_database(&database_path).expect("database initialization should succeed");

        let connection = Connection::open(&database_path).expect("database should open");
        let table_names = fetch_table_names(&connection);

        for expected_table in [
            "accounts",
            "java_installations",
            "mod_cache",
            "modrinth_project_aliases",
            "dependencies",
            "config_attribution",
            "global_settings",
            "modlist_settings",
            "modrinth_availability",
        ] {
            assert!(
                table_names
                    .iter()
                    .any(|table_name| table_name == expected_table),
                "missing expected table: {expected_table}"
            );
        }

        drop(connection);
        fs::remove_file(&database_path).expect("database file should be removable");
        fs::remove_dir_all(
            database_path
                .parent()
                .expect("database should have a parent"),
        )
        .expect("temporary directory should be removable");
    }

    #[test]
    fn initialize_database_is_idempotent() {
        let database_path = unique_test_database_path();

        initialize_database(&database_path).expect("first initialization should succeed");
        initialize_database(&database_path).expect("second initialization should also succeed");

        fs::remove_file(&database_path).expect("database file should be removable");
        fs::remove_dir_all(
            database_path
                .parent()
                .expect("database should have a parent"),
        )
        .expect("temporary directory should be removable");
    }

    #[test]
    fn initialize_database_migrates_legacy_mod_cache_unique_filename_constraint() {
        let database_path = unique_test_database_path();
        let parent_dir = database_path
            .parent()
            .expect("database should have a parent")
            .to_path_buf();
        fs::create_dir_all(&parent_dir).expect("parent directory should exist");

        let connection = Connection::open(&database_path).expect("database should open");
        connection
            .execute_batch(
                r#"
                CREATE TABLE mod_cache (
                    modrinth_project_id TEXT,
                    modrinth_version_id TEXT,
                    jar_filename        TEXT NOT NULL UNIQUE,
                    mc_version          TEXT NOT NULL,
                    mod_loader          TEXT NOT NULL,
                    file_hash           TEXT,
                    download_url        TEXT,
                    is_local            BOOLEAN DEFAULT FALSE,
                    PRIMARY KEY (modrinth_version_id)
                );
                "#,
            )
            .expect("legacy mod_cache schema should create");
        drop(connection);

        initialize_database(&database_path).expect("migration should succeed");

        let connection = Connection::open(&database_path).expect("database should reopen");
        connection
            .execute(
                "INSERT INTO mod_cache (modrinth_project_id, modrinth_version_id, jar_filename, mc_version, mod_loader, file_hash, download_url, is_local) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params!["mod-a", "version-a", "shared.jar", "1.21.6", "fabric", Option::<String>::None, Option::<String>::None, false],
            )
            .expect("first insert should succeed");
        connection
            .execute(
                "INSERT INTO mod_cache (modrinth_project_id, modrinth_version_id, jar_filename, mc_version, mod_loader, file_hash, download_url, is_local) VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
                params!["mod-b", "version-b", "shared.jar", "1.21.6", "fabric", Option::<String>::None, Option::<String>::None, false],
            )
            .expect("second insert with same filename should succeed after migration");

        drop(connection);
        fs::remove_file(&database_path).expect("database file should be removable");
        fs::remove_dir_all(&parent_dir).expect("temporary directory should be removable");
    }
}
