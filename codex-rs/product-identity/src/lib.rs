//! Product-facing distribution identity shared by Moedex components.

/// Public values that define the Moedex distribution while internal Codex
/// crate and protocol names remain stable.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProductIdentity {
    /// Name shown to users in command-line and application surfaces.
    pub display_name: &'static str,
    /// Installed executable name without an operating-system extension.
    pub executable_name: &'static str,
    /// Default directory below the user's home directory for local state.
    pub default_home_dir: &'static str,
    /// Environment variable that explicitly selects a Moedex home directory.
    pub primary_home_env: &'static str,
    /// Existing environment variable accepted as an explicit compatibility override.
    pub compatibility_home_env: &'static str,
    /// Repository that owns Moedex releases.
    pub github_repository: &'static str,
    /// Keyring service namespace owned by Moedex.
    pub credential_service: &'static str,
}

/// Public identity for the Moedex distribution.
pub const PRODUCT_IDENTITY: ProductIdentity = ProductIdentity {
    display_name: "Moedex",
    executable_name: "moedex",
    default_home_dir: ".moedex",
    primary_home_env: "MOEDEX_HOME",
    compatibility_home_env: "CODEX_HOME",
    github_repository: "zak-keown/moedex",
    credential_service: "Moedex Auth",
};
