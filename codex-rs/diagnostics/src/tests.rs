use super::Gauge;
use super::snapshot;
use pretty_assertions::assert_eq;

static GUARD_GAUGE: Gauge = Gauge::new("diagnostics.tests.guard");
static REGISTERED_GAUGE: Gauge = Gauge::new("diagnostics.tests.registered");

#[test]
fn gauge_guards_follow_the_measured_lifetime() {
    let first = GUARD_GAUGE.track();
    let second = GUARD_GAUGE.track();
    let value = || {
        snapshot()
            .gauges
            .into_iter()
            .find(|gauge| gauge.name == "diagnostics.tests.guard")
            .expect("used gauge should be registered")
            .value
    };

    assert_eq!(value(), 2);
    drop(first);
    assert_eq!(value(), 1);
    drop(second);
    assert_eq!(value(), 0);
}

#[test]
fn snapshot_includes_process_memory_and_registers_gauges_once() {
    REGISTERED_GAUGE.increment();
    REGISTERED_GAUGE.increment();
    let diagnostics = snapshot();
    let registered = diagnostics
        .gauges
        .iter()
        .filter(|gauge| gauge.name == "diagnostics.tests.registered")
        .collect::<Vec<_>>();

    assert_eq!(diagnostics.process.id, std::process::id());
    assert_eq!(registered.len(), 1);
    assert_eq!(registered[0].value, 2);
    #[cfg(any(target_os = "macos", target_os = "linux", target_os = "windows"))]
    assert!(
        diagnostics
            .process
            .resident_memory_bytes
            .is_some_and(|bytes| bytes > 0)
    );
    #[cfg(target_os = "macos")]
    assert!(
        diagnostics
            .process
            .physical_footprint_bytes
            .is_some_and(|bytes| bytes > 0)
    );
    #[cfg(not(target_os = "macos"))]
    assert_eq!(diagnostics.process.physical_footprint_bytes, None);
}

#[test]
fn compatibility_home_diagnostic_contains_only_path_and_source() {
    use codex_utils_absolute_path::AbsolutePathBuf;
    use codex_utils_home_dir::HomeSource;
    use codex_utils_home_dir::ResolvedProductHome;
    let root = tempfile::tempdir().expect("home");
    let home = ResolvedProductHome {
        path: AbsolutePathBuf::from_absolute_path(root.path()).expect("absolute"),
        source: HomeSource::CodexHomeCompatibility,
    };
    let report = super::home_diagnostic(home.clone());
    assert_eq!(
        report,
        super::HomeDiagnostic {
            path: home.path,
            source: home.source,
            shares_codex_state: true
        }
    );
}

#[test]
fn shared_home_guard_rejects_a_live_stock_owner_and_accepts_stale_lock() {
    use codex_utils_absolute_path::AbsolutePathBuf;
    use codex_utils_home_dir::HomeSource;
    use codex_utils_home_dir::ResolvedProductHome;
    let root = tempfile::tempdir().expect("home");
    let state = root.path().join("app-server-daemon");
    std::fs::create_dir(&state).expect("state directory");
    let owner = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(state.join("daemon.lock"))
        .expect("owner lock");
    owner.lock().expect("live owner");
    let home = ResolvedProductHome {
        path: AbsolutePathBuf::from_absolute_path(root.path()).expect("absolute"),
        source: HomeSource::CodexHomeCompatibility,
    };
    assert_eq!(
        super::acquire_home_write_guard(&home, super::HomeAccess::Store)
            .expect_err("live stock owner")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    drop(owner);
    super::acquire_home_write_guard(&home, super::HomeAccess::Store).expect("stale lock is safe");
}

#[test]
fn daemon_guard_probes_but_does_not_retain_legacy_startup_lock() {
    let home = tempfile::tempdir().expect("home");
    let path = home
        .path()
        .join("app-server-control/app-server-startup.lock");
    std::fs::create_dir_all(path.parent().expect("parent")).expect("directory");
    let owner = std::fs::OpenOptions::new()
        .create(true)
        .truncate(false)
        .read(true)
        .write(true)
        .open(&path)
        .expect("owner");
    owner.lock().expect("live owner");
    assert_eq!(
        super::acquire_selected_daemon_home_guard(home.path())
            .expect_err("live incompatible owner")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    owner.unlock().expect("stale owner");
    let _guard = super::acquire_selected_daemon_home_guard(home.path()).expect("stale lock");
    owner
        .try_lock()
        .expect("legacy child can acquire startup lock while parent guard lives");
}

#[test]
fn store_guard_creates_and_exclusively_holds_the_moedex_auth_lock() {
    let home = tempfile::tempdir().expect("home");
    let first = super::acquire_selected_home_write_guard(home.path()).expect("first guard");
    let lock_path = home.path().join("moedex-auth.lock");

    assert!(lock_path.is_file());
    assert_eq!(
        super::acquire_selected_home_write_guard(home.path())
            .expect_err("second writer must be excluded")
            .kind(),
        std::io::ErrorKind::WouldBlock
    );
    drop(first);
    super::acquire_selected_home_write_guard(home.path()).expect("released guard");
}
