use super::*;
use crate::auth::storage::AuthDotJson;
use crate::auth::storage::AuthStorageBackend;
use crate::auth::storage::FileAuthStorage;
use codex_config::types::AuthCredentialsStoreMode;
use codex_keyring_store::KeyringStore;
use codex_keyring_store::tests::MockKeyringStore;
use codex_protocol::auth::AuthMode;
use pretty_assertions::assert_eq;
use std::collections::HashMap;
use std::fs;
use std::sync::Arc;
use std::sync::Mutex;
use tempfile::tempdir;

fn api_key_auth(secret: &str) -> AuthDotJson {
    AuthDotJson {
        auth_mode: Some(AuthMode::ApiKey),
        openai_api_key: Some(secret.to_string()),
        tokens: None,
        last_refresh: None,
        agent_identity: None,
        personal_access_token: None,
        bedrock_api_key: None,
        bedrock_access_keys: None,
    }
}

fn storage(
    home: &std::path::Path,
    mode: AuthCredentialsStoreMode,
    namespace: AuthStorageNamespace,
) -> AuthStorage {
    AuthStorage::new(home.into(), mode, AuthKeyringBackendKind::Direct, namespace)
}

fn keyring_storage(
    home: &std::path::Path,
    backend: AuthKeyringBackendKind,
    namespace: AuthStorageNamespace,
    keyring: Arc<dyn KeyringStore>,
) -> AuthStorage {
    AuthStorage::new_with_keyring_store(
        home.into(),
        AuthCredentialsStoreMode::Keyring,
        backend,
        namespace,
        keyring,
    )
}

#[derive(Debug, Default)]
struct ServiceKeyring(Mutex<HashMap<(String, String), String>>);

impl KeyringStore for ServiceKeyring {
    fn load(
        &self,
        service: &str,
        account: &str,
    ) -> Result<Option<String>, codex_keyring_store::CredentialStoreError> {
        Ok(self
            .0
            .lock()
            .expect("service keyring")
            .get(&(service.to_string(), account.to_string()))
            .cloned())
    }

    fn save(
        &self,
        service: &str,
        account: &str,
        value: &str,
    ) -> Result<(), codex_keyring_store::CredentialStoreError> {
        self.0.lock().expect("service keyring").insert(
            (service.to_string(), account.to_string()),
            value.to_string(),
        );
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
            .expect("service keyring")
            .remove(&(service.to_string(), account.to_string()))
            .is_some())
    }
}

#[test]
fn importing_file_auth_writes_only_the_destination_and_redacts_outcome() -> anyhow::Result<()> {
    let source_home = tempdir()?;
    let destination_home = tempdir()?;
    let original = api_key_auth("stock-access-token");
    FileAuthStorage::new(source_home.path().into()).save(&original)?;
    let source = storage(
        source_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    let destination = storage(
        destination_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );

    let outcome = import_auth_record(&source, &destination)?;

    assert_eq!(outcome, AuthImportOutcome::Imported);
    assert_eq!(source.read_for_test()?, Some(original.clone()));
    assert_eq!(destination.read_for_test()?, Some(original));
    assert!(!format!("{outcome:?}").contains("stock-access-token"));
    Ok(())
}

#[test]
fn direct_keyring_import_uses_independent_services() -> anyhow::Result<()> {
    let home = tempdir()?;
    let keyring = Arc::new(ServiceKeyring::default());
    let source = keyring_storage(
        home.path(),
        AuthKeyringBackendKind::Direct,
        AuthStorageNamespace::Codex,
        keyring.clone(),
    );
    let destination = keyring_storage(
        home.path(),
        AuthKeyringBackendKind::Direct,
        AuthStorageNamespace::Moedex,
        keyring.clone(),
    );
    source.write_for_test(&api_key_auth("direct-secret"))?;

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::Imported
    );
    let key = crate::auth::storage::compute_store_key(home.path())?;
    assert!(keyring.load("Codex Auth", &key)?.is_some());
    assert!(keyring.load("Moedex Auth", &key)?.is_some());
    destination.delete_for_test()?;
    assert!(keyring.load("Codex Auth", &key)?.is_some());
    assert!(keyring.load("Moedex Auth", &key)?.is_none());
    Ok(())
}

#[test]
fn encrypted_import_uses_independent_files_and_logout_isolated() -> anyhow::Result<()> {
    let home = tempdir()?;
    let keyring = Arc::new(ServiceKeyring::default());
    let source = keyring_storage(
        home.path(),
        AuthKeyringBackendKind::Secrets,
        AuthStorageNamespace::Codex,
        keyring.clone(),
    );
    let destination = keyring_storage(
        home.path(),
        AuthKeyringBackendKind::Secrets,
        AuthStorageNamespace::Moedex,
        keyring,
    );
    let original = api_key_auth("encrypted-secret");
    source.write_for_test(&original)?;

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::Imported
    );
    assert!(home.path().join("secrets/codex_auth.age").is_file());
    assert!(home.path().join("secrets/moedex_auth.age").is_file());
    destination.delete_for_test()?;
    assert_eq!(source.read_for_test()?, Some(original));
    assert_eq!(destination.read_for_test()?, None);
    Ok(())
}

#[test]
fn destination_backend_failure_returns_only_sanitized_status() -> anyhow::Result<()> {
    let source_home = tempdir()?;
    let destination_home = tempdir()?;
    FileAuthStorage::new(source_home.path().into()).save(&api_key_auth("never-render-me"))?;
    let source = storage(
        source_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    let keyring = Arc::new(MockKeyringStore::default());
    keyring.set_error(
        &crate::auth::storage::compute_store_key(destination_home.path())?,
        keyring::Error::Invalid("unavailable".into(), "save".into()),
    );
    let destination = keyring_storage(
        destination_home.path(),
        AuthKeyringBackendKind::Direct,
        AuthStorageNamespace::Moedex,
        keyring,
    );

    let outcome = import_auth_record(&source, &destination)?;

    assert_eq!(outcome, AuthImportOutcome::Failed);
    assert!(!format!("{outcome:?}").contains("never-render-me"));
    assert_eq!(
        source.read_for_test()?,
        Some(api_key_auth("never-render-me"))
    );
    Ok(())
}

#[test]
fn strict_auto_import_does_not_use_stale_file_after_keyring_failure() -> anyhow::Result<()> {
    let source_home = tempdir()?;
    let destination_home = tempdir()?;
    let keyring = Arc::new(MockKeyringStore::default());
    FileAuthStorage::new(source_home.path().into()).save(&api_key_auth("stale-file-secret"))?;
    let account = codex_secrets::compute_keyring_account(source_home.path());
    let manager = codex_secrets::SecretsManager::new_with_keyring_store_and_namespace(
        source_home.path().into(),
        codex_secrets::SecretsBackendKind::Local,
        keyring.clone(),
        codex_secrets::LocalSecretsNamespace::CodexAuth,
    );
    let name = codex_secrets::SecretName::new("CODEX_AUTH")?;
    manager.set(
        &codex_secrets::SecretScope::Global,
        &name,
        &serde_json::to_string(&api_key_auth("encrypted-secret"))?,
    )?;
    keyring.set_error(
        &account,
        keyring::Error::Invalid("failure".into(), "load".into()),
    );
    let source = AuthStorage::new_with_keyring_store(
        source_home.path().into(),
        AuthCredentialsStoreMode::Auto,
        AuthKeyringBackendKind::Secrets,
        AuthStorageNamespace::Codex,
        keyring,
    );
    let destination = storage(
        destination_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::SignInRequired
    );
    assert_eq!(destination.read_for_test()?, None);
    Ok(())
}

#[test]
fn ephemeral_and_inaccessible_stores_return_sanitized_outcomes() -> anyhow::Result<()> {
    let source_home = tempdir()?;
    let destination_home = tempdir()?;
    let destination = storage(
        destination_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );
    let ephemeral = storage(
        source_home.path(),
        AuthCredentialsStoreMode::Ephemeral,
        AuthStorageNamespace::Codex,
    );
    ephemeral.write_for_test(&api_key_auth("ephemeral-secret"))?;
    assert_eq!(
        import_auth_record(&ephemeral, &destination)?,
        AuthImportOutcome::SignInRequired
    );

    fs::write(source_home.path().join("auth.json"), "{malformed-secret")?;
    let file = storage(
        source_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    assert_eq!(
        import_auth_record(&file, &destination)?,
        AuthImportOutcome::SignInRequired
    );
    FileAuthStorage::new(source_home.path().into()).save(&api_key_auth("source-secret"))?;
    let destination = storage(
        destination_home.path(),
        AuthCredentialsStoreMode::Ephemeral,
        AuthStorageNamespace::Moedex,
    );

    assert_eq!(
        import_auth_record(&file, &destination)?,
        AuthImportOutcome::Failed
    );
    assert_eq!(destination.read_for_test()?, None);
    Ok(())
}

#[cfg(unix)]
#[test]
fn file_import_tightens_existing_destination_permissions() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let source_home = tempdir()?;
    let destination_home = tempdir()?;
    FileAuthStorage::new(source_home.path().into()).save(&api_key_auth("secret"))?;
    let destination_file = destination_home.path().join("auth.json");
    fs::write(&destination_file, "{}")?;
    fs::set_permissions(&destination_file, fs::Permissions::from_mode(0o644))?;
    let source = storage(
        source_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    let destination = storage(
        destination_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::Imported
    );
    assert_eq!(
        fs::metadata(destination_file)?.permissions().mode() & 0o777,
        0o600
    );
    Ok(())
}
