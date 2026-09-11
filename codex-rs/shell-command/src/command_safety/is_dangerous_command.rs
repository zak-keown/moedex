use crate::bash::parse_shell_lc_literal_commands;
#[path = "windows_dangerous_commands.rs"]
mod windows_dangerous_commands;

/// The platform whose command semantics should be used for safety checks.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DangerousCommandPlatform {
    /// POSIX command and path semantics.
    Posix,
    /// Windows command and path semantics.
    Windows,
}

impl DangerousCommandPlatform {
    /// Returns the platform where the classifier is running.
    pub fn host() -> Self {
        if cfg!(windows) {
            Self::Windows
        } else {
            Self::Posix
        }
    }
}

/// Identifies the dangerous-command rule matched by a command invocation.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum DangerousCommandMatch {
    /// An `rm` invocation includes the force option.
    ForcedRm,
    /// Another dangerous-command rule matched.
    Other,
}

const MAX_DANGEROUS_COMMAND_WRAPPER_DEPTH: usize = 8;

/// Returns the dangerous-command rule matched by an already-tokenized command.
pub fn dangerous_command_match(command: &[String]) -> Option<DangerousCommandMatch> {
    dangerous_command_match_for_platform(command, DangerousCommandPlatform::host())
}

/// Returns the dangerous-command rule matched using the target platform's semantics.
pub fn dangerous_command_match_for_platform(
    command: &[String],
    platform: DangerousCommandPlatform,
) -> Option<DangerousCommandMatch> {
    dangerous_command_match_with_depth(command, /*wrapper_depth*/ 0, platform)
}

fn dangerous_command_match_with_depth(
    command: &[String],
    wrapper_depth: usize,
    platform: DangerousCommandPlatform,
) -> Option<DangerousCommandMatch> {
    if wrapper_depth > MAX_DANGEROUS_COMMAND_WRAPPER_DEPTH {
        return Some(DangerousCommandMatch::Other);
    }

    if let Some(dangerous_match) =
        dangerous_command_match_for_exec(command, wrapper_depth, platform)
    {
        return Some(dangerous_match);
    }

    // Support shell scripts where any literal command might be dangerous,
    // including commands nested in control flow or substitutions.
    if let Some(dangerous_match) = parse_shell_lc_literal_commands(command).and_then(|commands| {
        commands.iter().find_map(|command| {
            dangerous_command_match_with_depth(command, wrapper_depth + 1, platform)
        })
    }) {
        return Some(dangerous_match);
    }

    if platform == DangerousCommandPlatform::Windows
        && windows_dangerous_commands::is_dangerous_command_windows(command)
    {
        return Some(DangerousCommandMatch::Other);
    }

    None
}

/// Returns the PowerShell-specific rule matched using the target platform's semantics.
pub fn dangerous_powershell_words_match(
    command: &[String],
    platform: DangerousCommandPlatform,
) -> Option<DangerousCommandMatch> {
    if platform == DangerousCommandPlatform::Windows {
        windows_dangerous_commands::is_dangerous_powershell_words(command)
            .then_some(DangerousCommandMatch::Other)
    } else {
        None
    }
}

fn executable_name_lookup_key(raw: &str, platform: DangerousCommandPlatform) -> Option<String> {
    match platform {
        DangerousCommandPlatform::Posix => raw
            .rsplit('/')
            .next()
            .filter(|name| !name.is_empty())
            .map(str::to_owned),
        DangerousCommandPlatform::Windows => {
            let name = raw
                .rsplit(['/', '\\'])
                .next()
                .filter(|name| !name.is_empty())?;
            let name = match name.as_bytes() {
                [drive, b':', ..] if drive.is_ascii_alphabetic() => &name[2..],
                _ => name,
            };
            let name = name.to_ascii_lowercase();
            for suffix in [".exe", ".cmd", ".bat", ".com"] {
                if let Some(stripped) = name.strip_suffix(suffix) {
                    return Some(stripped.to_string());
                }
            }
            (!name.is_empty()).then_some(name)
        }
    }
}

fn dangerous_command_match_for_exec(
    command: &[String],
    wrapper_depth: usize,
    platform: DangerousCommandPlatform,
) -> Option<DangerousCommandMatch> {
    let cmd0 = command
        .first()
        .and_then(|command| executable_name_lookup_key(command, platform));

    match cmd0.as_deref() {
        Some("rm") if rm_args_include_force_option(&command[1..]) => {
            Some(DangerousCommandMatch::ForcedRm)
        }

        // For sudo <cmd>, simply check <cmd>.
        Some("sudo") => {
            dangerous_command_match_with_depth(&command[1..], wrapper_depth + 1, platform)
        }

        // Skip environment assignments before checking the command run by env.
        Some("env") => dangerous_command_match_for_env(command, wrapper_depth, platform),

        // A trap action is shell source stored in the first operand.
        Some("trap") => dangerous_command_match_for_trap(command, wrapper_depth, platform),

        // ── anything else ─────────────────────────────────────────────────
        _ => None,
    }
}

/// `env` flags that consume a value (the following argument, or an inline
/// `--flag=value`). If these are not skipped along with their value, the
/// wrapped command is misidentified and never inspected, letting e.g.
/// `env -C /tmp rm -rf .` slip past the classifier.
const ENV_VALUE_TAKING_FLAGS: &[&str] = &[
    "-C",
    "--chdir",
    "-u",
    "--unset",
    "-S",
    "--split-string",
    "-a",
    "--argv0",
];

fn dangerous_command_match_for_env(
    command: &[String],
    wrapper_depth: usize,
    platform: DangerousCommandPlatform,
) -> Option<DangerousCommandMatch> {
    let mut command_index = 1;
    while let Some(argument) = command.get(command_index) {
        let arg = argument.as_str();
        if arg == "--" {
            command_index += 1;
            break;
        }
        // NAME=VALUE environment assignment.
        if arg
            .split_once('=')
            .is_some_and(|(name, _)| !name.is_empty() && !name.starts_with('-'))
        {
            command_index += 1;
            continue;
        }
        // Flags that take no value.
        if matches!(
            arg,
            "-i" | "--ignore-environment" | "-0" | "--null" | "-v" | "--debug"
        ) {
            command_index += 1;
            continue;
        }
        // Long flag carrying an inline value, e.g. `--chdir=DIR`, `--unset=NAME`.
        if let Some((name, _)) = arg.split_once('=')
            && ENV_VALUE_TAKING_FLAGS.contains(&name)
        {
            command_index += 1;
            continue;
        }
        // Flag whose value is the following argument, e.g. `-C /tmp`, `-u NAME`.
        if ENV_VALUE_TAKING_FLAGS.contains(&arg) {
            command_index += 2;
            continue;
        }
        // Attached short value, e.g. `-C/tmp`, `-uPATH`.
        if arg.len() > 2
            && !arg.starts_with("--")
            && (arg.starts_with("-C")
                || arg.starts_with("-u")
                || arg.starts_with("-S")
                || arg.starts_with("-a"))
        {
            command_index += 1;
            continue;
        }
        break;
    }
    // A value-taking flag as the final token can advance the index past the end.
    let command_index = command_index.min(command.len());
    dangerous_command_match_with_depth(&command[command_index..], wrapper_depth + 1, platform)
}

fn dangerous_command_match_for_trap(
    command: &[String],
    wrapper_depth: usize,
    platform: DangerousCommandPlatform,
) -> Option<DangerousCommandMatch> {
    let mut action_index = 1;
    if command
        .get(action_index)
        .is_some_and(|argument| argument == "--")
    {
        action_index += 1;
    }
    let action = command
        .get(action_index)
        .filter(|action| !action.starts_with('-'))?;
    let shell_command = vec!["sh".to_string(), "-c".to_string(), action.clone()];
    dangerous_command_match_with_depth(&shell_command, wrapper_depth + 1, platform)
}

fn rm_args_include_force_option(args: &[String]) -> bool {
    args.iter()
        .take_while(|arg| arg.as_str() != "--")
        .any(|arg| {
            arg == "--force"
                || arg
                    .strip_prefix('-')
                    .is_some_and(|flags| !flags.starts_with('-') && flags.contains('f'))
        })
}

#[cfg(test)]
mod tests {
    use super::*;
    use pretty_assertions::assert_eq;

    fn vec_str(items: &[&str]) -> Vec<String> {
        items.iter().map(std::string::ToString::to_string).collect()
    }

    #[test]
    fn rm_rf_is_dangerous() {
        assert_eq!(
            dangerous_command_match(&vec_str(&["rm", "-rf", "/"])),
            Some(DangerousCommandMatch::ForcedRm)
        );
    }

    #[test]
    fn rm_f_is_dangerous() {
        assert_eq!(
            dangerous_command_match(&vec_str(&["rm", "-f", "/"])),
            Some(DangerousCommandMatch::ForcedRm)
        );
    }

    #[test]
    fn forced_rm_variants_are_dangerous() {
        for command in [
            vec_str(&["/bin/rm", "-fr", "/tmp/example"]),
            vec_str(&["rm", "-r", "-f", "/tmp/example"]),
            vec_str(&["rm", "--force", "/tmp/example"]),
            vec_str(&["rm", "/tmp/example", "-f"]),
            vec_str(&["sudo", "rm", "-rf", "/tmp/example"]),
            vec_str(&["env", "TARGET=/tmp/example", "rm", "-rf", "/tmp/example"]),
        ] {
            assert_eq!(
                dangerous_command_match(&command),
                Some(DangerousCommandMatch::ForcedRm),
                "{command:?}"
            );
        }
    }

    #[test]
    fn env_value_taking_flags_do_not_hide_wrapped_command() {
        for command in [
            vec_str(&["env", "-C", "/tmp", "rm", "-rf", "/tmp/example"]),
            vec_str(&["env", "--chdir", "/tmp", "rm", "-rf", "/tmp/example"]),
            vec_str(&["env", "--chdir=/tmp", "rm", "-rf", "/tmp/example"]),
            vec_str(&["env", "-u", "FOO", "rm", "-rf", "/tmp/example"]),
            vec_str(&[
                "env",
                "-u",
                "FOO",
                "-C",
                "/tmp",
                "rm",
                "-rf",
                "/tmp/example",
            ]),
            vec_str(&[
                "env",
                "-S",
                "rm -rf /tmp/example",
                "rm",
                "-rf",
                "/tmp/example",
            ]),
        ] {
            assert_eq!(
                dangerous_command_match(&command),
                Some(DangerousCommandMatch::ForcedRm),
                "{command:?}"
            );
        }
    }

    #[test]
    fn deeply_nested_command_wrappers_fail_closed() {
        for (depth, expected) in [
            (
                MAX_DANGEROUS_COMMAND_WRAPPER_DEPTH,
                DangerousCommandMatch::ForcedRm,
            ),
            (
                MAX_DANGEROUS_COMMAND_WRAPPER_DEPTH + 1,
                DangerousCommandMatch::Other,
            ),
        ] {
            let command = std::iter::repeat_n("env", depth)
                .chain(["rm", "-rf", "/tmp/example"])
                .map(str::to_owned)
                .collect::<Vec<_>>();

            assert_eq!(dangerous_command_match(&command), Some(expected));
        }
    }

    #[test]
    fn forced_rm_in_complex_shell_syntax_is_dangerous() {
        for script in [
            "printf x | rm -rf /tmp/example",
            "if test -d /tmp/example; then rm --force /tmp/example; fi",
            "rm -rf \"$TARGET\" >/dev/null",
            "for target in /tmp/a /tmp/b; do rm -r -f \"$target\"; done",
            "echo \"$(rm -rf /tmp/example)\"",
            "bash -c 'rm -rf /tmp/example'",
            "trap 'rm -rf /tmp/example' EXIT",
            "for a in '-C5a25KeRr' '--' '--json' '--bogus'; do HOME=$(mktemp -d) MDE_URL=http://127.0.0.1:1 MDE_TOKEN=x node cli/mde.cjs ls \"$a\" >/tmp/mde-review-out 2>/tmp/mde-review-err; code=$?; printf '%s\\t%s\\t%s\\n' \"$a\" \"$code\" \"$(tr '\\n' ' ' </tmp/mde-review-err)\"; rm -rf \"$HOME\"; done",
        ] {
            let command = vec_str(&["bash", "-lc", script]);
            assert_eq!(
                dangerous_command_match(&command),
                Some(DangerousCommandMatch::ForcedRm),
                "{script}"
            );
        }
    }

    #[test]
    fn non_forced_or_non_literal_rm_is_not_dangerous() {
        for command in [
            vec_str(&["rm", "-r", "/tmp/example"]),
            vec_str(&["rm", "--", "-f"]),
            vec_str(&["bash", "-lc", "echo 'rm -rf /tmp/example'"]),
            vec_str(&["bash", "-lc", "cmd=rm; $cmd -rf /tmp/example"]),
            vec_str(&["bash", "-lc", "if then rm -rf /tmp/example"]),
            vec_str(&["env", "TARGET=/tmp/example", "rm", "-r", "/tmp/example"]),
            vec_str(&["bash", "-lc", "trap 'echo rm -rf /tmp/example' EXIT"]),
        ] {
            assert_eq!(dangerous_command_match(&command), None, "{command:?}");
        }
    }

    #[test]
    fn direct_powershell_words_return_other_match_on_windows() {
        let command = vec_str(&["Remove-Item", "test", "-Force"]);

        if cfg!(windows) {
            assert_eq!(
                dangerous_powershell_words_match(&command, DangerousCommandPlatform::host()),
                Some(DangerousCommandMatch::Other)
            );
        } else {
            assert_eq!(
                dangerous_powershell_words_match(&command, DangerousCommandPlatform::host()),
                None
            );
        }
    }
}
