#[cfg(any(not(debug_assertions), test))]
use codex_install_context::InstallContext;
#[cfg(any(not(debug_assertions), test))]
use codex_install_context::InstallMethod;
#[cfg(any(not(debug_assertions), test))]
use codex_install_context::StandalonePlatform;

const RELEASE_URL: &str = "https://github.com/zak-keown/moedex/releases/latest";

/// Update action the CLI should perform after the TUI exits.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum UpdateAction {
    /// Install the latest Unix artifact published by the Moedex fork.
    GitHubReleaseUnix,
    /// Install the latest Windows artifact published by the Moedex fork.
    GitHubReleaseWindows,
    /// No updater is configured for the detected installation method.
    Disabled { release_url: &'static str },
}

impl UpdateAction {
    #[cfg(any(not(debug_assertions), test))]
    pub(crate) fn from_install_context(context: &InstallContext) -> Self {
        match &context.method {
            InstallMethod::Standalone { platform, .. } => match platform {
                StandalonePlatform::Unix => Self::GitHubReleaseUnix,
                StandalonePlatform::Windows => Self::GitHubReleaseWindows,
            },
            InstallMethod::Npm
            | InstallMethod::Bun
            | InstallMethod::VitePlus
            | InstallMethod::Pnpm
            | InstallMethod::Brew
            | InstallMethod::Other => Self::Disabled {
                release_url: RELEASE_URL,
            },
        }
    }

    #[cfg(not(debug_assertions))]
    fn enabled(self) -> Option<Self> {
        match self {
            Self::GitHubReleaseUnix | Self::GitHubReleaseWindows => Some(self),
            Self::Disabled { .. } => None,
        }
    }

    /// Returns the enabled update actions for invariant tests.
    #[cfg(test)]
    pub(crate) fn supported_for_tests() -> [Self; 2] {
        [Self::GitHubReleaseUnix, Self::GitHubReleaseWindows]
    }

    /// Returns the list of command-line arguments for invoking the update.
    pub fn command_args(self) -> (&'static str, &'static [&'static str]) {
        match self {
            Self::GitHubReleaseUnix => (
                "sh",
                &[
                    "-c",
                    "curl -fsSL https://github.com/zak-keown/moedex/releases/latest/download/install.sh | MOEDEX_NON_INTERACTIVE=1 sh",
                ],
            ),
            Self::GitHubReleaseWindows => (
                "powershell",
                &[
                    "-ExecutionPolicy",
                    "Bypass",
                    "-c",
                    "$env:MOEDEX_NON_INTERACTIVE=1; irm https://github.com/zak-keown/moedex/releases/latest/download/install.ps1 | iex",
                ],
            ),
            Self::Disabled { .. } => ("open", &[RELEASE_URL]),
        }
    }

    /// Returns string representation of the command-line arguments for invoking the update.
    pub fn command_str(self) -> String {
        let (command, args) = self.command_args();
        shlex::try_join(std::iter::once(command).chain(args.iter().copied()))
            .unwrap_or_else(|_| format!("{command} {}", args.join(" ")))
    }
}

#[cfg(not(debug_assertions))]
pub fn get_update_action() -> Option<UpdateAction> {
    UpdateAction::from_install_context(InstallContext::current()).enabled()
}

#[cfg(test)]
mod tests {
    use super::*;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use pretty_assertions::assert_eq;

    #[test]
    fn every_enabled_update_action_targets_moedex() {
        for action in UpdateAction::supported_for_tests() {
            let command = action.command_str();
            assert!(command.contains("zak-keown/moedex"), "{command}");
            assert!(!command.contains("@openai/codex"), "{command}");
            assert!(!command.contains("brew upgrade --cask codex"), "{command}");
            assert!(!command.contains("chatgpt.com/codex"), "{command}");
        }
    }

    #[test]
    fn maps_only_standalone_installs_to_enabled_update_actions() {
        let native_release_dir =
            AbsolutePathBuf::from_absolute_path(std::env::temp_dir().join("native-release"))
                .expect("temp dir path should be absolute");
        for method in [
            InstallMethod::Other,
            InstallMethod::Npm,
            InstallMethod::Bun,
            InstallMethod::VitePlus,
            InstallMethod::Pnpm,
            InstallMethod::Brew,
        ] {
            assert_eq!(
                UpdateAction::from_install_context(&InstallContext {
                    method,
                    package_layout: None,
                }),
                UpdateAction::Disabled {
                    release_url: RELEASE_URL,
                }
            );
        }
        assert_eq!(
            UpdateAction::from_install_context(&InstallContext {
                method: InstallMethod::Standalone {
                    platform: StandalonePlatform::Unix,
                    release_dir: native_release_dir.clone(),
                    resources_dir: Some(native_release_dir.join("codex-resources")),
                },
                package_layout: None,
            }),
            UpdateAction::GitHubReleaseUnix
        );
        assert_eq!(
            UpdateAction::from_install_context(&InstallContext {
                method: InstallMethod::Standalone {
                    platform: StandalonePlatform::Windows,
                    release_dir: native_release_dir.clone(),
                    resources_dir: Some(native_release_dir.join("codex-resources")),
                },
                package_layout: None,
            }),
            UpdateAction::GitHubReleaseWindows
        );
    }
}
