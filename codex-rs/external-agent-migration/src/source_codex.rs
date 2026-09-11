use crate::config_values::sanitized_codex_config;
use crate::model::CodexImportSelection;
use crate::model::ConflictPolicy;
use crate::sessions::records_codex::codex_rollout_thread_id;
use crate::sessions::records_codex::validate_codex_rollout;
use crate::source::codex::discover_rollouts;
use codex_utils_absolute_path::AbsolutePathBuf;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::collections::HashSet;
use std::fs;
#[cfg(unix)]
use std::fs::File;
use std::fs::OpenOptions;
use std::io;
use std::io::Write;
use std::path::Path;
use std::path::PathBuf;
use std::sync::Mutex;
use std::sync::OnceLock;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

const CONFIG_FILE: &str = "config.toml";
const AUTH_FILE: &str = "auth.json";
const IMPORT_LEDGER_FILE: &str = "moedex_codex_imports.json";
const BACKUP_DIR: &str = "codex-import-backups";

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportItemKind {
    Settings,
    Session,
    Credentials,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportPreviewItem {
    pub kind: ImportItemKind,
    pub relative_path: PathBuf,
    pub source_sha256: Option<String>,
    pub conflict: bool,
    pub requires_review: bool,
    pub review_reasons: Vec<String>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportPreview {
    pub id: String,
    pub selection: CodexImportSelection,
    pub items: Vec<ImportPreviewItem>,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportReport {
    pub preview_id: String,
    pub imported: usize,
    pub skipped: usize,
    pub already_present: usize,
    pub unsupported: usize,
    pub backups: Vec<PathBuf>,
    pub source_hashes: Vec<String>,
}

#[derive(Clone)]
struct PlannedItem {
    preview: ImportPreviewItem,
    source: Option<PathBuf>,
    payload: Option<Vec<u8>>,
}

#[derive(Clone)]
struct ImportPlan {
    source: PathBuf,
    destination: PathBuf,
    preview: ImportPreview,
    items: Vec<PlannedItem>,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
struct ImportLedgerRecord {
    relative_path: PathBuf,
    source_sha256: String,
}

#[derive(Clone, Debug, Default, Deserialize, Eq, PartialEq, Serialize)]
struct ImportLedger {
    records: Vec<ImportLedgerRecord>,
}

static PREVIEWS: OnceLock<Mutex<HashMap<String, ImportPlan>>> = OnceLock::new();
static TEMP_SEQUENCE: AtomicU64 = AtomicU64::new(0);

pub async fn preview_codex_import(
    source: AbsolutePathBuf,
    destination: AbsolutePathBuf,
    selection: CodexImportSelection,
) -> io::Result<ImportPreview> {
    let source = canonical_home(source.as_path())?;
    let destination = canonical_home(destination.as_path())?;
    ensure_distinct_homes(&source, &destination)?;
    if !source.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::NotFound,
            format!("Codex source home does not exist: {}", source.display()),
        ));
    }

    let mut items = Vec::new();
    if selection.settings {
        let config = source.join(CONFIG_FILE);
        if config.is_file() {
            let raw = fs::read_to_string(&config)?;
            let (payload, review_reasons) = sanitized_codex_config(&raw)?;
            items.push(planned_file(
                ImportItemKind::Settings,
                PathBuf::from(CONFIG_FILE),
                config,
                payload,
                &destination,
                review_reasons,
            )?);
        }
    }
    if selection.sessions {
        let destination_thread_ids = discover_rollouts(&destination)?
            .into_iter()
            .filter_map(|path| codex_rollout_thread_id(&path).ok())
            .collect::<HashSet<_>>();
        for rollout in discover_rollouts(&source)? {
            let relative_path = rollout
                .strip_prefix(&source)
                .map_err(io::Error::other)?
                .to_path_buf();
            if let Err(error) = validate_codex_rollout(&rollout) {
                items.push(PlannedItem {
                    preview: ImportPreviewItem {
                        kind: ImportItemKind::Session,
                        relative_path,
                        source_sha256: Some(sha256_file(&rollout)?),
                        conflict: false,
                        requires_review: true,
                        review_reasons: vec![format!(
                            "incompatible rollout requires manual review: {error}"
                        )],
                    },
                    source: None,
                    payload: None,
                });
                continue;
            }
            let thread_id = codex_rollout_thread_id(&rollout)?;
            if destination_thread_ids.contains(&thread_id)
                && !destination.join(&relative_path).exists()
            {
                items.push(PlannedItem {
                    preview: ImportPreviewItem {
                        kind: ImportItemKind::Session,
                        relative_path,
                        source_sha256: Some(sha256_file(&rollout)?),
                        conflict: true,
                        requires_review: true,
                        review_reasons: vec![
                            "destination already contains this thread ID; retained destination session"
                                .to_string(),
                        ],
                    },
                    source: None,
                    payload: None,
                });
                continue;
            }
            let payload = fs::read(&rollout)?;
            items.push(planned_file(
                ImportItemKind::Session,
                relative_path,
                rollout,
                payload,
                &destination,
                Vec::new(),
            )?);
        }
    }
    if selection.credentials {
        items.push(PlannedItem {
            preview: ImportPreviewItem {
                kind: ImportItemKind::Credentials,
                relative_path: PathBuf::from(AUTH_FILE),
                source_sha256: None,
                conflict: false,
                requires_review: true,
                review_reasons: vec![
                    "credential transfer is unavailable; sign in to Moedex instead".to_string(),
                ],
            },
            source: None,
            payload: None,
        });
    }

    let id = preview_id(&source, &destination, &selection, &items);
    let preview = ImportPreview {
        id: id.clone(),
        selection,
        items: items.iter().map(|item| item.preview.clone()).collect(),
    };
    previews()
        .lock()
        .map_err(|_| io::Error::other("Codex import preview registry is unavailable"))?
        .insert(
            id,
            ImportPlan {
                source,
                destination,
                preview: preview.clone(),
                items,
            },
        );
    Ok(preview)
}

pub async fn apply_codex_import(
    preview_id: &str,
    selection: CodexImportSelection,
) -> io::Result<ImportReport> {
    let plan = previews()
        .lock()
        .map_err(|_| io::Error::other("Codex import preview registry is unavailable"))?
        .get(preview_id)
        .cloned()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Codex import preview expired"))?;
    if plan.preview.selection != selection {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Codex import selection differs from its preview",
        ));
    }
    let source = canonical_home(&plan.source)?;
    let destination = canonical_home(&plan.destination)?;
    ensure_distinct_homes(&source, &destination)?;
    fs::create_dir_all(&destination)?;
    let mut ledger = load_ledger(&destination)?;
    let mut report = ImportReport {
        preview_id: preview_id.to_string(),
        ..ImportReport::default()
    };

    for item in plan.items {
        let Some(payload) = item.payload else {
            report.unsupported = report.unsupported.saturating_add(1);
            continue;
        };
        let Some(source_path) = item.source else {
            return Err(io::Error::other("Codex import file plan has no source"));
        };
        let Some(expected_hash) = item.preview.source_sha256.as_deref() else {
            return Err(io::Error::other(
                "Codex import file plan has no source hash",
            ));
        };
        if sha256_file(&source_path)? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source changed after preview: {}", source_path.display()),
            ));
        }
        report.source_hashes.push(expected_hash.to_string());
        let target = destination.join(&item.preview.relative_path);
        ensure_target_beneath_home(&destination, &target)?;
        if target.is_file() && sha256_file(&target)? == sha256_bytes(&payload) {
            report.already_present = report.already_present.saturating_add(1);
            record_import(&destination, &mut ledger, &item.preview, expected_hash)?;
            continue;
        }
        if target.exists() && selection.conflict_policy == ConflictPolicy::Skip {
            report.skipped = report.skipped.saturating_add(1);
            continue;
        }
        if let Some(parent) = target.parent() {
            fs::create_dir_all(parent)?;
        }
        let backup = if target.exists() {
            let backup = available_backup_path(
                &destination
                    .join(BACKUP_DIR)
                    .join(preview_id)
                    .join(&item.preview.relative_path),
            );
            if let Some(parent) = backup.parent() {
                fs::create_dir_all(parent)?;
            }
            ensure_target_beneath_home(&destination, &backup)?;
            fs::rename(&target, &backup)?;
            Some(backup)
        } else {
            None
        };
        if let Err(error) = write_atomic(&target, &payload, preview_id) {
            if let Some(backup) = backup.as_ref() {
                let _ = fs::rename(backup, &target);
            }
            return Err(error);
        }
        if let Some(backup) = backup {
            report.backups.push(backup);
        }
        record_import(&destination, &mut ledger, &item.preview, expected_hash)?;
        report.imported = report.imported.saturating_add(1);
    }
    Ok(report)
}

fn planned_file(
    kind: ImportItemKind,
    relative_path: PathBuf,
    source: PathBuf,
    payload: Vec<u8>,
    destination: &Path,
    review_reasons: Vec<String>,
) -> io::Result<PlannedItem> {
    let source_sha256 = sha256_file(&source)?;
    Ok(PlannedItem {
        preview: ImportPreviewItem {
            kind,
            conflict: destination.join(&relative_path).exists(),
            relative_path,
            source_sha256: Some(source_sha256),
            requires_review: !review_reasons.is_empty(),
            review_reasons,
        },
        source: Some(source),
        payload: Some(payload),
    })
}

fn ensure_distinct_homes(source: &Path, destination: &Path) -> io::Result<()> {
    if source == destination {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            "Codex source and Moedex destination resolve to the same home",
        ));
    }
    Ok(())
}

fn canonical_home(path: &Path) -> io::Result<PathBuf> {
    if path.exists() {
        return fs::canonicalize(path);
    }
    let parent = path
        .parent()
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "home path has no parent"))?;
    let file_name = path.file_name().ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidInput,
            "home path has no final component",
        )
    })?;
    Ok(fs::canonicalize(parent)?.join(file_name))
}

fn ensure_target_beneath_home(home: &Path, target: &Path) -> io::Result<()> {
    let mut existing = target;
    while !existing.exists() {
        existing = existing.parent().ok_or_else(|| {
            io::Error::new(
                io::ErrorKind::InvalidInput,
                "import target has no existing parent",
            )
        })?;
    }
    let canonical_existing = fs::canonicalize(existing)?;
    if !canonical_existing.starts_with(home) {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!(
                "import target escapes the Moedex home: {}",
                target.display()
            ),
        ));
    }
    Ok(())
}

fn previews() -> &'static Mutex<HashMap<String, ImportPlan>> {
    PREVIEWS.get_or_init(|| Mutex::new(HashMap::new()))
}

fn available_backup_path(preferred: &Path) -> PathBuf {
    if !preferred.exists() {
        return preferred.to_path_buf();
    }
    for index in 1_u64.. {
        let candidate = preferred.with_extension(format!("backup.{index}"));
        if !candidate.exists() {
            return candidate;
        }
    }
    unreachable!("an unbounded backup suffix search cannot be exhausted")
}

fn preview_id(
    source: &Path,
    destination: &Path,
    selection: &CodexImportSelection,
    items: &[PlannedItem],
) -> String {
    let mut digest = Sha256::new();
    digest.update(source.as_os_str().as_encoded_bytes());
    digest.update(destination.as_os_str().as_encoded_bytes());
    digest.update(serde_json::to_vec(selection).unwrap_or_default());
    for item in items {
        digest.update(item.preview.relative_path.as_os_str().as_encoded_bytes());
        if let Some(hash) = &item.preview.source_sha256 {
            digest.update(hash.as_bytes());
        }
    }
    format!("{:x}", digest.finalize())
}

fn sha256_file(path: &Path) -> io::Result<String> {
    Ok(sha256_bytes(&fs::read(path)?))
}

fn sha256_bytes(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

fn load_ledger(destination: &Path) -> io::Result<ImportLedger> {
    let path = destination.join(IMPORT_LEDGER_FILE);
    if !path.is_file() {
        return Ok(ImportLedger::default());
    }
    serde_json::from_slice(&fs::read(path)?)
        .map_err(|err| io::Error::new(io::ErrorKind::InvalidData, err))
}

fn record_import(
    destination: &Path,
    ledger: &mut ImportLedger,
    item: &ImportPreviewItem,
    source_sha256: &str,
) -> io::Result<()> {
    if !ledger.records.iter().any(|record| {
        record.relative_path == item.relative_path && record.source_sha256 == source_sha256
    }) {
        ledger.records.push(ImportLedgerRecord {
            relative_path: item.relative_path.clone(),
            source_sha256: source_sha256.to_string(),
        });
    }
    let payload = serde_json::to_vec_pretty(ledger).map_err(io::Error::other)?;
    write_atomic(&destination.join(IMPORT_LEDGER_FILE), &payload, "ledger")
}

fn write_atomic(path: &Path, payload: &[u8], suffix: &str) -> io::Result<()> {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .ok_or_else(|| io::Error::new(io::ErrorKind::InvalidInput, "invalid target filename"))?;
    let sequence = TEMP_SEQUENCE.fetch_add(1, Ordering::Relaxed);
    let process_id = std::process::id();
    let temp = path.with_file_name(format!(".{file_name}.{suffix}.{process_id}.{sequence}.tmp"));
    let mut file = OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(&temp)?;
    if let Err(error) = (|| {
        file.write_all(payload)?;
        file.sync_all()?;
        drop(file);
        fs::rename(&temp, path)
    })() {
        let _ = fs::remove_file(&temp);
        return Err(error);
    }
    sync_parent(path)
}

#[cfg(unix)]
fn sync_parent(path: &Path) -> io::Result<()> {
    if let Some(parent) = path.parent() {
        File::open(parent)?.sync_all()?;
    }
    Ok(())
}

#[cfg(not(unix))]
fn sync_parent(_path: &Path) -> io::Result<()> {
    Ok(())
}

#[cfg(test)]
#[path = "source_codex_tests.rs"]
mod tests;
