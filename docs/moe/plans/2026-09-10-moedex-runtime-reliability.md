# Moedex Runtime Reliability Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make queued work, retries, credential changes, compaction, session status, and operator controls survive interruption predictably while keeping long-session UI updates bounded.

**Architecture:** Extend the existing SQLite queue and `codex-queue-extension` rather than creating a second scheduler: durable run/generation records own queue acceptance, retry deadlines, and reconciliation. Keep credential and compaction policies in focused modules at their current request boundaries, expose typed lifecycle deltas through the existing protocol, and let the TUI consume those deltas through bounded status caches. Reuse the current archive/restore, model picker, clipboard, attachment snapshot, and finalized markdown-cache mechanisms.

**Tech Stack:** Rust 2024, Tokio, SQLx/SQLite, existing extension lifecycle API, app-server v2 protocol, Ratatui, `insta`, frozen Tokio time, existing `AttachmentStore` and thread-store APIs.

**Spec:** `specs/moedex/SPEC.md`, `specs/moedex/EXECUTION.md`, and `specs/moedex/EXPERIENCE.md` (R1–R5, R7, O3, O6, U1, U2, U7, U8, U11)

## Global Constraints

- Stage A home and namespace isolation is complete before durable Stage B state is created.
- Queue admission is bounded at 100 entries/thread, 64 KiB text/entry, 4 MiB text/thread, 10 attachments/input, 20 MiB attachment bytes/input, and 200 MiB pending attachment bytes/thread; reject before acknowledgement and never evict accepted input.
- Run states are `ready`, `active`, `waiting_capacity`, `waiting_backoff`, `paused`, `reconciling`, `completed`, `cancelled`, and `failed`.
- Backoff starts at 2 seconds, caps at 60 seconds, permits at most 8 total recovery attempts, and fits inside a 10-minute outer deadline; waiting time counts.
- A late event must match run ID, execution generation, instruction ID/revision, and attempt identity before it can advance work.
- Do not claim exactly-once external effects; ambiguous tool completion records `effect_unknown` and pauses dependent work until reconciled.
- Remote compaction deadline is 120 seconds, with one eligible local fallback; cancellation and invalid input never fall back.
- Git status refresh defaults to 15 seconds with a 2-second collection timeout and is shared/cancellable.
- Status/UI caches cap at 512 finalized rendered cells and 1,024 queued status events/view; coalesce status by identity without dropping durable completion events.
- Preserve existing authorization, sandbox, history, rollout, `.codex` project configuration, and app-server compatibility semantics.
- New model-visible fragments must implement `ContextualUserFragment`, default to at most 1,000 tokens each and 4,000 new generated tokens/turn, and never rewrite history.
- Core behavior supports Linux, macOS, and Windows; v2 API changes require schema generation and optional request fields use `#[ts(optional = nullable)]`.
- Agent-logic changes require integration tests; UI/copy changes require reviewed `insta` snapshots.
- Run focused crate tests through `just test`; ask the user before the complete workspace `just test` required by changes to core/protocol.
- After code changes run scoped `just fix -p <crate>` when applicable, then `just fmt`; do not rerun tests after `fix` or `fmt`.

## Open Decisions

No open decisions. The specs define the states, limits, retry/compaction defaults, persistence scope, and operator behavior needed for this stage.

## Not Yet Specified

- Hardware performance thresholds remain unset until the deterministic 300-turn work-count fixture establishes a baseline; this does not block the zero-relayout contract.
- Provider-specific attachment conversion can be added after capability reporting exists; unsupported delivery must preserve the queued item.

## Out of Scope

- Review/repair loops, evidence ledger, history inheritance policy, and cost attribution beyond already available usage fields; those ship in Stage C.
- Rooms, boards, attention inbox, observer, and follow-up proposals; those ship in Stage D.
- Non-progress detection R6; it is deliberately outside the requested Stage B slice.
- New provider protocols or research retrieval.

---

### Task 1: Durable run, generation, and queue-acceptance records (R1)

**Files:**
- Create: `codex-rs/state/queue_migrations/0003_runtime_recovery.sql`
- Create: `codex-rs/state/src/model/run_recovery.rs`
- Create: `codex-rs/state/src/runtime/run_recovery.rs`
- Create: `codex-rs/state/src/runtime/run_recovery_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/runtime/queued_items.rs`
- Modify: `codex-rs/state/src/runtime/queued_items_tests.rs`
- Modify: `codex-rs/thread-store/src/queue_store.rs`
- Modify: `codex-rs/thread-store/src/lib.rs`

**Interfaces:**
- Consumes: Existing `QueueStore`, `QueuedUserSubmissionRecord`, thread IDs, queue ordering, and SQLite queue database.
- Produces: `RunId`, `ExecutionGeneration`, `InstructionRevision`, `RunState`, `EffectState`, `RunRecoveryRecord`; `QueueStore::accept_for_turn(AcceptQueuedInput) -> ThreadStoreFuture<AcceptedQueuedInput>`; and atomic cancel/supersede/reorder operations.

- [ ] **Step 1: Write failing persistence and crash-window tests**

```rust
#[tokio::test]
async fn accepting_input_and_advancing_run_is_one_transaction() {
    let queued = store.enqueue(thread_id, payload()).await.unwrap();
    let accepted = store.accept_for_turn(AcceptQueuedInput {
        thread_id,
        queued_item_id: queued.id.clone(),
        run_id,
        generation: ExecutionGeneration(3),
        expected_revision: InstructionRevision(2),
        attempt_id,
    }).await.unwrap();
    assert_eq!(accepted.instruction_id, queued.id);
    assert!(store.list_page(thread_id, 0, 1).await.unwrap().is_empty());
    assert_eq!(store.run(run_id).await.unwrap().unwrap().state, RunState::Active);
}
```

Add restart tests for a transaction interrupted before commit, duplicate acceptance, cancelled generation, revised instruction, queue-full admission, and an active run becoming `reconciling` with `EffectState::Unknown`.

- [ ] **Step 2: Run state and thread-store tests to verify failure**

Run: `cd codex-rs && just test -p codex-state -p codex-thread-store`

Expected: FAIL because recovery records and transactional acceptance do not exist.

- [ ] **Step 3: Add versioned records and migration**

```rust
#[derive(Clone, Copy, Debug, Eq, PartialEq, Serialize, Deserialize)]
pub enum RunState { Ready, Active, WaitingCapacity, WaitingBackoff, Paused, Reconciling, Completed, Cancelled, Failed }

pub struct AcceptQueuedInput {
    pub thread_id: ThreadId,
    pub queued_item_id: String,
    pub run_id: RunId,
    pub generation: ExecutionGeneration,
    pub expected_revision: InstructionRevision,
    pub attempt_id: String,
}
```

The migration adds run, instruction revision/history, accepted-turn, operation, and scheduled-retry tables with unique `(run_id, generation, instruction_id, revision, attempt_id)` keys. Add byte-accounting columns and admission constraints; never delete history when an item is cancelled or superseded.

- [ ] **Step 4: Implement atomic queue transitions**

Use one SQLite transaction to compare generation/revision, insert accepted-turn identity, change run state, and remove the pending row. Duplicate identity returns the existing acceptance; a stale identity returns a typed conflict and leaves the queue unchanged.

- [ ] **Step 5: Verify, lint, and format**

Run: `cd codex-rs && just test -p codex-state -p codex-thread-store && just fix -p codex-state && just fix -p codex-thread-store && just fmt`

Expected: crash/replay, admission, cancellation, and revision tests pass.

- [ ] **Step 6: Commit durable identities**

```bash
git add codex-rs/state codex-rs/thread-store
git commit -m "feat: persist recoverable run and queue identities"
```

### Task 2: Generation-safe queue dispatch, overload recovery, and reconciliation (R1, R2, R7, O3)

**Files:**
- Create: `codex-rs/ext/queue/src/recovery.rs`
- Create: `codex-rs/ext/queue/src/recovery_tests.rs`
- Modify: `codex-rs/ext/queue/src/service.rs`
- Create: `codex-rs/ext/queue/src/service_tests.rs`
- Modify: `codex-rs/protocol/src/protocol.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Create: `codex-rs/core/tests/suite/queued_recovery.rs`

**Interfaces:**
- Consumes: Task 1 `RunRecoveryRecord` and `QueueStore::accept_for_turn`, existing `ThreadLifecycleContributor`, `StartIfIdleSubmission`, and structured `CodexErrorDetails`.
- Produces: `RecoveryPolicy`, `RetrySchedule`, `RecoveryCause`, `RunLifecycleEvent`, `ReconciliationOutcome`; `QueuedItemService::recover(run_id) -> Result<RunRecoveryRecord, QueueServiceError>`.

- [ ] **Step 1: Add failing end-to-end recovery tests**

```rust
#[tokio::test(start_paused = true)]
async fn stale_duplicate_timer_cannot_submit_superseded_work() {
    let harness = RecoveryHarness::with_policy(RecoveryPolicy::default()).await;
    let scheduled = harness.inject_overload().await;
    harness.replace_objective("new objective").await;
    harness.fire(scheduled.clone()).await;
    harness.fire(scheduled).await;
    assert_eq!(harness.accepted_turns().await, vec!["new objective"]);
}
```

Using `test_codex`, cover A/B/C where B is edited and C cancelled, crash between dispatch and acknowledgement, permanent malformed request, capacity reset beyond deadline, hanging request, cancel callback, and ambiguous non-idempotent tool result.

- [ ] **Step 2: Run focused tests and confirm current dispatch consumes too early**

Run: `cd codex-rs && just test -p codex-queue-extension -p codex-core queued_recovery`

Expected: FAIL because `service.rs` deletes on start without a durable acceptance/reconciliation record.

- [ ] **Step 3: Implement one outer recovery policy**

```rust
pub struct RecoveryPolicy {
    pub initial_backoff: Duration,
    pub maximum_backoff: Duration,
    pub maximum_attempts: u8,
    pub elapsed_budget: Duration,
}

pub struct RetrySchedule {
    pub run_id: RunId,
    pub generation: ExecutionGeneration,
    pub instruction_id: String,
    pub revision: InstructionRevision,
    pub attempt: u8,
    pub retry_at: SystemTime,
    pub deadline: SystemTime,
    pub cause: RecoveryCause,
}
```

Classify overload, capacity reset, permanent request, and ambiguous effect. Apply exponential backoff plus bounded jitter, but clamp request and timer deadlines to remaining outer budget. Count client and outer retries under the same logical attempt.

- [ ] **Step 4: Replace delete-after-start with transactional acceptance**

Dispatch only the FIFO eligible revision. After `start_turn_if_idle` establishes the turn identity, call `accept_for_turn`; on crash/restart, inspect accepted-turn and operation records before replay. Query supported operation status/idempotency metadata; otherwise set `effect_unknown`, pause dependent work, and emit the missing evidence.

- [ ] **Step 5: Expose bounded lifecycle deltas**

Add protocol events carrying authoritative state, cause, last meaningful event, active operation ID, pending count, next attempt, outer deadline, provider reset, and liveness `known_active | known_finished | unknown`. Keep payloads bounded and append-only.

- [ ] **Step 6: Verify and commit**

Run: `cd codex-rs && just test -p codex-queue-extension -p codex-protocol -p codex-core queued_recovery && just fix -p codex-queue-extension && just fix -p codex-protocol && just fix -p codex-core && just fmt`

Expected: all frozen-clock and crash/reconciliation cases pass. Because protocol/core changed, record that the complete `just test` remains the user-approved execution-time release gate.

```bash
git add codex-rs/ext/queue codex-rs/protocol codex-rs/core/tests/suite
git commit -m "feat: recover queued work within bounded deadlines"
```

### Task 3: Turn-bound credential generations (R3)

**Files:**
- Create: `codex-rs/login/src/auth/generation.rs`
- Create: `codex-rs/login/src/auth/generation_tests.rs`
- Modify: `codex-rs/login/src/auth/mod.rs`
- Modify: `codex-rs/login/src/auth/manager.rs`
- Modify: `codex-rs/core/src/client.rs`
- Modify: `codex-rs/core/src/client_tests.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Create: `codex-rs/core/tests/suite/credential_generation.rs`

**Interfaces:**
- Consumes: Existing auth manager, `auth_owner_generation` transport invalidation, and logical turn setup.
- Produces: `CredentialSnapshot { generation: CredentialGeneration, identity: AuthIdentity, state: CredentialState }`; `AuthManager::validated_snapshot()` and `AuthManager::activate_at_turn_boundary(...)`.

- [ ] **Step 1: Write failing safe-boundary tests**

```rust
#[tokio::test]
async fn replacement_during_stream_applies_to_the_next_turn() {
    let first = harness.start_stream().await;
    harness.install_validated(account_b()).await;
    first.finish().await;
    assert_eq!(first.request_identity(), account_a().identity);
    assert_eq!(harness.start_turn().await.request_identity(), account_b().identity);
}
```

Cover partial file read, explicit logout/revocation, cached websocket reset on generation change, invalid replacement, and mid-turn authentication failure without silent account switching.

- [ ] **Step 2: Run tests and observe failure**

Run: `cd codex-rs && just test -p codex-login -p codex-core credential_generation`

Expected: FAIL because validation/activation are not expressed as one boundary contract.

- [ ] **Step 3: Implement validated snapshots and activation**

```rust
pub enum CredentialState { Authenticated, LoggedOut, Revoked }

pub fn activate_at_turn_boundary(
    &self,
    candidate: ValidatedCredentialRecord,
) -> Result<CredentialSnapshot, AuthError>;
```

Bind all retries of an active turn to its snapshot generation. A partial read preserves the last valid snapshot, while a confirmed missing/revoked record activates `LoggedOut`/`Revoked`. Reuse the existing `auth_owner_generation` comparison to discard cached transport identity.

- [ ] **Step 4: Show next-dispatch identity without secrets**

Extend queued lifecycle/status data with a bounded provider/workspace identity label and credential generation for the next dispatch; omit token/account secrets.

- [ ] **Step 5: Verify and commit**

Run: `cd codex-rs && just test -p codex-login -p codex-core credential_generation && just fix -p codex-login && just fix -p codex-core && just fmt`

```bash
git add codex-rs/login codex-rs/core
git commit -m "feat: bind credentials to logical turns"
```

### Task 4: Unified compaction policy and independent resources (R4, R5)

**Files:**
- Create: `codex-rs/core/src/compaction_policy.rs`
- Create: `codex-rs/core/src/compaction_policy_tests.rs`
- Modify: `codex-rs/core/src/lib.rs`
- Modify: `codex-rs/core/src/compact.rs`
- Modify: `codex-rs/core/src/compact_remote_v2.rs`
- Modify: `codex-rs/config/src/config_toml.rs`
- Modify: `codex-rs/config/src/profile_toml.rs`
- Modify: `codex-rs/core/src/config/mod.rs`
- Modify: `codex-rs/core/src/config/config_tests.rs`
- Modify: `codex-rs/core/config.schema.json`
- Modify: `codex-rs/core/tests/suite/compact.rs`

**Interfaces:**
- Consumes: Existing manual/automatic compaction entry points, remote/local compactors, `ServiceTier`, cancellation token, and checkpoint publication.
- Produces: `CompactionPolicy { remote_deadline, local_fallback, service_tier }`, `CompactionAttempt`, `CompactionOutcome`; a single `run_compaction(policy, request, cancellation) -> CodexResult<CompactionOutcome>` path.

- [ ] **Step 1: Add failing parity and publication tests**

```rust
#[tokio::test(start_paused = true)]
async fn cancellation_never_starts_local_fallback() {
    let harness = CompactionHarness::remote_hangs();
    let run = harness.start(CompactionEntryPoint::Automatic).await;
    run.cancel();
    assert_eq!(run.await.unwrap(), CompactionOutcome::Interrupted);
    assert_eq!(harness.local_attempts(), 0);
}
```

Run identical timeout/transient/invalid/cancel cases for manual and automatic entry points. Add crash-before-publication, concurrent coalescing, stale generation, compaction-only tier/deadline, and unsupported explicit tier cases.

- [ ] **Step 2: Run focused tests to show policy divergence**

Run: `cd codex-rs && just test -p codex-core compact`

Expected: FAIL because entry points do not share the proposed policy/settings.

- [ ] **Step 3: Implement the policy module and config**

```rust
pub struct CompactionPolicy {
    pub remote_deadline: Duration,
    pub local_fallback: LocalFallback,
    pub service_tier: Option<String>,
}

pub enum LocalFallback { OnceOnEligibleFailure, Disabled }
```

Default to 120 seconds and one fallback on timeout/transient service failure. Cancellation and invalid input return directly. Validate explicit service tier against provider capabilities; unconfigured tier inherits provider behavior.

- [ ] **Step 4: Serialize/coalesce and publish atomically**

Key in-flight compaction by thread and context generation. Publish the replacement checkpoint only after the selected path fully succeeds and a generation compare succeeds; failed/stale candidates never replace the committed checkpoint.

- [ ] **Step 5: Regenerate config schema and verify**

Run: `cd codex-rs && just write-config-schema && just test -p codex-core compact && just fix -p codex-core && just fmt`

Expected: manual/automatic matrices match; ordinary requests omit compaction overrides.

- [ ] **Step 6: Commit compaction reliability**

```bash
git add codex-rs/core
git commit -m "feat: unify compaction recovery policy"
```

### Task 5: Live status and bounded long-session rendering (R7, O3, O6, U1)

**Files:**
- Create: `codex-rs/tui/src/session_status.rs`
- Create: `codex-rs/tui/src/session_status_tests.rs`
- Create: `codex-rs/tui/src/history_cell/finalized_layout_cache.rs`
- Create: `codex-rs/tui/src/history_cell/finalized_layout_cache_tests.rs`
- Modify: `codex-rs/tui/src/history_cell/mod.rs`
- Modify: `codex-rs/tui/src/chatwidget.rs`
- Modify: `codex-rs/tui/src/bottom_pane/status_line_setup.rs`
- Modify: `codex-rs/tui/src/bottom_pane/status_surface_preview.rs`
- Modify: `codex-rs/tui/src/app_event.rs`
- Modify: `codex-rs/tui/src/app.rs`

**Interfaces:**
- Consumes: Task 2 `RunLifecycleEvent`, current status-line fields, workspace command runner, rate-limit/usage events, and markdown render cache.
- Produces: `SessionStatusSnapshot`, `StatusField<T>::Known(T) | Unknown`, `StatusRefreshCoordinator`, and `FinalizedLayoutCache` capped at 512 entries.

- [ ] **Step 1: Add failing stale-result, narrow-view, and work-count tests**

```rust
#[tokio::test(start_paused = true)]
async fn old_git_result_cannot_update_a_new_cwd() {
    let mut status = StatusRefreshCoordinator::new(Duration::from_secs(15));
    let first = status.request(cwd_a());
    status.set_cwd(cwd_b());
    status.complete(first, git_a());
    assert_eq!(status.snapshot().git_branch, StatusField::Unknown);
}

#[test]
fn status_ticks_do_not_relayout_300_finalized_turns() {
    let mut fixture = TranscriptFixture::with_finalized_turns(300);
    fixture.prime_layout(80);
    fixture.apply_status_ticks(1_000);
    assert_eq!(fixture.finalized_relayouts(), 0);
}
```

Add resize/theme/content invalidation, 512-entry eviction/reload, 1,024-status-event coalescing, provider failure responsiveness, and narrow snapshot tests.

- [ ] **Step 2: Run TUI tests and capture failures**

Run: `cd codex-rs && just test -p codex-tui`

Expected: FAIL because refresh generations and deterministic layout-work counters are absent.

- [ ] **Step 3: Implement shared cancellable status refresh**

```rust
pub enum StatusField<T> { Known(T), Unknown }

pub struct StatusRefreshRequest {
    pub request_id: u64,
    pub cwd: AbsolutePathBuf,
    pub deadline: Instant,
}
```

Refresh Git every 15 seconds with a two-second timeout. Share one request per active cwd, cancel on cwd/view teardown, and compare request ID plus cwd before applying results. Show model/effort, cwd, branch/divergence/dirty, capacity, usage, recovery deadline, retry time, and liveness; unavailable values render as unknown.

- [ ] **Step 4: Isolate status invalidation from transcript layout**

Store finalized cell layouts by `(cell_identity, width, theme_revision, content_revision)` in a 512-entry LRU. Apply status-only events to the composer/title/status regions and coalesce the bounded live queue by `(thread_id, field)`. Width/theme/content changes invalidate only affected layouts.

- [ ] **Step 5: Review snapshots and verify**

Run: `cd codex-rs && just test -p codex-tui && cargo insta pending-snapshots -p codex-tui`

Read and accept intended normal/narrow/recovery snapshots, then run: `cd codex-rs && cargo insta accept -p codex-tui && just fix -p codex-tui && just fmt`

Expected: the 300-turn fixture records zero finalized relayouts for status ticks and exactly one width-driven reflow.

- [ ] **Step 6: Commit status/rendering work**

```bash
git add codex-rs/tui
git commit -m "feat: show reliable live session status"
```

### Task 6: Explicit session-only versus saved model settings (U2)

**Files:**
- Create: `codex-rs/tui/src/model_selection_scope.rs`
- Create: `codex-rs/tui/src/model_selection_scope_tests.rs`
- Modify: `codex-rs/tui/src/app_event.rs`
- Modify: `codex-rs/tui/src/app.rs`
- Modify: `codex-rs/tui/src/chatwidget.rs`
- Modify: `codex-rs/tui/src/chatwidget/model_popup_state.rs`
- Modify: `codex-rs/tui/src/chatwidget/model_popups.rs`
- Modify: `codex-rs/tui/src/app/config_persistence.rs`

**Interfaces:**
- Consumes: Existing `UpdateModel`, `UpdateReasoningEffort`, `PersistModelSelection`, and config editor.
- Produces: `SelectionPersistence::{SessionOnly, SaveDefault}` and `ModelSelectionIntent { model, effort, persistence }` propagated through nested pickers.

- [ ] **Step 1: Add failing scope/cancellation tests**

```rust
#[tokio::test]
async fn session_only_selection_never_writes_config() {
    let result = apply_selection(ModelSelectionIntent {
        model: "gpt-next".into(),
        effort: Some(ReasoningEffort::High),
        persistence: SelectionPersistence::SessionOnly,
    }).await;
    assert_eq!(result.active_model(), "gpt-next");
    assert_eq!(fs::read_to_string(config_path()).unwrap(), original_config());
}
```

Cover explicit save of only model/effort, cancellation at each nested picker, active-turn identity stability, next-turn application, and unchanged active child models.

- [ ] **Step 2: Run TUI tests and observe ambiguous persistence behavior**

Run: `cd codex-rs && just test -p codex-tui model_selection_scope`

Expected: FAIL because persistence scope is not a first-class value across picker events.

- [ ] **Step 3: Carry the scope through every picker event**

```rust
pub enum SelectionPersistence { SessionOnly, SaveDefault }

pub struct ModelSelectionIntent {
    pub model: String,
    pub effort: Option<ReasoningEffort>,
    pub persistence: SelectionPersistence,
}
```

Session-only emits runtime updates for the next safe turn. Save-default calls the existing config editor with only model and reasoning effort edits. Cancellation emits neither runtime nor persistence events.

- [ ] **Step 4: Snapshot both actions and verify**

Run: `cd codex-rs && just test -p codex-tui && cargo insta pending-snapshots -p codex-tui`

Review labels `Use for this session` and `Save as default`, accept intended snapshots, then run: `cd codex-rs && cargo insta accept -p codex-tui && just fix -p codex-tui && just fmt`

- [ ] **Step 5: Commit scoped settings**

```bash
git add codex-rs/tui
git commit -m "feat: distinguish session and saved model settings"
```

### Task 7: Resume cleanup and copy-draft operator actions (U7, U8)

**Files:**
- Create: `codex-rs/tui/src/bottom_pane/copy_draft.rs`
- Create: `codex-rs/tui/src/bottom_pane/copy_draft_tests.rs`
- Modify: `codex-rs/tui/src/bottom_pane/mod.rs`
- Modify: `codex-rs/tui/src/bottom_pane/chat_composer.rs`
- Modify: `codex-rs/tui/src/app_event.rs`
- Modify: `codex-rs/tui/src/app.rs`
- Modify: `codex-rs/tui/src/keymap.rs`
- Modify: `codex-rs/tui/src/resume_picker.rs`
- Modify: `codex-rs/tui/src/resume_picker/archive.rs`
- Modify: `codex-rs/tui/src/resume_picker/archive_tests.rs`
- Modify: `codex-rs/thread-store/src/store.rs`
- Modify: `codex-rs/thread-store/src/types.rs`

**Interfaces:**
- Consumes: Existing archive/unarchive/delete thread primitives, composer draft text, clipboard remote fallbacks, and keymap action menu.
- Produces: `copy_current_draft() -> CopyDraftOutcome`; deliberate resume-picker `Archive`, `Restore`, and `Delete` actions; `UnavailableThreadReference` tombstones.

- [ ] **Step 1: Add failing draft-preservation and lifecycle tests**

```rust
#[test]
fn copying_unicode_draft_preserves_all_input_state() {
    let before = composer.snapshot_input_state();
    let outcome = composer.copy_current_draft(&clipboard).unwrap();
    assert_eq!(clipboard.contents(), "line one\nλ line two");
    assert_eq!(outcome, CopyDraftOutcome::Copied);
    assert_eq!(composer.snapshot_input_state(), before);
}
```

Cover empty/unavailable clipboard feedback, active turn, queued input, modal fallback, archive/restore identity, active-owned deletion rejection, and a referenced deletion rendered as an unavailable-source tombstone.

- [ ] **Step 2: Run TUI/thread-store tests to verify gaps**

Run: `cd codex-rs && just test -p codex-tui -p codex-thread-store`

Expected: archive/restore tests pass as baseline; new copy-draft and tombstone/deletion tests fail.

- [ ] **Step 3: Implement copy without composer mutation**

Read only user-authored draft text and call the existing clipboard path. Add a distinct discoverable action; if its chord conflicts or is unavailable, keep the action menu entry. Do not call interrupt, submit, flush, or dequeue.

- [ ] **Step 4: Complete deliberate cleanup semantics**

Keep archive/restore on existing lifecycle primitives. Put permanent delete behind existing confirmation, reject while an active writer owns the thread, preserve references as typed tombstones, and keep board membership removal outside thread deletion.

- [ ] **Step 5: Review snapshots, verify, and commit**

Run: `cd codex-rs && just test -p codex-tui -p codex-thread-store && cargo insta pending-snapshots -p codex-tui`

Accept reviewed help/menu/picker/error snapshots, then run: `cd codex-rs && cargo insta accept -p codex-tui && just fix -p codex-tui && just fix -p codex-thread-store && just fmt`

```bash
git add codex-rs/tui codex-rs/thread-store
git commit -m "feat: add safe draft copy and session cleanup"
```

### Task 8: Immutable queued attachments and end-to-end Stage B qualification (R1, R2, R3, R4, R5, R7, O3, O6, U1, U2, U7, U8, U11)

**Files:**
- Create: `codex-rs/state/queue_migrations/0004_queued_attachment_accounting.sql`
- Create: `codex-rs/ext/queue/src/attachments.rs`
- Create: `codex-rs/ext/queue/src/attachments_tests.rs`
- Modify: `codex-rs/ext/queue/Cargo.toml`
- Modify: `codex-rs/ext/queue/BUILD.bazel`
- Modify: `codex-rs/ext/queue/src/service.rs`
- Modify: `codex-rs/state/src/runtime/queued_items.rs`
- Modify: `codex-rs/protocol/src/protocol.rs`
- Modify: `codex-rs/tui/src/bottom_pane/pending_input_preview.rs`
- Modify: `codex-rs/tui/src/chatwidget.rs`
- Create: `codex-rs/core/tests/suite/stage_b_recovery.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Modify: `moedex-behavior-manifest.json`

**Interfaces:**
- Consumes: Existing `AttachmentStore`, `snapshot_local_user_input`, Task 1 admission accounting, Task 2 recovery, and TUI queue controls.
- Produces: `QueuedAttachment { id, content_identity, media_type, byte_len, store_ref }`, immutable per-revision attachment lists, provider capability recovery outcomes, and Stage B manifest gates.

- [ ] **Step 1: Add failing immutable-content and capacity tests**

```rust
#[tokio::test]
async fn delivery_uses_admitted_bytes_after_source_changes() {
    fs::write(&path, b"first").unwrap();
    let queued = service.enqueue(thread_id, local_file_input(&path)).await.unwrap();
    fs::write(&path, b"second").unwrap();
    let delivered = service.materialize(&queued.id).await.unwrap();
    assert_eq!(delivered.attachment_bytes(), b"first");
}
```

Cover deleted source, restart/order, ten-file and 20 MiB input limits, 200 MiB thread limit, atomic rejection, revision replacement, in-flight immutability, unsupported provider capability, and attachment-store full behavior.

- [ ] **Step 2: Run focused tests and establish current coverage**

Run: `cd codex-rs && just test -p codex-queue-extension -p codex-state -p codex-core stage_b_recovery`

Expected: local image/audio snapshot baselines may pass; generic file references, byte accounting, and provider recovery tests fail.

- [ ] **Step 3: Persist attachment bytes before queue acknowledgement**

```rust
pub struct QueuedAttachment {
    pub id: String,
    pub content_identity: String,
    pub media_type: String,
    pub byte_len: u64,
    pub store_ref: AttachmentRef,
}
```

Validate permission, media metadata, item/thread capacity, and provider hard limits before accepting. Persist bytes through `AttachmentStore`, compute content identity, then commit queue revision plus accounting in one recoverable admission. Editing creates a new revision and retains the accepted in-flight revision.

- [ ] **Step 4: Preserve unsupported queued input and expose recovery**

When the selected provider cannot deliver a stored attachment, leave the item pending and emit an actionable capability outcome. TUI list/edit/remove/inspect actions operate on stable item and attachment IDs and never reread original paths.

- [ ] **Step 5: Add the integrated Stage B fault fixture**

Exercise queue A/B/C with attachments, overload/backoff, credential replacement, compaction cancellation, crash/reconciliation, restart, status display, draft copy, archive/restore, and 300-turn status ticks through public helpers. Assert no duplicate turn/tool effect, no lost accepted input, and bounded UI work.

- [ ] **Step 6: Update manifest and regenerate protocol schemas**

Map R1–R5, R7, O3, O6, U1, U2, U7, U8, and U11 to focused tests/artifacts in `moedex-behavior-manifest.json`.

Run: `cd codex-rs && just write-app-server-schema && just test -p codex-state -p codex-thread-store -p codex-queue-extension -p codex-login -p codex-protocol -p codex-core stage_b_recovery -p codex-tui && cargo insta pending-snapshots -p codex-tui`

Expected: no pending unexpected snapshot or schema diff; all Stage B fault cases pass. Before release, obtain the required user approval and run the complete `cd codex-rs && just test` because core/protocol changed.

- [ ] **Step 7: Lint, format, and commit**

Run: `cd codex-rs && just fix -p codex-state && just fix -p codex-thread-store && just fix -p codex-queue-extension && just fix -p codex-login && just fix -p codex-protocol && just fix -p codex-core && just fix -p codex-tui && just fmt`

```bash
git add codex-rs/state codex-rs/ext/queue codex-rs/protocol codex-rs/tui codex-rs/core/tests/suite moedex-behavior-manifest.json
git commit -m "feat: qualify durable rich queued input"
```
