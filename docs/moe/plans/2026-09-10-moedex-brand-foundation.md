# Moedex Brand Foundation Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Ship a separately installable, visibly Moedex-branded local distribution with isolated state, explicit Codex import, fork-owned updates, release provenance, and a regression manifest for custom behavior.

**Architecture:** Add a small `codex-product-identity` crate as the single source for public name, executable, home variables, release channel, credential namespace, and build provenance while retaining existing `codex-*` crate and wire names. Resolve the local home once through the existing home utility, pass it into same-host components, and implement import as a typed, preview-first service that copies through existing config, thread-store, and auth adapters. Package qualification consumes a versioned behavior manifest and validates actual artifacts rather than treating compilation as proof of compatibility.

**Tech Stack:** Rust 2024, Tokio, Clap, Serde/TOML/JSON, SQLx-backed thread state, existing auth and thread-store adapters, Node.js ESM wrapper, Python packaging scripts, GitHub Actions, `insta`.

**Spec:** `specs/moedex/SPEC.md`, `specs/moedex/BRAND.md`, and `specs/moedex/EXPERIENCE.md` (REBRAND, U4, U5)

## Global Constraints

- Public identity is **Moedex**, public executable is `moedex` (`moedex.exe` on Windows), and source distribution is `zak-keown/moedex`.
- Keep `codex-*` Rust crate names, internal module names, app-server wire methods/fields, rollout format, provider names, provider environment variables, LICENSE, NOTICE, and upstream attribution.
- Never modify code related to `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR`.
- Home precedence is nonempty `MOEDEX_HOME`, then nonempty `CODEX_HOME` compatibility override, then `~/.moedex`; blank variables are unset and invalid explicit paths fail.
- Repository-local `.codex` configuration and skills retain their existing precedence and names.
- Import is explicit, preview-first, copy-only, conflict-skip by default, and credentials are unselected by default.
- Initial update channel is GitHub releases for `zak-keown/moedex`; npm remains disabled until `@zak-keown/moedex` ownership is verified.
- New v2 app-server request optionals use `#[ts(optional = nullable)]`, wire names are camelCase, and schema changes require `just write-app-server-schema`.
- Core behavior supports Linux, macOS, and Windows, including client/exec-server split-host configurations.
- User-visible UI changes require reviewed `insta` snapshots; do not add tests for statically defined values.
- Every Rust task ends with `just fmt`; run the named crate tests before `just fix -p <crate>`, and do not rerun tests after `fix` or `fmt`.
- Any Cargo dependency change requires `just bazel-lock-update` and the corresponding `MODULE.bazel.lock` update.

## Open Decisions

No open decisions. The spec resolves typography-first branding, GitHub Releases as the initial channel, independent `0.x` distribution versions with upstream provenance, and `moedex import codex` as the command. Public npm publication remains out of scope until namespace ownership is verified.

## Not Yet Specified

- A custom logo, icon, palette, and ASCII mark can be specified after the text-first identity is usable.
- Homebrew tap and Windows package-manager channels can be specified after the GitHub artifact channel is qualified.

## Out of Scope

- Publishing any package or release; this plan builds and qualifies artifacts but does not mutate registries or GitHub Releases.
- Renaming internal crates, protocol fields, rollout records, `.codex` project configuration, provider products, or historical messages; those names are compatibility or attribution surfaces.
- Automatic migration or deletion of `~/.codex`; coexistence requires explicit copy and preserved source data.
- Android/Termux packaging; U10 belongs to Stage G.

---

### Task 1: Product identity, home resolution, and provenance seam (B3, B10, B11)

**Files:**
- Create: `codex-rs/product-identity/Cargo.toml`
- Create: `codex-rs/product-identity/BUILD.bazel`
- Create: `codex-rs/product-identity/src/lib.rs`
- Create: `codex-rs/product-identity/src/lib_tests.rs`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/utils/home-dir/Cargo.toml`
- Modify: `codex-rs/utils/home-dir/BUILD.bazel`
- Modify: `codex-rs/utils/home-dir/src/lib.rs`
- Modify: `codex-rs/build-info/Cargo.toml`
- Modify: `codex-rs/build-info/BUILD.bazel`
- Modify: `codex-rs/build-info/src/lib.rs`
- Modify: `codex-rs/build-info/src/build_info_tests.rs`

**Interfaces:**
- Consumes: Existing `AbsolutePathBuf`, `InstallContext`, and package manifest version resolution.
- Produces: `ProductIdentity`, `PRODUCT_IDENTITY`, `HomeSource`, `ResolvedProductHome`, `resolve_product_home(moedex_home: Option<&OsStr>, codex_home: Option<&OsStr>, user_home: &Path) -> io::Result<ResolvedProductHome>`, and `BuildProvenance` accessors on `BuildInfo`.

- [ ] **Step 1: Write failing identity and home-resolution tests**

```rust
#[test]
fn moedex_home_wins_and_blank_values_are_unset() {
    let root = TempDir::new().unwrap();
    let moedex = root.path().join("moedex");
    let codex = root.path().join("codex");
    fs::create_dir_all(&moedex).unwrap();
    fs::create_dir_all(&codex).unwrap();
    assert_eq!(
        resolve_product_home(Some(moedex.as_os_str()), Some(codex.as_os_str()), root.path()).unwrap(),
        ResolvedProductHome { path: moedex.canonicalize().unwrap().abs(), source: HomeSource::MoedexHome },
    );
    assert_eq!(
        resolve_product_home(Some(OsStr::new("")), Some(OsStr::new("")), root.path()).unwrap().source,
        HomeSource::Default,
    );
}

#[test]
fn version_leads_with_distribution_and_retains_upstream() {
    let info = BuildInfo::for_test("0.1.0", "forksha", "upstreamsha", "github");
    assert_eq!(info.display_version(), "Moedex 0.1.0");
    assert_eq!(info.provenance().upstream_commit, "upstreamsha");
}
```

- [ ] **Step 2: Run focused tests and confirm the new API is absent**

Run: `cd codex-rs && just test -p codex-utils-home-dir -p codex-build-info`

Expected: FAIL because `resolve_product_home`, `HomeSource`, and `BuildProvenance` do not exist.

- [ ] **Step 3: Add the identity crate and implement strict home precedence**

```rust
pub struct ProductIdentity {
    pub display_name: &'static str,
    pub executable_name: &'static str,
    pub default_home_dir: &'static str,
    pub primary_home_env: &'static str,
    pub compatibility_home_env: &'static str,
    pub github_repository: &'static str,
    pub credential_service: &'static str,
}

pub const PRODUCT_IDENTITY: ProductIdentity = ProductIdentity {
    display_name: "Moedex",
    executable_name: "moedex",
    default_home_dir: ".moedex",
    primary_home_env: "MOEDEX_HOME",
    compatibility_home_env: "CODEX_HOME",
    github_repository: "zak-keown/moedex",
    credential_service: "Moedex Auth",
};

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum HomeSource { MoedexHome, CodexHomeCompatibility, Default }
```

Implement validation by reusing the current explicit-path metadata/canonicalization behavior. Keep `find_codex_home()` as a compatibility function that returns `find_product_home()?.path`; add `find_product_home()` for diagnostics that need the source.

- [ ] **Step 4: Extend build information without changing protocol compatibility versions**

```rust
#[derive(Clone, Debug, Deserialize, Eq, PartialEq, Serialize)]
pub struct BuildProvenance {
    pub distribution_version: Version,
    pub fork_commit: String,
    pub upstream_commit: String,
    pub release_channel: String,
}
```

Read `STABLE_UPSTREAM_GIT_COMMIT` and `MOEDEX_RELEASE_CHANNEL` at the final-binary macro call site. Leave app-server protocol version fields untouched.

- [ ] **Step 5: Update Cargo/Bazel declarations and verify**

Run: `cd codex-rs && just bazel-lock-update && just test -p codex-product-identity -p codex-utils-home-dir -p codex-build-info && just fix -p codex-product-identity && just fix -p codex-utils-home-dir && just fix -p codex-build-info && just fmt`

Expected: all focused tests pass; generated dependency lock is current.

- [ ] **Step 6: Commit the seam atomically**

```bash
git add codex-rs/Cargo.toml codex-rs/product-identity codex-rs/utils/home-dir codex-rs/build-info MODULE.bazel.lock
git commit -m "feat: establish Moedex product identity"
```

### Task 2: Isolate local runtime and credential namespaces (B3, B4, B7, U4)

**Files:**
- Modify: `codex-rs/login/Cargo.toml`
- Modify: `codex-rs/login/BUILD.bazel`
- Modify: `codex-rs/login/src/auth/storage.rs`
- Modify: `codex-rs/login/src/auth/storage_tests.rs`
- Modify: `codex-rs/app-server-daemon/Cargo.toml`
- Modify: `codex-rs/app-server-daemon/BUILD.bazel`
- Modify: `codex-rs/app-server-daemon/src/lib.rs`
- Modify: `codex-rs/app-server-daemon/src/backend/pid_start.rs`
- Modify: `codex-rs/app-server-daemon/src/remote_control_client.rs`
- Modify: `codex-rs/diagnostics/Cargo.toml`
- Modify: `codex-rs/diagnostics/BUILD.bazel`
- Modify: `codex-rs/diagnostics/src/lib.rs`
- Modify: `codex-rs/diagnostics/src/tests.rs`
- Modify: `codex-rs/exec-server/src/environment_config.rs`
- Modify: `codex-rs/exec-server/src/remote/registration_retry_tests.rs`

**Interfaces:**
- Consumes: `PRODUCT_IDENTITY`, `find_product_home() -> io::Result<ResolvedProductHome>`, existing auth storage adapters, and existing daemon path constructors.
- Produces: `HomeDiagnostic { path: AbsolutePathBuf, source: HomeSource, shares_codex_state: bool }`; namespaced local daemon endpoint and auth service; same-host `MOEDEX_HOME` propagation with remote-host independent resolution.

- [ ] **Step 1: Add failing coexistence tests**

```rust
#[test]
fn keyring_service_is_moedex_owned() {
    assert_eq!(keyring_service_name(), "Moedex Auth");
}

#[test]
fn compatibility_home_is_visible_without_secrets() {
    let report = home_diagnostic(ResolvedProductHome { path: fixture.abs(), source: HomeSource::CodexHomeCompatibility });
    assert!(report.shares_codex_state);
    assert!(!format!("{report:?}").contains("token"));
}
```

Add a daemon test that stock and Moedex homes produce different socket/pipe/PID paths, plus an exec-server fixture proving a Windows remote uses its own configured path.

- [ ] **Step 2: Run the affected crate tests and observe failure**

Run: `cd codex-rs && just test -p codex-login -p codex-app-server-daemon -p codex-diagnostics -p codex-exec-server`

Expected: FAIL on old `Codex Auth` and shared daemon naming.

- [ ] **Step 3: Route namespaces through product identity**

```rust
pub fn keyring_service_name() -> &'static str {
    PRODUCT_IDENTITY.credential_service
}

pub fn home_diagnostic(home: ResolvedProductHome) -> HomeDiagnostic {
    HomeDiagnostic {
        shares_codex_state: home.source == HomeSource::CodexHomeCompatibility,
        path: home.path,
        source: home.source,
    }
}
```

Use `moedex-app-server` for local daemon endpoint identity. Propagate the resolved absolute home only to same-host child processes; omit the local path from remote exec-server registration so `environment_config.rs` resolves that host's environment and filesystem.

- [ ] **Step 4: Add the shared-home ownership guard**

Before daemon/store writer acquisition, emit a diagnostic when `HomeSource::CodexHomeCompatibility` is selected and reject acquisition if an incompatible live owner lock is found. Include path and source only.

- [ ] **Step 5: Verify isolation and format**

Run: `cd codex-rs && just bazel-lock-update && just test -p codex-login -p codex-app-server-daemon -p codex-diagnostics -p codex-exec-server && just fix -p codex-login && just fix -p codex-app-server-daemon && just fix -p codex-diagnostics && just fix -p codex-exec-server && just fmt`

Expected: coexistence, logout isolation, same-host propagation, and split-host tests pass.

- [ ] **Step 6: Commit the namespace isolation**

```bash
git add codex-rs/login codex-rs/app-server-daemon codex-rs/diagnostics codex-rs/exec-server MODULE.bazel.lock
git commit -m "feat: isolate Moedex runtime state"
```

### Task 3: Public executable and user-visible text identity (B1, B2, B12)

**Files:**
- Modify: `codex-rs/cli/Cargo.toml`
- Modify: `codex-rs/cli/src/main.rs`
- Modify: `codex-rs/cli/src/lib.rs`
- Modify: `codex-rs/cli/src/login.rs`
- Modify: `codex-rs/cli/src/doctor/output.rs`
- Modify: `codex-rs/tui/src/bottom_pane/status_surface_preview.rs`
- Modify: `codex-rs/tui/src/history_cell/session.rs`
- Modify: `codex-rs/tui/src/onboarding/welcome.rs`
- Modify: `codex-rs/tui/src/terminal_title.rs`
- Modify: `codex-rs/tui/src/chatwidget/tests.rs`
- Modify: `codex-cli/package.json`
- Modify: `codex-cli/bin/codex.js`
- Modify: `README.md`

**Interfaces:**
- Consumes: `PRODUCT_IDENTITY` and existing Clap/TUI snapshot infrastructure.
- Produces: `moedex` executable, `@zak-keown/moedex` package metadata (unpublished), branded CLI/TUI copy, and a reviewed remaining-name inventory in the README attribution section.

- [ ] **Step 1: Add failing packaged CLI and snapshot expectations**

```rust
#[test]
fn clap_command_is_moedex() {
    let command = Cli::command();
    assert_eq!(command.get_name(), "moedex");
}
```

Add snapshots for welcome, session header, status/about, login provider text, update error guidance, and terminal-title preview. Assert provider labels still say OpenAI/ChatGPT where they describe the provider.

- [ ] **Step 2: Run focused tests to capture old identity**

Run: `cd codex-rs && just test -p codex-cli -p codex-tui`

Expected: FAIL with `codex`/`Codex` in application-name positions and new `.snap.new` files.

- [ ] **Step 3: Rename public entrypoints while keeping internal crate names**

```toml
default-run = "moedex"

[[bin]]
name = "moedex"
path = "src/main.rs"
```

Set npm `name` to `@zak-keown/moedex`, `bin.moedex` to the existing launcher path, and update the launcher to resolve `moedex`/`moedex.exe` from its own package layout. Do not install `bin.codex`.

- [ ] **Step 4: Replace application-name copy through the identity constant**

Use `PRODUCT_IDENTITY.display_name` for headers, onboarding, status/about, notifications, titles, errors, and help examples. Keep explicit wording: `Moedex is an independent fork of OpenAI Codex (https://github.com/zak-keown/moedex).`

- [ ] **Step 5: Review and accept intended snapshots, then verify**

Run: `cd codex-rs && cargo insta pending-snapshots -p codex-tui`

Read every generated `*.snap.new`; confirm remaining OpenAI/Codex labels describe provider, protocol compatibility, historical content, or attribution. Then run: `cd codex-rs && cargo insta accept -p codex-tui && just test -p codex-cli -p codex-tui && just fix -p codex-cli && just fix -p codex-tui && just fmt`

Expected: CLI and TUI tests pass and no unintended snapshot remains pending.

- [ ] **Step 6: Smoke the Node wrapper locally**

Run: `node codex-cli/bin/codex.js --help`

Expected: help leads with `moedex`; missing-binary recovery refers only to `zak-keown/moedex` channels.

- [ ] **Step 7: Commit the public identity**

```bash
git add codex-rs/cli codex-rs/tui codex-cli README.md
git commit -m "feat: expose the Moedex command and interface"
```

### Task 4: Preview-first Codex home import (B5, B6)

**Files:**
- Create: `codex-rs/external-agent-migration/src/source_codex.rs`
- Create: `codex-rs/external-agent-migration/src/source/codex.rs`
- Create: `codex-rs/external-agent-migration/src/source_codex_tests.rs`
- Create: `codex-rs/external-agent-migration/src/sessions/records_codex.rs`
- Create: `codex-rs/external-agent-migration/src/sessions/records_codex_tests.rs`
- Modify: `codex-rs/external-agent-migration/src/lib.rs`
- Modify: `codex-rs/external-agent-migration/src/model.rs`
- Modify: `codex-rs/external-agent-migration/src/service.rs`
- Modify: `codex-rs/external-agent-migration/src/config_values.rs`
- Modify: `codex-rs/external-agent-migration/src/sessions/mod.rs`
- Modify: `codex-rs/cli/src/lib.rs`
- Create: `codex-rs/cli/src/import_codex.rs`
- Create: `codex-rs/cli/src/import_codex_tests.rs`

**Interfaces:**
- Consumes: Existing typed config migration, session append/ledger logic, thread-store APIs, `ResolvedProductHome`, and CLI command dispatch.
- Produces: `CodexImportSelection { settings: bool, sessions: bool, credentials: bool, conflict_policy: ConflictPolicy }`, `preview_codex_import(source, destination, selection) -> Result<ImportPreview>`, and `apply_codex_import(preview_id, selection) -> Result<ImportReport>`.

- [ ] **Step 1: Write failing import safety tests**

```rust
#[tokio::test]
async fn preview_and_cancel_leave_both_homes_byte_identical() {
    let before = hash_tree(source.path()).await;
    let destination_before = hash_tree(destination.path()).await;
    let preview = preview_codex_import(source.abs(), destination.abs(), settings_and_sessions()).await.unwrap();
    assert!(!preview.items.is_empty());
    drop(preview);
    assert_eq!(before, hash_tree(source.path()).await);
    assert_eq!(destination_before, hash_tree(destination.path()).await);
}
```

Cover canonical source-equals-destination, symlink equality, conflict skip, explicit replacement backup, interrupted apply and idempotent rerun, typed config with home-bound path review, session conflict, preserved trust state, and secret-free rendering.

- [ ] **Step 2: Run migration and CLI tests to verify failure**

Run: `cd codex-rs && just test -p codex-external-agent-migration -p codex-cli`

Expected: FAIL because Codex is not an import source and the command is absent.

- [ ] **Step 3: Add the Codex source and immutable preview plan**

```rust
#[derive(Clone, Debug, Eq, PartialEq, Serialize)]
pub struct CodexImportSelection {
    pub settings: bool,
    pub sessions: bool,
    pub credentials: bool,
    pub conflict_policy: ConflictPolicy,
}

pub enum ConflictPolicy { Skip, ReplaceWithBackup }
```

Canonicalize and compare source/destination before scanning. Parse config through existing typed loaders, mark absolute/home-bound paths and hooks for review, and read sessions through supported rollout/thread-store snapshots rather than copying live SQLite database files.

- [ ] **Step 4: Implement transactional apply and the explicit CLI**

Expose `moedex import codex --dry-run --settings --sessions`; require `--credentials` separately and default it off. Copy to temporary destination siblings, fsync where supported, rename atomically, and write an import ledger only after each item commits. On replacement, create a destination backup first and retain source hashes in the report.

- [ ] **Step 5: Verify import behavior**

Run: `cd codex-rs && just test -p codex-external-agent-migration -p codex-cli && just fix -p codex-external-agent-migration && just fix -p codex-cli && just fmt`

Expected: preview/cancel have zero writes, session resume succeeds, source hashes are unchanged, and imported trust is not elevated.

- [ ] **Step 6: Commit import planning and non-secret data migration**

```bash
git add codex-rs/external-agent-migration codex-rs/cli
git commit -m "feat: add explicit Codex home import"
```

### Task 5: Credential import through storage adapters (B7)

**Files:**
- Create: `codex-rs/login/src/auth/import.rs`
- Create: `codex-rs/login/src/auth/import_tests.rs`
- Modify: `codex-rs/login/src/auth/mod.rs`
- Modify: `codex-rs/login/src/auth/storage.rs`
- Modify: `codex-rs/external-agent-migration/src/source/codex.rs`
- Modify: `codex-rs/cli/src/import_codex.rs`

**Interfaces:**
- Consumes: `CodexImportSelection`, legacy/current auth storage adapters, and the `Moedex Auth` namespace from Task 2.
- Produces: `AuthImportOutcome::{Imported, SignInRequired, Skipped, Failed}` and `import_auth_record(source: &dyn AuthStorage, destination: &dyn AuthStorage) -> Result<AuthImportOutcome>`.

- [ ] **Step 1: Add failing backend and redaction tests**

```rust
#[tokio::test]
async fn importing_auth_writes_only_the_moedex_backend() {
    let outcome = import_auth_record(&stock, &moedex).await.unwrap();
    assert_eq!(outcome, AuthImportOutcome::Imported);
    assert_eq!(stock.read().await.unwrap(), original_stock_auth);
    assert_eq!(moedex.read().await.unwrap(), original_stock_auth);
    assert!(!format!("{outcome:?}").contains("access_token"));
}
```

Cover file, keyring, encrypted-store failure, refresh-token invalidation, restrictive file permissions/ACLs, logout isolation, and absence of tokens in preview, process args, logs, and temp directories.

- [ ] **Step 2: Run login and migration tests and confirm failure**

Run: `cd codex-rs && just test -p codex-login -p codex-external-agent-migration -p codex-cli`

Expected: FAIL because auth import adapters do not exist.

- [ ] **Step 3: Implement adapter-to-adapter migration**

```rust
pub async fn import_auth_record(
    source: &dyn AuthStorage,
    destination: &dyn AuthStorage,
) -> Result<AuthImportOutcome> {
    let Some(record) = source.read().await? else { return Ok(AuthImportOutcome::Skipped); };
    validate_importable_auth(&record)?;
    destination.write(&record).await?;
    Ok(AuthImportOutcome::Imported)
}
```

Keep the record in memory, never place plaintext in a staging file, and render only category/count/status. Map unsafe backend migration and refresh invalidation to `SignInRequired`.

- [ ] **Step 4: Verify and format**

Run: `cd codex-rs && just test -p codex-login -p codex-external-agent-migration -p codex-cli && just fix -p codex-login && just fix -p codex-external-agent-migration && just fix -p codex-cli && just fmt`

Expected: all backend, redaction, and logout-isolation fixtures pass.

- [ ] **Step 5: Commit credential import**

```bash
git add codex-rs/login codex-rs/external-agent-migration codex-rs/cli
git commit -m "feat: import Codex credentials safely"
```

### Task 6: Fork-owned updates and artifact packaging (B8, B9, B10)

**Files:**
- Modify: `codex-rs/tui/src/updates.rs`
- Modify: `codex-rs/tui/src/update_action.rs`
- Modify: `codex-rs/tui/src/history_cell/tests.rs`
- Modify: `codex-cli/bin/codex.js`
- Modify: `codex-cli/scripts/build_npm_package.py`
- Modify: `scripts/build_codex_package.py`
- Modify: `scripts/codex_package/layout.py`
- Modify: `scripts/codex_package/test_layout.py`
- Modify: `scripts/codex_package/smoke_tests/test_codex_package.py`
- Modify: `scripts/install/install.sh`
- Modify: `scripts/install/install.ps1`
- Modify: `scripts/install/test_install_sh.py`
- Modify: `.github/workflows/rust-release.yml`
- Modify: `.github/workflows/rust-release-windows.yml`
- Modify: `.github/workflows/rust-release-prepare.yml`

**Interfaces:**
- Consumes: `PRODUCT_IDENTITY`, `BuildProvenance`, existing `InstallContext`, and current package-layout helper resolution.
- Produces: `UpdateChannel::GitHubReleases { repository: "zak-keown/moedex" }`, fork-only `UpdateAction`s, checksummed artifact manifest, and packages whose helpers resolve from the same release layout.

- [ ] **Step 1: Add failing fork-channel and conflicting-PATH smoke tests**

```rust
#[test]
fn every_enabled_update_action_targets_moedex() {
    for action in UpdateAction::supported_for_tests() {
        assert!(!action.command_str().contains("@openai/codex"));
        assert!(!action.command_str().contains("brew upgrade --cask codex"));
    }
}
```

In the Python smoke fixture, place a fake stock `codex` and helper binaries first on `PATH`, launch the staged `moedex`, and assert all selected helper paths are inside the staged release directory. Remove one required helper and assert qualification fails.

- [ ] **Step 2: Run existing packaging tests and observe old channels**

Run: `python -m pytest scripts/codex_package scripts/install/test_install_sh.py && cd codex-rs && just test -p codex-tui`

Expected: FAIL because release URLs and update commands target OpenAI.

- [ ] **Step 3: Implement GitHub-first update selection**

```rust
const LATEST_RELEASE_URL: &str =
    "https://api.github.com/repos/zak-keown/moedex/releases/latest";

pub enum UpdateAction {
    GitHubReleaseUnix,
    GitHubReleaseWindows,
    Disabled { release_url: &'static str },
}
```

Do not synthesize npm, Homebrew, or OpenAI fallback actions. An unknown/unconfigured install context returns the disabled action and the fork release URL.

- [ ] **Step 4: Stage complete same-release artifacts**

Package `moedex`, app-server, code-mode host, proxy, OS sandbox helpers, and shipped voice resources with a version/provenance/checksum manifest. Resolve private `codex-*` helper names relative to the package manifest, never bare `PATH`.

- [ ] **Step 5: Disable inherited upstream publication hooks**

Change release workflows to build/dry-run only unless fork-owned credentials and repository targets are explicitly configured. Remove OpenAI npm publication, website deployment, signing assumptions, and cross-repository update jobs from reachable default jobs.

- [ ] **Step 6: Verify packaging and snapshots**

Run: `python -m pytest scripts/codex_package scripts/install/test_install_sh.py && cd codex-rs && just test -p codex-tui && cargo insta pending-snapshots -p codex-tui`

Review and accept intended update snapshots, then run: `cd codex-rs && cargo insta accept -p codex-tui && just fix -p codex-tui && just fmt`

Expected: all package tests pass, missing helpers fail qualification, and no action targets an upstream distribution channel.

- [ ] **Step 7: Commit release isolation**

```bash
git add codex-rs/tui codex-cli scripts .github/workflows
git commit -m "build: package Moedex from fork-owned channels"
```

### Task 7: Versioned behavior manifest and Stage A qualification (B5, B13, U5)

**Files:**
- Create: `moedex-behavior-manifest.json`
- Create: `scripts/moedex_behavior_manifest.py`
- Create: `scripts/test_moedex_behavior_manifest.py`
- Create: `.github/workflows/moedex-qualification.yml`
- Modify: `README.md`

**Interfaces:**
- Consumes: Focused test targets and artifact manifest produced by Tasks 1–6.
- Produces: Schema-versioned `moedex-behavior-manifest.json` entries with `requirement`, `status`, `upstreamBase`, `tests`, `artifacts`, and `exclusions`; `scripts/moedex_behavior_manifest.py verify --artifact-dir <path>`.

- [ ] **Step 1: Write the failing manifest verifier tests**

```python
def test_missing_required_helper_fails(tmp_path):
    manifest = fixture_manifest(required_artifacts=["bin/moedex", "bin/codex-app-server"])
    (tmp_path / "bin").mkdir()
    (tmp_path / "bin/moedex").touch()
    result = verify(manifest, tmp_path)
    assert result.errors == ["missing artifact: bin/codex-app-server"]

def test_every_stage_a_requirement_has_a_gate():
    result = verify_manifest(Path("moedex-behavior-manifest.json"))
    assert result.unmapped_requirements == []
```

- [ ] **Step 2: Run tests and confirm the verifier is absent**

Run: `python -m pytest scripts/test_moedex_behavior_manifest.py`

Expected: FAIL because the manifest and verifier do not exist.

- [ ] **Step 3: Implement strict manifest validation**

```json
{
  "schemaVersion": 1,
  "product": "Moedex",
  "upstreamBase": "8e2afc09126c0cea4c282725fe68af43adad73d7",
  "requirements": [
    {"id":"B1","status":"preserved","tests":["codex-cli::clap_command_is_moedex"],"artifacts":["bin/moedex"],"exclusions":[]}
  ]
}
```

Require all B1–B13, U4, and U5 entries, known statuses (`preserved`, `superseded`, `intentional-change`, `broken`), at least one executable gate for preserved/superseded behavior, explicit exclusions, and exact artifact checks.

- [ ] **Step 4: Add fork-upgrade and packaged qualification jobs**

Make CI run manifest validation, CLI/helper smoke tests, import safety tests, home/auth coexistence tests, protocol/schema compatibility, and a source inventory that rejects newly introduced upstream update targets. Stamp the tested upstream base and artifact checksums into retained output.

- [ ] **Step 5: Document install, compatibility, import, update, and uninstall behavior**

Update top-level `README.md` with supported OS/architectures, local/GitHub artifact installation, home precedence and sharing hazard, `moedex import codex --dry-run`, update channel, provider attribution, and uninstall language that explicitly preserves `~/.moedex` unless separately deleted.

- [ ] **Step 6: Run final Stage A qualification**

Run: `python -m pytest scripts/test_moedex_behavior_manifest.py scripts/codex_package scripts/install/test_install_sh.py && python scripts/moedex_behavior_manifest.py verify --artifact-dir dist-fixture && cd codex-rs && just test -p codex-product-identity -p codex-utils-home-dir -p codex-build-info -p codex-login -p codex-app-server-daemon -p codex-diagnostics -p codex-external-agent-migration -p codex-cli -p codex-tui && just fmt`

Expected: every Stage A requirement maps to a passing test/artifact or deliberate exclusion; packaged command/help/import/update smoke paths pass.

- [ ] **Step 7: Commit the qualification contract**

```bash
git add moedex-behavior-manifest.json scripts/moedex_behavior_manifest.py scripts/test_moedex_behavior_manifest.py .github/workflows/moedex-qualification.yml README.md
git commit -m "test: qualify Moedex brand behavior"
```
