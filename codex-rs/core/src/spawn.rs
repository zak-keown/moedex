use codex_network_proxy::NetworkProxy;
use codex_utils_absolute_path::AbsolutePathBuf;
use std::collections::HashMap;
use std::path::PathBuf;
use std::process::Stdio;
use tokio::process::Child;
use tokio::process::Command;
use tracing::trace;

use codex_protocol::permissions::NetworkSandboxPolicy;
use codex_protocol::shell_environment::is_non_inheritable_env_var;

/// Experimental environment variable that will be set to some non-empty value
/// if both of the following are true:
///
/// 1. The process was spawned by Codex as part of a shell tool call.
/// 2. NetworkSandboxPolicy is restricted for the tool call.
///
/// We may try to have just one environment variable for all sandboxing
/// attributes, so this may change in the future.
pub const CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR: &str = "CODEX_SANDBOX_NETWORK_DISABLED";

/// Should be set when the process is spawned under a sandbox. Currently, the
/// value is "seatbelt" for macOS, but it may change in the future to
/// accommodate sandboxing configuration and other sandboxing mechanisms.
pub const CODEX_SANDBOX_ENV_VAR: &str = "CODEX_SANDBOX";

#[derive(Debug, Clone, Copy)]
pub enum StdioPolicy {
    RedirectForShellTool,
    Inherit,
}

/// Spawns the appropriate child process for the exec params and sandbox settings,
/// ensuring the args and environment variables used to create the `Command`
/// (and `Child`) honor the configuration.
///
/// For now, we take `NetworkSandboxPolicy` as a parameter to spawn_child()
/// because we need to determine whether to set the
/// `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` environment variable.
pub(crate) struct SpawnChildRequest<'a> {
    pub program: PathBuf,
    pub args: Vec<String>,
    pub arg0: Option<&'a str>,
    pub cwd: AbsolutePathBuf,
    pub network_sandbox_policy: NetworkSandboxPolicy,
    pub network: Option<&'a NetworkProxy>,
    pub stdio_policy: StdioPolicy,
    pub env: HashMap<String, String>,
}

/// Render a child process environment for logging as a sorted list of variable
/// NAMES only, never their values. `env` legitimately carries ambient
/// credentials (`OPENAI_API_KEY`, `GITHUB_TOKEN`, `AWS_SECRET_ACCESS_KEY`, ...)
/// that must reach the child, and this module's trace output is captured into
/// the local log DB and feedback ring buffer by default, so values must never
/// be written to a log record.
fn redacted_env_for_log(env: &HashMap<String, String>) -> String {
    let mut names = env.keys().map(String::as_str).collect::<Vec<_>>();
    names.sort_unstable();
    format!("{names:?}")
}

pub(crate) async fn spawn_child_async(request: SpawnChildRequest<'_>) -> std::io::Result<Child> {
    let SpawnChildRequest {
        program,
        args,
        arg0,
        cwd,
        network_sandbox_policy,
        network,
        stdio_policy,
        mut env,
    } = request;

    env.retain(|name, _| !is_non_inheritable_env_var(name));

    let env_for_log = redacted_env_for_log(&env);
    trace!(
        "spawn_child_async: {program:?} {args:?} {arg0:?} {cwd:?} {network_sandbox_policy:?} {stdio_policy:?} env={env_for_log}"
    );

    let mut cmd = Command::new(&program);
    #[cfg(unix)]
    cmd.arg0(arg0.map_or_else(|| program.to_string_lossy().to_string(), String::from));
    cmd.args(args);
    cmd.current_dir(cwd);
    if let Some(network) = network {
        network.apply_to_env(&mut env);
    }
    // macOS fd cleanup must keep the shell escalation socket.
    #[cfg(target_os = "macos")]
    let inherited_fd = env
        .get(codex_shell_escalation::ESCALATE_SOCKET_ENV_VAR)
        .and_then(|fd| fd.parse().ok());
    cmd.env_clear();
    cmd.envs(env);

    if !network_sandbox_policy.is_enabled() {
        cmd.env(CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR, "1");
    }

    // If this Codex process dies (including being killed via SIGKILL), we want
    // any child processes that were spawned as part of a `"shell"` tool call
    // to also be terminated.

    #[cfg(unix)]
    unsafe {
        let detach_from_tty = matches!(stdio_policy, StdioPolicy::RedirectForShellTool);
        #[cfg(target_os = "linux")]
        let parent_pid = libc::getpid();
        cmd.pre_exec(move || {
            if detach_from_tty {
                codex_utils_pty::process_group::detach_from_tty()?;
            }

            // This relies on prctl(2), so it only works on Linux.
            #[cfg(target_os = "linux")]
            {
                // This prctl call effectively requests, "deliver SIGTERM when my
                // current parent dies."
                codex_utils_pty::process_group::set_parent_death_signal(parent_pid)?;
            }
            // macOS cannot receive the fd with close-on-exec set atomically.
            #[cfg(target_os = "macos")]
            codex_utils_pty::pty::close_inherited_fds_except(inherited_fd.as_slice());
            Ok(())
        });
    }

    match stdio_policy {
        StdioPolicy::RedirectForShellTool => {
            // Do not create a file descriptor for stdin because otherwise some
            // commands may hang forever waiting for input. For example, ripgrep has
            // a heuristic where it may try to read from stdin as explained here:
            // https://github.com/BurntSushi/ripgrep/blob/e2362d4d5185d02fa857bf381e7bd52e66fafc73/crates/core/flags/hiargs.rs#L1101-L1103
            cmd.stdin(Stdio::null());

            cmd.stdout(Stdio::piped()).stderr(Stdio::piped());
        }
        StdioPolicy::Inherit => {
            // Inherit stdin, stdout, and stderr from the parent process.
            cmd.stdin(Stdio::inherit())
                .stdout(Stdio::inherit())
                .stderr(Stdio::inherit());
        }
    }

    cmd.kill_on_drop(true).spawn()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn redacted_env_for_log_omits_values() {
        let mut env = HashMap::new();
        env.insert("OPENAI_API_KEY".to_string(), "sk-super-secret".to_string());
        env.insert("AWS_SECRET_ACCESS_KEY".to_string(), "aws-top-secret".to_string());
        env.insert("PATH".to_string(), "/usr/local/bin".to_string());

        let rendered = redacted_env_for_log(&env);

        // Variable names are safe to log; values (which include real
        // credentials that reach the child) must never appear.
        assert!(rendered.contains("OPENAI_API_KEY"), "names must be logged: {rendered}");
        assert!(rendered.contains("AWS_SECRET_ACCESS_KEY"));
        assert!(rendered.contains("PATH"));
        assert!(
            !rendered.contains("sk-super-secret"),
            "secret values must not be logged: {rendered}"
        );
        assert!(
            !rendered.contains("aws-top-secret"),
            "secret values must not be logged: {rendered}"
        );
        assert!(
            !rendered.contains("/usr/local/bin"),
            "values must not be logged: {rendered}"
        );
    }
}
