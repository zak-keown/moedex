use codex_product_identity::PRODUCT_IDENTITY;
use codex_utils_absolute_path::AbsolutePathBuf;
use dirs::home_dir;
use std::ffi::OsStr;
use std::io;
use std::path::Path;
use std::path::PathBuf;

/// The input that selected the effective product home directory.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeSource {
    /// The Moedex-specific home override was set.
    MoedexHome,
    /// The established Codex home override was selected for compatibility.
    CodexHomeCompatibility,
    /// No explicit override was selected, so the Moedex default is in use.
    Default,
}

/// The product home directory and the source that selected it.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct ResolvedProductHome {
    /// The validated, absolute product home directory.
    pub path: AbsolutePathBuf,
    /// The environment or default that selected [`Self::path`].
    pub source: HomeSource,
}

/// Resolve a product home from explicitly supplied environment values.
///
/// A nonempty `MOEDEX_HOME` wins over a nonempty `CODEX_HOME` compatibility
/// override. Explicit paths must exist, be directories, and canonicalize. The
/// default path is returned without requiring it to exist.
pub fn resolve_product_home(
    moedex_home: Option<&OsStr>,
    codex_home: Option<&OsStr>,
    user_home: &Path,
) -> io::Result<ResolvedProductHome> {
    if let Some(home) = resolve_explicit_product_home(moedex_home, codex_home) {
        return home;
    }

    Ok(ResolvedProductHome {
        path: AbsolutePathBuf::from_absolute_path(
            user_home.join(PRODUCT_IDENTITY.default_home_dir),
        )?,
        source: HomeSource::Default,
    })
}

fn resolve_explicit_product_home(
    moedex_home: Option<&OsStr>,
    codex_home: Option<&OsStr>,
) -> Option<io::Result<ResolvedProductHome>> {
    if let Some(moedex_home) = moedex_home.filter(|path| !path.is_empty()) {
        return Some(resolve_explicit_home(
            moedex_home,
            PRODUCT_IDENTITY.primary_home_env,
            HomeSource::MoedexHome,
        ));
    }

    if let Some(codex_home) = codex_home.filter(|path| !path.is_empty()) {
        return Some(resolve_explicit_home(
            codex_home,
            PRODUCT_IDENTITY.compatibility_home_env,
            HomeSource::CodexHomeCompatibility,
        ));
    }

    None
}

fn resolve_explicit_home(
    home: &OsStr,
    environment_variable: &str,
    source: HomeSource,
) -> io::Result<ResolvedProductHome> {
    let path = PathBuf::from(home);
    let metadata = std::fs::metadata(&path).map_err(|err| match err.kind() {
        io::ErrorKind::NotFound => io::Error::new(
            io::ErrorKind::NotFound,
            format!("{environment_variable} points to {home:?}, but that path does not exist"),
        ),
        _ => io::Error::new(
            err.kind(),
            format!("failed to read {environment_variable} {home:?}: {err}"),
        ),
    })?;

    if !metadata.is_dir() {
        return Err(io::Error::new(
            io::ErrorKind::InvalidInput,
            format!("{environment_variable} points to {home:?}, but that path is not a directory"),
        ));
    }

    let canonical = path.canonicalize().map_err(|err| {
        io::Error::new(
            err.kind(),
            format!("failed to canonicalize {environment_variable} {home:?}: {err}"),
        )
    })?;
    Ok(ResolvedProductHome {
        path: AbsolutePathBuf::from_absolute_path(canonical)?,
        source,
    })
}

/// Return the product home directory and the source that selected it.
pub fn find_product_home() -> io::Result<ResolvedProductHome> {
    let moedex_home =
        std::env::var_os(PRODUCT_IDENTITY.primary_home_env).filter(|path| !path.is_empty());
    let codex_home =
        std::env::var_os(PRODUCT_IDENTITY.compatibility_home_env).filter(|path| !path.is_empty());
    find_product_home_from_env(moedex_home.as_deref(), codex_home.as_deref(), home_dir)
}

fn find_product_home_from_env<F>(
    moedex_home: Option<&OsStr>,
    codex_home: Option<&OsStr>,
    find_user_home: F,
) -> io::Result<ResolvedProductHome>
where
    F: FnOnce() -> Option<PathBuf>,
{
    if let Some(home) = resolve_explicit_product_home(moedex_home, codex_home) {
        return home;
    }

    let user_home = find_user_home()
        .ok_or_else(|| io::Error::new(io::ErrorKind::NotFound, "Could not find home directory"))?;
    resolve_product_home(
        /*moedex_home*/ None, /*codex_home*/ None, &user_home,
    )
}

/// Returns the product configuration directory without exposing its source.
///
/// This compatibility name remains for internal callers. New diagnostics that
/// need to report whether the Codex compatibility override is active should
/// use [`find_product_home`].
pub fn find_codex_home() -> io::Result<AbsolutePathBuf> {
    Ok(find_product_home()?.path)
}

#[cfg(test)]
fn find_codex_home_from_env(codex_home_env: Option<&str>) -> io::Result<AbsolutePathBuf> {
    find_product_home_from_env(
        /*moedex_home*/ None,
        codex_home_env.map(OsStr::new),
        home_dir,
    )
    .map(|home| home.path)
}

#[cfg(test)]
mod tests {
    use super::HomeSource;
    use super::ResolvedProductHome;
    use super::find_codex_home_from_env;
    use super::find_product_home_from_env;
    use super::resolve_product_home;
    use codex_utils_absolute_path::AbsolutePathBuf;
    use dirs::home_dir;
    use pretty_assertions::assert_eq;
    use std::ffi::OsStr;
    use std::fs;
    use std::io::ErrorKind;
    use std::path::PathBuf;
    use tempfile::TempDir;

    #[test]
    fn explicit_product_home_does_not_require_default_home_discovery() {
        let root = TempDir::new().expect("temp home");
        let moedex_home = root.path().join("moedex-home");
        fs::create_dir_all(&moedex_home).expect("create Moedex home");

        assert_eq!(
            find_product_home_from_env(Some(moedex_home.as_os_str()), None, || None::<PathBuf>)
                .expect("explicit Moedex home"),
            ResolvedProductHome {
                path: AbsolutePathBuf::from_absolute_path(
                    moedex_home
                        .canonicalize()
                        .expect("canonicalize Moedex home"),
                )
                .expect("absolute Moedex home"),
                source: HomeSource::MoedexHome,
            },
        );
    }

    #[test]
    fn moedex_home_wins_over_compatibility_home() {
        let root = TempDir::new().expect("temp home");
        let moedex_home = root.path().join("moedex-home");
        let codex_home = root.path().join("codex-home");
        fs::create_dir_all(&moedex_home).expect("create Moedex home");
        fs::create_dir_all(&codex_home).expect("create Codex home");

        assert_eq!(
            resolve_product_home(
                Some(moedex_home.as_os_str()),
                Some(codex_home.as_os_str()),
                root.path(),
            )
            .expect("resolve Moedex home"),
            ResolvedProductHome {
                path: AbsolutePathBuf::from_absolute_path(
                    moedex_home
                        .canonicalize()
                        .expect("canonicalize Moedex home"),
                )
                .expect("absolute Moedex home"),
                source: HomeSource::MoedexHome,
            },
        );
    }

    #[test]
    fn blank_home_overrides_use_the_moedex_default() {
        let root = TempDir::new().expect("temp home");

        assert_eq!(
            resolve_product_home(Some(OsStr::new("")), Some(OsStr::new("")), root.path())
                .expect("resolve default home"),
            ResolvedProductHome {
                path: AbsolutePathBuf::from_absolute_path(root.path().join(".moedex"))
                    .expect("absolute default home"),
                source: HomeSource::Default,
            },
        );
    }

    #[test]
    fn invalid_moedex_home_does_not_fall_back_to_compatibility_home() {
        let root = TempDir::new().expect("temp home");
        let codex_home = root.path().join("codex-home");
        fs::create_dir_all(&codex_home).expect("create Codex home");
        let missing_moedex_home = root.path().join("missing-moedex-home");

        let error = resolve_product_home(
            Some(missing_moedex_home.as_os_str()),
            Some(codex_home.as_os_str()),
            root.path(),
        )
        .expect_err("invalid Moedex home must fail");

        assert_eq!(error.kind(), ErrorKind::NotFound);
        assert!(
            error.to_string().contains("MOEDEX_HOME"),
            "unexpected error: {error}"
        );
    }

    #[test]
    fn find_codex_home_env_missing_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let missing = temp_home.path().join("missing-codex-home");
        let missing_str = missing
            .to_str()
            .expect("missing codex home path should be valid utf-8");

        let err = find_codex_home_from_env(Some(missing_str)).expect_err("missing CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::NotFound);
        assert!(
            err.to_string().contains("CODEX_HOME"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_file_path_is_fatal() {
        let temp_home = TempDir::new().expect("temp home");
        let file_path = temp_home.path().join("codex-home.txt");
        fs::write(&file_path, "not a directory").expect("write temp file");
        let file_str = file_path
            .to_str()
            .expect("file codex home path should be valid utf-8");

        let err = find_codex_home_from_env(Some(file_str)).expect_err("file CODEX_HOME");
        assert_eq!(err.kind(), ErrorKind::InvalidInput);
        assert!(
            err.to_string().contains("not a directory"),
            "unexpected error: {err}"
        );
    }

    #[test]
    fn find_codex_home_env_valid_directory_canonicalizes() {
        let temp_home = TempDir::new().expect("temp home");
        let temp_str = temp_home
            .path()
            .to_str()
            .expect("temp codex home path should be valid utf-8");

        let resolved = find_codex_home_from_env(Some(temp_str)).expect("valid CODEX_HOME");
        let expected = temp_home
            .path()
            .canonicalize()
            .expect("canonicalize temp home");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }

    #[test]
    fn find_codex_home_without_env_uses_default_home_dir() {
        let resolved =
            find_codex_home_from_env(/*codex_home_env*/ None).expect("default CODEX_HOME");
        let mut expected = home_dir().expect("home dir");
        expected.push(".moedex");
        let expected = AbsolutePathBuf::from_absolute_path(expected).expect("absolute home");
        assert_eq!(resolved, expected);
    }
}
