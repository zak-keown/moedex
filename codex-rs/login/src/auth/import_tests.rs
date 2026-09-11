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
        keyring.clone(),
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
fn moedex_file_auth_in_a_shared_compatibility_home_never_touches_stock_auth() -> anyhow::Result<()>
{
    let shared_home = tempdir()?;
    let stock = storage(
        shared_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    let moedex = storage(
        shared_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );
    let stock_auth = api_key_auth("stock-secret");
    stock.write_for_test(&stock_auth)?;
    let stock_bytes = fs::read(shared_home.path().join("auth.json"))?;

    assert_eq!(moedex.read_for_test()?, None);
    moedex.write_for_test(&api_key_auth("moedex-secret"))?;
    assert_eq!(fs::read(shared_home.path().join("auth.json"))?, stock_bytes);
    assert!(shared_home.path().join("moedex-auth.json").is_file());

    assert!(moedex.delete_for_test()?);
    assert_eq!(fs::read(shared_home.path().join("auth.json"))?, stock_bytes);
    assert_eq!(stock.read_for_test()?, Some(stock_auth));
    Ok(())
}

#[test]
fn moedex_auto_fallback_in_a_shared_home_preserves_stock_auth() -> anyhow::Result<()> {
    let shared_home = tempdir()?;
    let stock = storage(
        shared_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    stock.write_for_test(&api_key_auth("stock-secret"))?;
    let stock_bytes = fs::read(shared_home.path().join("auth.json"))?;
    let keyring = Arc::new(MockKeyringStore::default());
    let account = crate::auth::storage::compute_store_key(shared_home.path())?;
    let moedex = AuthStorage::new_with_keyring_store(
        shared_home.path().into(),
        AuthCredentialsStoreMode::Auto,
        AuthKeyringBackendKind::Direct,
        AuthStorageNamespace::Moedex,
        keyring.clone(),
    );

    assert_eq!(moedex.read_for_test()?, None);
    keyring.set_error(
        &account,
        keyring::Error::Invalid("unavailable".into(), "save".into()),
    );
    moedex.write_for_test(&api_key_auth("moedex-secret"))?;
    assert_eq!(fs::read(shared_home.path().join("auth.json"))?, stock_bytes);
    assert!(shared_home.path().join("moedex-auth.json").is_file());
    moedex.delete_for_test()?;
    assert_eq!(fs::read(shared_home.path().join("auth.json"))?, stock_bytes);
    Ok(())
}

#[test]
fn file_conflict_skips_and_explicit_replace_requires_sign_in() -> anyhow::Result<()> {
    let source_home = tempdir()?;
    let destination_home = tempdir()?;
    let source_auth = api_key_auth("source-secret");
    let destination_auth = api_key_auth("destination-secret");
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
    source.write_for_test(&source_auth)?;
    destination.write_for_test(&destination_auth)?;

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::Conflict
    );
    assert_eq!(destination.read_for_test()?, Some(destination_auth.clone()));
    assert_eq!(
        replace_auth_record(&source, &destination)?,
        AuthImportOutcome::SignInRequired
    );
    assert_eq!(destination.read_for_test()?, Some(destination_auth));
    assert_eq!(source.read_for_test()?, Some(source_auth));
    Ok(())
}

#[test]
fn direct_keyring_import_uses_independent_services() -> anyhow::Result<()> {
    let root = tempdir()?;
    let source_home = root.path().join("source");
    let destination_home = root.path().join("destination");
    fs::create_dir_all(&source_home)?;
    fs::create_dir_all(&destination_home)?;
    let keyring = Arc::new(ServiceKeyring::default());
    let source = keyring_storage(
        &source_home,
        AuthKeyringBackendKind::Direct,
        AuthStorageNamespace::Codex,
        keyring.clone(),
    );
    let destination = keyring_storage(
        &destination_home,
        AuthKeyringBackendKind::Direct,
        AuthStorageNamespace::Moedex,
        keyring.clone(),
    );
    source.write_for_test(&api_key_auth("direct-secret"))?;
    let destination_auth = api_key_auth("direct-destination");
    destination.write_for_test(&destination_auth)?;

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::Conflict
    );
    assert_eq!(destination.read_for_test()?, Some(destination_auth));
    let source_key = crate::auth::storage::compute_store_key(&source_home)?;
    let destination_key = crate::auth::storage::compute_store_key(&destination_home)?;
    assert!(keyring.load("Codex Auth", &source_key)?.is_some());
    assert!(keyring.load("Moedex Auth", &destination_key)?.is_some());
    destination.delete_for_test()?;
    assert!(keyring.load("Codex Auth", &source_key)?.is_some());
    assert!(keyring.load("Moedex Auth", &destination_key)?.is_none());
    Ok(())
}

#[test]
fn encrypted_import_uses_independent_files_and_logout_isolated() -> anyhow::Result<()> {
    let root = tempdir()?;
    let source_home = root.path().join("source");
    let destination_home = root.path().join("destination");
    fs::create_dir_all(&source_home)?;
    fs::create_dir_all(&destination_home)?;
    let keyring = Arc::new(ServiceKeyring::default());
    let source = keyring_storage(
        &source_home,
        AuthKeyringBackendKind::Secrets,
        AuthStorageNamespace::Codex,
        keyring.clone(),
    );
    let destination = keyring_storage(
        &destination_home,
        AuthKeyringBackendKind::Secrets,
        AuthStorageNamespace::Moedex,
        keyring,
    );
    let original = api_key_auth("encrypted-secret");
    source.write_for_test(&original)?;
    let destination_auth = api_key_auth("encrypted-destination");
    destination.write_for_test(&destination_auth)?;

    assert_eq!(
        import_auth_record(&source, &destination)?,
        AuthImportOutcome::Conflict
    );
    assert_eq!(destination.read_for_test()?, Some(destination_auth));
    assert!(source_home.join("secrets/codex_auth.age").is_file());
    assert!(destination_home.join("secrets/moedex_auth.age").is_file());
    destination.delete_for_test()?;
    assert_eq!(source.read_for_test()?, Some(original));
    assert_eq!(destination.read_for_test()?, None);
    Ok(())
}

#[test]
fn import_rejects_equal_and_symlink_aliased_homes() -> anyhow::Result<()> {
    let home = tempdir()?;
    let source = storage(
        home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Codex,
    );
    let destination = storage(
        home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );
    source.write_for_test(&api_key_auth("same-home-secret"))?;
    assert_eq!(
        import_auth_record(&source, &destination)
            .expect_err("same home")
            .kind(),
        std::io::ErrorKind::InvalidInput
    );

    #[cfg(unix)]
    {
        let root = tempdir()?;
        let real = root.path().join("real");
        let alias = root.path().join("alias");
        fs::create_dir(&real)?;
        std::os::unix::fs::symlink(&real, &alias)?;
        let source = storage(
            &real,
            AuthCredentialsStoreMode::File,
            AuthStorageNamespace::Codex,
        );
        let destination = storage(
            &alias,
            AuthCredentialsStoreMode::File,
            AuthStorageNamespace::Moedex,
        );
        source.write_for_test(&api_key_auth("alias-secret"))?;
        assert_eq!(
            import_auth_record(&source, &destination)
                .expect_err("aliased home")
                .kind(),
            std::io::ErrorKind::InvalidInput
        );
    }
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
fn file_save_tightens_existing_destination_permissions() -> anyhow::Result<()> {
    use std::os::unix::fs::PermissionsExt;

    let destination_home = tempdir()?;
    let destination_file = destination_home.path().join("moedex-auth.json");
    fs::write(&destination_file, "{}")?;
    fs::set_permissions(&destination_file, fs::Permissions::from_mode(0o644))?;
    let destination = storage(
        destination_home.path(),
        AuthCredentialsStoreMode::File,
        AuthStorageNamespace::Moedex,
    );

    destination.write_for_test(&api_key_auth("secret"))?;
    assert_eq!(
        fs::metadata(destination_file)?.permissions().mode() & 0o777,
        0o600
    );
    Ok(())
}
