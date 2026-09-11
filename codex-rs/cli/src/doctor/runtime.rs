//! Captures how this Codex process was launched.
//!
//! Runtime diagnostics answer provenance questions that are hard to infer from
//! user reports: which binary is running, which install channel it resembles,
//! which platform it targets, and whether the search command comes from bundled
//! package files or from PATH.

use std::env;

use codex_build_info::BuildInfo;
use codex_build_info::BuildProvenance;
use codex_install_context::InstallContext;
use codex_install_context::InstallMethod;

use super::CheckStatus;
use super::DoctorCheck;
use super::describe_install_context;
use super::doctor_install_context;
use super::push_path_detail;

/// Builds the process provenance row for the current Codex executable.
///
/// This check is informational and should not fail on its own; inconsistent
/// install state is reported by the installation and update checks instead.
pub(super) fn runtime_check() -> DoctorCheck {
    let current_exe = env::current_exe().ok();
    runtime_check_with_provenance(BuildInfo::get().provenance(), current_exe.as_deref())
}

fn runtime_check_with_provenance(
    provenance: BuildProvenance,
    current_exe: Option<&std::path::Path>,
) -> DoctorCheck {
    let install_context = doctor_install_context(current_exe);
    let os = env::consts::OS;
    let arch = env::consts::ARCH;
    let platform = format!("{os}-{arch}");
    let install_method = install_method_name(&install_context);
    let mut details = vec![
        format!("distribution version: {}", provenance.distribution_version),
        format!("platform: {platform}"),
        format!(
            "install method: {}",
            describe_install_context(&install_context)
        ),
        format!("fork commit: {}", provenance.fork_commit),
        format!("upstream base: {}", provenance.upstream_commit),
        format!("release channel: {}", provenance.release_channel),
    ];
    push_path_detail(&mut details, "current executable", current_exe);

    DoctorCheck::new(
        "runtime.provenance",
        "runtime",
        CheckStatus::Ok,
        format!("running {install_method} on {platform}"),
    )
    .details(details)
}

/// Inspects the search command selected by the install context without executing it.
///
/// Package-layout installs should point at a bundled ripgrep binary, while local
/// installs without that layout usually resolve rg from PATH. A warning here
/// means features that depend on file search may degrade even when the CLI
/// launches.
pub(super) fn search_check() -> DoctorCheck {
    let current_exe = env::current_exe().ok();
    let install_context = doctor_install_context(current_exe.as_deref());
    let rg_command = install_context.rg_command();
    let provider = search_provider(&install_context);
    let mut details = vec![
        format!("search command: {}", rg_command.display()),
        format!("search provider: {provider}"),
    ];

    let status = if rg_command.components().count() > 1 {
        match std::fs::metadata(&rg_command) {
            Ok(metadata) if metadata.is_file() => {
                details.push("search command readiness: file exists".to_string());
                CheckStatus::Ok
            }
            Ok(_) => {
                details.push("search command readiness: path is not a file".to_string());
                CheckStatus::Warning
            }
            Err(err) => {
                details.push(format!("search command readiness: {err}"));
                CheckStatus::Warning
            }
        }
    } else {
        match which::which(&rg_command) {
            Ok(path) => {
                details.push(format!("search command path: {}", path.display()));
                details.push("search command readiness: found; execution not verified".to_string());
                CheckStatus::Ok
            }
            Err(err) => {
                details.push(format!("search command readiness: {err}"));
                CheckStatus::Warning
            }
        }
    };

    let summary = match status {
        CheckStatus::Ok => format!("search command found ({provider}); execution not verified"),
        CheckStatus::Warning => "search command could not be verified".to_string(),
        CheckStatus::Fail => unreachable!(),
    };
    let mut check = DoctorCheck::new("runtime.search", "search", status, summary).details(details);
    if status != CheckStatus::Ok {
        check = check.remediation("Install ripgrep or repair the bundled Codex package.");
    }
    check
}

fn install_method_name(context: &InstallContext) -> &'static str {
    match &context.method {
        InstallMethod::Standalone { .. } => "standalone",
        InstallMethod::Npm => "npm",
        InstallMethod::Bun => "bun",
        InstallMethod::VitePlus => "vite+",
        InstallMethod::Pnpm => "pnpm",
        InstallMethod::Brew => "brew",
        InstallMethod::Other => "local build",
    }
}

fn search_provider(context: &InstallContext) -> &'static str {
    let rg_command = context.rg_command();
    let from_package_layout = context
        .package_layout
        .as_ref()
        .and_then(|package_layout| package_layout.path_dir.as_ref())
        .is_some_and(|path_dir| rg_command.starts_with(path_dir));
    let from_legacy_standalone = matches!(
        &context.method,
        InstallMethod::Standalone {
            resources_dir: Some(resources_dir),
            ..
        } if rg_command.starts_with(resources_dir)
    );

    if from_package_layout || from_legacy_standalone {
        "bundled"
    } else {
        "system"
    }
}

#[cfg(test)]
mod tests {
    use super::super::CheckStatus;
    use super::super::DoctorReport;
    use super::super::output::HumanOutputOptions;
    use super::super::output::render_human_report;
    use super::super::redacted_json_report;
    use super::runtime_check_with_provenance;
    use codex_build_info::BuildProvenance;
    use pretty_assertions::assert_eq;

    #[test]
    fn runtime_provenance_exposes_stamped_distribution_metadata() {
        let check = runtime_check_with_provenance(
            BuildProvenance {
                distribution_version: "1.2.3".parse().expect("valid version"),
                fork_commit: "fork-commit".to_string(),
                upstream_commit: "upstream-base".to_string(),
                release_channel: "github".to_string(),
            },
            /*current_exe*/ None,
        );

        assert_eq!(
            check.details,
            vec![
                "distribution version: 1.2.3".to_string(),
                format!(
                    "platform: {}-{}",
                    std::env::consts::OS,
                    std::env::consts::ARCH
                ),
                "install method: other".to_string(),
                "fork commit: fork-commit".to_string(),
                "upstream base: upstream-base".to_string(),
                "release channel: github".to_string(),
                "current executable: none".to_string(),
            ]
        );

        let report = DoctorReport {
            schema_version: 1,
            generated_at: "test".to_string(),
            overall_status: CheckStatus::Ok,
            codex_version: "1.2.3".to_string(),
            checks: vec![check],
        };
        let human = render_human_report(
            &report,
            HumanOutputOptions {
                show_details: true,
                show_all: false,
                ascii: true,
                color_enabled: false,
            },
        );
        for value in ["1.2.3", "fork-commit", "upstream-base", "github"] {
            assert!(human.contains(value), "missing {value} from:\n{human}");
        }
        let json = serde_json::to_value(redacted_json_report(&report)).expect("serialize report");
        assert_eq!(
            json["checks"]["runtime.provenance"]["details"]["distribution version"],
            "1.2.3"
        );
        assert_eq!(
            json["checks"]["runtime.provenance"]["details"]["fork commit"],
            "fork-commit"
        );
        assert_eq!(
            json["checks"]["runtime.provenance"]["details"]["upstream base"],
            "upstream-base"
        );
        assert_eq!(
            json["checks"]["runtime.provenance"]["details"]["release channel"],
            "github"
        );
    }
}
