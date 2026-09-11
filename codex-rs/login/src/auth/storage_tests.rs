use super::*;
use crate::token_data::IdTokenInfo;
use anyhow::Context;
use base64::Engine;
use codex_secrets::LocalSecretsNamespace;
use codex_secrets::SecretScope;
use codex_secrets::SecretsBackendKind;
use codex_secrets::SecretsManager;
use codex_secrets::compute_keyring_account;
use pretty_assertions::assert_eq;
use serde_json::json;
use tempfile::tempdir;

use codex_keyring_store::tests::MockKeyringStore;
use keyring::Error as KeyringError;

#[tokio::test]
async fn file_storage_load_returns_auth_dot_json() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some("test-key".to_string()),
        tokens: None,
        last_refresh: Some(Utc::now()),
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage
        .save(&auth_dot_json)
        .context("failed to save auth file")?;

    let loaded = storage.load().context("failed to load auth file")?;
    assert_eq!(Some(auth_dot_json), loaded);
    Ok(())
}

#[tokio::test]
async fn file_storage_save_persists_auth_dot_json() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some("test-key".to_string()),
        tokens: None,
        last_refresh: Some(Utc::now()),
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    let file = get_auth_file(codex_home.path());
    storage
        .save(&auth_dot_json)
        .context("failed to save auth file")?;

    let same_auth_dot_json = storage
        .try_read_auth_json(&file)
        .context("failed to read auth file after save")?;
    assert_eq!(auth_dot_json, same_auth_dot_json);
    Ok(())
}

#[test]
fn file_storage_replace_failure_preserves_previous_auth_and_removes_plaintext_temp()
-> anyhow::Result<()> {
    let home = tempdir()?;
    let storage = FileAuthStorage::new(home.path().into());
    let original = auth_with_prefix("original");
    storage.save(&original)?;

    let error = storage
        .save_with_atomic_replace_for_test(&auth_with_prefix("replacement"), |_temp, _target| {
            Err(std::io::Error::other("injected replace failure"))
        })
        .expect_err("replace must fail");

    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert_eq!(storage.load()?, Some(original));
    assert_eq!(auth_residue_count(home.path())?, 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_backup_cleanup_failure_rolls_back_after_replacement() -> anyhow::Result<()> {
    use std::cell::Cell;

    let home = tempdir()?;
    let storage = FileAuthStorage::new(home.path().into());
    let original = auth_with_prefix("original");
    storage.save(&original)?;
    let rejected_cleanup = Cell::new(false);

    let error = storage
        .save_with_cleanup_ops_for_test(
            &auth_with_prefix("replacement"),
            |path| {
                if path
                    .extension()
                    .is_some_and(|extension| extension == "backup")
                {
                    rejected_cleanup.set(true);
                    return Err(std::io::Error::other("injected backup cleanup failure"));
                }
                std::fs::remove_file(path)
            },
            sync_parent_directory,
        )
        .expect_err("cleanup failure must roll back");

    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(rejected_cleanup.get());
    assert_eq!(storage.load()?, Some(original));
    assert_eq!(auth_residue_count(home.path())?, 0);
    Ok(())
}

#[cfg(unix)]
#[test]
fn unix_success_syncs_parent_after_plaintext_backup_removal() -> anyhow::Result<()> {
    let home = tempdir()?;
    let storage = FileAuthStorage::new(home.path().into());
    storage.save(&auth_with_prefix("original"))?;
    let replacement = auth_with_prefix("replacement");
    let mut sync_count = 0;

    storage.save_with_cleanup_ops_for_test(
        &replacement,
        |path| std::fs::remove_file(path),
        |parent| {
            sync_count += 1;
            sync_parent_directory(parent)
        },
    )?;

    assert_eq!(sync_count, 2);
    assert_eq!(storage.load()?, Some(replacement));
    assert_eq!(auth_residue_count(home.path())?, 0);
    Ok(())
}

#[test]
fn windows_backup_cleanup_failure_abstraction_rolls_back_coherently() -> anyhow::Result<()> {
    use std::cell::Cell;

    let home = tempdir()?;
    let target = home.path().join("auth.json");
    let temp = home.path().join(".auth.json.test.tmp");
    std::fs::write(&target, "original")?;
    std::fs::write(&temp, "replacement")?;
    let rejected_cleanup = Cell::new(false);

    let error = replace_auth_file_windows_with_ops(
        &temp,
        &target,
        |from, to| std::fs::rename(from, to),
        |path| {
            if path.to_string_lossy().contains("auth-backup") && !rejected_cleanup.replace(true) {
                return Err(std::io::Error::other("injected backup cleanup failure"));
            }
            std::fs::remove_file(path)
        },
    )
    .expect_err("cleanup failure must roll back");

    assert_eq!(error.kind(), std::io::ErrorKind::Other);
    assert!(rejected_cleanup.get());
    assert_eq!(std::fs::read_to_string(target)?, "original");
    assert_eq!(auth_residue_count(home.path())?, 0);
    Ok(())
}

fn auth_residue_count(home: &Path) -> std::io::Result<usize> {
    Ok(std::fs::read_dir(home)?
        .filter_map(Result::ok)
        .filter(|entry| {
            let name = entry.file_name();
            let name = name.to_string_lossy();
            name.ends_with(".tmp") || name.ends_with(".backup") || name.contains("auth-backup")
        })
        .count())
}

#[tokio::test]
async fn file_storage_round_trips_agent_identity_auth() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let agent_identity = jwt_with_payload(json!({
        "agent_runtime_id": "agent-runtime-id",
        "agent_private_key": "private-key",
        "account_id": "account-id",
        "chatgpt_user_id": "user-id",
        "email": "user@example.com",
        "plan_type": "pro",
        "chatgpt_account_is_fedramp": false,
    }));
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::AgentIdentity),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
        agent_identity: Some(AgentIdentityStorage::Jwt(agent_identity)),
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage.save(&auth_dot_json)?;

    let loaded = storage.load()?;
    assert_eq!(Some(auth_dot_json), loaded);
    Ok(())
}

#[tokio::test]
async fn file_storage_round_trips_registered_agent_identity_auth() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let record = AgentIdentityAuthRecord {
        agent_runtime_id: "agent-runtime-id".to_string(),
        agent_private_key: "private-key".to_string(),
        account_id: "account-id".to_string(),
        chatgpt_user_id: "user-id".to_string(),
        email: Some("user@example.com".to_string()),
        plan_type: AccountPlanType::Pro,
        chatgpt_account_is_fedramp: false,
        task_id: Some("task-id".to_string()),
    };
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::Chatgpt),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
        agent_identity: Some(AgentIdentityStorage::Record(record)),
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage.save(&auth_dot_json)?;

    let loaded = storage.load()?;
    assert_eq!(Some(auth_dot_json), loaded);
    Ok(())
}

#[tokio::test]
async fn file_storage_loads_empty_agent_identity_email_as_none() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let auth_file = get_auth_file(codex_home.path());
    std::fs::write(
        &auth_file,
        serde_json::to_string_pretty(&json!({
            "auth_mode": "chatgpt",
            "agent_identity": {
                "agent_runtime_id": "agent-runtime-id",
                "agent_private_key": "private-key",
                "account_id": "account-id",
                "chatgpt_user_id": "user-id",
                "email": "",
                "plan_type": "pro",
                "chatgpt_account_is_fedramp": false,
            },
        }))?,
    )?;

    let loaded = storage.load()?;

    assert_eq!(
        loaded,
        Some(AuthDotJson {
            auth_mode: Some(AuthMode::Chatgpt),
            openai_api_key: None,
            tokens: None,
            last_refresh: None,
            agent_identity: Some(AgentIdentityStorage::Record(AgentIdentityAuthRecord {
                agent_runtime_id: "agent-runtime-id".to_string(),
                agent_private_key: "private-key".to_string(),
                account_id: "account-id".to_string(),
                chatgpt_user_id: "user-id".to_string(),
                email: None,
                plan_type: AccountPlanType::Pro,
                chatgpt_account_is_fedramp: false,
                task_id: None,
            })),
            personal_access_token: None,
            bedrock_api_key: None,
            bedrock_access_keys: None,
        })
    );
    Ok(())
}

#[tokio::test]
async fn file_storage_writes_missing_agent_identity_email_as_empty_string() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::Chatgpt),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
        agent_identity: Some(AgentIdentityStorage::Record(AgentIdentityAuthRecord {
            agent_runtime_id: "agent-runtime-id".to_string(),
            agent_private_key: "private-key".to_string(),
            account_id: "account-id".to_string(),
            chatgpt_user_id: "user-id".to_string(),
            email: None,
            plan_type: AccountPlanType::Pro,
            chatgpt_account_is_fedramp: false,
            task_id: None,
        })),
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage.save(&auth_dot_json)?;

    let auth_file = get_auth_file(codex_home.path());
    let saved: serde_json::Value = serde_json::from_str(&std::fs::read_to_string(auth_file)?)?;
    assert_eq!(saved["agent_identity"]["email"], "");
    assert_eq!(storage.load()?, Some(auth_dot_json));
    Ok(())
}

#[tokio::test]
async fn file_storage_round_trips_personal_access_token_auth() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::PersonalAccessToken),
        openai_api_key: None,
        tokens: None,
        last_refresh: None,
        agent_identity: None,
        personal_access_token: Some("at-example".to_string()),
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage.save(&auth_dot_json)?;

    let loaded = storage.load()?;
    assert_eq!(Some(auth_dot_json), loaded);
    Ok(())
}

#[tokio::test]
async fn file_storage_loads_agent_identity_as_jwt() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let storage = FileAuthStorage::new(codex_home.path().to_path_buf());
    let agent_identity_jwt = jwt_with_payload(json!({
        "agent_runtime_id": "agent-runtime-id",
        "agent_private_key": "private-key",
        "account_id": "account-id",
        "chatgpt_user_id": "user-id",
        "email": "user@example.com",
        "plan_type": "pro",
        "chatgpt_account_is_fedramp": false,
    }));
    let auth_file = get_auth_file(codex_home.path());
    std::fs::write(
        &auth_file,
        serde_json::to_string_pretty(&json!({
            "auth_mode": "agentIdentity",
            "agent_identity": agent_identity_jwt,
        }))?,
    )?;

    let loaded = storage.load()?;

    assert_eq!(
        loaded.expect("auth should load").agent_identity,
        Some(AgentIdentityStorage::Jwt(agent_identity_jwt))
    );
    Ok(())
}

#[test]
fn file_storage_delete_removes_auth_file() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some("sk-test-key".to_string()),
        tokens: None,
        last_refresh: None,
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };
    let storage = create_auth_storage(
        dir.path().to_path_buf(),
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::default(),
    );
    storage.save(&auth_dot_json)?;
    assert!(dir.path().join("auth.json").exists());
    let storage = FileAuthStorage::new(dir.path().to_path_buf());
    let removed = storage.delete()?;
    assert!(removed);
    assert!(!dir.path().join("auth.json").exists());
    Ok(())
}

#[test]
fn ephemeral_storage_save_load_delete_is_in_memory_only() -> anyhow::Result<()> {
    let dir = tempdir()?;
    let storage = create_auth_storage(
        dir.path().to_path_buf(),
        AuthCredentialsStoreMode::Ephemeral,
        AuthKeyringBackendKind::default(),
    );
    let auth_dot_json = AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some("sk-ephemeral".to_string()),
        tokens: None,
        last_refresh: Some(Utc::now()),
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage.save(&auth_dot_json)?;
    let loaded = storage.load()?;
    assert_eq!(Some(auth_dot_json), loaded);

    let removed = storage.delete()?;
    assert!(removed);
    let loaded = storage.load()?;
    assert_eq!(None, loaded);
    assert!(!get_auth_file(dir.path()).exists());
    Ok(())
}

fn seed_secrets_backend_and_fallback_auth_file_for_delete(
    mock_keyring: &MockKeyringStore,
    codex_home: &Path,
    auth: &AuthDotJson,
) -> anyhow::Result<PathBuf> {
    let manager = SecretsManager::new_with_keyring_store_and_namespace(
        codex_home.to_path_buf(),
        SecretsBackendKind::Local,
        Arc::new(mock_keyring.clone()),
        LocalSecretsNamespace::MoedexAuth,
    );
    manager.set(
        &SecretScope::Global,
        &CODEX_AUTH_SECRET_NAME,
        &serde_json::to_string(auth)?,
    )?;
    let auth_file = get_auth_file(codex_home);
    std::fs::write(&auth_file, "stale")?;
    Ok(auth_file)
}

fn seed_secrets_backend_with_auth(
    mock_keyring: &MockKeyringStore,
    codex_home: &Path,
    auth: &AuthDotJson,
) -> anyhow::Result<()> {
    let manager = SecretsManager::new_with_keyring_store_and_namespace(
        codex_home.to_path_buf(),
        SecretsBackendKind::Local,
        Arc::new(mock_keyring.clone()),
        LocalSecretsNamespace::MoedexAuth,
    );
    manager.set(
        &SecretScope::Global,
        &CODEX_AUTH_SECRET_NAME,
        &serde_json::to_string(auth)?,
    )?;
    Ok(())
}

fn assert_keyring_saved_auth_and_removed_fallback(
    mock_keyring: &MockKeyringStore,
    codex_home: &Path,
    expected: &AuthDotJson,
) -> anyhow::Result<()> {
    let manager = SecretsManager::new_with_keyring_store_and_namespace(
        codex_home.to_path_buf(),
        SecretsBackendKind::Local,
        Arc::new(mock_keyring.clone()),
        LocalSecretsNamespace::MoedexAuth,
    );
    let saved_value = manager
        .get(&SecretScope::Global, &CODEX_AUTH_SECRET_NAME)?
        .context("encrypted auth entry should exist")?;
    let expected_serialized = serde_json::to_string(expected)?;
    assert_eq!(saved_value, expected_serialized);
    let old_key = compute_store_key(codex_home)?;
    assert!(
        mock_keyring.saved_value(&old_key).is_none(),
        "legacy keyring auth entry should not be used"
    );
    let secrets_key = compute_keyring_account(codex_home);
    assert!(
        mock_keyring.saved_value(&secrets_key).is_some(),
        "secrets backend should persist an encryption passphrase in the keyring"
    );
    assert!(encrypted_auth_file(codex_home).exists());
    let auth_file = get_auth_file(codex_home);
    assert!(
        !auth_file.exists(),
        "fallback auth.json should be removed after keyring save"
    );
    Ok(())
}

fn encrypted_auth_file(codex_home: &Path) -> PathBuf {
    codex_home.join("secrets").join("moedex_auth.age")
}

fn id_token_with_prefix(prefix: &str) -> IdTokenInfo {
    #[derive(Serialize)]
    struct Header {
        alg: &'static str,
        typ: &'static str,
    }

    let header = Header {
        alg: "none",
        typ: "JWT",
    };
    let payload = json!({
        "email": format!("{prefix}@example.com"),
        "https://api.openai.com/auth": {
            "chatgpt_account_id": format!("{prefix}-account"),
        },
    });
    let encode = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let header_b64 = encode(&serde_json::to_vec(&header).expect("serialize header"));
    let payload_b64 = encode(&serde_json::to_vec(&payload).expect("serialize payload"));
    let signature_b64 = encode(b"sig");
    let fake_jwt = format!("{header_b64}.{payload_b64}.{signature_b64}");

    crate::token_data::parse_chatgpt_jwt_claims(&fake_jwt).expect("fake JWT should parse")
}

fn auth_with_prefix(prefix: &str) -> AuthDotJson {
    AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some(format!("{prefix}-api-key")),
        tokens: Some(TokenData {
            id_token: id_token_with_prefix(prefix),
            access_token: format!("{prefix}-access"),
            refresh_token: format!("{prefix}-refresh"),
            account_id: Some(format!("{prefix}-account-id")),
        }),
        last_refresh: None,
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    }
}

fn jwt_with_payload(payload: serde_json::Value) -> String {
    let encode = |bytes: &[u8]| base64::engine::general_purpose::URL_SAFE_NO_PAD.encode(bytes);
    let header_b64 = encode(br#"{"alg":"EdDSA","typ":"JWT"}"#);
    let payload_b64 = encode(&serde_json::to_vec(&payload).expect("payload should serialize"));
    let signature_b64 = encode(b"sig");
    format!("{header_b64}.{payload_b64}.{signature_b64}")
}

#[test]
fn secrets_keyring_auth_storage_load_returns_deserialized_auth() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = SecretsKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    let expected = AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some("sk-test".to_string()),
        tokens: None,
        last_refresh: None,
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };
    seed_secrets_backend_with_auth(&mock_keyring, codex_home.path(), &expected)?;

    let loaded = storage.load()?;
    assert_eq!(Some(expected), loaded);
    Ok(())
}

#[test]
fn keyring_auth_storage_compute_store_key_for_home_directory() -> anyhow::Result<()> {
    let codex_home = PathBuf::from("~/.codex");

    let key = compute_store_key(codex_home.as_path())?;

    assert_eq!(key, "cli|940db7b1d0e4eb40");
    Ok(())
}

#[test]
fn direct_keyring_auth_storage_saves_legacy_keyring_entry() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = DirectKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    let auth_file = get_auth_file(codex_home.path());
    std::fs::write(&auth_file, "stale")?;
    let auth = auth_with_prefix("direct");

    storage.save(&auth)?;

    let legacy_key = compute_store_key(codex_home.path())?;
    let saved_value = mock_keyring
        .saved_value(&legacy_key)
        .context("direct keyring auth entry should exist")?;
    assert_eq!(saved_value, serde_json::to_string(&auth)?);
    assert!(!encrypted_auth_file(codex_home.path()).exists());
    assert!(
        !auth_file.exists(),
        "fallback auth.json should be removed after keyring save"
    );
    assert_eq!(storage.load()?, Some(auth));
    Ok(())
}

#[test]
fn direct_keyring_auth_storage_delete_removes_keyring_and_file() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = DirectKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    let auth = auth_with_prefix("direct-delete");
    storage.save(&auth)?;
    let auth_file = get_auth_file(codex_home.path());
    std::fs::write(&auth_file, "stale")?;

    let removed = storage.delete()?;

    assert!(removed, "delete should report removal");
    assert_eq!(storage.load()?, None, "keyring auth should be removed");
    assert!(
        mock_keyring
            .saved_value(&compute_store_key(codex_home.path())?)
            .is_none(),
        "legacy keyring auth entry should be removed"
    );
    assert!(
        !auth_file.exists(),
        "fallback auth.json should be removed after keyring delete"
    );
    assert!(!encrypted_auth_file(codex_home.path()).exists());
    Ok(())
}

#[test]
fn factory_uses_secrets_backend_only_when_requested() -> anyhow::Result<()> {
    let direct_home = tempdir()?;
    let direct_keyring = MockKeyringStore::default();
    let direct_storage = create_auth_storage_with_store(
        direct_home.path().to_path_buf(),
        AuthCredentialsStoreMode::Keyring,
        Arc::new(direct_keyring.clone()),
        AuthKeyringBackendKind::Direct,
    );
    let direct_auth = auth_with_prefix("factory-direct");
    direct_storage.save(&direct_auth)?;
    assert!(
        direct_keyring
            .saved_value(&compute_store_key(direct_home.path())?)
            .is_some()
    );
    assert!(!encrypted_auth_file(direct_home.path()).exists());

    let secrets_home = tempdir()?;
    let secrets_keyring = MockKeyringStore::default();
    let secrets_storage = create_auth_storage_with_store(
        secrets_home.path().to_path_buf(),
        AuthCredentialsStoreMode::Keyring,
        Arc::new(secrets_keyring.clone()),
        AuthKeyringBackendKind::Secrets,
    );
    let secrets_auth = auth_with_prefix("factory-secrets");
    secrets_storage.save(&secrets_auth)?;
    assert!(
        secrets_keyring
            .saved_value(&compute_keyring_account(secrets_home.path()))
            .is_some()
    );
    assert!(encrypted_auth_file(secrets_home.path()).exists());
    Ok(())
}

#[test]
fn secrets_keyring_auth_storage_save_persists_and_removes_fallback_file() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = SecretsKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    let auth_file = get_auth_file(codex_home.path());
    std::fs::write(&auth_file, "stale")?;
    let auth = AuthDotJson {
        auth_mode: Some(AuthMode::Chatgpt),
        openai_api_key: None,
        tokens: Some(TokenData {
            id_token: Default::default(),
            access_token: "access".to_string(),
            refresh_token: "refresh".to_string(),
            account_id: Some("account".to_string()),
        }),
        last_refresh: Some(Utc::now()),
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    };

    storage.save(&auth)?;

    assert_keyring_saved_auth_and_removed_fallback(&mock_keyring, codex_home.path(), &auth)?;
    Ok(())
}

#[test]
fn secrets_keyring_auth_storage_delete_removes_keyring_and_file() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = SecretsKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    let auth = auth_with_prefix("to-delete");
    let auth_file = seed_secrets_backend_and_fallback_auth_file_for_delete(
        &mock_keyring,
        codex_home.path(),
        &auth,
    )?;

    let removed = storage.delete()?;

    assert!(removed, "delete should report removal");
    assert_eq!(storage.load()?, None, "encrypted auth should be removed");
    assert!(
        !auth_file.exists(),
        "fallback auth.json should be removed after keyring delete"
    );
    Ok(())
}

#[test]
fn secrets_keyring_auth_storage_delete_removes_legacy_direct_keyring_entry() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let direct_storage = DirectKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    direct_storage.save(&auth_with_prefix("legacy-direct"))?;
    let storage = SecretsKeyringAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
    );
    let auth = auth_with_prefix("to-delete");
    let auth_file = seed_secrets_backend_and_fallback_auth_file_for_delete(
        &mock_keyring,
        codex_home.path(),
        &auth,
    )?;

    let removed = storage.delete()?;

    assert!(removed, "delete should report removal");
    assert_eq!(storage.load()?, None, "encrypted auth should be removed");
    assert_eq!(
        direct_storage.load()?,
        None,
        "legacy direct keyring auth should be removed"
    );
    assert!(
        !auth_file.exists(),
        "fallback auth.json should be removed after keyring delete"
    );
    Ok(())
}

#[test]
fn auto_auth_storage_load_prefers_keyring_value() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = AutoAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
        AuthKeyringBackendKind::Secrets,
    );
    let keyring_auth = auth_with_prefix("keyring");
    seed_secrets_backend_with_auth(&mock_keyring, codex_home.path(), &keyring_auth)?;

    let file_auth = auth_with_prefix("file");
    storage.file_storage.save(&file_auth)?;

    let loaded = storage.load()?;
    assert_eq!(loaded, Some(keyring_auth));
    Ok(())
}

#[test]
fn auto_auth_storage_load_uses_file_when_keyring_empty() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = AutoAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring),
        AuthKeyringBackendKind::Secrets,
    );

    let expected = auth_with_prefix("file-only");
    storage.file_storage.save(&expected)?;

    let loaded = storage.load()?;
    assert_eq!(loaded, Some(expected));
    Ok(())
}

#[test]
fn auto_auth_storage_load_falls_back_when_keyring_errors() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = AutoAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
        AuthKeyringBackendKind::Secrets,
    );
    let key = compute_keyring_account(codex_home.path());

    let encrypted = auth_with_prefix("encrypted");
    seed_secrets_backend_with_auth(&mock_keyring, codex_home.path(), &encrypted)?;
    mock_keyring.set_error(&key, KeyringError::Invalid("error".into(), "load".into()));

    let expected = auth_with_prefix("fallback");
    storage.file_storage.save(&expected)?;

    let loaded = storage.load()?;
    assert_eq!(loaded, Some(expected));
    Ok(())
}

#[test]
fn auto_auth_storage_save_prefers_keyring() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = AutoAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
        AuthKeyringBackendKind::Secrets,
    );
    let stale = auth_with_prefix("stale");
    storage.file_storage.save(&stale)?;

    let expected = auth_with_prefix("to-save");
    storage.save(&expected)?;

    assert_keyring_saved_auth_and_removed_fallback(&mock_keyring, codex_home.path(), &expected)?;
    Ok(())
}

#[test]
fn auto_auth_storage_save_falls_back_when_keyring_errors() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = AutoAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
        AuthKeyringBackendKind::Secrets,
    );
    let key = compute_keyring_account(codex_home.path());
    mock_keyring.set_error(&key, KeyringError::Invalid("error".into(), "save".into()));

    let auth = auth_with_prefix("fallback");
    storage.save(&auth)?;

    let auth_file = get_auth_file(codex_home.path());
    assert!(
        auth_file.exists(),
        "fallback auth.json should be created when keyring save fails"
    );
    let saved = storage
        .file_storage
        .load()?
        .context("fallback auth should exist")?;
    assert_eq!(saved, auth);
    assert!(
        mock_keyring.saved_value(&key).is_none(),
        "keyring should not contain value when save fails"
    );
    Ok(())
}

#[test]
fn auto_auth_storage_delete_removes_keyring_and_file() -> anyhow::Result<()> {
    let codex_home = tempdir()?;
    let mock_keyring = MockKeyringStore::default();
    let storage = AutoAuthStorage::new(
        codex_home.path().to_path_buf(),
        Arc::new(mock_keyring.clone()),
        AuthKeyringBackendKind::Secrets,
    );
    let auth = auth_with_prefix("to-delete");
    let auth_file = seed_secrets_backend_and_fallback_auth_file_for_delete(
        &mock_keyring,
        codex_home.path(),
        &auth,
    )?;

    let removed = storage.delete()?;

    assert!(removed, "delete should report removal");
    assert_eq!(storage.load()?, None, "encrypted auth should be removed");
    assert!(
        !auth_file.exists(),
        "fallback auth.json should be removed after delete"
    );
    Ok(())
}

#[test]
fn direct_auth_uses_moedex_service() -> anyhow::Result<()> {
    auth_uses_moedex_service_and_logout_preserves_stock(AuthKeyringBackendKind::Direct)
}
#[test]
fn encrypted_auth_uses_moedex_service() -> anyhow::Result<()> {
    auth_uses_moedex_service_and_logout_preserves_stock(AuthKeyringBackendKind::Secrets)
}
fn auth_uses_moedex_service_and_logout_preserves_stock(
    backend: AuthKeyringBackendKind,
) -> anyhow::Result<()> {
    #[derive(Debug, Default)]
    struct Services(Mutex<HashMap<(String, String), String>>);
    impl KeyringStore for Services {
        fn load(
            &self,
            service: &str,
            account: &str,
        ) -> Result<Option<String>, codex_keyring_store::CredentialStoreError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .get(&(service.into(), account.into()))
                .cloned())
        }
        fn save(
            &self,
            service: &str,
            account: &str,
            value: &str,
        ) -> Result<(), codex_keyring_store::CredentialStoreError> {
            self.0
                .lock()
                .unwrap()
                .insert((service.into(), account.into()), value.into());
            Ok(())
        }
        fn delete(
            &self,
            service: &str,
            account: &str,
        ) -> Result<bool, codex_keyring_store::CredentialStoreError> {
            Ok(self
                .0
                .lock()
                .unwrap()
                .remove(&(service.into(), account.into()))
                .is_some())
        }
    }
    let home = tempdir()?;
    let store = Arc::new(Services::default());
    let key = compute_store_key(home.path())?;
    let stock = serde_json::to_string(&auth_with_prefix("stock"))?;
    store.save("Codex Auth", &key, &stock)?;
    let stock_manager = SecretsManager::new_with_keyring_store_and_namespace(
        home.path().into(),
        SecretsBackendKind::Local,
        store.clone(),
        LocalSecretsNamespace::CodexAuth,
    );
    stock_manager.set(&SecretScope::Global, &CODEX_AUTH_SECRET_NAME, &stock)?;
    let stock_file = home.path().join("secrets/codex_auth.age");
    let stock_bytes = std::fs::read(&stock_file)?;
    let auth = auth_with_prefix("moedex");
    let storage = create_keyring_auth_storage(home.path().into(), store.clone(), backend);
    storage.save(&auth)?;
    let account = match backend {
        AuthKeyringBackendKind::Direct => key.clone(),
        AuthKeyringBackendKind::Secrets => compute_keyring_account(home.path()),
    };
    assert!(
        store.load("Moedex Auth", &account)?.is_some(),
        "Moedex service owns credentials"
    );
    assert_eq!(storage.load()?, Some(auth));
    storage.delete()?;
    assert_eq!(store.load("Codex Auth", &key)?, Some(stock.clone()));
    assert_eq!(std::fs::read(stock_file)?, stock_bytes);
    assert_eq!(
        stock_manager.get(&SecretScope::Global, &CODEX_AUTH_SECRET_NAME)?,
        Some(stock)
    );
    Ok(())
}

#[test]
fn auth_writer_rejects_live_stock_owner_without_overwriting_credentials() -> anyhow::Result<()> {
    let home = tempdir()?;
    let state = home.path().join("app-server-daemon");
    std::fs::create_dir(&state)?;
    let owner = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state.join("daemon.lock"))?;
    owner.lock()?;
    let auth = auth_with_prefix("stock");
    let original = serde_json::to_string(&auth)?;
    std::fs::write(get_auth_file(home.path()), &original)?;
    let storage = create_auth_storage(
        home.path().into(),
        AuthCredentialsStoreMode::File,
        AuthKeyringBackendKind::Direct,
    );
    assert_eq!(
        storage
            .save(&auth_with_prefix("moedex"))
            .expect_err("live owner")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        storage.delete().expect_err("live owner").kind(),
        std::io::ErrorKind::WouldBlock
    );
    assert_eq!(
        std::fs::read_to_string(get_auth_file(home.path()))?,
        original
    );
    Ok(())
}

#[test]
fn encrypted_auth_load_with_missing_key_never_mutates_keyring_under_stock_lock()
-> anyhow::Result<()> {
    let home = tempdir()?;
    let keyring = Arc::new(MockKeyringStore::default());
    let storage = SecretsKeyringAuthStorage::new(home.path().into(), keyring.clone());
    storage.save(&auth_with_prefix("existing"))?;
    let ciphertext = std::fs::read(encrypted_auth_file(home.path()))?;
    let account = compute_keyring_account(home.path());
    keyring.delete(KEYRING_SERVICE, &account)?;
    let stock = home.path().join("app-server-daemon");
    std::fs::create_dir(&stock)?;
    let owner = OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(stock.join("daemon.lock"))?;
    owner.lock()?;
    let guarded = GuardedAuthStorage {
        home: home.path().into(),
        backend: Arc::new(storage),
    };
    assert!(guarded.load().is_err());
    assert!(
        keyring.saved_value(&account).is_none(),
        "read must not persist a new key"
    );
    assert_eq!(std::fs::read(encrypted_auth_file(home.path()))?, ciphertext);
    Ok(())
}
