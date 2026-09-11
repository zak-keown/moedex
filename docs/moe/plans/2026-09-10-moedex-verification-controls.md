# Moedex Verification Controls Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver Stage C snapshot-bound review, bounded repair, evidence, usage, safe editing, subagent-history, repetition, and workflow-policy controls for requirements R6, O1, O2, U3, C10, C12, C13, and C14.

**Architecture:** Add focused `codex-workspace-snapshot` and `codex-verification` crates for immutable content identity and verification-domain logic, while durable records use narrow modules in the existing state crate. Core integrates these services at current review, tool, usage, hook, and spawn boundaries; app-server v2 exposes inspectable controls and status without changing v1 or rollout formats.

**Tech Stack:** Rust 2024, Tokio, serde, SHA-256, SQLx/SQLite, existing Codex core/app-server/TUI integration helpers, ts-rs/schemars, insta, Bazel.

**Spec:** `specs/moedex/SPEC.md`, `specs/moedex/EXECUTION.md`, `specs/moedex/EXPERIENCE.md`, and `specs/moedex/RESEARCH.md`

## Global Constraints

- Preserve Linux, macOS, Windows, and supported cross-OS app-server/exec-server behavior.
- Do not modify `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR` code.
- New model-visible fragments must be typed `ContextualUserFragment` implementations, default to at most 1,000 tokens each, stay under 4,000 newly injected tokens per turn across Moedex enhancements, and never exceed 10,000 tokens.
- Do not rewrite prior model-visible history; append bounded deltas and retrieve details on demand.
- Keep storage and domain logic outside `codex-core`; keep new Rust modules below 500 LoC where practical and do not grow `chatwidget.rs` for these features.
- Every queue/store has explicit count, byte, and retention bounds; reject admission before acknowledgement rather than evicting accepted user input.
- New app-server methods are v2, camelCase on the wire, cursor-paginated for lists, `#[ts(optional = nullable)]` on optional request fields, and experimental-gated; run `just write-app-server-schema` after API changes.
- Use existing authorization, permission, sandbox, hook, and tool-event paths; workflow policy cannot grant permissions or become a security boundary.
- Use `just test`, never direct `cargo test`; run `just fmt` after source changes, and run scoped `just fix -p <project>` before finalizing without rerunning tests afterward.
- Any Cargo dependency change requires `just bazel-lock-update` from `codex-rs` and the resulting `MODULE.bazel.lock` in the same commit.

## Open Decisions

None. The specification fixes initial bounds, default modes, and delivery ordering; price-table contents are data supplied through the versioned price-source interface rather than a release-blocking product choice.

## Not Yet Specified

None for this stage. Live-provider price qualification and default editing-tool selection remain later release experiments and do not block shipping these controls disabled or explicitly selected.

## Out of Scope

- Alternate provider adapters (C11), selective research context (C1-C9), job wakeups (C15), and coordination/boards (Stage D); they consume these interfaces in later plans.
- Fuzzy relocation for structured edits; the first slice rejects stale or ambiguous content.
- Provider invoice guarantees; monetary enforcement is conservative admission with explicit in-flight and unpriced usage.
- General product documentation under `docs/`; this plan changes code, generated schemas, tests, and top-level behavior manifests only.

---

### Task 1: Immutable Workspace Snapshot Crate

**Files:**
- Create: `codex-rs/workspace-snapshot/Cargo.toml`
- Create: `codex-rs/workspace-snapshot/BUILD.bazel`
- Create: `codex-rs/workspace-snapshot/src/lib.rs`
- Create: `codex-rs/workspace-snapshot/src/capture.rs`
- Create: `codex-rs/workspace-snapshot/src/capture_tests.rs`
- Create: `codex-rs/workspace-snapshot/src/types.rs`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/MODULE.bazel.lock`

**Interfaces:**
- Consumes: existing `codex_git_utils` repository discovery and `AbsolutePathBuf` path validation.
- Produces: `SnapshotId`, `ContentId`, `WorkspaceSnapshot`, `SnapshotEntry`, `SnapshotExclusion`, `CaptureRequest`, `CaptureError`, and `capture_workspace(request: CaptureRequest) -> impl Future<Output = Result<WorkspaceSnapshot, CaptureError>> + Send`.

- [ ] **Step 1: Write capture tests for dirty, untracked, excluded, and unstable workspaces**

```rust
#[tokio::test]
async fn capture_identifies_selected_dirty_and_untracked_content() {
    let repo = TestRepo::new();
    repo.commit("tracked.rs", "fn old() {}\n");
    repo.write("tracked.rs", "fn new() {}\n");
    repo.write("selected.rs", "pub const N: u8 = 1;\n");
    repo.write("ignored.log", "secret\n");

    let snapshot = capture_workspace(CaptureRequest {
        root: repo.root(),
        comparison_base: ComparisonBase::Head,
        scope: CaptureScope::Paths(vec!["tracked.rs".into(), "selected.rs".into()]),
        max_entries: 10_000,
        max_bytes: 2 * 1024 * 1024 * 1024,
    })
    .await
    .unwrap();

    assert_eq!(snapshot.entries.len(), 2);
    assert!(snapshot.entries.iter().any(|entry| entry.path == "selected.rs"));
    assert!(snapshot.exclusions.iter().all(|item| item.path != "selected.rs"));
    assert_eq!(snapshot.id, snapshot.recompute_id().unwrap());
}

#[tokio::test]
async fn capture_rejects_a_tree_that_changes_during_capture() {
    let repo = MutatingTestRepo::change_after_first_read("src/lib.rs");
    assert_eq!(
        capture_workspace(repo.request()).await.unwrap_err(),
        CaptureError::UnstableCapture { paths: vec!["src/lib.rs".into()] }
    );
}
```

- [ ] **Step 2: Run the crate test and verify the new crate is absent**

Run: `cd codex-rs && just test -p codex-workspace-snapshot`

Expected: FAIL because package `codex-workspace-snapshot` does not exist.

- [ ] **Step 3: Implement canonical content identities and two-pass capture validation**

```rust
pub struct CaptureRequest {
    pub root: AbsolutePathBuf,
    pub comparison_base: ComparisonBase,
    pub scope: CaptureScope,
    pub max_entries: usize,
    pub max_bytes: u64,
}

pub async fn capture_workspace(
    request: CaptureRequest,
) -> Result<WorkspaceSnapshot, CaptureError> {
    let manifest = enumerate_scope(&request).await?;
    let entries = hash_entries(&request.root, &manifest).await?;
    let verified = hash_entries(&request.root, &manifest).await?;
    if entries != verified {
        return Err(CaptureError::UnstableCapture {
            paths: changed_paths(&entries, &verified),
        });
    }
    WorkspaceSnapshot::from_parts(request, entries)
}
```

Canonical serialization includes schema version, normalized slash-separated relative path, file kind, executable bit, byte length, SHA-256 content identity, comparison base, and sorted exclusions. Reject symlink escapes, count/byte overflow, unreadable selected files, and non-UTF-8 path ambiguity explicitly; hash file bytes without normalizing line endings.

- [ ] **Step 4: Register Cargo/Bazel metadata and refresh the Bazel lock**

```toml
[package]
name = "codex-workspace-snapshot"
version.workspace = true
edition.workspace = true
license.workspace = true

[dependencies]
codex-utils-absolute-path = { workspace = true }
serde = { workspace = true, features = ["derive"] }
sha2 = { workspace = true }
thiserror = { workspace = true }
tokio = { workspace = true, features = ["fs"] }
```

Run: `cd codex-rs && just bazel-lock-update`

Expected: `MODULE.bazel.lock` records the new workspace crate/dependency graph.

- [ ] **Step 5: Run tests and format**

Run: `cd codex-rs && just test -p codex-workspace-snapshot`

Expected: PASS, including unstable capture and exact dirty/untracked identity cases.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/Cargo.toml codex-rs/MODULE.bazel.lock codex-rs/workspace-snapshot
git commit -m "feat(verification): add immutable workspace snapshots"
```

### Task 2: Verification Domain and Durable Evidence Ledger

**Files:**
- Create: `codex-rs/verification/Cargo.toml`
- Create: `codex-rs/verification/BUILD.bazel`
- Create: `codex-rs/verification/src/lib.rs`
- Create: `codex-rs/verification/src/evidence.rs`
- Create: `codex-rs/verification/src/progress.rs`
- Create: `codex-rs/verification/src/progress_tests.rs`
- Create: `codex-rs/state/migrations/0056_verification_evidence.sql`
- Create: `codex-rs/state/src/model/verification.rs`
- Create: `codex-rs/state/src/runtime/verification.rs`
- Create: `codex-rs/state/src/runtime/verification_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Modify: `codex-rs/state/Cargo.toml`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/MODULE.bazel.lock`

**Interfaces:**
- Consumes: Task 1 `SnapshotId` and `ContentId`.
- Produces: `EvidenceId`, `EvidenceStatus::{Passed,Failed,Skipped,Interrupted,Unavailable}`, `EvidenceRecord`, `Applicability::{Current,Stale}`, `ActionObservation`, `ProgressDetector::observe(ActionObservation) -> ProgressAssessment`, and `StateRuntime::{record_evidence,list_evidence,evidence_applicability}`.

- [ ] **Step 1: Write state and progress tests**

```rust
#[tokio::test]
async fn duplicate_evidence_is_idempotent_and_changed_content_is_stale() {
    let db = test_runtime().await;
    let record = evidence_fixture("ev-1", "snap-a", EvidenceStatus::Passed);
    db.record_evidence(&record).await.unwrap();
    db.record_evidence(&record).await.unwrap();
    assert_eq!(db.list_evidence("claim-1", None, 100).await.unwrap().data, vec![record]);
    assert_eq!(
        db.evidence_applicability("ev-1", SnapshotId::new("snap-b")).await.unwrap(),
        Applicability::Stale
    );
}

#[test]
fn unchanged_failed_actions_pause_but_declared_waits_and_changed_artifacts_do_not() {
    let mut detector = ProgressDetector::new(ProgressPolicy::default());
    for sequence in 1..=3 {
        detector.observe(failed_test(sequence, ContentId::new("same")));
    }
    assert!(matches!(detector.assess(), ProgressAssessment::Pause { .. }));
    detector.observe(declared_wait(4));
    detector.observe(failed_test(5, ContentId::new("changed")));
    assert_eq!(detector.assess(), ProgressAssessment::Continue);
}
```

- [ ] **Step 2: Run focused tests and verify failure**

Run: `cd codex-rs && just test -p codex-verification`

Expected: FAIL because the crate and interfaces are not defined.

- [ ] **Step 3: Implement evidence types and bounded action normalization**

```rust
pub struct ProgressDetector {
    policy: ProgressPolicy,
    window: VecDeque<ActionObservation>,
}

impl ProgressDetector {
    pub fn observe(&mut self, observation: ActionObservation) -> ProgressAssessment {
        self.window.push_back(observation);
        self.window.truncate_front(self.policy.window_size.saturating_sub(1));
        assess_equivalent_failures(&self.window, self.policy.failure_threshold)
    }
}
```

Normalize tool identity plus structured arguments with insignificant whitespace removed, outcome class, and relevant `ContentId`; retain at most 12 observations and bounded previews. Mark polling/subscription actions as declared waits and reset an unsuccessful sequence only for relevant content/state changes.

- [ ] **Step 4: Add the transactional evidence schema and state API**

```sql
CREATE TABLE verification_evidence (
    evidence_id TEXT PRIMARY KEY,
    claim_id TEXT NOT NULL,
    snapshot_id TEXT NOT NULL,
    status TEXT NOT NULL,
    executor TEXT NOT NULL,
    environment_json TEXT NOT NULL,
    started_at INTEGER NOT NULL,
    ended_at INTEGER NOT NULL,
    artifact_refs_json TEXT NOT NULL,
    applicability_scope_json TEXT NOT NULL,
    record_json TEXT NOT NULL
);
CREATE INDEX verification_evidence_claim_idx
    ON verification_evidence(claim_id, ended_at DESC, evidence_id DESC);
```

Use `INSERT INTO verification_evidence (evidence_id, claim_id, snapshot_id, status, executor, environment_json, started_at, ended_at, artifact_refs_json, applicability_scope_json, record_json) VALUES (?, ?, ?, ?, ?, ?, ?, ?, ?, ?, ?) ON CONFLICT(evidence_id) DO NOTHING`, then compare the stored canonical record and reject a mismatched replay. List by `(ended_at,evidence_id)` cursor and cap pages at 100.

- [ ] **Step 5: Register the crates, migration compile data, and Bazel lock**

Run: `cd codex-rs && just bazel-lock-update`

Expected: Cargo and Bazel resolve `codex-verification`, `codex-state` includes migration `0056`, and no lock drift remains.

- [ ] **Step 6: Run tests and format**

Run: `cd codex-rs && just test -p codex-verification && just test -p codex-state`

Expected: PASS with idempotent replay, stale applicability, pagination, status fidelity, and repetition exceptions.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/Cargo.toml codex-rs/MODULE.bazel.lock codex-rs/verification codex-rs/state
git commit -m "feat(verification): persist evidence and detect non-progress"
```

### Task 3: Snapshot-Bound Review and Bounded Repair Orchestrator

**Files:**
- Create: `codex-rs/core/src/session/review_cycle.rs`
- Create: `codex-rs/core/src/session/review_cycle_tests.rs`
- Create: `codex-rs/core/tests/suite/review_cycle.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Modify: `codex-rs/core/src/session/mod.rs`
- Modify: `codex-rs/core/src/session/review.rs`
- Modify: `codex-rs/core/src/session/handlers.rs`
- Modify: `codex-rs/core/src/review_prompts.rs`
- Modify: `codex-rs/core/Cargo.toml`
- Modify: `codex-rs/core/BUILD.bazel`
- Modify: `codex-rs/protocol/src/protocol.rs`

**Interfaces:**
- Consumes: Task 1 `capture_workspace`/`WorkspaceSnapshot`; Task 2 `EvidenceRecord`, `ProgressDetector`, and state evidence API.
- Produces: `ReviewCycleId`, `ReviewCyclePolicy { max_iterations: 3, max_elapsed: 30m, unchanged_finding_limit: 2 }`, `ReviewCycleState`, `ReviewCycleTermination`, `Op::StartReviewCycle`, and review events containing snapshot/currentness/remaining-findings fields.

- [ ] **Step 1: Add an integration test for external edits and unchanged findings**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn review_cycle_keeps_original_findings_stale_and_stops_repeated_repairs() -> anyhow::Result<()> {
    let test = test_codex().build_with_auto_env().await?;
    test.write_file("src/lib.rs", "pub fn value() -> u8 { 1 }\n")?;
    let first = mount_review_findings(&test.server, "finding-a").await;
    let repeated = mount_review_findings(&test.server, "finding-a").await;
    test.codex.submit(Op::StartReviewCycle(review_request())).await?;
    first.wait_until_satisfied().await;
    test.write_file("src/lib.rs", "pub fn value() -> u8 { 2 }\n")?;
    repeated.wait_until_satisfied().await;

    let terminal = wait_for_event(&test.codex, |event| match event {
        EventMsg::ReviewCycleCompleted(event) => Some(event.clone()),
        _ => None,
    }).await;
    assert_eq!(terminal.termination, ReviewCycleTermination::StaleInput);
    assert_eq!(terminal.iterations, 1);
    assert!(!terminal.remaining_findings.is_empty());
    Ok(())
}
```

- [ ] **Step 2: Run the integration test and verify failure**

Run: `cd codex-rs && just test -p codex-core review_cycle_keeps_original_findings_stale_and_stops_repeated_repairs`

Expected: FAIL because `StartReviewCycle` and snapshot-aware review events do not exist.

- [ ] **Step 3: Implement one serialized cycle owner per workspace/scope**

```rust
pub(crate) async fn run_review_cycle(
    session: Arc<Session>,
    turn: Arc<TurnContext>,
    request: ReviewCycleRequest,
) -> ReviewCycleCompletedEvent {
    let lease = session.services.review_cycles.acquire(request.scope_key()).await?;
    let mut cycle = ReviewCycleState::capture(request, &turn).await?;
    while cycle.policy.allows_next(&cycle) {
        let findings = run_review_for_snapshot(&session, &turn, &cycle.snapshot).await?;
        if findings.status.is_inconclusive() || !cycle.required_checks_passed() {
            break;
        }
        cycle = repair_and_recapture(&session, &turn, cycle, findings).await?;
    }
    drop(lease);
    cycle.into_completed_event()
}
```

Before each repair and before presenting findings as current, recapture and compare actual content identity. Never overwrite external edits; terminate `StaleInput`. Require explicit reviewer status plus required check evidence, and preserve failed/skipped/interrupted checks and unresolved findings. Stop for 3 iterations, 30 elapsed minutes, 2 unchanged finding fingerprints, R6 pause, cancellation, authorization denial, or run budget exhaustion.

- [ ] **Step 4: Attach evidence to review/check events and bounded model context**

```rust
pub struct ReviewEvidenceFragment {
    pub cycle_id: ReviewCycleId,
    pub snapshot_id: SnapshotId,
    pub status: ReviewStatus,
    pub finding_count: usize,
    pub evidence_ids: Vec<EvidenceId>,
    pub truncated: bool,
}

impl ContextualUserFragment for ReviewEvidenceFragment {
    fn context_kind(&self) -> ContextKind { ContextKind::ReviewEvidence }
}
```

Serialize only IDs, status, counts, and bounded finding summaries; retain full findings as artifacts/state records. Replayed completion events use the cycle/event identity and cannot start another repair.

- [ ] **Step 5: Run core tests and format**

Run: `cd codex-rs && just test -p codex-core review_cycle`

Expected: PASS for external mutation, competing entry points, selected untracked content, exclusions, failed checks, unchanged findings, cancellation, and replay.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/core codex-rs/protocol
git commit -m "feat(review): bind bounded repair cycles to snapshots"
```

### Task 4: App-Server v2 Verification API and Completion Reports

**Files:**
- Create: `codex-rs/app-server-protocol/src/protocol/v2/verification.rs`
- Create: `codex-rs/app-server/src/request_processors/verification_processor.rs`
- Create: `codex-rs/app-server/tests/suite/v2/verification.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`
- Modify: `codex-rs/app-server/src/message_processor.rs`
- Modify: `codex-rs/app-server/src/request_processors/mod.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/schema/typescript/v2/`
- Modify: `codex-rs/app-server-protocol/schema/json/`

**Interfaces:**
- Consumes: Task 2 evidence store and Task 3 review-cycle operations/events.
- Produces: experimental v2 methods `verification/start`, `verification/read`, and `verification/evidence/list`; `VerificationReport` separates `verified`, `failed`, `stale`, and `unverified` claims.

- [ ] **Step 1: Write public JSON-RPC tests for report truthfulness**

```rust
#[tokio::test]
async fn verification_report_separates_stale_failed_and_unverified_claims() {
    let server = TestAppServer::builder().build().await;
    seed_evidence(&server, vec![passed("a", "snap-1"), failed("b", "snap-2")]).await;
    mutate_snapshot(&server, "snap-1").await;
    let report: VerificationReadResponse = server
        .send_request("verification/read", json!({ "verificationId": "verify-1" }))
        .await;
    assert_eq!(report.report.stale_claims, vec![claim("a")]);
    assert_eq!(report.report.failed_claims, vec![claim("b")]);
    assert_eq!(report.report.unverified_claims, vec![claim("c")]);
    assert!(report.report.verified_claims.is_empty());
}
```

- [ ] **Step 2: Run the app-server test and verify failure**

Run: `cd codex-rs && just test -p codex-app-server verification_report_separates_stale_failed_and_unverified_claims`

Expected: FAIL with unknown `verification/read` method/types.

- [ ] **Step 3: Define camelCase v2 request/response types and processor routing**

```rust
#[derive(Serialize, Deserialize, JsonSchema, TS, ExperimentalApi)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct VerificationEvidenceListParams {
    pub verification_id: String,
    #[ts(optional = nullable)]
    pub cursor: Option<String>,
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct VerificationEvidenceListResponse {
    pub data: Vec<EvidenceRecord>,
    pub next_cursor: Option<String>,
}
```

`verification/start` accepts thread ID, review target, required check names, and optional repair policy; validate limits before submitting the core op. `verification/read` recomputes applicability against current snapshot rather than trusting a stored current flag. Missing artifact handles remain visible as unavailable.

- [ ] **Step 4: Regenerate and inspect stable/experimental schemas**

Run: `cd codex-rs && just write-app-server-schema && just write-app-server-schema --experimental`

Expected: generated TS/JSON contains only v2 experimental methods, camelCase fields, nullable optional request fields, and paginated list shapes.

- [ ] **Step 5: Run protocol and app-server tests, then format**

Run: `cd codex-rs && just test -p codex-app-server-protocol && just test -p codex-app-server verification`

Expected: PASS, including stale evidence, lost artifacts, skipped checks, pagination, and duplicate delivery.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/app-server-protocol codex-rs/app-server
git commit -m "feat(app-server): expose verification reports in v2"
```

### Task 5: Stale-Safe Structured Editing Tool

**Files:**
- Create: `codex-rs/structured-edit/Cargo.toml`
- Create: `codex-rs/structured-edit/BUILD.bazel`
- Create: `codex-rs/structured-edit/src/lib.rs`
- Create: `codex-rs/structured-edit/src/edit.rs`
- Create: `codex-rs/structured-edit/src/edit_tests.rs`
- Create: `codex-rs/core/src/tools/handlers/structured_edit.rs`
- Create: `codex-rs/core/src/tools/handlers/structured_edit_tests.rs`
- Modify: `codex-rs/core/src/tools/handlers/mod.rs`
- Modify: `codex-rs/core/src/tools/registry.rs`
- Modify: `codex-rs/core/src/tools/spec.rs`
- Modify: `codex-rs/core/Cargo.toml`
- Modify: `codex-rs/core/BUILD.bazel`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/MODULE.bazel.lock`

**Interfaces:**
- Consumes: Task 1 `ContentId`; existing permission/sandbox, hook, `TurnDiffTracker`, and file-change event paths used by `apply_patch`.
- Produces: optional tool `structured_edit`; `ReadAnchor { path, content_id, start_byte, end_byte }`; `StructuredEditRequest`; `apply_structured_edits(request, fs) -> Result<StructuredEditOutcome, StructuredEditError>`.

- [ ] **Step 1: Write transactional edit tests**

```rust
#[test]
fn stale_multi_edit_returns_fresh_context_and_changes_nothing() {
    let fs = TestFs::with_files([
        ("a.rs", "same\nsame\n"),
        ("b.rs", "old\n"),
    ]);
    let request = request(vec![
        replace("a.rs", content_id("same\nsame\n"), 0..5, "new\n"),
        replace("b.rs", content_id("older\n"), 0..4, "new\n"),
    ]);
    let error = apply_structured_edits(request, &fs).unwrap_err();
    assert_eq!(error.kind, StructuredEditErrorKind::StaleContent);
    assert_eq!(fs.read("a.rs"), "same\nsame\n");
    assert_eq!(fs.read("b.rs"), "old\n");
    assert!(error.fresh_context.unwrap().byte_len <= 4096);
}
```

- [ ] **Step 2: Run the crate test and verify failure**

Run: `cd codex-rs && just test -p codex-structured-edit`

Expected: FAIL because the structured-edit crate does not exist.

- [ ] **Step 3: Implement exact byte-range edits with all-or-nothing validation**

```rust
pub fn apply_structured_edits(
    request: StructuredEditRequest,
    fs: &dyn EditFileSystem,
) -> Result<StructuredEditOutcome, StructuredEditError> {
    let validated = request.edits.iter().map(|edit| validate_anchor(edit, fs)).collect::<Result<Vec<_>, _>>()?;
    reject_overlapping_ranges(&validated)?;
    fs.atomic_replace_many(render_replacements(validated)?)?;
    Ok(StructuredEditOutcome::from_validated(request.transaction_id, validated))
}
```

Compute `ContentId` from exact bytes, require UTF-8 boundaries for textual replacements, preserve detected CRLF/LF outside edited ranges, reject repeated-line ambiguity through exact offsets, and return at most 4 KiB of nearby current context. Validate every target before writing any target; fuzzy matching is absent.

- [ ] **Step 4: Register the optional model-selected tool through existing tool plumbing**

```rust
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct StructuredEditArgs {
    transaction_id: String,
    edits: Vec<StructuredEditInput>,
}
```

Expose no more than 32 edits/call. Route pre/post hooks, permission checking, sandbox validation, diff tracking, and `PatchApplyBegin`/`PatchApplyEnd`-equivalent ordinary file-change events through the same abstractions as `apply_patch`; do not bypass them with direct `std::fs` writes in core.

- [ ] **Step 5: Add benchmark fixture for model/tool selection evidence**

```rust
#[derive(Serialize)]
struct EditBenchmarkResult {
    fixture: String,
    tool: String,
    correct: bool,
    retries: u32,
    input_tokens: u64,
    output_tokens: u64,
}
```

Create a fixed fixture runner under `codex-rs/structured-edit/tests/benchmark.rs` that compares `structured_edit` and existing patch application on repeated lines, Unicode, CRLF, and stale reads. Keep the tool opt-in; benchmark output is retained JSON evidence and does not automatically change defaults.

- [ ] **Step 6: Refresh Bazel metadata, test, and format**

Run: `cd codex-rs && just bazel-lock-update`

Run: `cd codex-rs && just test -p codex-structured-edit && just test -p codex-core structured_edit`

Expected: PASS for stale external writes, repeated lines, overlapping ranges, newline variants, Unicode boundaries, transactional failure, permissions, and ordinary diff events.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/Cargo.toml codex-rs/MODULE.bazel.lock codex-rs/structured-edit codex-rs/core
git commit -m "feat(tools): add stale-safe structured editing"
```

### Task 6: Attributable Usage and Conservative Spend Reservations

**Files:**
- Create: `codex-rs/state/migrations/0057_usage_ledger.sql`
- Create: `codex-rs/state/src/model/usage_ledger.rs`
- Create: `codex-rs/state/src/runtime/usage_ledger.rs`
- Create: `codex-rs/state/src/runtime/usage_ledger_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Modify: `codex-rs/protocol/src/response_usage.rs`
- Modify: `codex-rs/core/src/client_common.rs`
- Modify: `codex-rs/core/src/session/turn.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v2/thread_usage.rs`
- Modify: `codex-rs/app-server/src/request_processors/thread_processor.rs`
- Modify: `codex-rs/core/Cargo.toml`
- Modify: `codex-rs/core/BUILD.bazel`

**Interfaces:**
- Consumes: existing provider response usage/request identity, thread ID, parent/child graph identity, goal/task identity, and Task 2 evidence identity.
- Produces: `UsageRecord`, `PriceQuote { source, version, currency, effective_at, amount }`, `CostEstimate::{Known,Unknown}`, `UsageReservation`, `StateRuntime::{reserve_usage,settle_usage,usage_summary}`, and extended v2 `ThreadUsage` known/unpriced/reserved fields.

- [ ] **Step 1: Write concurrent reservation and reconciliation tests**

```rust
#[tokio::test]
async fn parallel_reservations_cannot_exceed_parent_budget() {
    let db = test_runtime().await;
    db.set_spend_budget("goal-1", usd_micros(1_000)).await.unwrap();
    let (left, right) = tokio::join!(
        db.reserve_usage(reservation("r1", 700)),
        db.reserve_usage(reservation("r2", 700)),
    );
    assert_eq!(usize::from(left.is_ok()) + usize::from(right.is_ok()), 1);
}

#[tokio::test]
async fn duplicate_late_usage_settles_once_and_unknown_price_stays_unknown() {
    let db = test_runtime().await;
    db.record_usage(unpriced_usage("request-1", 42)).await.unwrap();
    db.record_usage(unpriced_usage("request-1", 42)).await.unwrap();
    let summary = db.usage_summary("thread-1").await.unwrap();
    assert_eq!(summary.unpriced_request_count, 1);
    assert_eq!(summary.known_estimated_cost, None);
}
```

- [ ] **Step 2: Run state tests and verify failure**

Run: `cd codex-rs && just test -p codex-state usage_ledger`

Expected: FAIL because reservation and usage-ledger APIs are absent.

- [ ] **Step 3: Implement atomic reserve/settle schema and APIs**

```sql
CREATE TABLE usage_reservations (
    reservation_id TEXT PRIMARY KEY,
    parent_budget_id TEXT NOT NULL,
    request_id TEXT NOT NULL UNIQUE,
    reserved_micros INTEGER NOT NULL,
    status TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    settled_at INTEGER
);
CREATE TABLE usage_records (
    request_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    parent_thread_id TEXT,
    task_id TEXT,
    model TEXT NOT NULL,
    input_tokens INTEGER,
    cached_input_tokens INTEGER,
    output_tokens INTEGER,
    price_json TEXT,
    record_json TEXT NOT NULL
);
```

Use `BEGIN IMMEDIATE` to sum active reservations and settled known costs, then admit or reject one reservation atomically. Under an enforcing monetary budget, require a known quote or configured conservative fallback. Settle once per request, preserve usage on failed/cancelled requests, release unused reservation, and record overage separately.

- [ ] **Step 4: Attribute provider usage at the request completion boundary**

```rust
let usage = UsageRecord::from_response(
    request_id.clone(),
    turn_context.thread_id(),
    turn_context.parent_thread_id,
    turn_context.active_goal_id(),
    provider.model_slug(),
    response.usage,
    price_catalog.quote(provider.id(), provider.model_slug(), now),
);
state_db.settle_usage(reservation_id, usage).await?;
```

Retries keep distinct provider request IDs but share task attribution. Cache savings must expose the counterfactual formula and price version. Aggregate children by stable thread edges, never display missing values as zero, and keep existing credit estimates distinct from USD estimates.

- [ ] **Step 5: Extend v2 usage output and regenerate schemas**

```rust
pub struct ThreadUsage {
    pub thread_id: String,
    pub estimated_usage_credits_micros: i64,
    pub estimated_usage_usd_micros: Option<i64>,
    pub reserved_usage_usd_micros: Option<i64>,
    pub unpriced_request_count: u32,
    pub cache_savings_usd_micros: Option<i64>,
    pub price_source_version: Option<String>,
    pub groups: Vec<ThreadUsageBreakdownGroup>,
}
```

Run: `cd codex-rs && just write-app-server-schema && just write-app-server-schema --experimental`

- [ ] **Step 6: Run tests and format**

Run: `cd codex-rs && just test -p codex-state usage_ledger && just test -p codex-core usage && just test -p codex-app-server-protocol`

Expected: PASS for concurrency, duplicates, late corrections, cancellation, retries, child aggregation, failed-request usage, missing prices, reservation release, and overage display.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/state codex-rs/protocol codex-rs/core codex-rs/app-server-protocol codex-rs/app-server
git commit -m "feat(usage): attribute cost and reserve spend atomically"
```

### Task 7: Enforced Subagent History Policy

**Files:**
- Modify: `codex-rs/features/src/feature_configs.rs`
- Modify: `codex-rs/core/src/config/mod.rs`
- Modify: `codex-rs/core/src/config/config_tests.rs`
- Create: `codex-rs/core/src/tools/handlers/multi_agents_v2/history_policy.rs`
- Create: `codex-rs/core/src/tools/handlers/multi_agents_v2/history_policy_tests.rs`
- Modify: `codex-rs/core/src/tools/handlers/multi_agents_v2/spawn.rs`
- Modify: `codex-rs/core/src/tools/handlers/multi_agents_v2/mod.rs`
- Modify: `codex-rs/core/src/thread_rollout.rs`
- Modify: `codex-rs/core/src/thread_rollout_truncation_tests.rs`
- Modify: `codex-rs/core/config.schema.json`

**Interfaces:**
- Consumes: existing `fork_turns`, rollout truncation, effective child instructions, and parent/role configuration.
- Produces: `SubagentHistoryPolicy { default_turns: 1, max_turns: 3, max_tokens: 8_000, allow_full_history: false }` and `resolve_history_request(request, parent, role) -> Result<ResolvedHistorySelection, HistoryPolicyError>`.

- [ ] **Step 1: Write policy tests for defaults, ceilings, and zero-request rejection**

```rust
#[test]
fn policy_rejects_disallowed_full_history_before_spawn() {
    let policy = SubagentHistoryPolicy::default();
    assert_eq!(
        resolve_history_request(ForkTurns::All, &policy, None).unwrap_err(),
        HistoryPolicyError::FullHistoryDisabled { max_turns: 3, max_tokens: 8_000 }
    );
}

#[tokio::test]
async fn rejected_oversized_turn_allocates_no_child_and_sends_no_request() {
    let test = spawn_test_with_history(single_turn_over_tokens(8_000)).await;
    let result = test.spawn(ForkTurns::Last(1)).await;
    assert!(matches!(result, Err(HistoryPolicyError::TokenBudgetExceeded { .. })));
    assert_eq!(test.agent_count().await, 1);
    assert_eq!(test.model_request_count(), 0);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cd codex-rs && just test -p codex-core history_policy`

Expected: FAIL because `SubagentHistoryPolicy` is absent.

- [ ] **Step 3: Add validated configuration and effective parent/role intersection**

```rust
pub struct SubagentHistoryPolicy {
    pub default_turns: usize,
    pub max_turns: usize,
    pub max_tokens: usize,
    pub allow_full_history: bool,
}

pub fn effective_policy(
    parent: &SubagentHistoryPolicy,
    role: Option<&SubagentHistoryPolicy>,
) -> SubagentHistoryPolicy {
    SubagentHistoryPolicy {
        default_turns: role.map_or(parent.default_turns, |p| p.default_turns.min(parent.max_turns)),
        max_turns: role.map_or(parent.max_turns, |p| p.max_turns.min(parent.max_turns)),
        max_tokens: role.map_or(parent.max_tokens, |p| p.max_tokens.min(parent.max_tokens)),
        allow_full_history: parent.allow_full_history && role.is_some_and(|p| p.allow_full_history),
    }
}
```

Reject invalid config (`default_turns > max_turns`, zero token budget, maximum above installation policy) during loading. Treat omitted `fork_turns` as one recent user turn for new Moedex config. Preserve old rollout reads; policy applies only to new spawn admission.

- [ ] **Step 4: Enforce before child allocation and model dispatch**

```rust
let history = resolve_history_request(
    args.fork_turns(),
    &turn.config.multi_agent_v2.subagent_history,
    role.history_policy(),
)?;
let bounded_rollout = select_rollout_history(parent_rollout, history)?;
session.services.agent_control.spawn_agent_with_history(config, bounded_rollout).await
```

Estimate the complete inherited selection conservatively before calling agent control. Preserve normal system/developer instruction assembly; do not copy parent-specific instruction items into the selected rollout. On overflow, return effective limits plus `fork_turns="none"` and smaller-turn alternatives without silently dropping turns.

- [ ] **Step 5: Regenerate config schema, test, and format**

Run: `cd codex-rs && just write-config-schema`

Run: `cd codex-rs && just test -p codex-core history_policy && just test -p codex-core thread_rollout_truncation`

Expected: PASS for none/all/one/three/over-limit/invalid requests, role narrowing, shared goal budgets, no-request rejection, and legacy rollout readability.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/features codex-rs/core
git commit -m "feat(agents): enforce bounded inherited history"
```

### Task 8: Explicit Advisory and Enforcing Workflow Policies

**Files:**
- Create: `codex-rs/verification/src/policy.rs`
- Create: `codex-rs/verification/src/policy_tests.rs`
- Create: `codex-rs/state/migrations/0058_workflow_policy_decisions.sql`
- Create: `codex-rs/state/src/runtime/workflow_policy.rs`
- Create: `codex-rs/state/src/runtime/workflow_policy_tests.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Modify: `codex-rs/config/src/hook_config.rs`
- Modify: `codex-rs/config/src/hooks_tests.rs`
- Modify: `codex-rs/hooks/src/engine/mod.rs`
- Create: `codex-rs/hooks/src/engine/policy.rs`
- Create: `codex-rs/hooks/src/engine/policy_tests.rs`
- Modify: `codex-rs/core/src/tools/orchestrator.rs`
- Modify: `codex-rs/core/src/tools/handlers/apply_patch.rs`
- Modify: `codex-rs/core/src/tools/handlers/unified_exec.rs`
- Modify: `codex-rs/protocol/src/protocol.rs`

**Interfaces:**
- Consumes: Task 2 evidence/applicability, existing hook previews/runs, existing tool authorization, and Task 3 review evidence.
- Produces: `WorkflowPolicyMode::{Advisory,Enforcing}`, `PolicyFailureMode::{Allow,Block}`, `PolicyDecision`, `PolicyEvaluator::evaluate(action, evidence) -> PolicyDecision`, and durable policy-decision events.

- [ ] **Step 1: Write deterministic policy tests**

```rust
#[test]
fn advisory_failure_records_reason_without_blocking() {
    let decision = evaluator(policy(WorkflowPolicyMode::Advisory))
        .evaluate(action("git push"), evidence_without_required_test());
    assert_eq!(decision.outcome, PolicyOutcome::AdvisoryFailure);
    assert!(decision.may_execute);
    assert_eq!(decision.missing_evidence, vec!["test:codex-core"]);
}

#[test]
fn enforcing_timeout_uses_configured_failure_mode() {
    let policy = policy_with_timeout(WorkflowPolicyMode::Enforcing, PolicyFailureMode::Block);
    assert_eq!(evaluator(policy).timeout(action("release")).outcome, PolicyOutcome::BlockedTimeout);
}
```

- [ ] **Step 2: Run tests and verify failure**

Run: `cd codex-rs && just test -p codex-verification policy`

Expected: FAIL because workflow policy types do not exist.

- [ ] **Step 3: Implement versioned policy evaluation over current evidence**

```rust
pub struct PolicyDecision {
    pub decision_id: PolicyDecisionId,
    pub policy_id: String,
    pub policy_version: String,
    pub action: EvaluatedAction,
    pub mode: WorkflowPolicyMode,
    pub outcome: PolicyOutcome,
    pub evidence_ids: Vec<EvidenceId>,
    pub exception: Option<PolicyException>,
    pub may_execute: bool,
}
```

Evaluate required evidence against current snapshot applicability, record missing/stale/failed items distinctly, and sort conflicting policies by stable policy ID. Any enforcing block wins; advisory failures remain visible. Exceptions name policy/version/action/scope/actor/reason and cannot widen underlying sandbox or approval authority.

- [ ] **Step 4: Extend hook configuration and tool preflight without replacing permission checks**

```toml
[[hooks.PreToolUse]]
matcher = "git push"
policy_id = "verified-before-push"
policy_version = "1"
mode = "enforcing"
failure_mode = "block"
requires_evidence = ["test:codex-core", "review:current"]
timeout_sec = 10
```

Run policy after existing hook input validation and before the action could execute, then continue through ordinary permission/sandbox checks when allowed. Timeouts and malformed policy output follow `failure_mode`; show the concrete decision reason in the tool error/event. Async hooks cannot enforce.

- [ ] **Step 5: Persist policy decisions and verify replay/conflicts**

```sql
CREATE TABLE workflow_policy_decisions (
    decision_id TEXT PRIMARY KEY,
    action_id TEXT NOT NULL,
    policy_id TEXT NOT NULL,
    policy_version TEXT NOT NULL,
    outcome TEXT NOT NULL,
    record_json TEXT NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE INDEX workflow_policy_action_idx
    ON workflow_policy_decisions(action_id, policy_id);
```

Test missing prerequisites, stale evidence, timeout, explicit exception, duplicate decision replay, and advisory/enforcing conflicts through `apply_patch` and unified exec integration paths.

- [ ] **Step 6: Run crate tests and format**

Run: `cd codex-rs && just test -p codex-verification policy && just test -p codex-hooks policy && just test -p codex-state workflow_policy && just test -p codex-core workflow_policy`

Expected: PASS with advisory execution, enforcing pre-execution block, configured timeout behavior, visible reasons, and unchanged permission enforcement.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/verification codex-rs/state codex-rs/config codex-rs/hooks codex-rs/core codex-rs/protocol
git commit -m "feat(policy): add explicit workflow enforcement modes"
```

### Task 9: Stage C End-to-End Qualification and Behavior Manifest

**Files:**
- Create: `codex-rs/core/tests/suite/moedex_stage_c.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Create: `codex-rs/app-server/tests/suite/v2/moedex_stage_c.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Create: `scripts/qualification/stage-c-verification.sh`
- Modify: `moedex-behavior-manifest.json`

**Interfaces:**
- Consumes: all interfaces produced by Tasks 1-8.
- Produces: one packaged Stage C qualification command and manifest mappings from R6/O1/O2/U3/C10/C12/C13/C14 to executable evidence.

- [ ] **Step 1: Add the end-to-end fault matrix**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stage_c_completion_requires_current_review_and_passing_checks() -> anyhow::Result<()> {
    let test = test_codex().build_with_auto_env().await?;
    let cycle = test.start_review_cycle(required_checks(["test:core"])).await?;
    test.external_edit("src/lib.rs", "pub fn changed() {}\n")?;
    test.complete_check(cycle.id(), EvidenceStatus::Passed).await?;
    let report = test.verification_report(cycle.id()).await?;
    assert_eq!(report.overall, VerificationOverall::Incomplete);
    assert_eq!(report.stale_claims.len(), 1);
    Ok(())
}
```

The suite also exercises duplicate/out-of-order evidence, cancellation, three unchanged failures, declared job polling, stale structured edits, parallel spend reservations, disallowed history, policy timeout, artifact loss, and app-server restart.

- [ ] **Step 2: Run the Stage C tests**

Run: `cd codex-rs && just test -p codex-core moedex_stage_c && just test -p codex-app-server moedex_stage_c`

Expected: PASS with public core and JSON-RPC behavior across the complete Stage C journey.

- [ ] **Step 3: Add a deterministic qualification script**

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../codex-rs"
just test -p codex-workspace-snapshot
just test -p codex-verification
just test -p codex-structured-edit
just test -p codex-state verification
just test -p codex-core moedex_stage_c
just test -p codex-app-server-protocol
just test -p codex-app-server moedex_stage_c
```

- [ ] **Step 4: Record exact manifest evidence mappings**

```json
{
  "stage": "C",
  "requirements": {
    "R6": ["codex-core::moedex_stage_c::repetition_pauses"],
    "O1": ["codex-core::moedex_stage_c::review_snapshot_stales"],
    "O2": ["codex-core::moedex_stage_c::repair_limits_stop"],
    "U3": ["codex-core::history_policy"],
    "C10": ["codex-state::usage_ledger"],
    "C12": ["codex-structured-edit::edit_tests"],
    "C13": ["codex-app-server::moedex_stage_c"],
    "C14": ["codex-hooks::policy"]
  }
}
```

Merge this object into the Stage A behavior manifest schema without removing prior mappings.

- [ ] **Step 5: Run scoped lint fixes and formatting as the final mutation**

Run: `cd codex-rs && just fix -p codex-workspace-snapshot && just fix -p codex-verification && just fix -p codex-structured-edit && just fix -p codex-state && just fix -p codex-core && just fix -p codex-app-server-protocol && just fix -p codex-app-server`

Run: `cd codex-rs && just fmt`

Expected: commands complete without unresolved lint or formatting changes. Do not rerun tests after `fix` or `fmt` unless those commands report a substantive failure requiring a code change.

- [ ] **Step 6: Commit**

```bash
git add codex-rs/core/tests codex-rs/app-server/tests scripts/qualification/stage-c-verification.sh moedex-behavior-manifest.json
git commit -m "test(moedex): qualify Stage C verification controls"
```
