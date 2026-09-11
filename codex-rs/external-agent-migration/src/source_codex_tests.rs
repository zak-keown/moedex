use super::*;
use codex_utils_absolute_path::AbsolutePathBuf;
use pretty_assertions::assert_eq;
use std::fs;
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
    format!(
        "{{\"timestamp\":\"2026-09-10T00:00:00Z\",\"type\":\"session_meta\",\"payload\":{{\"id\":\"00000000-0000-4000-8000-000000000001\",\"timestamp\":\"2026-09-10T00:00:00Z\",\"cwd\":{},\"originator\":\"codex_cli_rs\",\"cli_version\":\"0.1.0\",\"source\":\"cli\",\"model_provider\":\"openai\"}}}}\n",
        serde_json::to_string(cwd).expect("cwd")
    )
}
