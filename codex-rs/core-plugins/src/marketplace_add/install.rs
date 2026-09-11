use super::MarketplaceAddError;
use std::fs;
use std::path::Path;
use std::path::PathBuf;
use std::process::Command;

pub(super) fn clone_git_source(
    url: &str,
    ref_name: Option<&str>,
    sparse_paths: &[String],
    destination: &Path,
) -> Result<(), MarketplaceAddError> {
    let destination_string = destination.to_string_lossy().to_string();
    if sparse_paths.is_empty() {
        run_git(
            &["clone", url, destination_string.as_str()],
            /*cwd*/ None,
        )?;
        if let Some(ref_name) = ref_name {
            run_git(
                &["checkout", ref_name],
                Some(Path::new(&destination_string)),
            )?;
        }
        verify_pinned_sha(ref_name, destination)?;
        return Ok(());
    }

    run_git(
        &[
            "clone",
            "--filter=blob:none",
            "--no-checkout",
            url,
            destination_string.as_str(),
        ],
        /*cwd*/ None,
    )?;
    let mut sparse_args = vec!["sparse-checkout", "set"];
    sparse_args.extend(sparse_paths.iter().map(String::as_str));
    run_git(&sparse_args, Some(destination))?;
    run_git(&["checkout", ref_name.unwrap_or("HEAD")], Some(destination))?;
    verify_pinned_sha(ref_name, destination)?;
    Ok(())
}

/// When `ref_name` is a full 40-hex SHA (an immutable commit pin, e.g.
/// `codex marketplace add <url>#<sha>`), require the actually-checked-out
/// revision to match it. `git checkout <name>` prefers a branch/tag named
/// exactly like the SHA over the raw object, so without this a remote that
/// contains such a ref could silently install a different, attacker-chosen tree.
/// Mirrors `loader::clone_git_plugin_source`'s post-checkout verification.
fn verify_pinned_sha(ref_name: Option<&str>, destination: &Path) -> Result<(), MarketplaceAddError> {
    let Some(ref_name) = ref_name else {
        return Ok(());
    };
    if !is_full_git_sha(ref_name) {
        return Ok(());
    }
    let checked_out = run_git_output(&["rev-parse", "HEAD"], Some(destination))?;
    if !checked_out.eq_ignore_ascii_case(ref_name) {
        return Err(MarketplaceAddError::Internal(format!(
            "checked out Git SHA {checked_out} does not match requested SHA {ref_name}"
        )));
    }
    Ok(())
}

fn is_full_git_sha(value: &str) -> bool {
    value.len() == 40 && value.chars().all(|ch| ch.is_ascii_hexdigit())
}

pub(super) fn safe_marketplace_dir_name(
    marketplace_name: &str,
) -> Result<String, MarketplaceAddError> {
    let safe = marketplace_name
        .chars()
        .map(|ch| {
            if ch.is_ascii_alphanumeric() || matches!(ch, '-' | '_' | '.') {
                ch
            } else {
                '-'
            }
        })
        .collect::<String>();
    let safe = safe.trim_matches('.').to_string();
    if safe.is_empty() || safe == ".." {
        return Err(MarketplaceAddError::InvalidRequest(format!(
            "marketplace name '{marketplace_name}' cannot be used as an install directory"
        )));
    }
    Ok(safe)
}

pub(super) fn ensure_marketplace_destination_is_inside_install_root(
    install_root: &Path,
    destination: &Path,
) -> Result<(), MarketplaceAddError> {
    let install_root = install_root.canonicalize().map_err(|err| {
        MarketplaceAddError::Internal(format!(
            "failed to resolve marketplace install root {}: {err}",
            install_root.display()
        ))
    })?;
    let destination_parent = destination
        .parent()
        .ok_or_else(|| {
            MarketplaceAddError::Internal("marketplace destination has no parent".to_string())
        })?
        .canonicalize()
        .map_err(|err| {
            MarketplaceAddError::Internal(format!(
                "failed to resolve marketplace destination parent {}: {err}",
                destination.display()
            ))
        })?;
    if !destination_parent.starts_with(&install_root) {
        return Err(MarketplaceAddError::InvalidRequest(format!(
            "marketplace destination {} is outside install root {}",
            destination.display(),
            install_root.display()
        )));
    }
    Ok(())
}

pub(super) fn replace_marketplace_root(
    staged_root: &Path,
    destination: &Path,
) -> std::io::Result<()> {
    if let Some(parent) = destination.parent() {
        fs::create_dir_all(parent)?;
    }
    fs::rename(staged_root, destination)
}

pub(super) fn marketplace_staging_root(install_root: &Path) -> PathBuf {
    install_root.join(".staging")
}

fn run_git(args: &[&str], cwd: Option<&Path>) -> Result<(), MarketplaceAddError> {
    let mut command = Command::new("git");
    command
        .args(["-c", codex_git_utils::SAFE_BARE_REPOSITORY_CONFIG])
        .args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = command.output().map_err(|err| {
        MarketplaceAddError::Internal(format!("failed to run git {}: {err}", args.join(" ")))
    })?;
    if output.status.success() {
        return Ok(());
    }

    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    Err(MarketplaceAddError::Internal(format!(
        "git {} failed with status {}\nstdout:\n{}\nstderr:\n{}",
        args.join(" "),
        output.status,
        stdout.trim(),
        stderr.trim()
    )))
}

fn run_git_output(args: &[&str], cwd: Option<&Path>) -> Result<String, MarketplaceAddError> {
    let mut command = Command::new("git");
    command
        .args(["-c", codex_git_utils::SAFE_BARE_REPOSITORY_CONFIG])
        .args(args);
    command.env("GIT_TERMINAL_PROMPT", "0");
    if let Some(cwd) = cwd {
        command.current_dir(cwd);
    }

    let output = command.output().map_err(|err| {
        MarketplaceAddError::Internal(format!("failed to run git {}: {err}", args.join(" ")))
    })?;
    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        return Err(MarketplaceAddError::Internal(format!(
            "git {} failed with status {}\nstderr:\n{}",
            args.join(" "),
            output.status,
            stderr.trim()
        )));
    }
    Ok(String::from_utf8_lossy(&output.stdout).trim().to_string())
}

#[cfg(test)]
mod tests {
    use super::*;

    fn git(args: &[&str], cwd: &Path) -> String {
        let out = Command::new("git")
            .args(["-c", codex_git_utils::SAFE_BARE_REPOSITORY_CONFIG])
            .args(args)
            .current_dir(cwd)
            .output()
            .expect("run git");
        assert!(
            out.status.success(),
            "git {args:?} failed: {}",
            String::from_utf8_lossy(&out.stderr)
        );
        String::from_utf8_lossy(&out.stdout).trim().to_string()
    }

    #[test]
    fn clone_git_source_rejects_sha_that_resolves_to_hostile_branch() {
        let repo = tempfile::tempdir().expect("repo");
        let dest_root = tempfile::tempdir().expect("dest");
        git(&["init"], repo.path());
        git(&["config", "user.email", "test@example.com"], repo.path());
        git(&["config", "user.name", "Test User"], repo.path());
        fs::write(repo.path().join("marker.txt"), "benign").unwrap();
        git(&["add", "."], repo.path());
        git(&["commit", "-m", "benign"], repo.path());
        let benign_sha = git(&["rev-parse", "HEAD"], repo.path());
        fs::write(repo.path().join("marker.txt"), "malicious").unwrap();
        git(&["add", "."], repo.path());
        git(&["commit", "-m", "malicious"], repo.path());
        let malicious_sha = git(&["rev-parse", "HEAD"], repo.path());
        // Name the (malicious) default branch exactly like the benign commit's SHA.
        git(&["branch", "-m", &benign_sha], repo.path());

        let destination = dest_root.path().join("clone");
        let err = clone_git_source(
            &repo.path().display().to_string(),
            Some(&benign_sha),
            &[],
            &destination,
        )
        .expect_err("a branch named like the SHA must not satisfy the pin");

        assert!(
            matches!(err, MarketplaceAddError::Internal(ref msg)
                if msg == &format!(
                    "checked out Git SHA {malicious_sha} does not match requested SHA {benign_sha}"
                )),
            "unexpected error: {err:?}"
        );
    }

    #[test]
    fn clone_git_source_accepts_matching_sha() {
        let repo = tempfile::tempdir().expect("repo");
        let dest_root = tempfile::tempdir().expect("dest");
        git(&["init"], repo.path());
        git(&["config", "user.email", "test@example.com"], repo.path());
        git(&["config", "user.name", "Test User"], repo.path());
        fs::write(repo.path().join("marker.txt"), "content").unwrap();
        git(&["add", "."], repo.path());
        git(&["commit", "-m", "c1"], repo.path());
        let sha = git(&["rev-parse", "HEAD"], repo.path());

        let destination = dest_root.path().join("clone");
        clone_git_source(
            &repo.path().display().to_string(),
            Some(&sha),
            &[],
            &destination,
        )
        .expect("cloning a genuine pinned SHA must succeed");
    }
}
