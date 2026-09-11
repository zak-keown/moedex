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
use sha2::Digest;
use sha2::Sha256;
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
#[derive(Clone)]
pub struct AuthStorage {
    backend: Arc<dyn AuthStorageBackend>,
    home: PathBuf,
    mode: AuthCredentialsStoreMode,
}

/// Opaque credential state captured for a preview-bound import.
///
/// Credential material and its fingerprint remain private and this type deliberately does not
/// implement `Debug` or serialization.
#[derive(Clone, Eq, PartialEq)]
pub struct AuthImportPreview {
    source: AuthRecordState,
    destination: AuthRecordState,
}

#[derive(Clone, Eq, PartialEq)]
enum AuthRecordState {
    Empty,
    Record {
        fingerprint: [u8; 32],
        importable: bool,
    },
    Unavailable,
}

impl AuthImportPreview {
    /// Returns whether the destination already held a credential when previewed.
    pub fn destination_has_credentials(&self) -> bool {
        matches!(self.destination, AuthRecordState::Record { .. })
    }

    /// Returns whether source credential storage could not be inspected safely.
    pub fn source_is_unavailable(&self) -> bool {
        self.source == AuthRecordState::Unavailable
    }

    /// Returns whether destination credential storage could not be inspected safely.
    pub fn destination_is_unavailable(&self) -> bool {
        self.destination == AuthRecordState::Unavailable
    }

    /// Returns whether the previewed source record exists but cannot be imported offline.
    pub fn source_requires_sign_in(&self) -> bool {
        matches!(
            self.source,
            AuthRecordState::Record {
                importable: false,
                ..
            }
        )
    }
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
    let preview = preview_auth_record(source, destination)?;
    if preview.source_is_unavailable() {
        return Ok(AuthImportOutcome::SignInRequired);
    }
    if preview.destination_is_unavailable() {
        return Ok(AuthImportOutcome::Failed);
    }
    import_auth_record_from_preview(source, destination, &preview)
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
    let preview = preview_auth_record(source, destination)?;
    if preview.source_is_unavailable() {
        return Ok(AuthImportOutcome::SignInRequired);
    }
    if preview.destination_is_unavailable() {
        return Ok(AuthImportOutcome::Failed);
    }
    replace_auth_record_from_preview(source, destination, &preview)
}

/// Captures source and destination credential state without exposing credential material.
pub fn preview_auth_record(
    source: &AuthStorage,
    destination: &AuthStorage,
) -> std::io::Result<AuthImportPreview> {
    ensure_distinct_homes(&source.home, &destination.home)?;
    Ok(AuthImportPreview {
        source: read_auth_state(source)?.0,
        destination: read_auth_state(destination)?.0,
    })
}

/// Applies exactly the credential state represented by `preview`.
///
/// Any source or destination change requires a new preview and leaves the destination untouched.
pub fn import_auth_record_from_preview(
    source: &AuthStorage,
    destination: &AuthStorage,
    preview: &AuthImportPreview,
) -> std::io::Result<AuthImportOutcome> {
    import_auth_record_from_preview_with_replacement(
        source,
        destination,
        preview,
        /*replace*/ false,
    )
}

/// Attempts preview-bound replacement of an existing destination credential.
pub fn replace_auth_record_from_preview(
    source: &AuthStorage,
    destination: &AuthStorage,
    preview: &AuthImportPreview,
) -> std::io::Result<AuthImportOutcome> {
    import_auth_record_from_preview_with_replacement(
        source,
        destination,
        preview,
        /*replace*/ true,
    )
}

fn import_auth_record_from_preview_with_replacement(
    source: &AuthStorage,
    destination: &AuthStorage,
    preview: &AuthImportPreview,
    replace: bool,
) -> std::io::Result<AuthImportOutcome> {
    ensure_distinct_homes(&source.home, &destination.home)?;
    if source.mode == AuthCredentialsStoreMode::Ephemeral {
        return Ok(AuthImportOutcome::SignInRequired);
    }
    if destination.mode == AuthCredentialsStoreMode::Ephemeral {
        return Ok(AuthImportOutcome::Failed);
    }
    let (source_state, source_record) = read_auth_state(source)?;
    if source_state != preview.source {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "source credentials changed after preview; run preview again",
        ));
    }
    let Ok(_guard) = codex_diagnostics::acquire_selected_home_write_guard(&destination.home) else {
        return Ok(AuthImportOutcome::Failed);
    };
    let (destination_state, _) = read_auth_state(destination)?;
    if destination_state != preview.destination {
        return Err(std::io::Error::new(
            std::io::ErrorKind::InvalidData,
            "destination credentials changed after preview; run preview again",
        ));
    }
    let record = match (source_state, source_record) {
        (AuthRecordState::Unavailable, _) => return Ok(AuthImportOutcome::SignInRequired),
        (AuthRecordState::Empty, _) => return Ok(AuthImportOutcome::Skipped),
        (
            AuthRecordState::Record {
                importable: false, ..
            },
            _,
        ) => return Ok(AuthImportOutcome::SignInRequired),
        (AuthRecordState::Record { .. }, Some(record)) => record,
        (AuthRecordState::Record { .. }, None) => {
            return Err(std::io::Error::other(
                "credential snapshot lost its source record",
            ));
        }
    };
    match destination_state {
        AuthRecordState::Record { .. } if replace => {
            return Ok(AuthImportOutcome::SignInRequired);
        }
        AuthRecordState::Record { .. } => return Ok(AuthImportOutcome::Conflict),
        AuthRecordState::Unavailable => return Ok(AuthImportOutcome::Failed),
        AuthRecordState::Empty => {}
    }
    match destination.backend.save(&record) {
        Ok(()) => Ok(AuthImportOutcome::Imported),
        Err(_) => Ok(AuthImportOutcome::Failed),
    }
}

fn read_auth_state(
    storage: &AuthStorage,
) -> std::io::Result<(AuthRecordState, Option<AuthDotJson>)> {
    let record = match storage.backend.load_for_import() {
        Ok(record) => record,
        Err(_) => return Ok((AuthRecordState::Unavailable, None)),
    };
    let Some(record) = record else {
        return Ok((AuthRecordState::Empty, None));
    };
    let serialized = serde_json::to_vec(&record).map_err(std::io::Error::other)?;
    let fingerprint: [u8; 32] = Sha256::digest(&serialized).into();
    let importable = validate_importable_auth(&record);
    Ok((
        AuthRecordState::Record {
            fingerprint,
            importable,
        },
        Some(record),
    ))
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
