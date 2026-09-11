use codex_config::types::AuthCredentialsStoreMode;
use codex_config::types::AuthKeyringBackendKind;
use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

#[derive(Clone, Copy, Default)]
pub(crate) struct AuthStorageConfig {
    pub mode: AuthCredentialsStoreMode,
    pub keyring_backend: AuthKeyringBackendKind,
}

pub(crate) fn auth_storage_config(home: &Path) -> io::Result<AuthStorageConfig> {
    let raw = match fs::read_to_string(home.join("config.toml")) {
        Ok(raw) => raw,
        Err(error) if error.kind() == io::ErrorKind::NotFound => {
            return Ok(AuthStorageConfig::default());
        }
        Err(error) => {
            return Err(io::Error::new(
                error.kind(),
                "auth storage config is unreadable",
            ));
        }
    };
    let value = raw.parse::<toml::Value>().map_err(|_| {
        io::Error::new(io::ErrorKind::InvalidData, "auth storage config is invalid")
    })?;
    let mode = match value.get("cli_auth_credentials_store") {
        Some(value) => value.clone().try_into().map_err(|_| {
            io::Error::new(io::ErrorKind::InvalidData, "auth storage mode is invalid")
        })?,
        None => AuthCredentialsStoreMode::default(),
    };
    let uses_secrets = match value
        .get("features")
        .and_then(|features| features.get("secret_auth_storage"))
    {
        Some(value) => value.as_bool().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                "auth storage feature setting is invalid",
            )
        })?,
        None => false,
    };
    Ok(AuthStorageConfig {
        mode,
        keyring_backend: if uses_secrets {
            AuthKeyringBackendKind::Secrets
        } else {
            AuthKeyringBackendKind::Direct
        },
    })
}

#[derive(Clone, Copy)]
pub(crate) struct RolloutDiscoveryLimits {
    pub max_directories: usize,
    pub max_files: usize,
    pub max_item_bytes: u64,
    pub max_total_bytes: u64,
}

pub(crate) fn discover_rollouts(
    source_home: &Path,
    limits: RolloutDiscoveryLimits,
) -> io::Result<Vec<PathBuf>> {
    let mut files = Vec::new();
    let mut directories = Vec::new();
    for root in [
        source_home.join("sessions"),
        source_home.join("archived_sessions"),
    ] {
        if root.is_dir() {
            if directories.len() >= limits.max_directories {
                return Err(limit_error("directory", limits.max_directories));
            }
            directories.push(root);
        }
    }
    let mut directory_count = directories.len();
    let mut total_bytes = 0_u64;
    while let Some(directory) = directories.pop() {
        for entry in fs::read_dir(directory)? {
            let entry = entry?;
            let file_type = entry.file_type()?;
            let path = entry.path();
            if file_type.is_dir() {
                directory_count = directory_count.saturating_add(1);
                if directory_count > limits.max_directories {
                    return Err(limit_error("directory", limits.max_directories));
                }
                directories.push(path);
                continue;
            }
            if !file_type.is_file()
                || path
                    .extension()
                    .is_none_or(|extension| extension != "jsonl")
            {
                continue;
            }
            if files.len() >= limits.max_files {
                return Err(limit_error("item", limits.max_files));
            }
            let item_bytes = entry.metadata()?.len();
            if item_bytes > limits.max_item_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Codex rollout exceeds the {}-byte item limit: {}",
                        limits.max_item_bytes,
                        path.display()
                    ),
                ));
            }
            total_bytes = total_bytes.checked_add(item_bytes).ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex rollout discovery byte count overflowed",
                )
            })?;
            if total_bytes > limits.max_total_bytes {
                return Err(io::Error::new(
                    io::ErrorKind::InvalidData,
                    format!(
                        "Codex rollout discovery exceeds the {}-byte aggregate limit",
                        limits.max_total_bytes
                    ),
                ));
            }
            files.push(path);
        }
    }
    files.sort();
    files.dedup();
    Ok(files)
}

fn limit_error(kind: &str, limit: usize) -> io::Error {
    io::Error::new(
        io::ErrorKind::InvalidData,
        format!("Codex rollout discovery exceeds the {limit}-{kind} limit"),
    )
}
