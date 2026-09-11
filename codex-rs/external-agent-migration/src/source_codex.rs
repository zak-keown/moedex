use crate::config_values::sanitized_codex_config;
use crate::model::CodexImportSelection;
use crate::model::ConflictPolicy;
use crate::sessions::records_codex::codex_rollout_thread_id;
use crate::sessions::records_codex::validate_codex_rollout;
use crate::source::codex::RolloutDiscoveryLimits;
use crate::source::codex::discover_rollouts;
use codex_protocol::ThreadId;
use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_path::write_atomically as replace_text_atomically;
use serde::Deserialize;
use serde::Serialize;
use sha2::Digest;
use sha2::Sha256;
use std::collections::HashMap;
use std::collections::HashSet;
use std::collections::VecDeque;
use std::fs;
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
const MAX_PREVIEWS: usize = 8;
const MAX_PREVIEW_ITEMS: usize = 1_000;
const MAX_ITEM_BYTES: usize = 20 * 1024 * 1024;
const MAX_PREVIEW_BYTES: usize = 256 * 1024 * 1024;
const MAX_REGISTRY_BYTES: usize = 512 * 1024 * 1024;
const MAX_DISCOVERY_DIRECTORIES: usize = 4_096;

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
    pub items: Vec<ImportItemOutcome>,
}

#[derive(Clone, Copy, Debug, Deserialize, Eq, PartialEq, Serialize)]
#[serde(rename_all = "snake_case")]
pub enum ImportDisposition {
    Imported,
    SkippedConflict,
    AlreadyPresent,
    Unsupported,
}

#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct ImportItemOutcome {
    pub kind: ImportItemKind,
    pub relative_path: PathBuf,
    pub disposition: ImportDisposition,
    pub source_sha256: Option<String>,
    pub reason: Option<String>,
}

#[derive(Clone)]
struct PlannedItem {
    preview: ImportPreviewItem,
    source: Option<PathBuf>,
    payload: Option<Vec<u8>>,
    thread_id: Option<ThreadId>,
}

#[derive(Clone)]
struct ImportPlan {
    source: PathBuf,
    destination: PathBuf,
    preview: ImportPreview,
    items: Vec<PlannedItem>,
    stored_bytes: usize,
}

#[derive(Default)]
struct PreviewRegistry {
    plans: HashMap<String, ImportPlan>,
    order: VecDeque<String>,
    stored_bytes: usize,
}

struct FileSnapshot {
    path: PathBuf,
    bytes: Vec<u8>,
    sha256: String,
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

static PREVIEWS: OnceLock<Mutex<PreviewRegistry>> = OnceLock::new();
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
    let mut stored_bytes = 0_usize;
    if selection.settings {
        let config = source.join(CONFIG_FILE);
        if config.is_file() {
            let snapshot = read_stable_snapshot(&config)?;
            let raw = std::str::from_utf8(&snapshot.bytes).map_err(|_| {
                io::Error::new(io::ErrorKind::InvalidData, "Codex config is not UTF-8")
            })?;
            let (payload, review_reasons) = sanitized_codex_config(raw)?;
            let item = planned_snapshot(
                ImportItemKind::Settings,
                PathBuf::from(CONFIG_FILE),
                snapshot,
                payload,
                &destination,
                review_reasons,
                None,
            )?;
            push_planned_item(&mut items, &mut stored_bytes, item)?;
        }
    }
    if selection.sessions {
        let destination_thread_ids = destination_thread_ids(&destination)?;
        let reserved_items = usize::from(selection.credentials);
        let max_rollouts = MAX_PREVIEW_ITEMS.saturating_sub(items.len() + reserved_items);
        let max_rollout_bytes = MAX_PREVIEW_BYTES.saturating_sub(stored_bytes);
        for rollout in discover_rollouts(
            &source,
            rollout_discovery_limits(max_rollouts, max_rollout_bytes),
        )? {
            let relative_path = rollout
                .strip_prefix(&source)
                .map_err(io::Error::other)?
                .to_path_buf();
            let snapshot = read_stable_snapshot(&rollout)?;
            if let Err(error) = validate_codex_rollout(&snapshot.bytes) {
                let item = PlannedItem {
                    preview: ImportPreviewItem {
                        kind: ImportItemKind::Session,
                        relative_path,
                        source_sha256: Some(snapshot.sha256),
                        conflict: false,
                        requires_review: true,
                        review_reasons: vec![format!(
                            "incompatible rollout requires manual review: {error}"
                        )],
                    },
                    source: None,
                    payload: None,
                    thread_id: None,
                };
                push_planned_item(&mut items, &mut stored_bytes, item)?;
                continue;
            }
            let thread_id = codex_rollout_thread_id(&snapshot.bytes)?;
            if destination_thread_ids.contains(&thread_id) {
                let item = PlannedItem {
                    preview: ImportPreviewItem {
                        kind: ImportItemKind::Session,
                        relative_path,
                        source_sha256: Some(snapshot.sha256),
                        conflict: true,
                        requires_review: true,
                        review_reasons: vec![
                            "destination already contains this thread ID; retained destination session"
                                .to_string(),
                        ],
                    },
                    source: None,
                    payload: None,
                    thread_id: Some(thread_id),
                };
                push_planned_item(&mut items, &mut stored_bytes, item)?;
                continue;
            }
            let payload = snapshot.bytes.clone();
            let item = planned_snapshot(
                ImportItemKind::Session,
                relative_path,
                snapshot,
                payload,
                &destination,
                Vec::new(),
                Some(thread_id),
            )?;
            push_planned_item(&mut items, &mut stored_bytes, item)?;
        }
    }
    if selection.credentials {
        let item = PlannedItem {
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
            thread_id: None,
        };
        push_planned_item(&mut items, &mut stored_bytes, item)?;
    }

    validate_preview_bounds(items.len(), stored_bytes)?;

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
                stored_bytes,
            },
        );
    Ok(preview)
}

fn validate_preview_bounds(item_count: usize, stored_bytes: usize) -> io::Result<()> {
    if item_count > MAX_PREVIEW_ITEMS {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Codex import preview exceeds the {MAX_PREVIEW_ITEMS}-item limit"),
        ));
    }
    if stored_bytes > MAX_PREVIEW_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Codex import preview exceeds the {MAX_PREVIEW_BYTES}-byte limit"),
        ));
    }
    Ok(())
}

fn push_planned_item(
    items: &mut Vec<PlannedItem>,
    stored_bytes: &mut usize,
    item: PlannedItem,
) -> io::Result<()> {
    let item_bytes = item.payload.as_ref().map_or(0, Vec::len);
    let next_bytes = stored_bytes.checked_add(item_bytes).ok_or_else(|| {
        io::Error::new(
            io::ErrorKind::InvalidData,
            "Codex import preview byte count overflowed",
        )
    })?;
    validate_preview_bounds(items.len().saturating_add(1), next_bytes)?;
    *stored_bytes = next_bytes;
    items.push(item);
    Ok(())
}

pub async fn apply_codex_import(
    preview_id: &str,
    selection: CodexImportSelection,
) -> io::Result<ImportReport> {
    let plan = previews()
        .lock()
        .map_err(|_| io::Error::other("Codex import preview registry is unavailable"))?
        .plans
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
            report.items.push(item_outcome(
                &item.preview,
                ImportDisposition::Unsupported,
                item.preview.review_reasons.first().cloned(),
            ));
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
        if sha256_file_bounded(&source_path)? != expected_hash {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!("source changed after preview: {}", source_path.display()),
            ));
        }
        report.source_hashes.push(expected_hash.to_string());
        let target = destination.join(&item.preview.relative_path);
        ensure_target_beneath_home(&destination, &target)?;
        if let Some(thread_id) = item.thread_id {
            let destination_thread_ids = destination_thread_ids(&destination)?;
            if destination_thread_ids.contains(&thread_id) {
                report.skipped = report.skipped.saturating_add(1);
                report.items.push(item_outcome(
                    &item.preview,
                    ImportDisposition::SkippedConflict,
                    Some(
                        "destination already contains this thread ID; retained destination session"
                            .to_string(),
                    ),
                ));
                continue;
            }
        }
        if target.is_file() && sha256_file_bounded(&target)? == sha256_bytes(&payload) {
            report.already_present = report.already_present.saturating_add(1);
            record_import(&destination, &mut ledger, &item.preview, expected_hash)?;
            report.items.push(item_outcome(
                &item.preview,
                ImportDisposition::AlreadyPresent,
                None,
            ));
            continue;
        }
        if target.exists()
            && (item.preview.kind == ImportItemKind::Session
                || selection.conflict_policy == ConflictPolicy::Skip)
        {
            report.skipped = report.skipped.saturating_add(1);
            report.items.push(item_outcome(
                &item.preview,
                ImportDisposition::SkippedConflict,
                Some("destination path already exists; retained destination item".to_string()),
            ));
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
        report.items.push(item_outcome(
            &item.preview,
            ImportDisposition::Imported,
            None,
        ));
    }
    cancel_codex_import(preview_id)?;
    Ok(report)
}

pub fn cancel_codex_import(preview_id: &str) -> io::Result<bool> {
    previews()
        .lock()
        .map_err(|_| io::Error::other("Codex import preview registry is unavailable"))
        .map(|mut previews| previews.remove(preview_id).is_some())
}

fn planned_snapshot(
    kind: ImportItemKind,
    relative_path: PathBuf,
    snapshot: FileSnapshot,
    payload: Vec<u8>,
    destination: &Path,
    review_reasons: Vec<String>,
    thread_id: Option<ThreadId>,
) -> io::Result<PlannedItem> {
    if snapshot.bytes.len() > MAX_ITEM_BYTES || payload.len() > MAX_ITEM_BYTES {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Codex import item exceeds the {MAX_ITEM_BYTES}-byte limit"),
        ));
    }
    Ok(PlannedItem {
        preview: ImportPreviewItem {
            kind,
            conflict: destination.join(&relative_path).exists(),
            relative_path,
            source_sha256: Some(snapshot.sha256),
            requires_review: !review_reasons.is_empty(),
            review_reasons,
        },
        source: Some(snapshot.path),
        payload: Some(payload),
        thread_id,
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

fn previews() -> &'static Mutex<PreviewRegistry> {
    PREVIEWS.get_or_init(|| Mutex::new(PreviewRegistry::default()))
}

impl PreviewRegistry {
    fn insert(&mut self, id: String, plan: ImportPlan) {
        self.remove(&id);
        while self.plans.len() >= MAX_PREVIEWS
            || self.stored_bytes.saturating_add(plan.stored_bytes) > MAX_REGISTRY_BYTES
        {
            let Some(oldest) = self.order.pop_front() else {
                break;
            };
            if let Some(evicted) = self.plans.remove(&oldest) {
                self.stored_bytes = self.stored_bytes.saturating_sub(evicted.stored_bytes);
            }
        }
        self.stored_bytes = self.stored_bytes.saturating_add(plan.stored_bytes);
        self.order.push_back(id.clone());
        self.plans.insert(id, plan);
    }

    fn remove(&mut self, id: &str) -> Option<ImportPlan> {
        let removed = self.plans.remove(id)?;
        self.stored_bytes = self.stored_bytes.saturating_sub(removed.stored_bytes);
        self.order.retain(|candidate| candidate != id);
        Some(removed)
    }
}

fn item_outcome(
    item: &ImportPreviewItem,
    disposition: ImportDisposition,
    reason: Option<String>,
) -> ImportItemOutcome {
    ImportItemOutcome {
        kind: item.kind,
        relative_path: item.relative_path.clone(),
        disposition,
        source_sha256: item.source_sha256.clone(),
        reason,
    }
}

fn destination_thread_ids(destination: &Path) -> io::Result<HashSet<ThreadId>> {
    destination_thread_ids_with_snapshot_reader(destination, read_stable_snapshot)
}

fn destination_thread_ids_with_snapshot_reader(
    destination: &Path,
    mut read_snapshot: impl FnMut(&Path) -> io::Result<FileSnapshot>,
) -> io::Result<HashSet<ThreadId>> {
    let mut thread_ids = HashSet::new();
    let mut stored_bytes = 0_usize;
    for path in discover_rollouts(
        destination,
        rollout_discovery_limits(MAX_PREVIEW_ITEMS, MAX_PREVIEW_BYTES),
    )? {
        let snapshot = read_snapshot(&path)?;
        stored_bytes = stored_bytes
            .checked_add(snapshot.bytes.len())
            .ok_or_else(|| {
                io::Error::new(
                    io::ErrorKind::InvalidData,
                    "Codex rollout inventory byte count overflowed",
                )
            })?;
        if stored_bytes > MAX_PREVIEW_BYTES {
            return Err(io::Error::new(
                io::ErrorKind::InvalidData,
                format!(
                    "Codex rollout inventory exceeds the {MAX_PREVIEW_BYTES}-byte aggregate limit"
                ),
            ));
        }
        validate_codex_rollout(&snapshot.bytes)?;
        thread_ids.insert(codex_rollout_thread_id(&snapshot.bytes)?);
    }
    Ok(thread_ids)
}

fn rollout_discovery_limits(max_files: usize, max_total_bytes: usize) -> RolloutDiscoveryLimits {
    RolloutDiscoveryLimits {
        max_directories: MAX_DISCOVERY_DIRECTORIES,
        max_files,
        max_item_bytes: MAX_ITEM_BYTES as u64,
        max_total_bytes: max_total_bytes as u64,
    }
}

fn read_stable_snapshot(path: &Path) -> io::Result<FileSnapshot> {
    read_stable_snapshot_with_after_read(path, || {})
}

fn read_stable_snapshot_with_after_read(
    path: &Path,
    after_read: impl FnOnce(),
) -> io::Result<FileSnapshot> {
    use std::io::Read;

    let mut file = File::open(path)?;
    let before = file.metadata()?;
    if before.len() > MAX_ITEM_BYTES as u64 {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("Codex import item exceeds the {MAX_ITEM_BYTES}-byte limit"),
        ));
    }
    let mut bytes = Vec::with_capacity(before.len() as usize);
    file.read_to_end(&mut bytes)?;
    after_read();
    let after = file.metadata()?;
    if before.len() != after.len()
        || before.modified().ok() != after.modified().ok()
        || bytes.len() as u64 != before.len()
    {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!("source changed while previewing: {}", path.display()),
        ));
    }
    Ok(FileSnapshot {
        path: path.to_path_buf(),
        sha256: sha256_bytes(&bytes),
        bytes,
    })
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

fn sha256_file_bounded(path: &Path) -> io::Result<String> {
    Ok(read_stable_snapshot(path)?.sha256)
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
    let payload = serde_json::to_string_pretty(ledger).map_err(io::Error::other)?;
    replace_text_atomically(&destination.join(IMPORT_LEDGER_FILE), &payload)
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
