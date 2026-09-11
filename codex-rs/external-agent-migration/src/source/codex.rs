use std::fs;
use std::io;
use std::path::Path;
use std::path::PathBuf;

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
