use anyhow::{Context, Result};
use rusqlite::Connection;
use serde_json::Value;
use uuid::Uuid;

use crate::token_storage::{EncryptedAccountsRepository, PlaintextAccountRecord, SecretStore};

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct OfflinePlayableAccount {
    pub microsoft_id: String,
    pub username: String,
    pub offline_uuid: String,
    pub profile_data: Option<String>,
}

pub struct OfflineAccountService<'connection, S> {
    encrypted_accounts: EncryptedAccountsRepository<'connection, S>,
}

impl<'connection, S: SecretStore> OfflineAccountService<'connection, S> {
    pub fn new(connection: &'connection Connection, secret_store: S) -> Self {
        Self {
            encrypted_accounts: EncryptedAccountsRepository::new(connection, secret_store),
        }
    }

    pub fn active_offline_account(&self) -> Result<Option<OfflinePlayableAccount>> {
        self.encrypted_accounts
            .load_active_account()?
            .map(build_offline_playable_account)
            .transpose()
    }
}

pub fn build_offline_playable_account(
    account: PlaintextAccountRecord,
) -> Result<OfflinePlayableAccount> {
    let username = resolve_cached_profile_username(
        account.profile_data.as_deref(),
        account.xbox_gamertag.as_deref(),
    )?
    .with_context(|| {
        format!(
            "account '{}' does not contain cached profile information for offline mode",
            account.microsoft_id
        )
    })?;

    Ok(OfflinePlayableAccount {
        microsoft_id: account.microsoft_id,
        offline_uuid: deterministic_offline_uuid(&username).to_string(),
        username,
        profile_data: account.profile_data,
    })
}

pub fn deterministic_offline_uuid(username: &str) -> Uuid {
    Uuid::new_v5(&offline_uuid_namespace(), username.as_bytes())
}

pub fn resolve_cached_profile_username(
    profile_data: Option<&str>,
    xbox_gamertag: Option<&str>,
) -> Result<Option<String>> {
    if let Some(profile_data) = profile_data {
        let parsed = serde_json::from_str::<Value>(profile_data)
            .with_context(|| "failed to parse cached account profile_data JSON".to_string())?;

        if let Some(username) = find_username_in_profile_json(&parsed) {
            return Ok(Some(username));
        }
    }

    Ok(xbox_gamertag
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToString::to_string))
}

fn find_username_in_profile_json(value: &Value) -> Option<String> {
    const USERNAME_KEYS: &[&str] = &["username", "name", "preferred_username", "gamertag"];

    match value {
        Value::Object(map) => {
            for key in USERNAME_KEYS {
                if let Some(username) = map
                    .get(*key)
                    .and_then(Value::as_str)
                    .map(str::trim)
                    .filter(|value| !value.is_empty())
                {
                    return Some(username.to_string());
                }
            }

            map.values().find_map(find_username_in_profile_json)
        }
        Value::Array(values) => values.iter().find_map(find_username_in_profile_json),
        _ => None,
    }
}

fn offline_uuid_namespace() -> Uuid {
    Uuid::from_u128(0x9dc8a124_4f68_4b12_9af6_5e7f2c9c1a01)
}

#[cfg(test)]
mod tests {
    use std::collections::HashMap;
    use std::env;
    use std::fs;
    use std::path::PathBuf;
    use std::sync::{Arc, Mutex};
    use std::time::{SystemTime, UNIX_EPOCH};

    use rusqlite::Connection;

    use crate::database::initialize_database;
    use crate::token_storage::{EncryptedAccountsRepository, PlaintextAccountRecord, SecretStore};

    use super::{
        build_offline_playable_account, deterministic_offline_uuid,
        resolve_cached_profile_username, OfflineAccountService,
    };

    fn unique_test_root() -> PathBuf {
        let timestamp = SystemTime::now()
            .duration_since(UNIX_EPOCH)
            .expect("system time before unix epoch")
            .as_nanos();

        env::temp_dir().join(format!("cubic-launcher-offline-account-test-{timestamp}"))
    }

    #[derive(Clone, Default)]
    struct MemorySecretStore {
        values: Arc<Mutex<HashMap<String, String>>>,
    }

    impl SecretStore for MemorySecretStore {
        fn get_secret(&self, key: &str) -> anyhow::Result<Option<String>> {
            Ok(self
                .values
                .lock()
                .expect("secret store mutex poisoned")
                .get(key)
                .cloned())
        }

        fn set_secret(&self, key: &str, secret: &str) -> anyhow::Result<()> {
            self.values
                .lock()
                .expect("secret store mutex poisoned")
                .insert(key.to_string(), secret.to_string());
            Ok(())
        }
    }

    #[test]
    fn deterministic_offline_uuid_is_stable_for_same_username() {
        let first = deterministic_offline_uuid("PlayerOne");
        let second = deterministic_offline_uuid("PlayerOne");
        let third = deterministic_offline_uuid("PlayerTwo");

        assert_eq!(first, second);
        assert_ne!(first, third);
    }

    #[test]
    fn resolves_username_from_cached_profile_json_before_gamertag() {
        let username = resolve_cached_profile_username(
            Some(r#"{"profile":{"name":"CachedPlayer"}}"#),
            Some("GamertagFallback"),
        )
        .expect("username should resolve");

        assert_eq!(username.as_deref(), Some("CachedPlayer"));
    }

    #[test]
    fn falls_back_to_gamertag_when_profile_json_has_no_username() {
        let username = resolve_cached_profile_username(
            Some(r#"{"profile":{"id":"abc"}}"#),
            Some("GamertagFallback"),
        )
        .expect("username should resolve");

        assert_eq!(username.as_deref(), Some("GamertagFallback"));
    }

    #[test]
    fn builds_offline_playable_account_with_deterministic_uuid() {
        let account = build_offline_playable_account(PlaintextAccountRecord {
            microsoft_id: "account-a".into(),
            xbox_gamertag: Some("PlayerA".into()),
            minecraft_uuid: Some("online-uuid".into()),
            access_token: Some("access-a".into()),
            refresh_token: Some("refresh-a".into()),
            profile_data: Some(r#"{"name":"CachedPlayerA"}"#.into()),
            is_active: true,
        })
        .expect("offline account should build");

        assert_eq!(account.microsoft_id, "account-a");
        assert_eq!(account.username, "CachedPlayerA");
        assert_eq!(
            account.offline_uuid,
            deterministic_offline_uuid("CachedPlayerA").to_string()
        );
    }

    #[test]
    fn active_offline_account_uses_cached_profile_from_encrypted_storage() {
        let root_dir = unique_test_root();
        let database_path = root_dir.join("launcher_data.db");

        initialize_database(&database_path).expect("database should initialize");
        let connection = Connection::open(&database_path).expect("database should open");
        let secret_store = MemorySecretStore::default();
        let encrypted_accounts =
            EncryptedAccountsRepository::new(&connection, secret_store.clone());
        encrypted_accounts
            .upsert_account(&PlaintextAccountRecord {
                microsoft_id: "account-a".into(),
                xbox_gamertag: Some("PlayerA".into()),
                minecraft_uuid: Some("online-uuid".into()),
                access_token: Some("access-a".into()),
                refresh_token: Some("refresh-a".into()),
                profile_data: Some(r#"{"profile":{"preferred_username":"OfflinePlayer"}}"#.into()),
                is_active: true,
            })
            .expect("account should store");

        let service = OfflineAccountService::new(&connection, secret_store);
        let offline_account = service
            .active_offline_account()
            .expect("offline account should load")
            .expect("offline account should exist");

        assert_eq!(offline_account.username, "OfflinePlayer");
        assert_eq!(
            offline_account.offline_uuid,
            deterministic_offline_uuid("OfflinePlayer").to_string()
        );

        drop(connection);
        fs::remove_dir_all(&root_dir).expect("temporary root should be removable");
    }
}
