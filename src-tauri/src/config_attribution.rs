use std::fs::{self, OpenOptions};
use std::io::{BufRead, BufReader, Write};
use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use rusqlite::{params, Connection};
use serde::{Deserialize, Serialize};

pub const JAVA_AGENT_OUTPUT_PATH_PROPERTY: &str = "cubic.agent.output.path";
pub const JAVA_AGENT_MODS_CACHE_DIR_PROPERTY: &str = "cubic.agent.mods.cache.dir";

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct ConfigAttributionEvent {
    pub config_path: String,
    pub jar_filename: String,
    pub source_class: Option<String>,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct ConfigAttributionLaunchConfig {
    pub agent_jar_path: PathBuf,
    pub output_file_path: PathBuf,
    pub mods_cache_dir: PathBuf,
}

impl ConfigAttributionLaunchConfig {
    pub fn to_jvm_args(&self) -> Vec<String> {
        vec![
            format!("-javaagent:{}", self.agent_jar_path.display()),
            format!(
                "-D{}={}",
                JAVA_AGENT_OUTPUT_PATH_PROPERTY,
                self.output_file_path.display()
            ),
            format!(
                "-D{}={}",
                JAVA_AGENT_MODS_CACHE_DIR_PROPERTY,
                self.mods_cache_dir.display()
            ),
        ]
    }
}

pub fn read_events_from_ndjson(path: &Path) -> Result<Vec<ConfigAttributionEvent>> {
    if !path.exists() {
        return Ok(Vec::new());
    }

    let file = fs::File::open(path)
        .with_context(|| format!("failed to open attribution file at {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut events = Vec::new();

    for (line_number, line) in reader.lines().enumerate() {
        let line = line.with_context(|| {
            format!(
                "failed to read line {} from attribution file {}",
                line_number + 1,
                path.display()
            )
        })?;

        if line.trim().is_empty() {
            continue;
        }

        let event = serde_json::from_str::<ConfigAttributionEvent>(&line).with_context(|| {
            format!(
                "failed to parse attribution event on line {} in {}",
                line_number + 1,
                path.display()
            )
        })?;

        events.push(event);
    }

    Ok(events)
}

pub fn append_events_to_ndjson(path: &Path, events: &[ConfigAttributionEvent]) -> Result<()> {
    if let Some(parent) = path.parent() {
        fs::create_dir_all(parent).with_context(|| {
            format!(
                "failed to create parent directories for attribution file {}",
                path.display()
            )
        })?;
    }

    let mut file = OpenOptions::new()
        .create(true)
        .append(true)
        .open(path)
        .with_context(|| format!("failed to open attribution file at {}", path.display()))?;

    for event in events {
        let line = serde_json::to_string(event)
            .with_context(|| "failed to serialize config attribution event".to_string())?;
        writeln!(file, "{line}")
            .with_context(|| format!("failed to append attribution event to {}", path.display()))?;
    }

    Ok(())
}

pub fn persist_events(connection: &Connection, events: &[ConfigAttributionEvent]) -> Result<()> {
    for event in events {
        connection.execute(
            r#"
            INSERT INTO config_attribution (
                config_path,
                jar_filename,
                source_class
            ) VALUES (?1, ?2, ?3)
            ON CONFLICT(config_path) DO UPDATE SET
                jar_filename = excluded.jar_filename,
                source_class = excluded.source_class,
                timestamp = CURRENT_TIMESTAMP
            "#,
            params![&event.config_path, &event.jar_filename, &event.source_class],
        )?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::database::initialize_database;

    use super::{
        append_events_to_ndjson, persist_events, read_events_from_ndjson, ConfigAttributionEvent,
        ConfigAttributionLaunchConfig, JAVA_AGENT_MODS_CACHE_DIR_PROPERTY,
        JAVA_AGENT_OUTPUT_PATH_PROPERTY,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!(
            "cubic-launcher-config-attribution-test-{timestamp}"
        ))
    }

    #[test]
    fn launch_config_builds_expected_jvm_args() {
        let config = ConfigAttributionLaunchConfig {
            agent_jar_path: PathBuf::from("java-agent/build/libs/config-agent.jar"),
            output_file_path: PathBuf::from("temp/config-attribution.ndjson"),
            mods_cache_dir: PathBuf::from("cache/mods"),
        };

        let args = config.to_jvm_args();

        assert_eq!(args[0], "-javaagent:java-agent/build/libs/config-agent.jar");
        assert_eq!(
            args[1],
            format!(
                "-D{}=temp/config-attribution.ndjson",
                JAVA_AGENT_OUTPUT_PATH_PROPERTY
            )
        );
        assert_eq!(
            args[2],
            format!("-D{}=cache/mods", JAVA_AGENT_MODS_CACHE_DIR_PROPERTY)
        );
    }

    #[test]
    fn ndjson_roundtrip_preserves_events() {
        let root_dir = unique_test_root();
        let file_path = root_dir.join("config-attribution.ndjson");
        let events = vec![
            ConfigAttributionEvent {
                config_path: "config/sodium-options.json".into(),
                jar_filename: "sodium.jar".into(),
                source_class: Some("me.jellysquid.mods.sodium.client.SodiumClientMod".into()),
            },
            ConfigAttributionEvent {
                config_path: "config/create-client.toml".into(),
                jar_filename: "create.jar".into(),
                source_class: None,
            },
        ];

        append_events_to_ndjson(&file_path, &events).expect("events should append");
        let reloaded = read_events_from_ndjson(&file_path).expect("events should reload");

        assert_eq!(reloaded, events);

        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }

    #[test]
    fn persist_events_upserts_config_attribution_rows() {
        let root_dir = unique_test_root();
        let database_path = root_dir.join("launcher_data.db");

        initialize_database(&database_path).expect("database should initialize");
        let connection = Connection::open(&database_path).expect("database should open");

        persist_events(
            &connection,
            &[
                ConfigAttributionEvent {
                    config_path: "config/sodium-options.json".into(),
                    jar_filename: "sodium.jar".into(),
                    source_class: Some("first.Source".into()),
                },
                ConfigAttributionEvent {
                    config_path: "config/sodium-options.json".into(),
                    jar_filename: "sodium-new.jar".into(),
                    source_class: Some("second.Source".into()),
                },
            ],
        )
        .expect("events should persist");

        let row = {
            let mut statement = connection
                .prepare(
                    "SELECT jar_filename, source_class FROM config_attribution WHERE config_path = ?1",
                )
                .expect("query should prepare");
            statement
                .query_row(["config/sodium-options.json"], |row| {
                    Ok((row.get::<_, String>(0)?, row.get::<_, Option<String>>(1)?))
                })
                .expect("row should exist")
        };

        assert_eq!(row.0, "sodium-new.jar");
        assert_eq!(row.1.as_deref(), Some("second.Source"));

        drop(connection);
        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }
}
