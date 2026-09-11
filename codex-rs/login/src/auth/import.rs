use super::BedrockAccessKeysAuth;
use super::BedrockApiKeyAuth;
use super::storage::AuthDotJson;
use super::storage::AuthStorageBackend;
pub use super::storage::AuthStorageNamespace;
use super::storage::create_auth_storage_with_store_and_namespace;
use codex_config::types::AuthCredentialsStoreMode;
use codex_config::types::AuthKeyringBackendKind;
use codex_keyring_store::DefaultKeyringStore;
use codex_keyring_store::KeyringStore;
use codex_protocol::auth::AuthMode;
use std::path::PathBuf;
use std::sync::Arc;

/// Sanitized result of an explicit credential import.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthImportOutcome {
    Imported,
    Conflict,
    SignInRequired,
    Skipped,
    Failed,
}

/// A namespaced auth adapter used for explicit credential import.
///
/// The wrapped backend remains private so callers cannot serialize credentials
/// into command arguments, reports, or staging files.
pub struct AuthStorage {
    backend: Arc<dyn AuthStorageBackend>,
    home: PathBuf,
    mode: AuthCredentialsStoreMode,
}

impl std::fmt::Debug for AuthStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthStorage")
            .field("mode", &self.mode)
            .finish_non_exhaustive()
    }
}

impl AuthStorage {
    pub fn new(
        home: PathBuf,
        mode: AuthCredentialsStoreMode,
        keyring_backend_kind: AuthKeyringBackendKind,
        namespace: AuthStorageNamespace,
    ) -> Self {
        Self::new_with_keyring_store(
            home,
            mode,
            keyring_backend_kind,
            namespace,
            Arc::new(DefaultKeyringStore),
        )
    }

    fn new_with_keyring_store(
        home: PathBuf,
        mode: AuthCredentialsStoreMode,
        keyring_backend_kind: AuthKeyringBackendKind,
        namespace: AuthStorageNamespace,
        keyring_store: Arc<dyn KeyringStore>,
    ) -> Self {
        Self {
            backend: create_auth_storage_with_store_and_namespace(
                home.clone(),
                mode,
                keyring_store,
                keyring_backend_kind,
                namespace,
            ),
            home,
            mode,
        }
    }

    #[cfg(test)]
    fn read_for_test(&self) -> std::io::Result<Option<AuthDotJson>> {
        self.backend.load()
    }

    #[cfg(test)]
    fn write_for_test(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        self.backend.save(auth)
    }

    #[cfg(test)]
    fn delete_for_test(&self) -> std::io::Result<bool> {
        self.backend.delete()
    }
}

/// Copies one validated auth record between namespaced storage adapters.
///
/// Import never refreshes or rotates the source record. Source access failures
/// and records that cannot be validated offline require a fresh sign-in.
pub fn import_auth_record(
    source: &AuthStorage,
    destination: &AuthStorage,
) -> std::io::Result<AuthImportOutcome> {
    import_auth_record_with_replacement(source, destination, /*replace*/ false)
}

/// Attempts explicit replacement of an existing destination credential.
///
/// Persistent backend-specific backup is not currently guaranteed, so a
/// conflict is converted to a sign-in requirement without changing either
/// credential record.
pub fn replace_auth_record(
    source: &AuthStorage,
    destination: &AuthStorage,
) -> std::io::Result<AuthImportOutcome> {
    import_auth_record_with_replacement(source, destination, /*replace*/ true)
}

fn import_auth_record_with_replacement(
    source: &AuthStorage,
    destination: &AuthStorage,
    replace: bool,
) -> std::io::Result<AuthImportOutcome> {
    ensure_distinct_homes(&source.home, &destination.home)?;
    if source.mode == AuthCredentialsStoreMode::Ephemeral {
        return Ok(AuthImportOutcome::SignInRequired);
    }
    if destination.mode == AuthCredentialsStoreMode::Ephemeral {
        return Ok(AuthImportOutcome::Failed);
    }
    let record = match source.backend.load_for_import() {
        Ok(Some(record)) => record,
        Ok(None) => return Ok(AuthImportOutcome::Skipped),
        Err(_) => return Ok(AuthImportOutcome::SignInRequired),
    };
    if !validate_importable_auth(&record) {
        return Ok(AuthImportOutcome::SignInRequired);
    }
    let Ok(_guard) = codex_diagnostics::acquire_selected_home_write_guard(&destination.home) else {
        return Ok(AuthImportOutcome::Failed);
    };
    match destination.backend.load_for_import() {
        Ok(Some(_)) if replace => return Ok(AuthImportOutcome::SignInRequired),
        Ok(Some(_)) => return Ok(AuthImportOutcome::Conflict),
        Ok(None) => {}
        Err(_) => return Ok(AuthImportOutcome::Failed),
    }
    match destination.backend.save(&record) {
        Ok(()) => Ok(AuthImportOutcome::Imported),
        Err(_) => Ok(AuthImportOutcome::Failed),
    }
}

fn ensure_distinct_homes(
    source: &std::path::Path,
    destination: &std::path::Path,
) -> std::io::Result<()> {
    if canonical_home(source)? == canonical_home(destination)? {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "credential source and destination resolve to the same home",
        ));
    }
    Ok(())
}

fn canonical_home(path: &std::path::Path) -> std::io::Result<PathBuf> {
    if path.exists() {
        return std::fs::canonicalize(path);
    }
    let parent = path.parent().ok_or_else(|| {
        std::io::Error::new(std::io::ErrorKind::InvalidInput, "auth home has no parent")
    })?;
    let name = path.file_name().ok_or_else(|| {
        std::io::Error::new(
            std::io::ErrorKind::InvalidInput,
            "auth home has no final component",
        )
    })?;
    Ok(std::fs::canonicalize(parent)?.join(name))
}

fn validate_importable_auth(auth: &AuthDotJson) -> bool {
    match auth.auth_mode {
        Some(AuthMode::ApiKey) => nonempty(auth.openai_api_key.as_deref()),
        Some(AuthMode::Chatgpt | AuthMode::ChatgptAuthTokens) => {
            auth.tokens.as_ref().is_some_and(|tokens| {
                nonempty(Some(&tokens.access_token)) && nonempty(Some(&tokens.refresh_token))
            })
        }
        Some(AuthMode::AgentIdentity) => auth
            .agent_identity
            .as_ref()
            .is_some_and(super::storage::AgentIdentityStorage::has_auth_material),
        Some(AuthMode::PersonalAccessToken) => nonempty(auth.personal_access_token.as_deref()),
        Some(AuthMode::BedrockApiKey) => auth
            .bedrock_api_key
            .as_ref()
            .is_some_and(valid_bedrock_api_key),
        Some(AuthMode::BedrockAccessKeys) => auth
            .bedrock_access_keys
            .as_ref()
            .is_some_and(valid_bedrock_access_keys),
        Some(AuthMode::Headers) | None => false,
    }
}

fn valid_bedrock_api_key(auth: &BedrockApiKeyAuth) -> bool {
    nonempty(Some(&auth.api_key)) && nonempty(Some(&auth.region))
}

fn valid_bedrock_access_keys(auth: &BedrockAccessKeysAuth) -> bool {
    nonempty(Some(&auth.access_key_id)) && nonempty(Some(&auth.secret_access_key))
}

fn nonempty(value: Option<&str>) -> bool {
    value.is_some_and(|value| !value.trim().is_empty())
}

#[cfg(test)]
#[path = "import_tests.rs"]
mod tests;
