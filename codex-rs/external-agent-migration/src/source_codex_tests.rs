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
async fn apply_consumes_a_preview_before_mutating_destination() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(source.join("config.toml"), "model = \"gpt-5\"\n").expect("config");
    let selection = CodexImportSelection {
        settings: true,
        sessions: false,
        credentials: false,
        conflict_policy: ConflictPolicy::Skip,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");

    let (first, second) = tokio::join!(
        apply_codex_import(&preview.id, preview.selection.clone()),
        apply_codex_import(&preview.id, preview.selection.clone()),
    );
    let results = [first, second];

    assert_eq!(results.iter().filter(|result| result.is_ok()).count(), 1);
    assert_eq!(
        results
            .iter()
            .filter_map(|result| result.as_ref().err())
            .map(std::io::Error::kind)
            .collect::<Vec<_>>(),
        vec![std::io::ErrorKind::NotFound]
    );
}

#[tokio::test]
async fn apply_rejects_a_destination_created_after_preview() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(source.join("config.toml"), "model = \"source\"\n").expect("source config");
    let selection = CodexImportSelection {
        settings: true,
        sessions: false,
        credentials: false,
        conflict_policy: ConflictPolicy::ReplaceWithBackup,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");
    fs::write(destination.join("config.toml"), "model = \"new-owner\"\n")
        .expect("destination config");

    let error = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect_err("destination change must invalidate preview");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert_eq!(
        fs::read_to_string(destination.join("config.toml")).expect("destination config"),
        "model = \"new-owner\"\n"
    );
}

#[cfg(unix)]
#[tokio::test]
async fn imported_sessions_are_private() {
    use std::os::unix::fs::PermissionsExt;

    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let relative = PathBuf::from("sessions/2026/09/10/rollout-private.jsonl");
    let rollout = source.join(&relative);
    fs::create_dir_all(rollout.parent().expect("rollout parent")).expect("source sessions");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(&rollout, valid_rollout_line(root.path())).expect("rollout");
    let selection = CodexImportSelection {
        settings: false,
        sessions: true,
        credentials: false,
        conflict_policy: ConflictPolicy::Skip,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");

    apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");

    assert_eq!(
        fs::metadata(destination.join(relative))
            .expect("imported session")
            .permissions()
            .mode()
            & 0o777,
        0o600
    );
}

#[tokio::test]
async fn explicitly_selected_credentials_import_without_exposing_secret_in_public_preview() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    let secret = "credential-import-secret";
    fs::write(
        source.join("auth.json"),
        serde_json::json!({
            "auth_mode": "apikey",
            "OPENAI_API_KEY": secret,
            "tokens": null,
            "last_refresh": null
        })
        .to_string(),
    )
    .expect("source auth");

    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: false,
            sessions: false,
            credentials: true,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect("preview");
    assert!(!format!("{preview:?}").contains(secret));
    assert_eq!(preview.items[0].kind, ImportItemKind::Credentials);
    assert_eq!(preview.items[0].source_sha256, None);

    let report = apply_codex_import(&preview.id, preview.selection.clone())
        .await
        .expect("apply");
    assert_eq!(report.imported, 1);
    assert_eq!(report.items[0].disposition, ImportDisposition::Imported);
    assert!(!format!("{report:?}").contains(secret));
    assert!(
        fs::read_to_string(destination.join("moedex-auth.json"))
            .expect("destination auth")
            .contains(secret)
    );
    assert!(
        fs::read_to_string(source.join("auth.json"))
            .expect("source auth")
            .contains(secret)
    );
    fs::write(
        destination.join("moedex-auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"destination-secret"}"#,
    )
    .expect("destination auth");
    let preview = preview_codex_import(
        abs(&source),
        abs(&destination),
        CodexImportSelection {
            settings: false,
            sessions: false,
            credentials: true,
            conflict_policy: ConflictPolicy::Skip,
        },
    )
    .await
    .expect("conflict preview");
    assert!(preview.items[0].conflict);
    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("conflict apply");
    assert_eq!(
        report.items[0].disposition,
        ImportDisposition::SkippedConflict
    );
    assert!(
        fs::read_to_string(destination.join("moedex-auth.json"))
            .expect("destination auth")
            .contains("destination-secret")
    );
}

#[tokio::test]
async fn credential_replacement_reports_a_non_secret_backup_handle() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"source-secret"}"#,
    )
    .expect("source auth");
    fs::write(
        destination.join("moedex-auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"destination-secret"}"#,
    )
    .expect("destination auth");
    let selection = CodexImportSelection {
        settings: false,
        sessions: false,
        credentials: true,
        conflict_policy: ConflictPolicy::ReplaceWithBackup,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");

    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("replace credentials");

    assert_eq!(report.imported, 1);
    assert_eq!(report.credential_backups.len(), 1);
    assert!(!format!("{report:?}").contains("source-secret"));
    assert!(!format!("{report:?}").contains("destination-secret"));
    assert!(
        fs::read_to_string(destination.join("moedex-auth.json"))
            .expect("destination auth")
            .contains("source-secret")
    );
    assert!(
        fs::read_to_string(&report.credential_backups[0])
            .expect("credential backup")
            .contains("destination-secret")
    );
}

#[tokio::test]
async fn credential_apply_rejects_a_source_change_after_preview_without_mutation() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"previewed-secret"}"#,
    )
    .expect("source auth");
    let selection = CodexImportSelection {
        settings: false,
        sessions: false,
        credentials: true,
        conflict_policy: ConflictPolicy::Skip,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");

    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"changed-secret"}"#,
    )
    .expect("changed source auth");
    let error = apply_codex_import(&preview.id, preview.selection.clone())
        .await
        .expect_err("changed source must require a new preview");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("source credentials changed"));
    assert!(!destination.join("moedex-auth.json").exists());
    assert!(!format!("{error:?}").contains("previewed-secret"));
    assert!(!format!("{error:?}").contains("changed-secret"));
    let expired = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect_err("changed source invalidates the preview");
    assert_eq!(expired.kind(), std::io::ErrorKind::NotFound);
}

#[tokio::test]
async fn a_new_credential_preview_cannot_replace_an_older_preview_registration() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"first-secret"}"#,
    )
    .expect("first source auth");
    let selection = CodexImportSelection {
        settings: false,
        sessions: false,
        credentials: true,
        conflict_policy: ConflictPolicy::Skip,
    };
    let first = preview_codex_import(abs(&source), abs(&destination), selection.clone())
        .await
        .expect("first preview");
    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"second-secret"}"#,
    )
    .expect("second source auth");
    let second = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("second preview");

    assert_ne!(first.id, second.id);
    let error = apply_codex_import(&first.id, first.selection.clone())
        .await
        .expect_err("the first preview must retain its original credential state");
    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(!destination.join("moedex-auth.json").exists());
    let expired = apply_codex_import(&first.id, first.selection)
        .await
        .expect_err("the stale first preview must be invalidated");
    assert_eq!(expired.kind(), std::io::ErrorKind::NotFound);
    cancel_codex_import(&second.id).expect("cancel second preview");
}

#[tokio::test]
async fn credential_apply_rejects_a_destination_change_after_preview_without_mutation() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"apikey","OPENAI_API_KEY":"source-secret"}"#,
    )
    .expect("source auth");
    let selection = CodexImportSelection {
        settings: false,
        sessions: false,
        credentials: true,
        conflict_policy: ConflictPolicy::Skip,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");
    let destination_bytes = br#"{"auth_mode":"apikey","OPENAI_API_KEY":"destination-secret"}"#;
    fs::write(destination.join("moedex-auth.json"), destination_bytes)
        .expect("changed destination auth");

    let error = apply_codex_import(&preview.id, preview.selection.clone())
        .await
        .expect_err("changed destination must require a new preview");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(
        error
            .to_string()
            .contains("destination credentials changed")
    );
    assert_eq!(
        fs::read(destination.join("moedex-auth.json")).expect("destination auth"),
        destination_bytes
    );
    assert!(!format!("{error:?}").contains("destination-secret"));
    let expired = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect_err("changed destination invalidates the preview");
    assert_eq!(expired.kind(), std::io::ErrorKind::NotFound);
}

#[tokio::test]
async fn missing_or_invalid_credentials_report_sign_in_required() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(
        source.join("auth.json"),
        r#"{"auth_mode":"chatgpt","OPENAI_API_KEY":null,"tokens":{"id_token":"e30.e30.c2ln","access_token":"expired","refresh_token":""}}"#,
    )
    .expect("invalid source auth");

    let selection = CodexImportSelection {
        settings: false,
        sessions: false,
        credentials: true,
        conflict_policy: ConflictPolicy::Skip,
    };
    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");
    let report = apply_codex_import(&preview.id, preview.selection.clone())
        .await
        .expect("apply");

    assert_eq!(report.sign_in_required, 1);
    assert_eq!(
        report.items[0].disposition,
        ImportDisposition::SignInRequired
    );
    assert!(!destination.join("moedex-auth.json").exists());
}

#[tokio::test]
async fn ambiguous_auth_storage_config_fails_closed() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    fs::create_dir_all(&source).expect("source");
    fs::create_dir_all(&destination).expect("destination");
    let secret = "stale-file-secret";
    fs::write(
        source.join("auth.json"),
        format!(r#"{{"auth_mode":"apikey","OPENAI_API_KEY":"{secret}"}}"#),
    )
    .expect("source auth");
    fs::write(
        source.join("config.toml"),
        "cli_auth_credentials_store = [invalid]",
    )
    .expect("invalid source config");
    let selection = CodexImportSelection {
        settings: false,
        sessions: false,
        credentials: true,
        conflict_policy: ConflictPolicy::Skip,
    };

    let preview = preview_codex_import(abs(&source), abs(&destination), selection)
        .await
        .expect("preview");
    let report = apply_codex_import(&preview.id, preview.selection.clone())
        .await
        .expect("apply");
    assert_eq!(report.sign_in_required, 1);
    assert!(!destination.join("moedex-auth.json").exists());

    fs::remove_file(source.join("config.toml")).expect("remove invalid source config");
    fs::write(
        destination.join("config.toml"),
        "[features]\nsecret_auth_storage = [false]",
    )
    .expect("invalid destination config");
    let preview = preview_codex_import(abs(&source), abs(&destination), preview.selection)
        .await
        .expect("preview");
    let report = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect("apply");
    assert_eq!(report.failed, 1);
    assert!(!destination.join("moedex-auth.json").exists());
    assert!(
        fs::read_to_string(source.join("auth.json"))
            .expect("source auth")
            .contains(secret)
    );
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
fn destination_inventory_propagates_snapshot_read_errors() {
    let root = TempDir::new().expect("tempdir");
    let rollout = root.path().join("sessions/rollout.jsonl");
    fs::create_dir_all(rollout.parent().expect("parent")).expect("sessions");
    fs::write(&rollout, valid_rollout_line(root.path())).expect("rollout");

    let error = destination_thread_ids_with_snapshot_reader(root.path(), |_| {
        Err(std::io::Error::new(
            std::io::ErrorKind::PermissionDenied,
            "synthetic unreadable rollout",
        ))
    })
    .expect_err("destination read errors must block inventory");

    assert_eq!(error.kind(), std::io::ErrorKind::PermissionDenied);
}

#[test]
fn destination_inventory_rejects_oversized_rollouts() {
    let root = TempDir::new().expect("tempdir");
    let rollout = root.path().join("sessions/rollout.jsonl");
    fs::create_dir_all(rollout.parent().expect("parent")).expect("sessions");
    fs::File::create(&rollout)
        .expect("rollout")
        .set_len(MAX_ITEM_BYTES as u64 + 1)
        .expect("oversized rollout");

    let error = destination_thread_ids(root.path())
        .expect_err("oversized destination rollout must block inventory");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("item limit"));
}

#[test]
fn destination_inventory_rejects_mutating_rollouts() {
    use std::io::Write;

    let root = TempDir::new().expect("tempdir");
    let rollout = root.path().join("sessions/rollout.jsonl");
    fs::create_dir_all(rollout.parent().expect("parent")).expect("sessions");
    fs::write(&rollout, valid_rollout_line(root.path())).expect("rollout");

    let error = destination_thread_ids_with_snapshot_reader(root.path(), |path| {
        read_stable_snapshot_with_after_read(path, || {
            let mut source = fs::OpenOptions::new()
                .append(true)
                .open(path)
                .expect("open rollout");
            source.write_all(b"changed").expect("append rollout");
            source.sync_all().expect("sync rollout");
        })
    })
    .expect_err("mutating destination rollout must block inventory");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn destination_inventory_rejects_unparsable_rollouts() {
    let root = TempDir::new().expect("tempdir");
    let rollout = root.path().join("sessions/rollout.jsonl");
    fs::create_dir_all(rollout.parent().expect("parent")).expect("sessions");
    fs::write(&rollout, "not a rollout\n").expect("rollout");

    let error = destination_thread_ids(root.path())
        .expect_err("unparsable destination rollout must block inventory");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[test]
fn destination_inventory_rejects_invalid_records_after_valid_session_metadata() {
    let root = TempDir::new().expect("tempdir");
    let rollout = root.path().join("sessions/rollout.jsonl");
    fs::create_dir_all(rollout.parent().expect("parent")).expect("sessions");
    fs::write(
        &rollout,
        format!("{}not valid json\n", valid_rollout_line(root.path())),
    )
    .expect("rollout");

    let error = destination_thread_ids(root.path())
        .expect_err("invalid trailing destination record must block inventory");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
}

#[tokio::test]
async fn source_discovery_rejects_too_many_rollouts_before_reading_them() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let sessions = source.join("sessions");
    fs::create_dir_all(&sessions).expect("sessions");
    fs::create_dir_all(&destination).expect("destination");
    for index in 0..=MAX_PREVIEW_ITEMS {
        fs::File::create(sessions.join(format!("rollout-{index}.jsonl"))).expect("rollout");
    }

    let error = preview_codex_import(
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
    .expect_err("too many source rollouts must be rejected");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("item limit"));
}

#[tokio::test]
async fn source_discovery_rejects_aggregate_rollout_bytes_before_reading_them() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let sessions = source.join("sessions");
    fs::create_dir_all(&sessions).expect("sessions");
    fs::create_dir_all(&destination).expect("destination");
    for index in 0..=MAX_PREVIEW_BYTES / MAX_ITEM_BYTES {
        fs::File::create(sessions.join(format!("rollout-{index}.jsonl")))
            .expect("rollout")
            .set_len(MAX_ITEM_BYTES as u64)
            .expect("sparse rollout");
    }

    let error = preview_codex_import(
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
    .expect_err("aggregate source rollout bytes must be rejected");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("aggregate limit"));
}

#[test]
fn source_discovery_rejects_too_many_directories() {
    use crate::source::codex::RolloutDiscoveryLimits;
    use crate::source::codex::discover_rollouts;

    let root = TempDir::new().expect("tempdir");
    fs::create_dir_all(root.path().join("sessions/nested")).expect("sessions");

    let error = discover_rollouts(
        root.path(),
        RolloutDiscoveryLimits {
            max_directories: 1,
            max_files: MAX_PREVIEW_ITEMS,
            max_item_bytes: MAX_ITEM_BYTES as u64,
            max_total_bytes: MAX_PREVIEW_BYTES as u64,
        },
    )
    .expect_err("too many source directories must be rejected");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("directory limit"));
}

#[tokio::test]
async fn apply_rejects_a_source_that_expands_beyond_the_item_limit() {
    let root = TempDir::new().expect("tempdir");
    let source = root.path().join("source");
    let destination = root.path().join("destination");
    let relative = "sessions/2026/09/10/rollout-expanded.jsonl";
    let rollout = source.join(relative);
    fs::create_dir_all(rollout.parent().expect("parent")).expect("sessions");
    fs::create_dir_all(&destination).expect("destination");
    fs::write(&rollout, valid_rollout_line(root.path())).expect("rollout");
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
    fs::OpenOptions::new()
        .write(true)
        .open(&rollout)
        .expect("source rollout")
        .set_len(MAX_ITEM_BYTES as u64 + 1)
        .expect("expanded rollout");

    let error = apply_codex_import(&preview.id, preview.selection)
        .await
        .expect_err("expanded source must be rejected before hashing");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(error.to_string().contains("byte limit"));
    assert!(!destination.join(relative).exists());
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

#[test]
fn preview_accumulation_rejects_before_retaining_an_over_limit_item() {
    let mut items = Vec::new();
    let mut stored_bytes = MAX_PREVIEW_BYTES;
    let item = PlannedItem {
        preview: preview_item("sessions/rollout.jsonl"),
        source: None,
        payload: Some(vec![0]),
        thread_id: None,
        credential: None,
        destination_state: None,
    };

    let error = push_planned_item(&mut items, &mut stored_bytes, item)
        .expect_err("over-limit item must not be retained");

    assert_eq!(error.kind(), std::io::ErrorKind::InvalidData);
    assert!(items.is_empty());
    assert_eq!(stored_bytes, MAX_PREVIEW_BYTES);
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
