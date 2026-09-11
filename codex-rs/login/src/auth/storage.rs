use chrono::DateTime;
use chrono::Utc;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::fmt::Debug;
use std::fs::File;
use std::fs::OpenOptions;
use std::io::Read;
use std::io::Write;
#[cfg(unix)]
use std::os::unix::fs::OpenOptionsExt;
#[cfg(unix)]
use std::os::unix::fs::PermissionsExt;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Arc;
use std::sync::Mutex;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;
use tracing::warn;

use super::BedrockAccessKeysAuth;
use super::BedrockApiKeyAuth;
use crate::token_data::TokenData;
use codex_agent_identity::AgentIdentityJwtClaims;
use codex_agent_identity::decode_agent_identity_jwt;
use codex_config::types::AuthCredentialsStoreMode;
pub use codex_config::types::AuthKeyringBackendKind;
use codex_keyring_store::DefaultKeyringStore;
use codex_keyring_store::KeyringStore;
use codex_protocol::account::PlanType as AccountPlanType;
use codex_protocol::auth::AuthMode;
use codex_secrets::LocalSecretsNamespace;
use codex_secrets::SecretName;
use codex_secrets::SecretScope;
use codex_secrets::SecretsBackendKind;
use codex_secrets::SecretsManager;
use once_cell::sync::Lazy;

/// Expected structure for $CODEX_HOME/auth.json.
#[derive(Deserialize, Serialize, Clone, PartialEq)]
pub struct AuthDotJson {
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub auth_mode: Option<AuthMode>,

    #[serde(rename = "OPENAI_API_KEY")]
    pub openai_api_key: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub tokens: Option<TokenData>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub last_refresh: Option<DateTime<Utc>>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub agent_identity: Option<AgentIdentityStorage>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub personal_access_token: Option<String>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bedrock_api_key: Option<BedrockApiKeyAuth>,

    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub bedrock_access_keys: Option<BedrockAccessKeysAuth>,
}

impl Debug for AuthDotJson {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("AuthDotJson")
            .field("auth_mode", &self.auth_mode)
            .field(
                "openai_api_key",
                &self.openai_api_key.as_ref().map(|_| "<redacted>"),
            )
            .field("tokens", &self.tokens.as_ref().map(|_| "<redacted>"))
            .field("last_refresh", &self.last_refresh)
            .field(
                "agent_identity",
                &self.agent_identity.as_ref().map(|_| "<redacted>"),
            )
            .field(
                "personal_access_token",
                &self.personal_access_token.as_ref().map(|_| "<redacted>"),
            )
            .field("bedrock_api_key", &self.bedrock_api_key)
            .field("bedrock_access_keys", &self.bedrock_access_keys)
            .finish()
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
#[serde(untagged)]
pub enum AgentIdentityStorage {
    Jwt(String),
    Record(AgentIdentityAuthRecord),
}

impl AgentIdentityStorage {
    pub fn has_auth_material(&self) -> bool {
        match self {
            Self::Jwt(jwt) => !jwt.trim().is_empty(),
            Self::Record(record) => {
                !record.agent_runtime_id.trim().is_empty()
                    && !record.agent_private_key.trim().is_empty()
            }
        }
    }

    pub(crate) fn as_record(&self) -> Option<&AgentIdentityAuthRecord> {
        match self {
            Self::Jwt(_) => None,
            Self::Record(record) => Some(record),
        }
    }
}

#[derive(Deserialize, Serialize, Clone, Debug, PartialEq, Eq)]
pub struct AgentIdentityAuthRecord {
    pub agent_runtime_id: String,
    pub agent_private_key: String,
    pub account_id: String,
    pub chatgpt_user_id: String,
    #[serde(
        default,
        deserialize_with = "deserialize_optional_non_empty_string",
        serialize_with = "serialize_optional_string_as_empty"
    )]
    pub email: Option<String>,
    pub plan_type: AccountPlanType,
    pub chatgpt_account_is_fedramp: bool,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub task_id: Option<String>,
}

fn deserialize_optional_non_empty_string<'de, D>(
    deserializer: D,
) -> Result<Option<String>, D::Error>
where
    D: serde::Deserializer<'de>,
{
    Option::<String>::deserialize(deserializer).map(|value| value.filter(|value| !value.is_empty()))
}

fn serialize_optional_string_as_empty<S>(
    value: &Option<String>,
    serializer: S,
) -> Result<S::Ok, S::Error>
where
    S: serde::Serializer,
{
    value.as_deref().unwrap_or_default().serialize(serializer)
}

impl AgentIdentityAuthRecord {
    pub(crate) fn from_agent_identity_jwt(jwt: &str) -> std::io::Result<Self> {
        let claims =
            decode_agent_identity_jwt(jwt, /*jwks*/ None).map_err(std::io::Error::other)?;

        Ok(claims.into())
    }
}

impl From<AgentIdentityJwtClaims> for AgentIdentityAuthRecord {
    fn from(claims: AgentIdentityJwtClaims) -> Self {
        Self {
            agent_runtime_id: claims.agent_runtime_id,
            agent_private_key: claims.agent_private_key,
            account_id: claims.account_id,
            chatgpt_user_id: claims.chatgpt_user_id,
            email: claims.email,
            plan_type: claims.plan_type.into(),
            chatgpt_account_is_fedramp: claims.chatgpt_account_is_fedramp,
            task_id: None,
        }
    }
}

pub(super) fn get_auth_file(codex_home: &Path) -> PathBuf {
    codex_home.join("auth.json")
}

fn get_auth_file_in_namespace(codex_home: &Path, namespace: AuthStorageNamespace) -> PathBuf {
    match namespace {
        AuthStorageNamespace::Codex => get_auth_file(codex_home),
        AuthStorageNamespace::Moedex => codex_home.join("moedex-auth.json"),
    }
}

fn delete_file_if_exists(
    codex_home: &Path,
    namespace: AuthStorageNamespace,
) -> std::io::Result<bool> {
    let auth_file = get_auth_file_in_namespace(codex_home, namespace);
    match std::fs::remove_file(&auth_file) {
        Ok(()) => Ok(true),
        Err(err) if err.kind() == std::io::ErrorKind::NotFound => Ok(false),
        Err(err) => Err(err),
    }
}

pub(super) trait AuthStorageBackend: Debug + Send + Sync {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>>;
    fn load_for_import(&self) -> std::io::Result<Option<AuthDotJson>> {
        self.load()
    }
    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()>;
    fn delete(&self) -> std::io::Result<bool>;
    fn replace_with_backup(&self, _auth: &AuthDotJson) -> std::io::Result<String> {
        Err(std::io::Error::new(
            std::io::ErrorKind::Unsupported,
            "credential replacement is unsupported for this storage backend",
        ))
    }
}

fn auth_backup_id() -> String {
    let timestamp = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap_or_default()
        .as_nanos();
    let sequence = AUTH_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    format!("{}-{timestamp}-{sequence}", std::process::id())
}

#[derive(Clone, Debug)]
pub(super) struct FileAuthStorage {
    codex_home: PathBuf,
    namespace: AuthStorageNamespace,
}

impl FileAuthStorage {
    #[cfg(test)]
    pub(super) fn new(codex_home: PathBuf) -> Self {
        Self::new_in_namespace(codex_home, AuthStorageNamespace::Codex)
    }

    fn new_in_namespace(codex_home: PathBuf, namespace: AuthStorageNamespace) -> Self {
        Self {
            codex_home,
            namespace,
        }
    }

    /// Attempt to read and parse the `auth.json` file in the given `CODEX_HOME` directory.
    /// Returns the full AuthDotJson structure.
    pub(super) fn try_read_auth_json(&self, auth_file: &Path) -> std::io::Result<AuthDotJson> {
        let mut file = File::open(auth_file)?;
        let mut contents = String::new();
        file.read_to_string(&mut contents)?;
        let auth_dot_json: AuthDotJson = serde_json::from_str(&contents)?;

        Ok(auth_dot_json)
    }

    fn save_with_atomic_replace(
        &self,
        auth: &AuthDotJson,
        replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.save_with_atomic_replace_and_cleanup(
            auth,
            replace,
            |path| std::fs::remove_file(path),
            sync_parent_directory,
        )
    }

    fn save_with_atomic_replace_and_cleanup(
        &self,
        auth: &AuthDotJson,
        replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
        remove_backup: impl FnMut(&Path) -> std::io::Result<()>,
        sync_parent: impl FnMut(&Path) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        let auth_file = get_auth_file_in_namespace(&self.codex_home, self.namespace);
        let parent = auth_file.parent().ok_or_else(|| {
            std::io::Error::new(std::io::ErrorKind::InvalidInput, "auth path has no parent")
        })?;
        std::fs::create_dir_all(parent)?;
        let sequence = AUTH_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let file_name = auth_file
            .file_name()
            .and_then(|name| name.to_str())
            .ok_or_else(|| std::io::Error::other("auth filename is not UTF-8"))?;
        let temp = parent.join(format!(
            ".{file_name}.{}.{sequence}.tmp",
            std::process::id()
        ));
        let json_data = serde_json::to_vec_pretty(auth)?;
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&temp)?;
        let result = (|| {
            file.write_all(&json_data)?;
            file.sync_all()?;
            drop(file);
            replace_auth_file_with_rollback(
                &temp,
                &auth_file,
                parent,
                replace,
                remove_backup,
                sync_parent,
            )
        })();
        if result.is_err() {
            let _ = std::fs::remove_file(&temp);
        }
        result
    }

    #[cfg(test)]
    pub(super) fn save_with_atomic_replace_for_test(
        &self,
        auth: &AuthDotJson,
        replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.save_with_atomic_replace(auth, replace)
    }

    #[cfg(all(test, unix))]
    pub(super) fn save_with_cleanup_ops_for_test(
        &self,
        auth: &AuthDotJson,
        remove_backup: impl FnMut(&Path) -> std::io::Result<()>,
        sync_parent: impl FnMut(&Path) -> std::io::Result<()>,
    ) -> std::io::Result<()> {
        self.save_with_atomic_replace_and_cleanup(
            auth,
            replace_auth_file,
            remove_backup,
            sync_parent,
        )
    }
}

#[cfg(unix)]
fn replace_auth_file_with_rollback(
    temp: &Path,
    target: &Path,
    parent: &Path,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    mut remove_backup: impl FnMut(&Path) -> std::io::Result<()>,
    mut sync_parent: impl FnMut(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    let previous = if target.exists() {
        std::fs::set_permissions(target, std::fs::Permissions::from_mode(0o600))?;
        let sequence = AUTH_TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
        let backup = parent.join(format!(
            ".auth.json.{}.{}.backup",
            std::process::id(),
            sequence
        ));
        std::fs::hard_link(target, &backup)?;
        Some(backup)
    } else {
        None
    };

    if let Err(error) = replace(temp, target) {
        if let Some(previous) = previous {
            let _ = remove_backup(&previous);
        }
        return Err(error);
    }
    if let Err(error) = sync_parent(parent) {
        restore_previous_auth(target, previous.as_deref(), parent, &mut sync_parent);
        return Err(error);
    }
    if let Some(previous) = previous {
        if let Err(error) = remove_backup(&previous) {
            restore_previous_auth(target, Some(&previous), parent, &mut sync_parent);
            return Err(error);
        }
        sync_parent(parent)?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn replace_auth_file_with_rollback(
    temp: &Path,
    target: &Path,
    parent: &Path,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    _remove_backup: impl FnMut(&Path) -> std::io::Result<()>,
    mut sync_parent: impl FnMut(&Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    replace(temp, target)?;
    sync_parent(parent)
}

#[cfg(unix)]
fn restore_previous_auth(
    target: &Path,
    previous: Option<&Path>,
    parent: &Path,
    sync_parent: &mut impl FnMut(&Path) -> std::io::Result<()>,
) {
    match previous {
        Some(previous) => {
            let _ = std::fs::rename(previous, target);
        }
        None => {
            let _ = std::fs::remove_file(target);
        }
    }
    let _ = sync_parent(parent);
}

impl AuthStorageBackend for FileAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        let auth_file = get_auth_file_in_namespace(&self.codex_home, self.namespace);
        let auth_dot_json = match self.try_read_auth_json(&auth_file) {
            Ok(auth) => auth,
            Err(err) if err.kind() == std::io::ErrorKind::NotFound => return Ok(None),
            Err(err) => return Err(err),
        };
        Ok(Some(auth_dot_json))
    }

    fn save(&self, auth_dot_json: &AuthDotJson) -> std::io::Result<()> {
        self.save_with_atomic_replace(auth_dot_json, replace_auth_file)
    }

    fn delete(&self) -> std::io::Result<bool> {
        delete_file_if_exists(&self.codex_home, self.namespace)
    }

    fn replace_with_backup(&self, auth: &AuthDotJson) -> std::io::Result<String> {
        let current = self.load()?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "credential destination is empty",
            )
        })?;
        let backup_dir = self.codex_home.join("moedex-import-backups");
        std::fs::create_dir_all(&backup_dir)?;
        let backup = backup_dir.join(format!("moedex-auth-{}.json", auth_backup_id()));
        let mut options = OpenOptions::new();
        options.write(true).create_new(true);
        #[cfg(unix)]
        options.mode(0o600);
        let mut file = options.open(&backup)?;
        serde_json::to_writer_pretty(&mut file, &current).map_err(std::io::Error::other)?;
        file.sync_all()?;
        sync_parent_directory(&backup_dir)?;
        if let Err(error) = self.save(auth) {
            let _ = std::fs::remove_file(&backup);
            return Err(error);
        }
        Ok(backup.display().to_string())
    }
}

#[cfg(not(target_os = "windows"))]
fn replace_auth_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    std::fs::rename(temp, target)
}

#[cfg(target_os = "windows")]
fn replace_auth_file(temp: &Path, target: &Path) -> std::io::Result<()> {
    replace_auth_file_windows_with_ops(
        temp,
        target,
        |from, to| std::fs::rename(from, to),
        replace_existing_auth_file_windows,
    )
}

#[cfg(any(target_os = "windows", test))]
fn replace_auth_file_windows_with_ops(
    temp: &Path,
    target: &Path,
    create: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
    replace: impl FnOnce(&Path, &Path) -> std::io::Result<()>,
) -> std::io::Result<()> {
    if target.exists() {
        replace(target, temp)
    } else {
        create(temp, target)
    }
}

#[cfg(target_os = "windows")]
fn replace_existing_auth_file_windows(target: &Path, replacement: &Path) -> std::io::Result<()> {
    use std::iter::once;
    use std::os::windows::ffi::OsStrExt;
    use windows_sys::Win32::Storage::FileSystem::ReplaceFileW;

    let target = target
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    let replacement = replacement
        .as_os_str()
        .encode_wide()
        .chain(once(0))
        .collect::<Vec<_>>();
    // Do not ignore ACL merge errors: the restrictive replacement inherits the
    // existing auth file's ACL only when ReplaceFileW can preserve it safely.
    // SAFETY: both paths are NUL-terminated for the duration of the call. The
    // optional backup, exclude, and reserved pointers are intentionally null.
    let replaced = unsafe {
        ReplaceFileW(
            target.as_ptr(),
            replacement.as_ptr(),
            std::ptr::null(),
            0,
            std::ptr::null(),
            std::ptr::null(),
        )
    };
    if replaced == 0 {
        Err(std::io::Error::last_os_error())
    } else {
        Ok(())
    }
}

#[cfg(unix)]
fn sync_parent_directory(parent: &Path) -> std::io::Result<()> {
    File::open(parent)?.sync_all()
}

#[cfg(not(unix))]
fn sync_parent_directory(_parent: &Path) -> std::io::Result<()> {
    Ok(())
}

static CODEX_AUTH_SECRET_NAME: Lazy<SecretName> =
    Lazy::new(|| match SecretName::new("CODEX_AUTH") {
        Ok(name) => name,
        Err(err) => unreachable!("CODEX_AUTH should be a valid secret name: {err}"),
    });
static AUTH_TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);
const CODEX_KEYRING_SERVICE: &str = "Codex Auth";
#[cfg(test)]
const KEYRING_SERVICE: &str = codex_product_identity::PRODUCT_IDENTITY.credential_service;

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum AuthStorageNamespace {
    /// Stock Codex direct-keyring and encrypted-file names.
    Codex,
    /// Moedex-owned direct-keyring and encrypted-file names.
    Moedex,
}

impl AuthStorageNamespace {
    fn keyring_service(self) -> &'static str {
        match self {
            Self::Codex => CODEX_KEYRING_SERVICE,
            Self::Moedex => codex_product_identity::PRODUCT_IDENTITY.credential_service,
        }
    }

    fn local_secrets_namespace(self) -> LocalSecretsNamespace {
        match self {
            Self::Codex => LocalSecretsNamespace::CodexAuth,
            Self::Moedex => LocalSecretsNamespace::MoedexAuth,
        }
    }
}

// turns codex_home path into a stable, short key string
pub(super) fn compute_store_key(codex_home: &Path) -> std::io::Result<String> {
    let canonical = codex_home
        .canonicalize()
        .unwrap_or_else(|_| codex_home.to_path_buf());
    let path_str = canonical.to_string_lossy();
    let mut hasher = Sha256::new();
    hasher.update(path_str.as_bytes());
    let digest = hasher.finalize();
    let hex = format!("{digest:x}");
    let truncated = hex.get(..16).unwrap_or(&hex);
    Ok(format!("cli|{truncated}"))
}

#[derive(Clone, Debug)]
struct DirectKeyringAuthStorage {
    codex_home: PathBuf,
    keyring_store: Arc<dyn KeyringStore>,
    namespace: AuthStorageNamespace,
}

impl DirectKeyringAuthStorage {
    #[cfg(test)]
    fn new(codex_home: PathBuf, keyring_store: Arc<dyn KeyringStore>) -> Self {
        Self::new_in_namespace(codex_home, keyring_store, AuthStorageNamespace::Moedex)
    }

    fn new_in_namespace(
        codex_home: PathBuf,
        keyring_store: Arc<dyn KeyringStore>,
        namespace: AuthStorageNamespace,
    ) -> Self {
        Self {
            codex_home,
            keyring_store,
            namespace,
        }
    }

    fn load_from_keyring(&self, key: &str) -> std::io::Result<Option<AuthDotJson>> {
        match self
            .keyring_store
            .load(self.namespace.keyring_service(), key)
        {
            Ok(Some(serialized)) => serde_json::from_str(&serialized).map(Some).map_err(|err| {
                std::io::Error::other(format!(
                    "failed to deserialize CLI auth from keyring: {err}"
                ))
            }),
            Ok(None) => Ok(None),
            Err(error) => Err(std::io::Error::other(format!(
                "failed to load CLI auth from keyring: {}",
                error.message()
            ))),
        }
    }

    fn save_to_keyring(&self, key: &str, value: &str) -> std::io::Result<()> {
        match self
            .keyring_store
            .save(self.namespace.keyring_service(), key, value)
        {
            Ok(()) => Ok(()),
            Err(error) => {
                let message = format!(
                    "failed to write OAuth tokens to keyring: {}",
                    error.message()
                );
                warn!("{message}");
                Err(std::io::Error::other(message))
            }
        }
    }
}

impl AuthStorageBackend for DirectKeyringAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        let key = compute_store_key(&self.codex_home)?;
        self.load_from_keyring(&key)
    }

    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        let key = compute_store_key(&self.codex_home)?;
        // Simpler error mapping per style: prefer method reference over closure
        let serialized = serde_json::to_string(auth).map_err(std::io::Error::other)?;
        self.save_to_keyring(&key, &serialized)?;
        if let Err(err) = delete_file_if_exists(&self.codex_home, self.namespace) {
            warn!("failed to remove CLI auth fallback file: {err}");
        }
        Ok(())
    }

    fn delete(&self) -> std::io::Result<bool> {
        let key = compute_store_key(&self.codex_home)?;
        let keyring_removed = self
            .keyring_store
            .delete(self.namespace.keyring_service(), &key)
            .map_err(|err| {
                std::io::Error::other(format!("failed to delete auth from keyring: {err}"))
            })?;
        let file_removed = delete_file_if_exists(&self.codex_home, self.namespace)?;
        Ok(keyring_removed || file_removed)
    }

    fn replace_with_backup(&self, auth: &AuthDotJson) -> std::io::Result<String> {
        let key = compute_store_key(&self.codex_home)?;
        let current = self.load()?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "credential destination is empty",
            )
        })?;
        let backup_key = format!("{key}|import-backup|{}", auth_backup_id());
        let serialized = serde_json::to_string(&current).map_err(std::io::Error::other)?;
        self.save_to_keyring(&backup_key, &serialized)?;
        if let Err(error) = self.save(auth) {
            let _ = self
                .keyring_store
                .delete(self.namespace.keyring_service(), &backup_key);
            return Err(error);
        }
        Ok(format!(
            "keyring:{}:{backup_key}",
            self.namespace.keyring_service()
        ))
    }
}

#[derive(Clone)]
struct SecretsKeyringAuthStorage {
    codex_home: PathBuf,
    direct_storage: DirectKeyringAuthStorage,
    secrets_manager: SecretsManager,
}

impl Debug for SecretsKeyringAuthStorage {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("SecretsKeyringAuthStorage")
            .field("codex_home", &self.codex_home)
            .finish_non_exhaustive()
    }
}

impl SecretsKeyringAuthStorage {
    #[cfg(test)]
    fn new(codex_home: PathBuf, keyring_store: Arc<dyn KeyringStore>) -> Self {
        Self::new_in_namespace(codex_home, keyring_store, AuthStorageNamespace::Moedex)
    }

    fn new_in_namespace(
        codex_home: PathBuf,
        keyring_store: Arc<dyn KeyringStore>,
        namespace: AuthStorageNamespace,
    ) -> Self {
        let direct_storage = DirectKeyringAuthStorage::new_in_namespace(
            codex_home.clone(),
            Arc::clone(&keyring_store),
            namespace,
        );
        let secrets_manager = SecretsManager::new_with_keyring_store_and_namespace(
            codex_home.clone(),
            SecretsBackendKind::Local,
            keyring_store,
            namespace.local_secrets_namespace(),
        );
        Self {
            codex_home,
            direct_storage,
            secrets_manager,
        }
    }
}

impl AuthStorageBackend for SecretsKeyringAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        match self
            .secrets_manager
            .get(&SecretScope::Global, &CODEX_AUTH_SECRET_NAME)
            .map_err(|err| {
                std::io::Error::other(format!(
                    "failed to load CLI auth from encrypted auth storage: {err}"
                ))
            })? {
            Some(serialized) => serde_json::from_str(&serialized).map(Some).map_err(|err| {
                std::io::Error::other(format!(
                    "failed to deserialize CLI auth from encrypted auth storage: {err}"
                ))
            }),
            None => Ok(None),
        }
    }

    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        let serialized = serde_json::to_string(auth).map_err(std::io::Error::other)?;
        self.secrets_manager
            .set(&SecretScope::Global, &CODEX_AUTH_SECRET_NAME, &serialized)
            .map_err(|err| {
                let message =
                    format!("failed to write OAuth tokens to encrypted auth storage: {err}");
                warn!("{message}");
                std::io::Error::other(message)
            })?;
        if let Err(err) = delete_file_if_exists(&self.codex_home, self.direct_storage.namespace) {
            warn!("failed to remove CLI auth fallback file: {err}");
        }
        Ok(())
    }

    fn delete(&self) -> std::io::Result<bool> {
        let keyring_removed = self
            .secrets_manager
            .delete(&SecretScope::Global, &CODEX_AUTH_SECRET_NAME)
            .map_err(|err| {
                std::io::Error::other(format!(
                    "failed to delete auth from encrypted auth storage: {err}"
                ))
            })?;
        let direct_removed = self.direct_storage.delete()?;
        Ok(keyring_removed || direct_removed)
    }

    fn replace_with_backup(&self, auth: &AuthDotJson) -> std::io::Result<String> {
        let current = self.load()?.ok_or_else(|| {
            std::io::Error::new(
                std::io::ErrorKind::NotFound,
                "credential destination is empty",
            )
        })?;
        let backup_name = SecretName::new(&format!(
            "MOEDEX_AUTH_IMPORT_BACKUP_{}",
            auth_backup_id().replace('-', "_")
        ))
        .map_err(std::io::Error::other)?;
        let serialized = serde_json::to_string(&current).map_err(std::io::Error::other)?;
        self.secrets_manager
            .set(&SecretScope::Global, &backup_name, &serialized)
            .map_err(std::io::Error::other)?;
        if let Err(error) = self.save(auth) {
            let _ = self
                .secrets_manager
                .delete(&SecretScope::Global, &backup_name);
            return Err(error);
        }
        Ok(format!("encrypted-store:{backup_name}"))
    }
}

#[derive(Clone, Debug)]
struct AutoAuthStorage {
    keyring_storage: Arc<dyn AuthStorageBackend>,
    file_storage: Arc<FileAuthStorage>,
}

impl AutoAuthStorage {
    #[cfg(test)]
    fn new(
        codex_home: PathBuf,
        keyring_store: Arc<dyn KeyringStore>,
        keyring_backend_kind: AuthKeyringBackendKind,
    ) -> Self {
        Self {
            keyring_storage: create_keyring_auth_storage(
                codex_home.clone(),
                keyring_store,
                keyring_backend_kind,
            ),
            file_storage: Arc::new(FileAuthStorage::new_in_namespace(
                codex_home,
                AuthStorageNamespace::Moedex,
            )),
        }
    }
}

impl AuthStorageBackend for AutoAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        match self.keyring_storage.load() {
            Ok(Some(auth)) => Ok(Some(auth)),
            Ok(None) => self.file_storage.load(),
            Err(err) => {
                warn!("failed to load CLI auth from keyring, falling back to file storage: {err}");
                self.file_storage.load()
            }
        }
    }

    fn load_for_import(&self) -> std::io::Result<Option<AuthDotJson>> {
        match self.keyring_storage.load()? {
            Some(auth) => Ok(Some(auth)),
            None => self.file_storage.load(),
        }
    }

    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        match self.keyring_storage.save(auth) {
            Ok(()) => Ok(()),
            Err(err) => {
                warn!("failed to save auth to keyring, falling back to file storage: {err}");
                self.file_storage.save(auth)
            }
        }
    }

    fn delete(&self) -> std::io::Result<bool> {
        // Keyring storage will delete from disk as well
        self.keyring_storage.delete()
    }

    fn replace_with_backup(&self, auth: &AuthDotJson) -> std::io::Result<String> {
        match self.keyring_storage.load() {
            Ok(Some(_)) => self.keyring_storage.replace_with_backup(auth),
            Ok(None) => self.file_storage.replace_with_backup(auth),
            Err(_) => match self.file_storage.load()? {
                Some(_) => self.file_storage.replace_with_backup(auth),
                None => Err(std::io::Error::other(
                    "credential destination is unavailable",
                )),
            },
        }
    }
}

// A global in-memory store for mapping codex_home -> AuthDotJson.
static EPHEMERAL_AUTH_STORE: Lazy<Mutex<HashMap<String, AuthDotJson>>> =
    Lazy::new(|| Mutex::new(HashMap::new()));

#[derive(Clone, Debug)]
struct EphemeralAuthStorage {
    codex_home: PathBuf,
}

impl EphemeralAuthStorage {
    fn new(codex_home: PathBuf) -> Self {
        Self { codex_home }
    }

    fn with_store<F, T>(&self, action: F) -> std::io::Result<T>
    where
        F: FnOnce(&mut HashMap<String, AuthDotJson>, String) -> std::io::Result<T>,
    {
        let key = compute_store_key(&self.codex_home)?;
        let mut store = EPHEMERAL_AUTH_STORE
            .lock()
            .map_err(|_| std::io::Error::other("failed to lock ephemeral auth storage"))?;
        action(&mut store, key)
    }
}

impl AuthStorageBackend for EphemeralAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        self.with_store(|store, key| Ok(store.get(&key).cloned()))
    }

    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        self.with_store(|store, key| {
            store.insert(key, auth.clone());
            Ok(())
        })
    }

    fn delete(&self) -> std::io::Result<bool> {
        self.with_store(|store, key| Ok(store.remove(&key).is_some()))
    }
}

pub(super) fn create_auth_storage(
    codex_home: PathBuf,
    mode: AuthCredentialsStoreMode,
    keyring_backend_kind: AuthKeyringBackendKind,
) -> Arc<dyn AuthStorageBackend> {
    let keyring_store: Arc<dyn KeyringStore> = Arc::new(DefaultKeyringStore);
    if mode == AuthCredentialsStoreMode::Ephemeral {
        return create_auth_storage_with_store(
            codex_home,
            mode,
            keyring_store,
            keyring_backend_kind,
        );
    }
    Arc::new(GuardedAuthStorage {
        backend: create_auth_storage_with_store(
            codex_home.clone(),
            mode,
            keyring_store,
            keyring_backend_kind,
        ),
        home: codex_home,
    })
}

// One guard encloses the complete backend write, including Auto fallback and
// encrypted/direct cleanup, so nested storage adapters do not reacquire locks.
#[derive(Debug)]
struct GuardedAuthStorage {
    home: PathBuf,
    backend: Arc<dyn AuthStorageBackend>,
}

impl AuthStorageBackend for GuardedAuthStorage {
    fn load(&self) -> std::io::Result<Option<AuthDotJson>> {
        self.backend.load()
    }
    fn save(&self, auth: &AuthDotJson) -> std::io::Result<()> {
        let _guard = codex_diagnostics::acquire_selected_home_write_guard(&self.home)?;
        self.backend.save(auth)
    }
    fn delete(&self) -> std::io::Result<bool> {
        let _guard = codex_diagnostics::acquire_selected_home_write_guard(&self.home)?;
        self.backend.delete()
    }
    fn replace_with_backup(&self, auth: &AuthDotJson) -> std::io::Result<String> {
        let _guard = codex_diagnostics::acquire_selected_home_write_guard(&self.home)?;
        self.backend.replace_with_backup(auth)
    }
}

fn create_auth_storage_with_store(
    codex_home: PathBuf,
    mode: AuthCredentialsStoreMode,
    keyring_store: Arc<dyn KeyringStore>,
    keyring_backend_kind: AuthKeyringBackendKind,
) -> Arc<dyn AuthStorageBackend> {
    create_auth_storage_with_store_and_namespace(
        codex_home,
        mode,
        keyring_store,
        keyring_backend_kind,
        AuthStorageNamespace::Moedex,
    )
}

pub(super) fn create_auth_storage_with_store_and_namespace(
    codex_home: PathBuf,
    mode: AuthCredentialsStoreMode,
    keyring_store: Arc<dyn KeyringStore>,
    keyring_backend_kind: AuthKeyringBackendKind,
    namespace: AuthStorageNamespace,
) -> Arc<dyn AuthStorageBackend> {
    match mode {
        AuthCredentialsStoreMode::File => {
            Arc::new(FileAuthStorage::new_in_namespace(codex_home, namespace))
        }
        AuthCredentialsStoreMode::Keyring => create_keyring_auth_storage_in_namespace(
            codex_home,
            keyring_store,
            keyring_backend_kind,
            namespace,
        ),
        AuthCredentialsStoreMode::Auto => {
            let keyring_storage = create_keyring_auth_storage_in_namespace(
                codex_home.clone(),
                keyring_store,
                keyring_backend_kind,
                namespace,
            );
            Arc::new(AutoAuthStorage {
                keyring_storage,
                file_storage: Arc::new(FileAuthStorage::new_in_namespace(codex_home, namespace)),
            })
        }
        AuthCredentialsStoreMode::Ephemeral => Arc::new(EphemeralAuthStorage::new(codex_home)),
    }
}

#[cfg(test)]
fn create_keyring_auth_storage(
    codex_home: PathBuf,
    keyring_store: Arc<dyn KeyringStore>,
    keyring_backend_kind: AuthKeyringBackendKind,
) -> Arc<dyn AuthStorageBackend> {
    create_keyring_auth_storage_in_namespace(
        codex_home,
        keyring_store,
        keyring_backend_kind,
        AuthStorageNamespace::Moedex,
    )
}

fn create_keyring_auth_storage_in_namespace(
    codex_home: PathBuf,
    keyring_store: Arc<dyn KeyringStore>,
    keyring_backend_kind: AuthKeyringBackendKind,
    namespace: AuthStorageNamespace,
) -> Arc<dyn AuthStorageBackend> {
    match keyring_backend_kind {
        AuthKeyringBackendKind::Direct => Arc::new(DirectKeyringAuthStorage::new_in_namespace(
            codex_home,
            keyring_store,
            namespace,
        )),
        AuthKeyringBackendKind::Secrets => Arc::new(SecretsKeyringAuthStorage::new_in_namespace(
            codex_home,
            keyring_store,
            namespace,
        )),
    }
}

#[cfg(test)]
#[path = "storage_tests.rs"]
mod tests;
