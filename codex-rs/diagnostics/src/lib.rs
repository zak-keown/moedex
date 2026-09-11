use codex_utils_absolute_path::AbsolutePathBuf;
use codex_utils_home_dir::HomeSource;
use codex_utils_home_dir::ResolvedProductHome;
use std::sync::Mutex;
use std::sync::Once;
use std::sync::atomic::AtomicU64;
use std::sync::atomic::Ordering;

/// Effective local state location, with no credential or configuration contents.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct HomeDiagnostic {
    pub path: AbsolutePathBuf,
    pub source: HomeSource,
    pub shares_codex_state: bool,
}

/// Reports whether the explicit Codex compatibility override selected this home.
pub fn home_diagnostic(home: ResolvedProductHome) -> HomeDiagnostic {
    HomeDiagnostic {
        shares_codex_state: home.source == HomeSource::CodexHomeCompatibility,
        path: home.path,
        source: home.source,
    }
}

#[derive(Clone, Copy)]
enum HomeAccess {
    Store,
    Daemon,
}

/// Checks existing stock writer locks without reading their contents. Retain the
/// returned handles through the operation. Daemon launch only probes the startup
/// lock so a legacy child can acquire it; this cannot fence a later startup race.
/// Stock clients that do not take these locks cannot participate in this guard.
fn acquire_home_write_guard(
    home: &ResolvedProductHome,
    access: HomeAccess,
) -> std::io::Result<Vec<std::fs::File>> {
    if home.source == HomeSource::CodexHomeCompatibility {
        tracing::warn!(path = %home.path.display(), source = ?home.source, "shared product home");
    }
    let mut guards = Vec::new();
    if matches!(access, HomeAccess::Store) {
        std::fs::create_dir_all(home.path.as_path())?;
        let auth_lock_path = home.path.as_path().join("moedex-auth.lock");
        let mut options = std::fs::OpenOptions::new();
        options.create(true).truncate(false).read(true).write(true);
        #[cfg(unix)]
        {
            use std::os::unix::fs::OpenOptionsExt;
            options.mode(0o600);
        }
        let auth_lock = options.open(&auth_lock_path)?;
        if let Err(error) = auth_lock.try_lock() {
            let kind = match error {
                std::fs::TryLockError::WouldBlock => std::io::ErrorKind::WouldBlock,
                std::fs::TryLockError::Error(error) => error.kind(),
            };
            return Err(std::io::Error::new(
                kind,
                format!(
                    "Moedex credential store is busy: {}",
                    auth_lock_path.display()
                ),
            ));
        }
        guards.push(auth_lock);
    }
    for relative in [
        "app-server-daemon/daemon.lock",
        "app-server-control/app-server-startup.lock",
    ] {
        let path = home.path.as_path().join(relative);
        let file = match std::fs::OpenOptions::new()
            .read(true)
            .write(true)
            .open(&path)
        {
            Ok(file) => file,
            Err(error) if error.kind() == std::io::ErrorKind::NotFound => continue,
            Err(error) => return Err(error),
        };
        if let Err(error) = file.try_lock() {
            let kind = match error {
                std::fs::TryLockError::WouldBlock => std::io::ErrorKind::WouldBlock,
                std::fs::TryLockError::Error(error) => error.kind(),
            };
            return Err(std::io::Error::new(
                kind,
                format!(
                    "incompatible home owner: path={}, source={:?}",
                    home.path.display(),
                    home.source
                ),
            ));
        }
        // A legacy child acquires this startup lock itself before binding.
        // Probe its current owner, but do not hold it while waiting for the child.
        if matches!(access, HomeAccess::Store) || relative == "app-server-daemon/daemon.lock" {
            guards.push(file);
        }
    }
    Ok(guards)
}

/// Checks a caller-selected local path using its environment source when known.
pub fn acquire_selected_home_write_guard(
    path: &std::path::Path,
) -> std::io::Result<Vec<std::fs::File>> {
    acquire_selected_home_guard(path, HomeAccess::Store)
}

/// Checks stock ownership before a daemon operation, retaining the stock
/// operation lock but releasing its startup lock for a compatible legacy child.
pub fn acquire_selected_daemon_home_guard(
    path: &std::path::Path,
) -> std::io::Result<Vec<std::fs::File>> {
    acquire_selected_home_guard(path, HomeAccess::Daemon)
}

fn acquire_selected_home_guard(
    path: &std::path::Path,
    access: HomeAccess,
) -> std::io::Result<Vec<std::fs::File>> {
    let path = path.canonicalize().unwrap_or_else(|_| path.to_path_buf());
    let path = AbsolutePathBuf::try_from(path)?;
    let source = codex_utils_home_dir::find_product_home()
        .ok()
        .filter(|home| home.path == path)
        .map_or(HomeSource::MoedexHome, |home| home.source);
    acquire_home_write_guard(&ResolvedProductHome { path, source }, access)
}

static GAUGES: Mutex<Vec<&'static Gauge>> = Mutex::new(Vec::new());

/// A process-wide gauge that registers itself the first time it is used.
pub struct Gauge {
    name: &'static str,
    value: AtomicU64,
    registered: Once,
}

impl Gauge {
    /// Creates a gauge suitable for use in a `static` declaration.
    pub const fn new(name: &'static str) -> Self {
        Self {
            name,
            value: AtomicU64::new(0),
            registered: Once::new(),
        }
    }

    /// Increments the gauge and registers it if needed.
    pub fn increment(&'static self) {
        self.register();
        self.value.fetch_add(1, Ordering::Relaxed);
    }

    /// Decrements the gauge without allowing an underflow.
    pub fn decrement(&self) {
        let _ = self
            .value
            .fetch_update(Ordering::Relaxed, Ordering::Relaxed, |value| {
                Some(value.saturating_sub(1))
            });
    }

    /// Increments the gauge for the lifetime of the returned guard.
    pub fn track(&'static self) -> GaugeGuard {
        self.increment();
        GaugeGuard { gauge: self }
    }

    fn register(&'static self) {
        self.registered.call_once(|| {
            GAUGES
                .lock()
                .unwrap_or_else(std::sync::PoisonError::into_inner)
                .push(self);
        });
    }
}

/// Decrements its gauge when the measured object is dropped.
pub struct GaugeGuard {
    gauge: &'static Gauge,
}

impl Drop for GaugeGuard {
    fn drop(&mut self) {
        self.gauge.decrement();
    }
}

/// The current value of one registered diagnostic gauge.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct GaugeSnapshot {
    pub name: &'static str,
    pub value: u64,
}

/// Best-effort operating-system measurements for the current process.
#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub struct ProcessSnapshot {
    pub id: u32,
    pub resident_memory_bytes: Option<u64>,
    pub physical_footprint_bytes: Option<u64>,
}

/// Content-free diagnostic values contributed by this process.
#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DiagnosticsSnapshot {
    pub process: ProcessSnapshot,
    pub gauges: Vec<GaugeSnapshot>,
}

/// Collects built-in process measurements and every registered gauge.
pub fn snapshot() -> DiagnosticsSnapshot {
    let mut gauges = GAUGES
        .lock()
        .unwrap_or_else(std::sync::PoisonError::into_inner)
        .iter()
        .map(|gauge| GaugeSnapshot {
            name: gauge.name,
            value: gauge.value.load(Ordering::Relaxed),
        })
        .collect::<Vec<_>>();
    gauges.sort_unstable_by_key(|gauge| gauge.name);

    DiagnosticsSnapshot {
        process: process_snapshot(),
        gauges,
    }
}

#[cfg(target_os = "macos")]
fn process_snapshot() -> ProcessSnapshot {
    let usage = unsafe {
        let mut usage = std::mem::MaybeUninit::<libc::rusage_info_v0>::zeroed();
        // SAFETY: the kernel initializes this correctly sized buffer on success.
        if libc::proc_pid_rusage(
            libc::getpid(),
            libc::RUSAGE_INFO_V0,
            usage.as_mut_ptr().cast(),
        ) != 0
        {
            return empty_process_snapshot();
        }
        usage.assume_init()
    };

    ProcessSnapshot {
        id: std::process::id(),
        resident_memory_bytes: Some(usage.ri_resident_size),
        physical_footprint_bytes: Some(usage.ri_phys_footprint),
    }
}

#[cfg(target_os = "linux")]
fn process_snapshot() -> ProcessSnapshot {
    // SAFETY: querying the system page size does not access caller-owned memory.
    let page_size = u64::try_from(unsafe { libc::sysconf(libc::_SC_PAGESIZE) })
        .ok()
        .filter(|page_size| *page_size > 0);
    let resident_pages = std::fs::read_to_string("/proc/self/statm")
        .ok()
        .and_then(|statm| statm.split_whitespace().nth(1)?.parse::<u64>().ok());

    ProcessSnapshot {
        id: std::process::id(),
        resident_memory_bytes: resident_pages
            .zip(page_size)
            .map(|(pages, page_size)| pages.saturating_mul(page_size)),
        physical_footprint_bytes: None,
    }
}

#[cfg(target_os = "windows")]
fn process_snapshot() -> ProcessSnapshot {
    #[repr(C)]
    struct ProcessMemoryCounters {
        size: u32,
        page_fault_count: u32,
        peak_working_set_size: usize,
        working_set_size: usize,
        quota_peak_paged_pool_usage: usize,
        quota_paged_pool_usage: usize,
        quota_peak_non_paged_pool_usage: usize,
        quota_non_paged_pool_usage: usize,
        pagefile_usage: usize,
        peak_pagefile_usage: usize,
    }

    #[link(name = "kernel32")]
    unsafe extern "system" {
        fn GetCurrentProcess() -> *mut std::ffi::c_void;
        fn K32GetProcessMemoryInfo(
            process: *mut std::ffi::c_void,
            counters: *mut ProcessMemoryCounters,
            size: u32,
        ) -> i32;
    }

    let counters = unsafe {
        let mut counters = std::mem::MaybeUninit::<ProcessMemoryCounters>::zeroed();
        let size = u32::try_from(std::mem::size_of::<ProcessMemoryCounters>()).unwrap_or(u32::MAX);
        // SAFETY: the pseudo-handle is valid and the kernel initializes this
        // correctly sized writable buffer on success.
        if K32GetProcessMemoryInfo(GetCurrentProcess(), counters.as_mut_ptr(), size) == 0 {
            return empty_process_snapshot();
        }
        counters.assume_init()
    };

    ProcessSnapshot {
        id: std::process::id(),
        resident_memory_bytes: u64::try_from(counters.working_set_size).ok(),
        physical_footprint_bytes: None,
    }
}

#[cfg(not(any(target_os = "macos", target_os = "linux", target_os = "windows")))]
fn process_snapshot() -> ProcessSnapshot {
    empty_process_snapshot()
}

#[cfg(not(target_os = "linux"))]
fn empty_process_snapshot() -> ProcessSnapshot {
    ProcessSnapshot {
        id: std::process::id(),
        resident_memory_bytes: None,
        physical_footprint_bytes: None,
    }
}

#[cfg(test)]
#[path = "tests.rs"]
mod tests;
