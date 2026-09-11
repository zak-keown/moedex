use super::*;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

fn abs(path: &std::path::Path) -> AbsolutePathBuf {
    AbsolutePathBuf::from_absolute_path(path).expect("absolute path")
}

fn selection(conflict_policy: ConflictPolicy) -> CodexImportSelection {
    CodexImportSelection {
        settings: true,
        sessions: true,
        credentials: false,
        conflict_policy,
    }
}

#[tokio::test]
async fn preview_and_cancel_leave_both_homes_byte_identical() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(source.join("sessions/2026/09/10")).expect("source sessions");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(source.join("config.toml"), "model = \"gpt-5\"\n").expect("config");
    fs::write(
        source.join("sessions/2026/09/10/rollout-test.jsonl"),
        valid_rollout_line(root.path()),
    )
    .expect("rollout");
    let source_before = hash_tree(&source);
    let destination_before = hash_tree(&destination);

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        selection(ConflictPolicy::Skip),
    )
    .await
    .expect("preview");

    assert!(!preview.items.is_empty());
    drop(preview);
    assert_eq!(source_before, hash_tree(&source));
    assert_eq!(destination_before, hash_tree(&destination));
}

#[tokio::test]
async fn rejects_direct_and_symlinked_source_destination_equality() {
    let root = TempDir::new().expect("tempdir");
    let home = root.path().join("home");
    fs::create_dir_all(&home).expect("home");
    let error = preview_codex_import(abs(&home), abs(&home), selection(ConflictPolicy::Skip))
        .await
        .expect_err("same home must fail");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);

    #[cfg(unix)]
    {
        let alias = root.path().join("alias");
        std::os::unix::fs::symlink(&home, &alias).expect("symlink");
        let error = preview_codex_import(abs(&home), abs(&alias), selection(ConflictPolicy::Skip))
            .await
            .expect_err("symlinked home must fail");
        assert_eq!(error.kind(), std::io::ErrorKind::InvalidInput);
    }
}

#[tokio::test]
async fn settings_import_drops_trust_secrets_and_home_bound_values() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("config.toml"),
        format!(
            "model = \"gpt-5\"\napi_key = \"very-secret\"\nhooks = {{ command = \"{}\" }}\n[projects.\"/tmp/project\"]\ntrust_level = \"trusted\"\n",
            source.join("hook.sh").display()
        ),
    )
    .expect("config");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: true,
            sessions: false,
            credentials: false,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect("preview");
    assert!(format!("{preview:?}").find("very-secret").is_none());
    assert!(preview.items[0].requires_review);

    let report = apply_codex_import(&preview.id, preview.selection.clone())
        .await
        .expect("apply");
    assert_eq!(report.imported, 1);
    let imported = fs::read_to_string(destination.join("config.toml")).expect("imported config");
    assert!(imported.contains("model = \"gpt-5\""));
    assert!(!imported.contains("very-secret"));
    assert!(!imported.contains("trust_level"));
    assert!(!imported.contains("hook.sh"));
}

#[tokio::test]
async fn invalid_config_errors_do_not_render_source_secrets() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("config.toml"),
        "api_key = \"very-secret\"\ninvalid = [",
    )
    .expect("config");

    let error = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: true,
            sessions: false,
            credentials: false,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect_err("invalid config");

    assert!(!error.to_string().contains("very-secret"));
}

#[tokio::test]
async fn credential_free_settings_remove_literal_and_arbitrary_header_maps() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("config.toml"),
        r#"
[mcp_servers.docs]
url = "https://example.com/mcp"
http_headers = { Authorization = "Bearer mcp-secret", "X-Arbitrary" = "header-secret" }

[model_providers.custom]
name = "Custom"
base_url = "https://example.com/v1"
http_headers = { Authorization = "Bearer provider-secret", "X-Other" = "other-secret" }
"#,
    )
    .expect("config");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: true,
            sessions: false,
            credentials: false,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect("preview");
    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");
    let imported = fs::read_to_string(destination.join("config.toml")).expect("config");

    for secret in [
        "mcp-secret",
        "header-secret",
        "provider-secret",
        "other-secret",
    ] {
        assert!(!imported.contains(secret));
    }
    assert!(!imported.contains("http_headers"));
    assert!(imported.contains("https://example.com/v1"));
    assert_eq!(report.items[0].disposition, ImportDisposition::Imported);
}

#[tokio::test]
async fn conflict_skips_by_default_and_replacement_keeps_backup() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(source.join("config.toml"), "model = \"source\"\n").expect("source config");
    fs::write(destination.join("config.toml"), "model = \"destination\"\n")
        .expect("destination config");

    let skipped = preview_codex_import(
        abs(&source),
        abs(&destination),
        selection(ConflictPolicy::Skip),
    )
    .await
    .expect("skip preview");
    let report = apply_codex_import(&skipped.id, skipped.selection.clone())
        .await
        .expect("skip apply");
    assert_eq!(report.skipped, 1);
    assert_eq!(
        fs::read_to_string(destination.join("config.toml")).expect("destination config"),
        "model = \"destination\"\n"
    );

    let replacement = preview_codex_import(
        abs(&source),
        abs(&destination),
        selection(ConflictPolicy::ReplaceWithBackup),
    )
    .await
    .expect("replacement preview");
    let report = apply_codex_import(&replacement.id, replacement.selection.clone())
        .await
        .expect("replacement apply");
    assert_eq!(report.imported, 1);
    assert_eq!(report.backups.len(), 1);
    assert_eq!(
        fs::read_to_string(&report.backups[0]).expect("backup"),
        "model = \"destination\"\n"
    );
}

#[tokio::test]
async fn interrupted_apply_can_be_rerun_without_duplicate_commits() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let rollout = source.join("sessions/2026/09/10/rollout-test.jsonl");
    fs::create_dir_all(rollout.parent().expect("rollout parent")).expect("source sessions");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(source.join("config.toml"), "model = \"gpt-5\"\n").expect("config");
    fs::write(&rollout, valid_rollout_line(root.path())).expect("rollout");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        selection(ConflictPolicy::Skip),
    )
    .await
    .expect("preview");
    fs::write(&rollout, format!("{}\n", valid_rollout_line(root.path()))).expect("mutate rollout");
    let error = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect_err("changed source interrupts apply");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(destination.join("config.toml").is_file());

    let rerun = preview_codex_import(
        abs(&source),
        abs(&destination),
        selection(ConflictPolicy::Skip),
    )
    .await
    .expect("rerun preview");
    let report = apply_codex_import(&rerun.id, rerun.selection)
        .await
        .expect("rerun apply");
    assert_eq!(report.already_present, 1);
    assert_eq!(report.imported, 1);
    assert!(
        destination
            .join("sessions/2026/09/10/rollout-test.jsonl")
            .is_file()
    );
}

#[tokio::test]
async fn session_id_conflict_retains_the_destination_record() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let source_rollout = source.join("sessions/2026/09/10/rollout-source.jsonl");
    let destination_rollout = destination.join("sessions/2026/09/09/rollout-destination.jsonl");
    fs::create_dir_all(source_rollout.parent().expect("source parent")).expect("source sessions");
    fs::create_dir_all(destination_rollout.parent().expect("destination parent"))
        .expect("destination sessions");
    let record = valid_rollout_line(root.path());
    fs::write(&source_rollout, &record).expect("source rollout");
    fs::write(&destination_rollout, &record).expect("destination rollout");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: false,
            sessions: true,
            credentials: false,
            conflict_policy: ConflictPolicy::ReplaceWithBackup,
        },
    )
    .await
    .expect("preview");
    assert!(preview.items[0].conflict);
    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");
    assert_eq!(report.unsupported, 1);
    assert_eq!(report.items[0].disposition, ImportDisposition::Unsupported);
    assert_eq!(
        fs::read_to_string(destination_rollout).expect("destination"),
        record
    );
    assert!(
        !destination
            .join("sessions/2026/09/10/rollout-source.jsonl")
            .exists()
    );
}

#[tokio::test]
async fn imported_session_is_readable_through_the_local_thread_store() {
    use codex_state::SqliteConfig;
    use codex_thread_store::LocalThreadStore;
    use codex_thread_store::LocalThreadStoreConfig;

    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let relative = "sessions/2026/09/10/rollout-readable.jsonl";
    let source_rollout = source.join(relative);
    fs::create_dir_all(source_rollout.parent().expect("source parent")).expect("sessions");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(&source_rollout, valid_rollout_line(root.path())).expect("rollout");
    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: false,
            sessions: true,
            credentials: false,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect("preview");
    apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");

    let sqlite_home = abs(&destination.join("sqlite"));
    let store = LocalThreadStore::new(
        LocalThreadStoreConfig {
            codex_home: destination.clone(),
            sqlite: SqliteConfig::new_for_testing(sqlite_home),
            default_model_provider_id: "openai".to_string(),
        },
        None,
    );
    let thread = store
        .read_thread_by_rollout_path(
            destination.join(relative),
            /* include_archived */ false,
            /* include_history */ true,
        )
        .await
        .expect("read imported thread");

    assert_eq!(
        thread.thread_id.to_string(),
        "00000000-0000-4000-8000-000000000001"
    );
    assert!(thread.history.is_some());
}

#[tokio::test]
async fn same_path_session_id_conflict_is_never_replaced() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let relative = "sessions/2026/09/10/rollout-shared.jsonl";
    let source_rollout = source.join(relative);
    let destination_rollout = destination.join(relative);
    fs::create_dir_all(source_rollout.parent().expect("source parent")).expect("source sessions");
    fs::create_dir_all(destination_rollout.parent().expect("destination parent"))
        .expect("destination sessions");
    let source_record = valid_rollout_line(root.path());
    let destination_record = format!("{source_record}\n");
    fs::write(&source_rollout, source_record).expect("source rollout");
    fs::write(&destination_rollout, &destination_record).expect("destination rollout");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: false,
            sessions: true,
            credentials: false,
            conflict_policy: ConflictPolicy::ReplaceWithBackup,
        },
    )
    .await
    .expect("preview");
    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");

    assert_eq!(report.items[0].disposition, ImportDisposition::Unsupported);
    assert_eq!(
        fs::read_to_string(destination_rollout).expect("destination"),
        destination_record
    );
    assert!(report.backups.is_empty());
}

#[tokio::test]
async fn existing_different_session_is_never_replaced() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let relative = "sessions/2026/09/10/rollout-shared.jsonl";
    let source_rollout = source.join(relative);
    let destination_rollout = destination.join(relative);
    fs::create_dir_all(source_rollout.parent().expect("source parent")).expect("source sessions");
    fs::create_dir_all(destination_rollout.parent().expect("destination parent"))
        .expect("destination sessions");
    fs::write(&source_rollout, valid_rollout_line(root.path())).expect("source rollout");
    let destination_record =
        valid_rollout_line_with_id(root.path(), "00000000-0000-4000-8000-000000000002");
    fs::write(&destination_rollout, &destination_record).expect("destination rollout");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: false,
            sessions: true,
            credentials: false,
            conflict_policy: ConflictPolicy::ReplaceWithBackup,
        },
    )
    .await
    .expect("preview");
    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");

    assert_eq!(
        report.items[0].disposition,
        ImportDisposition::SkippedConflict
    );
    assert_eq!(
        fs::read_to_string(destination_rollout).expect("destination"),
        destination_record
    );
    assert!(report.backups.is_empty());
}

#[test]
fn stable_snapshot_rejects_a_concurrent_append() {
    use std::io::Write;

    let rollout = tempfile::NamedTempFile::new().expect("rollout");
    fs::write(rollout.path(), b"first").expect("initial source");
    let result = read_stable_snapshot_with_after_read(rollout.path(), || {
        let mut file = fs::OpenOptions::new()
            .append(true)
            .open(rollout.path())
            .expect("source");
        file.write_all(b"second").expect("append");
        file.sync_all().expect("sync");
    });
    let Err(error) = result else {
        panic!("concurrent source mutation must fail");
    };
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn preview_registry_evicts_its_oldest_entry_at_the_count_limit() {
    let mut registry = PreviewRegistry::default();
    for index in 0..=MAX_PREVIEWS {
        let id = format!("preview-{index}");
        registry.insert(id.clone(), empty_plan(&id, 1));
    }

    assert!(!registry.plans.contains_key("preview-0"));
    assert_eq!(registry.plans.len(), MAX_PREVIEWS);
}

#[test]
fn preview_registry_evicts_before_crossing_its_aggregate_byte_limit() {
    let mut registry = PreviewRegistry::default();
    registry.insert("first".to_string(), empty_plan("first", MAX_PREVIEW_BYTES));
    registry.insert(
        "second".to_string(),
        empty_plan("second", MAX_PREVIEW_BYTES),
    );
    registry.insert("third".to_string(), empty_plan("third", 1));

    assert!(!registry.plans.contains_key("first"));
    assert!(registry.stored_bytes <= MAX_REGISTRY_BYTES);
}

#[test]
fn preview_bounds_reject_aggregate_bytes_and_item_count() {
    assert!(validate_preview_bounds(MAX_PREVIEW_ITEMS + 1, 0).is_err());
    assert!(validate_preview_bounds(1, MAX_PREVIEW_BYTES + 1).is_err());
}

#[tokio::test]
async fn preview_rejects_an_item_above_the_byte_limit() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    let oversized = fs::File::create(source.join("config.toml")).expect("config");
    oversized
        .set_len(MAX_ITEM_BYTES as u64 + 1)
        .expect("oversized config");

    let error = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: true,
            sessions: false,
            credentials: false,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect_err("oversized item must be rejected");
    assert!(error.to_string().contains("byte limit"));
}

#[test]
fn ledger_replacement_supports_multiple_records_and_idempotent_reruns() {
    let root = TempDir::new().expect("tempdir");
    let destination = root.path();
    let mut ledger = ImportLedger::default();
    let first = preview_item("config.toml");
    let second = preview_item("sessions/rollout.jsonl");

    record_import(destination, &mut ledger, &first, "first").expect("first record");
    record_import(destination, &mut ledger, &second, "second").expect("second record");
    record_import(destination, &mut ledger, &first, "first").expect("idempotent record");

    let loaded = load_ledger(destination).expect("ledger");
    assert_eq!(loaded.records.len(), 2);
}

fn empty_plan(id: &str, stored_bytes: usize) -> ImportPlan {
    ImportPlan {
        source: PathBuf::from("/source"),
        destination: PathBuf::from("/destination"),
        preview: ImportPreview {
            id: id.to_string(),
            selection: selection(ConflictPolicy::Skip),
            items: Vec::new(),
        },
        items: Vec::new(),
        stored_bytes,
    }
}

fn preview_item(path: &str) -> ImportPreviewItem {
    ImportPreviewItem {
        kind: ImportItemKind::Settings,
        relative_path: PathBuf::from(path),
        source_sha256: Some("hash".to_string()),
        conflict: false,
        requires_review: false,
        review_reasons: Vec::new(),
    }
}

fn hash_tree(root: &std::path::Path) -> Vec<(std::path::PathBuf, Vec<u8>)> {
    fn walk(
        base: &std::path::Path,
        path: &std::path::Path,
        output: &mut Vec<(std::path::PathBuf, Vec<u8>)>,
    ) {
        let mut entries = fs::read_dir(path)
            .expect("read dir")
            .collect::<Result<Vec<_>, _>>()
            .expect("entries");
        entries.sort_by_key(std::fs::DirEntry::file_name);
        for entry in entries {
            let path = entry.path();
            if path.is_dir() {
                walk(base, &path, output);
            } else {
                output.push((
                    path.strip_prefix(base).expect("relative").to_path_buf(),
                    fs::read(path).expect("read"),
                ));
            }
        }
    }
    let mut output = Vec::new();
    walk(root, root, &mut output);
    output
}

fn valid_rollout_line(cwd: &std::path::Path) -> String {
    valid_rollout_line_with_id(cwd, "00000000-0000-4000-8000-000000000001")
}

fn valid_rollout_line_with_id(cwd: &std::path::Path, id: &str) -> String {
    format!(
        "{{\"timestamp\":\"2026-09-10T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"{id}\",\"timestamp\":\"2026-09-10T00:00:00Z\",\"cwd\":{},\"originator\":\"codex_cli_rs\",\"cli_version\":\"0.1.0\",\"source\":\"cli\",\"model_provider\":\"openai\"}}}}\n",
        serde_json::to_string(cwd).expect("cwd")
    )
}
