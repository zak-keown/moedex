# Moedex Coordination Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver Stage D supervised continuation, progress observation, durable rooms and receipts, relay guards, boards, attention, named draft-safe messaging, completion proposals, and event-driven job wakeups for O4, O5, O7-O12, P1, P2, and C15.

**Architecture:** Add a focused `codex-coordination` crate for room/envelope/receipt state machines and bounded delivery rules, with durable SQLite tables behind narrow `codex-state` modules. Core adapts existing agent communication, goal generation, and unified exec events into those domain services; app-server v2 and focused TUI components provide user controls, boards, receipts, and attention without expanding `chatwidget.rs` or treating agent messages as user turns.

**Tech Stack:** Rust 2024, Tokio channels/watch, serde, SQLx/SQLite, existing agent graph/input queue/goal/unified-exec abstractions, app-server v2 with ts-rs/schemars, ratatui/insta, Bazel.

**Spec:** `specs/moedex/SPEC.md`, `specs/moedex/EXECUTION.md`, `specs/moedex/EXPERIENCE.md`, and `specs/moedex/RESEARCH.md`

## Global Constraints

- Stage D depends on Stage B durable input/run generations and Stage C snapshot, budget, evidence, and R6 progress interfaces; do not duplicate those contracts.
- Preserve Linux, macOS, Windows, and supported cross-OS app-server/exec-server behavior.
- Do not modify `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR` code.
- New model-visible fragments must be typed `ContextualUserFragment` implementations, default to at most 1,000 tokens each, stay under 4,000 newly injected tokens per turn across Moedex enhancements, and never exceed 10,000 tokens.
- Agent-originated messages and job events remain typed agent/tool-origin context and never become synthetic user-role prompts.
- Keep domain/storage logic outside `codex-core`; keep focused modules below 500 LoC where practical and add no standalone behavior methods to `codex-rs/tui/src/chatwidget.rs`.
- Persist acceptance before acknowledgement; replay cannot start one logical task twice, but uncertain external effects remain explicitly uncertain rather than claimed exactly once.
- Default bounds are 32 room members, 100 pending envelopes/target, 24-hour pending TTL, 4 relay hops, 30-day dedup retention, and 16 KiB stored coordination payloads.
- Boards are capped at 100/workspace and 1,000 memberships/board; list APIs return at most 100 items/page and never delete conversations when membership is removed.
- Monitor defaults are 8 subscriptions/thread, 256-byte patterns, 24-hour lifetime, 4 KiB retained event text, one wake/second/subscription, and 60 wakes/hour/thread; suppression/coalescing is visible.
- New app-server methods are v2, camelCase on the wire, cursor-paginated for lists, `#[ts(optional = nullable)]` on optional request fields, and experimental-gated; regenerate stable and experimental schemas after each API slice.
- User-visible UI changes require reviewed `insta` snapshots, including narrow terminals; status-only caches remain bounded and durable completion events do not use lossy UI queues.
- Use `just test`, never direct `cargo test`; run `just fmt` after source changes, and run scoped `just fix -p <project>` before finalizing without rerunning tests afterward.
- Any Cargo dependency change requires `just bazel-lock-update` from `codex-rs` and the resulting `MODULE.bazel.lock` in the same commit.

## Open Decisions

None. The specification fixes the first transport as local durable coordination within a Moedex home/project scope; remote/cloud transport can implement the same storage-neutral interface in a later plan.

## Not Yet Specified

None for Stage D. Cross-machine federation and shared hosted room discovery are future transport choices beyond this stage's local cross-root-session contract.

## Out of Scope

- Cross-harness handoff packages (P4), alternate provider adapters (C11), and research readers/context stores (Stage E/F).
- Automatically relaying replies, granting authority through room membership, or starting work merely because a proposal was generated.
- A new conversation store or scheduler hidden behind boards; boards reference existing thread IDs only.
- Starting commands from monitor subscriptions; subscriptions observe already-authorized unified exec sessions.

---

### Task 1: Coordination Domain State Machine and Durable Store

**Files:**
- Create: `codex-rs/coordination/Cargo.toml`
- Create: `codex-rs/coordination/BUILD.bazel`
- Create: `codex-rs/coordination/src/lib.rs`
- Create: `codex-rs/coordination/src/envelope.rs`
- Create: `codex-rs/coordination/src/delivery.rs`
- Create: `codex-rs/coordination/src/delivery_tests.rs`
- Create: `codex-rs/state/migrations/0059_coordination.sql`
- Create: `codex-rs/state/src/model/coordination.rs`
- Create: `codex-rs/state/src/runtime/coordination.rs`
- Create: `codex-rs/state/src/runtime/coordination_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/Cargo.toml`
- Modify: `codex-rs/state/BUILD.bazel`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/MODULE.bazel.lock`

**Interfaces:**
- Consumes: Stage B `RunId`/execution generation, existing `ThreadId`/`AgentPath`, project identity, and Stage C task/budget identity.
- Produces: `RoomId`, `ParticipantId`, `EnvelopeId`, `CoordinationEnvelope`, `CoordinationKind`, `Target`, `ReceiptState`, `DeliveryDecision`, `CoordinationPolicy`, `CoordinationStore` trait, and `StateRuntime` implementation methods for rooms, memberships, envelopes, receipts, readiness, dedup, expiry, and pagination.

- [ ] **Step 1: Write state-machine tests for readiness, replay, hops, and authority**

```rust
#[test]
fn message_waits_for_readiness_and_replayed_task_starts_once() {
    let mut delivery = DeliveryMachine::new(CoordinationPolicy::default());
    let envelope = task_envelope("env-1", "room-1", "sender", "target", 1);
    assert_eq!(delivery.accept(envelope.clone(), TargetReadiness::Disconnected), DeliveryDecision::Queued);
    assert_eq!(delivery.ready("target"), vec![DeliveryAction::Deliver("env-1".into())]);
    delivery.record("env-1", ReceiptState::Started).unwrap();
    assert_eq!(delivery.accept(envelope, TargetReadiness::Ready), DeliveryDecision::Duplicate(ReceiptState::Started));
}

#[test]
fn reply_and_over_limit_action_cannot_form_a_relay_loop() {
    assert_eq!(reply_envelope(4).next_relay(), Err(DeliveryError::ReplyRelayForbidden));
    assert_eq!(message_envelope(4).next_relay(), Err(DeliveryError::HopLimitExceeded { limit: 4 }));
}
```

- [ ] **Step 2: Run the new crate test and verify failure**

Run: `cd codex-rs && just test -p codex-coordination`

Expected: FAIL because package `codex-coordination` does not exist.

- [ ] **Step 3: Implement exhaustive envelope and receipt types**

```rust
pub enum CoordinationKind {
    Message { body: String },
    Task { objective: String, task_id: String },
    Action { action: AuthorizedAction },
    Reply { correlation_id: EnvelopeId, body: String },
}

pub enum ReceiptState {
    Persisted,
    Delivered,
    Accepted,
    Started,
    Completed,
    Rejected { reason: ReceiptRejection },
    Expired,
}

pub trait CoordinationStore: Send + Sync {
    fn persist_envelope(&self, envelope: CoordinationEnvelope) -> impl Future<Output = Result<Vec<DeliveryReceipt>, CoordinationError>> + Send;
    fn mark_ready(&self, participant: ParticipantId, generation: u64) -> impl Future<Output = Result<Vec<CoordinationEnvelope>, CoordinationError>> + Send;
    fn advance_receipt(&self, receipt: DeliveryReceipt) -> impl Future<Output = Result<DeliveryReceipt, CoordinationError>> + Send;
}
```

Validate 16 KiB payloads, 4 hops, expiry, room/project scope, immutable broadcast target snapshots, monotonic per-sender sequence, target generation, and explicit action authority before persistence. Reply envelopes cannot be transformed into tasks or relayed automatically.

- [ ] **Step 4: Add transactional room/envelope/receipt/dedup tables**

```sql
CREATE TABLE coordination_rooms (
    room_id TEXT PRIMARY KEY,
    project_id TEXT NOT NULL,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL,
    created_at INTEGER NOT NULL
);
CREATE TABLE coordination_memberships (
    room_id TEXT NOT NULL,
    participant_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    display_name TEXT NOT NULL,
    readiness TEXT NOT NULL,
    generation INTEGER NOT NULL,
    last_sequence INTEGER NOT NULL,
    PRIMARY KEY(room_id, participant_id)
);
CREATE TABLE coordination_envelopes (
    envelope_id TEXT PRIMARY KEY,
    room_id TEXT NOT NULL,
    sender_id TEXT NOT NULL,
    generation INTEGER NOT NULL,
    sequence INTEGER NOT NULL,
    expires_at INTEGER NOT NULL,
    envelope_json TEXT NOT NULL,
    UNIQUE(room_id, sender_id, generation, sequence)
);
CREATE TABLE coordination_receipts (
    envelope_id TEXT NOT NULL,
    target_id TEXT NOT NULL,
    state TEXT NOT NULL,
    updated_at INTEGER NOT NULL,
    detail_json TEXT NOT NULL,
    PRIMARY KEY(envelope_id, target_id)
);
```

Use one immediate transaction to validate room/member caps, persist the envelope, snapshot broadcast targets, and create `Persisted` receipts before returning success. Expiry updates pending receipts visibly; retain dedup keys for 30 days after terminal status, after which replay is rejected as too old rather than admitted as new.

- [ ] **Step 5: Register Cargo/Bazel metadata and refresh lock**

Run: `cd codex-rs && just bazel-lock-update`

Expected: the workspace resolves `codex-coordination`, state migration compile data includes `0059`, and lockfiles are current.

- [ ] **Step 6: Run tests and format**

Run: `cd codex-rs && just test -p codex-coordination && just test -p codex-state coordination`

Expected: PASS for disconnect/reconnect, project isolation, renamed identity, broadcast correlation, duplicate/out-of-order delivery, crash after persistence, readiness, authorization, backpressure, hops, expiry, and retention.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/Cargo.toml codex-rs/MODULE.bazel.lock codex-rs/coordination codex-rs/state
git commit -m "feat(coordination): persist rooms envelopes and receipts"
```

### Task 2: Core Room Delivery and Typed Agent-Origin Context

**Files:**
- Create: `codex-rs/core/src/coordination/mod.rs`
- Create: `codex-rs/core/src/coordination/service.rs`
- Create: `codex-rs/core/src/coordination/service_tests.rs`
- Create: `codex-rs/core/src/context/coordination.rs`
- Create: `codex-rs/core/tests/suite/coordination_delivery.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Modify: `codex-rs/core/src/lib.rs`
- Modify: `codex-rs/core/src/session/services.rs`
- Modify: `codex-rs/core/src/session/input_queue.rs`
- Modify: `codex-rs/core/src/session/turn_input.rs`
- Modify: `codex-rs/core/src/tools/handlers/multi_agents_v2/message_tool.rs`
- Modify: `codex-rs/protocol/src/protocol.rs`
- Modify: `codex-rs/protocol/src/turn_input.rs`
- Modify: `codex-rs/core/Cargo.toml`
- Modify: `codex-rs/core/BUILD.bazel`

**Interfaces:**
- Consumes: Task 1 `CoordinationStore`, envelopes/receipts/readiness; existing `InterAgentCommunication`, `InputQueue`, thread manager, agent graph, and run generation.
- Produces: `CoordinationService::{join_room,leave_room,set_readiness,send,activate_pending}`, `TurnInput::Coordination`, and bounded `CoordinationContextFragment`.

- [ ] **Step 1: Write integration tests around crash replay and typed origin**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
async fn durable_task_replay_activates_once_as_agent_origin() -> anyhow::Result<()> {
    let test = test_codex().build_with_auto_env().await?;
    let envelope = test.persist_task_envelope("env-1", "target").await?;
    test.crash_target_after_receipt(envelope.id()).await?;
    test.resume_target().await?;
    let requests = test.target_model_requests().await;
    assert_eq!(requests.len(), 1);
    assert!(requests[0].contains_context_kind("coordination"));
    assert!(!requests[0].contains_user_text(envelope.body()));
    Ok(())
}
```

- [ ] **Step 2: Run the core integration test and verify failure**

Run: `cd codex-rs && just test -p codex-core durable_task_replay_activates_once_as_agent_origin`

Expected: FAIL because durable coordination activation does not exist.

- [ ] **Step 3: Implement the service as the single state transition owner**

```rust
pub(crate) async fn activate_pending(
    &self,
    participant: ParticipantId,
    generation: u64,
) -> Result<Vec<EnvelopeId>, CoordinationActivationError> {
    let envelopes = self.store.mark_ready(participant, generation).await?;
    let mut activated = Vec::new();
    for envelope in envelopes {
        if self.store.claim_task_start(&envelope.id, generation).await? {
            self.input_queue.push_coordination(envelope.clone()).await?;
            activated.push(envelope.id);
        }
    }
    Ok(activated)
}
```

Message delivery queues context without stealing an active turn. Task delivery starts only after a durable `Started` claim. Cancellation/supersession makes old generation delivery terminal. Action delivery checks target-side authority immediately before activation as well as at admission.

- [ ] **Step 4: Add bounded provenance-bearing context**

```rust
pub struct CoordinationContextFragment {
    pub envelope_id: EnvelopeId,
    pub room_id: RoomId,
    pub sender: ParticipantSummary,
    pub kind: CoordinationContextKind,
    pub body: String,
    pub correlation_id: Option<EnvelopeId>,
}

impl ContextualUserFragment for CoordinationContextFragment {
    fn context_kind(&self) -> ContextKind { ContextKind::Coordination }
}
```

Render sender/room provenance, origin kind, and correlation identity within the 1,000-token fragment default. Never construct `UserInput::Text` for the message body. Append receipt state changes as deltas instead of rewriting the delivered item.

- [ ] **Step 5: Route existing same-tree messaging through the coordination adapter**

Keep current `send_message` and `followup_task` tool names and target resolution for parent/child paths, but persist an envelope and return correlated receipt status. Existing callers receive equivalent success behavior; cross-root room participants use the same service without requiring an agent-graph parent edge.

```rust
let receipts = session.services.coordination.send(CoordinationSendRequest {
    sender,
    room_id,
    target: Target::Participant(receiver_id),
    kind: delivery_mode.into_kind(message),
    generation: turn.execution_generation(),
}).await?;
Ok(FunctionToolOutput::from_text(receipt_summary(receipts), Some(true)))
```

- [ ] **Step 6: Run tests and format**

Run: `cd codex-rs && just test -p codex-core coordination_delivery && just test -p codex-core multi_agents_v2`

Expected: PASS for queued-before-ready messages, exactly one task start after replay, unauthorized action rejection, typed origin, stale generation, and existing parent/child messaging.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/core codex-rs/protocol
git commit -m "feat(core): deliver durable typed coordination messages"
```

### Task 3: Room and Messaging App-Server v2 API

**Files:**
- Create: `codex-rs/app-server-protocol/src/protocol/v2/coordination.rs`
- Create: `codex-rs/app-server/src/request_processors/coordination_processor.rs`
- Create: `codex-rs/app-server/tests/suite/v2/coordination.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`
- Modify: `codex-rs/app-server/src/message_processor.rs`
- Modify: `codex-rs/app-server/src/request_processors/mod.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/schema/typescript/v2/`
- Modify: `codex-rs/app-server-protocol/schema/json/`

**Interfaces:**
- Consumes: Task 2 `CoordinationService` and Task 1 room/envelope/receipt types.
- Produces: experimental v2 methods `room/create`, `room/join`, `room/leave`, `room/list`, `room/member/list`, `coordination/send`, `coordination/receipt/list`; notifications `room/changed` and `coordination/receipt/changed`.

- [ ] **Step 1: Write JSON-RPC tests for scoped name resolution and broadcast receipts**

```rust
#[tokio::test]
async fn send_resolves_names_inside_one_room_and_broadcast_snapshots_targets() {
    let server = TestAppServer::builder().build().await;
    let room = server.create_room("project-a", "reviewers").await;
    server.join(room.id(), participant("p1", "alex")).await;
    server.join(room.id(), participant("p2", "alex")).await;
    let ambiguous = server.send_request_error("coordination/send", json!({
        "roomId": room.id(), "target": { "type": "name", "name": "alex" },
        "kind": { "type": "message", "body": "status?" }
    })).await;
    assert_eq!(ambiguous.code, INVALID_PARAMS);
    let sent = server.broadcast(room.id(), "status?").await;
    assert_eq!(sent.receipts.len(), 2);
}
```

- [ ] **Step 2: Run the app-server test and verify failure**

Run: `cd codex-rs && just test -p codex-app-server send_resolves_names_inside_one_room_and_broadcast_snapshots_targets`

Expected: FAIL with unknown coordination methods.

- [ ] **Step 3: Define discriminated v2 types and route requests**

```rust
#[derive(Serialize, Deserialize, JsonSchema, TS, ExperimentalApi)]
#[serde(rename_all = "camelCase")]
#[ts(export_to = "v2/")]
pub struct CoordinationSendParams {
    pub room_id: String,
    pub target: CoordinationTarget,
    pub kind: CoordinationPayload,
    #[ts(optional = nullable)]
    pub expires_at: Option<i64>,
}

#[derive(Serialize, Deserialize, JsonSchema, TS)]
#[serde(tag = "type", rename_all = "camelCase")]
#[ts(tag = "type", rename_all = "camelCase", export_to = "v2/")]
pub enum CoordinationTarget {
    Participant { participant_id: String },
    Name { name: String },
    Broadcast,
}
```

Resolve names only within `roomId`; return candidates on ambiguity without sending. Broadcast target membership is frozen in the persistence transaction and returns per-target receipts including unavailable/disconnected outcomes. Stable participant ID survives rename.

- [ ] **Step 4: Regenerate schemas and inspect experimental gates**

Run: `cd codex-rs && just write-app-server-schema && just write-app-server-schema --experimental`

Expected: methods are v2-only and experimental; list payloads use `data`/`nextCursor`, optional params are nullable, and union tags/renames match in Rust and TS.

- [ ] **Step 5: Run tests and format**

Run: `cd codex-rs && just test -p codex-app-server-protocol && just test -p codex-app-server coordination`

Expected: PASS for same-name isolation across projects, ambiguity, rename, broadcast receipts, unavailable targets, project scope, expiry, and notifications.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/app-server-protocol codex-rs/app-server
git commit -m "feat(app-server): expose durable room coordination"
```

### Task 4: Boards Over Existing Threads

**Files:**
- Create: `codex-rs/state/migrations/0060_boards.sql`
- Create: `codex-rs/state/src/model/board.rs`
- Create: `codex-rs/state/src/runtime/boards.rs`
- Create: `codex-rs/state/src/runtime/boards_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Create: `codex-rs/app-server-protocol/src/protocol/v2/board.rs`
- Create: `codex-rs/app-server/src/request_processors/board_processor.rs`
- Create: `codex-rs/app-server/tests/suite/v2/board.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`
- Modify: `codex-rs/app-server/src/message_processor.rs`
- Modify: `codex-rs/app-server/src/request_processors/mod.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/schema/typescript/v2/`
- Modify: `codex-rs/app-server-protocol/schema/json/`

**Interfaces:**
- Consumes: existing thread/project identity and thread read/list APIs; no conversation storage mutation.
- Produces: `Board`, `BoardMembership`, revision-checked state APIs, and experimental v2 `board/create`, `board/update`, `board/delete`, `board/list`, `board/card/add`, `board/card/move`, `board/card/remove`, `board/card/list`.

- [ ] **Step 1: Write storage tests for multi-board membership and conflicts**

```rust
#[tokio::test]
async fn removing_one_card_keeps_other_membership_and_thread_history() {
    let db = test_runtime().await;
    let thread = db.create_test_thread("thread-1").await;
    let left = db.create_board(new_board("left")).await.unwrap();
    let right = db.create_board(new_board("right")).await.unwrap();
    db.add_board_card(left.id(), thread.id(), left.revision()).await.unwrap();
    db.add_board_card(right.id(), thread.id(), right.revision()).await.unwrap();
    db.remove_board_card(left.id(), thread.id(), left.revision() + 1).await.unwrap();
    assert_eq!(db.list_board_cards(right.id(), None, 100).await.unwrap().data.len(), 1);
    assert!(db.get_thread(thread.id()).await.unwrap().is_some());
}

#[tokio::test]
async fn concurrent_reorder_reports_revision_conflict() {
    let db = seeded_board().await;
    let revision = db.board("board-1").await.unwrap().revision;
    db.move_board_card("board-1", "thread-2", None, revision).await.unwrap();
    assert!(matches!(
        db.move_board_card("board-1", "thread-1", None, revision).await,
        Err(BoardError::RevisionConflict { .. })
    ));
}
```

- [ ] **Step 2: Run state tests and verify failure**

Run: `cd codex-rs && just test -p codex-state boards`

Expected: FAIL because board tables/APIs do not exist.

- [ ] **Step 3: Implement bounded independent board tables and transactions**

```sql
CREATE TABLE boards (
    board_id TEXT PRIMARY KEY,
    workspace_id TEXT NOT NULL,
    name TEXT NOT NULL,
    revision INTEGER NOT NULL,
    metadata_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
);
CREATE TABLE board_memberships (
    board_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    position INTEGER NOT NULL,
    search_metadata_json TEXT NOT NULL,
    seen_sequence INTEGER NOT NULL DEFAULT 0,
    PRIMARY KEY(board_id, thread_id)
);
```

Enforce 100 boards/workspace and 1,000 cards/board before insert. Each mutation requires `expected_revision`; use `BEGIN IMMEDIATE`, increment once, and return explicit current revision on conflict. Card removal/deletion touches only board tables, never threads, rollouts, sections, or project assignment.

- [ ] **Step 4: Add the v2 board API and notifications**

```rust
pub struct BoardCardMoveParams {
    pub board_id: String,
    pub thread_id: String,
    #[ts(optional = nullable)]
    pub before_thread_id: Option<String>,
    #[ts(type = "number")]
    pub expected_revision: i64,
}
```

Return `revisionConflict` structured error data containing current revision. Cursor pages cap at 100. Search uses persisted metadata plus authoritative thread metadata; it does not copy transcripts into board storage.

- [ ] **Step 5: Regenerate schemas, test, and format**

Run: `cd codex-rs && just write-app-server-schema && just write-app-server-schema --experimental`

Run: `cd codex-rs && just test -p codex-state boards && just test -p codex-app-server-protocol && just test -p codex-app-server board`

Expected: PASS for two-board membership, rename/reorder/remove/restart, full transcript retention, explicit concurrent conflict, caps, pagination, and notifications.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/state codex-rs/app-server-protocol codex-rs/app-server
git commit -m "feat(boards): organize existing threads with revision checks"
```

### Task 5: Durable Attention Inbox and Live Reconciliation

**Files:**
- Create: `codex-rs/state/migrations/0061_attention.sql`
- Create: `codex-rs/state/src/model/attention.rs`
- Create: `codex-rs/state/src/runtime/attention.rs`
- Create: `codex-rs/state/src/runtime/attention_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Create: `codex-rs/app-server-protocol/src/protocol/v2/attention.rs`
- Create: `codex-rs/app-server/src/request_processors/attention_processor.rs`
- Create: `codex-rs/app-server/tests/suite/v2/attention.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`
- Modify: `codex-rs/app-server/src/message_processor.rs`
- Modify: `codex-rs/app-server/src/request_processors/mod.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/schema/typescript/v2/`
- Modify: `codex-rs/app-server-protocol/schema/json/`

**Interfaces:**
- Consumes: Task 1 receipt events, Task 4 board memberships, Stage B authoritative run lifecycle, Stage C evidence/review results, and existing approval resolution.
- Produces: `AttentionEvent`, `AttentionKind`, `Liveness::{Live,Stopped,Reconciling,Unknown}`, per-membership seen cursors, `AttentionReconciler`, and v2 `attention/list`, `attention/markSeen`, `attention/reconcile` plus `attention/changed`.

- [ ] **Step 1: Write race and crash-reconciliation tests**

```rust
#[tokio::test]
async fn marking_old_sequence_seen_does_not_clear_new_result() {
    let db = seeded_attention().await;
    db.append_attention(result_event(11)).await.unwrap();
    db.mark_board_seen("board-1", "thread-1", 10, 3).await.unwrap();
    let page = db.list_attention("board-1", None, 100).await.unwrap();
    assert_eq!(page.data.iter().filter(|item| item.unread).map(|item| item.sequence).collect::<Vec<_>>(), vec![11]);
}

#[tokio::test]
async fn stale_running_state_is_unknown_until_reconciled() {
    let db = crashed_active_thread().await;
    assert_eq!(db.attention_liveness("thread-1").await.unwrap(), Liveness::Reconciling);
    db.record_reconciliation("thread-1", ReconciledStatus::Unknown).await.unwrap();
    assert_eq!(db.attention_liveness("thread-1").await.unwrap(), Liveness::Unknown);
}
```

- [ ] **Step 2: Run state tests and verify failure**

Run: `cd codex-rs && just test -p codex-state attention`

Expected: FAIL because attention APIs are absent.

- [ ] **Step 3: Implement append-only attention events and per-membership seen cursors**

```sql
CREATE TABLE attention_events (
    event_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    sequence INTEGER NOT NULL,
    kind TEXT NOT NULL,
    event_json TEXT NOT NULL,
    created_at INTEGER NOT NULL,
    UNIQUE(thread_id, sequence)
);
CREATE TABLE attention_seen (
    board_id TEXT NOT NULL,
    thread_id TEXT NOT NULL,
    membership_revision INTEGER NOT NULL,
    seen_sequence INTEGER NOT NULL,
    PRIMARY KEY(board_id, thread_id)
);
```

Project actual unresolved approvals, errors, active/paused/reconciling work, unread results, verification failures, and coordination outcomes from durable events. Treat a stored running flag as a hint requiring live reconciliation. Mark seen with `MAX(existing, requested)` only for the addressed board membership/revision.

- [ ] **Step 4: Expose paginated v2 queries and authoritative reconciliation**

```rust
pub struct AttentionListParams {
    pub board_id: String,
    #[ts(optional = nullable)]
    pub cursor: Option<String>,
    #[ts(optional = nullable)]
    pub limit: Option<u32>,
    #[ts(optional = nullable)]
    pub kinds: Option<Vec<AttentionKind>>,
}
```

`attention/reconcile` asks thread manager, exec process manager, queue/run store, and approval holders for current state, then appends the resolution. Already-resolved approvals disappear on synchronization; unavailable owners remain `Unknown`, never failed or live by assumption.

- [ ] **Step 5: Regenerate schemas, test, and format**

Run: `cd codex-rs && just write-app-server-schema && just write-app-server-schema --experimental`

Run: `cd codex-rs && just test -p codex-state attention && just test -p codex-app-server-protocol && just test -p codex-app-server attention`

Expected: PASS for crash/reconnect, unknown-to-authoritative transitions, result/seen race, per-board isolation, resolved approvals, pagination, duplicate events, and room disconnect not equaling failure.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/state codex-rs/app-server-protocol codex-rs/app-server
git commit -m "feat(attention): aggregate durable parallel-work status"
```

### Task 6: Draft-Safe Messaging and Attention TUI

**Files:**
- Create: `codex-rs/tui/src/coordination.rs`
- Create: `codex-rs/tui/src/coordination_tests.rs`
- Create: `codex-rs/tui/src/attention_panel.rs`
- Create: `codex-rs/tui/src/attention_panel_tests.rs`
- Create: `codex-rs/tui/src/snapshots/codex_tui__coordination_tests__draft_safe_message.snap`
- Create: `codex-rs/tui/src/snapshots/codex_tui__attention_panel_tests__narrow_attention.snap`
- Modify: `codex-rs/tui/src/lib.rs`
- Modify: `codex-rs/tui/src/app.rs`
- Modify: `codex-rs/tui/src/app_event.rs`
- Modify: `codex-rs/tui/src/app_event_sender.rs`
- Modify: `codex-rs/tui/src/bottom_pane/mod.rs`
- Modify: `codex-rs/tui/src/keymap.rs`

**Interfaces:**
- Consumes: Tasks 3 and 5 v2/domain events, existing composer draft snapshot/recovery, modal routing, interruption path, and ratatui wrapping/style helpers.
- Produces: `/room`, `/message`, `/broadcast`, and `/attention` actions; `CoordinationInboxState`; `AttentionPanel`; draft-preserving explicit interrupt action.

- [ ] **Step 1: Write interaction and snapshot tests across editing/modal/running states**

```rust
#[test]
fn ordinary_incoming_message_never_mutates_or_submits_the_draft() {
    for state in [UiState::Editing, UiState::PickerOpen, UiState::TurnRunning] {
        let mut app = app_with_state(state, "unsent α\nsecond line");
        app.handle_coordination(message_event("env-1", "review ready"));
        assert_eq!(app.draft_snapshot().text, "unsent α\nsecond line");
        assert_eq!(app.submitted_turn_count(), 0);
        assert_eq!(app.coordination_inbox().unread_count(), 1);
    }
}

#[test]
fn narrow_attention_panel_shows_unknown_and_receipt_state() {
    let panel = AttentionPanel::new(attention_fixture());
    insta::assert_snapshot!(render(panel, 42, 12));
}
```

- [ ] **Step 2: Run TUI tests and verify failure**

Run: `cd codex-rs && just test -p codex-tui coordination_tests && just test -p codex-tui attention_panel_tests`

Expected: FAIL because the focused UI components do not exist.

- [ ] **Step 3: Implement named send/broadcast commands and receipt views**

```rust
pub enum CoordinationAction {
    Send { room_id: RoomId, target: NameOrId, body: String },
    Broadcast { room_id: RoomId, body: String },
    Interrupt { envelope_id: EnvelopeId },
    OpenAttention { board_id: BoardId },
}
```

Name resolution errors open a disambiguation picker listing stable participant IDs and room scope. Broadcast confirmation shows the frozen target count and later per-target receipts. Use existing `word_wrap_lines`, `prefix_lines`, and `Stylize`; keep ordinary messages in a badge/inbox while draft or modal state is active.

- [ ] **Step 4: Preserve draft bytes through explicit authorized interrupt**

```rust
let saved = self.bottom_pane.draft_snapshot();
self.submit_existing_interrupt(envelope_id).await?;
self.bottom_pane.restore_draft(saved);
```

The interrupt action uses the existing authorization/interruption path. Save and restore text, cursor, pending paste metadata, and admitted attachments. Sender task cancellation cannot revoke or mutate the user's accepted draft. When readiness returns, claim activation once through Task 2.

- [ ] **Step 5: Generate, inspect, and accept snapshots**

Run: `cd codex-rs && just test -p codex-tui coordination_tests && just test -p codex-tui attention_panel_tests`

Run: `cd codex-rs && cargo insta pending-snapshots -p codex-tui`

Review every `*.snap.new` for editing, picker, running, ambiguous name, broadcast receipts, unknown liveness, stale evidence, and 42-column rendering.

Run: `cd codex-rs && cargo insta accept -p codex-tui`

Expected: no pending snapshots remain and draft bytes are unchanged in all interaction states.

- [ ] **Step 6: Format and commit**

Run: `cd codex-rs && just fmt`

```bash
git add codex-rs/tui
git commit -m "feat(tui): add draft-safe messaging and attention"
```

### Task 7: Supervised Continuation and Completion Proposals

**Files:**
- Create: `codex-rs/core/src/continuation/mod.rs`
- Create: `codex-rs/core/src/continuation/supervision.rs`
- Create: `codex-rs/core/src/continuation/supervision_tests.rs`
- Create: `codex-rs/state/migrations/0062_followup_proposals.sql`
- Create: `codex-rs/state/src/model/followup_proposal.rs`
- Create: `codex-rs/state/src/runtime/followup_proposals.rs`
- Create: `codex-rs/state/src/runtime/followup_proposals_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Modify: `codex-rs/core/src/session/handlers.rs`
- Modify: `codex-rs/core/src/tasks/mod.rs`
- Modify: `codex-rs/protocol/src/protocol.rs`
- Create: `codex-rs/app-server-protocol/src/protocol/v2/continuation.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/v2/mod.rs`
- Modify: `codex-rs/app-server-protocol/src/protocol/common.rs`

**Interfaces:**
- Consumes: Stage B `RunId`, objective revision, execution generation and continuation events; Stage C budgets/evidence; existing goal and input admission.
- Produces: `ContinuationMode::{Automatic,Preview,Manual}`, `ContinuationProposal`, `ContinuationDecision`, `FollowupProposal`, replay-safe proposal persistence, and v2 `continuation/respond`, `followupProposal/list`, `followupProposal/update`.

- [ ] **Step 1: Write frozen-clock generation and replay tests**

```rust
#[tokio::test(start_paused = true)]
async fn edited_preview_invalidates_old_countdown_event() {
    let supervisor = test_supervisor(ContinuationMode::Preview, Duration::from_secs(5));
    let first = supervisor.propose(proposal("p1", 7, "run tests")).await.unwrap();
    supervisor.edit(first.id(), "run focused tests", 8).await.unwrap();
    tokio::time::advance(Duration::from_secs(5)).await;
    assert_eq!(supervisor.dispatched_actions().await, vec![action("run focused tests", 8)]);
}

#[tokio::test]
async fn replayed_completion_creates_one_proposal_and_no_task() {
    let db = test_runtime().await;
    db.record_completion(completed("event-1", "objective-3")).await.unwrap();
    db.record_completion(completed("event-1", "objective-3")).await.unwrap();
    assert_eq!(db.list_followup_proposals("objective-3").await.unwrap().len(), 1);
    assert_eq!(db.started_task_count("objective-3").await.unwrap(), 0);
}
```

- [ ] **Step 2: Run core/state tests and verify failure**

Run: `cd codex-rs && just test -p codex-core supervision && just test -p codex-state followup_proposals`

Expected: FAIL because supervision/proposal interfaces are absent.

- [ ] **Step 3: Implement generation-safe continuation modes**

```rust
pub enum ContinuationDecision {
    Advance { proposal_id: ProposalId, revision: u64 },
    Edit { proposal_id: ProposalId, revision: u64, action: ProposedAction },
    Pause { proposal_id: ProposalId, revision: u64 },
    Cancel { proposal_id: ProposalId, revision: u64 },
}
```

Automatic retains existing continuation semantics and adds no approval. Preview schedules one five-second event keyed by proposal/revision/generation; edit/pause/cancel invalidates older callbacks. Manual schedules no timer. Accepted preview continues through ordinary permission checks and cannot grant action authority.

- [ ] **Step 4: Persist at most one follow-up proposal per completed objective revision**

```sql
CREATE TABLE followup_proposals (
    proposal_id TEXT PRIMARY KEY,
    completion_event_id TEXT NOT NULL UNIQUE,
    objective_id TEXT NOT NULL,
    objective_revision INTEGER NOT NULL,
    proposal_revision INTEGER NOT NULL,
    status TEXT NOT NULL,
    proposal_json TEXT NOT NULL,
    UNIQUE(objective_id, objective_revision)
);
```

Proposal generation is distinct from input/task admission. An authorized backlog runner may select only an item already within its declared scope and remaining token/time/spend limits. Use optimistic proposal revision checks so late generated text cannot overwrite user edits.

- [ ] **Step 5: Add v2 controls and regenerate schemas**

```rust
pub struct ContinuationRespondParams {
    pub thread_id: String,
    pub proposal_id: String,
    #[ts(type = "number")]
    pub expected_revision: i64,
    pub decision: ContinuationDecision,
}
```

Run: `cd codex-rs && just write-app-server-schema && just write-app-server-schema --experimental`

- [ ] **Step 6: Run tests and format**

Run: `cd codex-rs && just test -p codex-core supervision && just test -p codex-state followup_proposals && just test -p codex-app-server-protocol continuation`

Expected: PASS for old timer invalidation, manual wait, automatic unchanged behavior, replay, budget exhaustion, scope rejection, and user-edit conflict.

Run: `cd codex-rs && just fmt`

- [ ] **Step 7: Commit**

```bash
git add codex-rs/core codex-rs/state codex-rs/protocol codex-rs/app-server-protocol
git commit -m "feat(orchestration): supervise continuation and propose follow-ups"
```

### Task 8: Advisory Progress Observer

**Files:**
- Create: `codex-rs/core/src/observer.rs`
- Create: `codex-rs/core/src/observer_tests.rs`
- Create: `codex-rs/core/tests/suite/progress_observer.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Modify: `codex-rs/core/src/session/services.rs`
- Modify: `codex-rs/core/src/session/turn.rs`
- Modify: `codex-rs/core/src/tasks/mod.rs`
- Modify: `codex-rs/protocol/src/protocol.rs`
- Modify: `codex-rs/features/src/feature_configs.rs`
- Modify: `codex-rs/core/src/config/mod.rs`
- Modify: `codex-rs/core/src/config/config_tests.rs`
- Modify: `codex-rs/core/config.schema.json`

**Interfaces:**
- Consumes: Stage C `ProgressDetector`, evidence records, usage attribution/reservations, Stage B run event sequence, and objective revision.
- Produces: `ObserverPolicy`, `ProgressObservation`, `ObserverFinding::{ObjectiveDrift,MissingVerification,RepetitiveFailure}`, and append-only `ProgressObserverEvent`.

- [ ] **Step 1: Write integration tests proving advisory and bounded behavior**

```rust
#[tokio::test(start_paused = true)]
async fn observer_cites_missing_test_without_mutating_goal_or_dispatching() -> anyhow::Result<()> {
    let test = test_codex().observer_enabled().build_with_auto_env().await?;
    test.emit_completed_tool_operations(10).await?;
    let finding = test.wait_for_observer_finding().await?;
    assert_eq!(finding.kind, ObserverFindingKind::MissingVerification);
    assert_eq!(finding.event_sequences, vec![3, 5, 8, 10]);
    assert_eq!(test.goal_revision().await?, 1);
    assert_eq!(test.observer_tool_dispatch_count(), 0);
    Ok(())
}
```

- [ ] **Step 2: Run the integration test and verify failure**

Run: `cd codex-rs && just test -p codex-core progress_observer`

Expected: FAIL because observer scheduling/events are absent.

- [ ] **Step 3: Implement debounced, budgeted observation scheduling**

```rust
pub struct ObserverPolicy {
    pub enabled: bool,
    pub operations_between_observations: u32,
    pub minimum_interval: Duration,
    pub max_observations_per_run: u32,
}

impl Default for ObserverPolicy {
    fn default() -> Self {
        Self {
            enabled: false,
            operations_between_observations: 10,
            minimum_interval: Duration::from_secs(120),
            max_observations_per_run: 3,
        }
    }
}
```

Atomically claim observation number before issuing a model request so duplicate events do not overrun the budget. Bound input to cited event summaries/evidence IDs. Attribute tokens/cost to the owning run. Expose no tool runtime to the observer and no goal/budget mutation handle. Disable cancels pending scheduling and stale generation results remain inspectable without affecting the new run.

- [ ] **Step 4: Validate config and append bounded findings**

```rust
pub struct ProgressObservation {
    pub observation_id: String,
    pub run_id: RunId,
    pub objective_revision: u64,
    pub event_sequences: Vec<u64>,
    pub findings: Vec<ObserverFinding>,
    pub usage_request_id: String,
}
```

Cap three findings/observation, 12 cited events/finding, and 1,000 model-visible tokens. Findings are advisory events shown through Task 5 attention; they cannot pause or continue a run. Invalid zero/negative intervals fail config load.

- [ ] **Step 5: Regenerate config schema, test, and format**

Run: `cd codex-rs && just write-config-schema`

Run: `cd codex-rs && just test -p codex-core progress_observer`

Expected: PASS for missing verification, objective drift, R6 repetition evidence, duplicate events, two-minute debounce, three-observation cap, usage attribution, disable cancellation, and unchanged authority.

Run: `cd codex-rs && just fmt`

- [ ] **Step 6: Commit**

```bash
git add codex-rs/core codex-rs/features codex-rs/protocol
git commit -m "feat(observer): add bounded advisory progress checks"
```

### Task 9: Event-Driven Wakeups for Authorized Unified Exec Sessions

**Files:**
- Create: `codex-rs/job-monitor/Cargo.toml`
- Create: `codex-rs/job-monitor/BUILD.bazel`
- Create: `codex-rs/job-monitor/src/lib.rs`
- Create: `codex-rs/job-monitor/src/matcher.rs`
- Create: `codex-rs/job-monitor/src/matcher_tests.rs`
- Create: `codex-rs/state/migrations/0063_job_subscriptions.sql`
- Create: `codex-rs/state/src/model/job_subscription.rs`
- Create: `codex-rs/state/src/runtime/job_subscriptions.rs`
- Create: `codex-rs/state/src/runtime/job_subscriptions_tests.rs`
- Modify: `codex-rs/state/src/model/mod.rs`
- Modify: `codex-rs/state/src/runtime.rs`
- Modify: `codex-rs/state/src/lib.rs`
- Modify: `codex-rs/state/BUILD.bazel`
- Create: `codex-rs/core/src/context/job_event.rs`
- Create: `codex-rs/core/src/tools/handlers/job_subscription.rs`
- Create: `codex-rs/core/src/tools/handlers/job_subscription_tests.rs`
- Modify: `codex-rs/core/src/tools/handlers/mod.rs`
- Modify: `codex-rs/core/src/tools/registry.rs`
- Modify: `codex-rs/core/src/unified_exec/process.rs`
- Modify: `codex-rs/core/src/unified_exec/process_manager.rs`
- Modify: `codex-rs/core/Cargo.toml`
- Modify: `codex-rs/core/BUILD.bazel`
- Modify: `codex-rs/Cargo.toml`
- Modify: `codex-rs/MODULE.bazel.lock`

**Interfaces:**
- Consumes: already-authorized unified exec process identity/event stream, Stage B thread readiness/run generation, Task 2 typed activation, and existing process cancellation/exit behavior.
- Produces: `JobSubscription`, `SubscriptionMode::{OneShot,Persistent}`, bounded `StreamMatcher`, tools `job_subscribe`/`job_subscription_list`/`job_subscription_cancel`, and `JobEventContextFragment`.

- [ ] **Step 1: Write matcher tests for split chunks, exit drain, floods, and cancellation**

```rust
#[test]
fn split_final_match_is_emitted_before_exit_closes_subscription() {
    let mut matcher = StreamMatcher::new(literal_pattern("tests passed"), MatchLimits::default()).unwrap();
    assert!(matcher.ingest(Stream::Stdout, b"tests pa").events.is_empty());
    let matched = matcher.ingest(Stream::Stdout, b"ssed\n");
    let drained = matcher.finish(ProcessExit::Code(0));
    assert_eq!(matched.events.len() + drained.events.len(), 1);
    assert!(drained.closed);
}

#[test]
fn flood_coalesces_with_visible_suppressed_count() {
    let mut matcher = StreamMatcher::new(literal_pattern("tick"), MatchLimits::default()).unwrap();
    let outcome = (0..1000).fold(MatchBatch::default(), |mut batch, _| {
        batch.merge(matcher.ingest(Stream::Stderr, b"tick\n"));
        batch
    });
    assert!(outcome.retained_bytes <= 4096);
    assert!(outcome.suppressed_count > 0);
}
```

- [ ] **Step 2: Run the new crate test and verify failure**

Run: `cd codex-rs && just test -p codex-job-monitor`

Expected: FAIL because package `codex-job-monitor` does not exist.

- [ ] **Step 3: Implement bounded streaming literal matching**

```rust
pub struct MatchLimits {
    pub max_pattern_bytes: usize,
    pub max_event_bytes: usize,
    pub minimum_wake_interval: Duration,
    pub max_wakes_per_hour: u32,
}

pub fn ingest(&mut self, stream: Stream, chunk: &[u8]) -> MatchBatch {
    self.tail.extend_from_slice(chunk);
    let events = find_literal_matches(&self.tail, &self.pattern, self.limits.max_event_bytes);
    self.tail = suffix_for_split_match(&self.tail, self.pattern.len());
    self.rate_limit_and_coalesce(stream, events)
}
```

Initial patterns are escaped literals, not unbounded regex. Cap patterns at 256 bytes, retained text at 4 KiB, and carry only `pattern.len()-1` bytes across chunks. Track stdout/stderr provenance. One-shot transitions terminal after its first eligible accepted match.

- [ ] **Step 4: Persist subscription lifecycle and admission limits**

```sql
CREATE TABLE job_subscriptions (
    subscription_id TEXT PRIMARY KEY,
    thread_id TEXT NOT NULL,
    run_generation INTEGER NOT NULL,
    process_id TEXT NOT NULL,
    mode TEXT NOT NULL,
    pattern BLOB NOT NULL,
    expires_at INTEGER NOT NULL,
    state TEXT NOT NULL,
    wake_count INTEGER NOT NULL,
    suppressed_count INTEGER NOT NULL,
    record_json TEXT NOT NULL
);
```

Reject a ninth active subscription before acknowledgement. Maximum lifetime is 24 hours. Cancellation and run supersession atomically invalidate pending wake claims. Persist accepted match identity before scheduling activation; exhausted budgets update suppression counts without starting a turn.

- [ ] **Step 5: Subscribe to existing process events without spawning a command**

```rust
pub(crate) async fn subscribe_job(
    manager: &UnifiedExecProcessManager,
    request: JobSubscribeRequest,
) -> Result<JobSubscription, JobSubscribeError> {
    let process = manager.get_owned_process(request.process_id, request.thread_id).await?;
    let receiver = process.subscribe_events();
    request.store.persist_subscription(request.subscription).await?;
    spawn_subscription_actor(receiver, request.store, request.activator);
    Ok(request.subscription)
}
```

The request must name a process already owned by the thread and authorized through ordinary exec. Drain the final output event before handling `Exited`/`Closed`. An eligible unsaturated event for a ready thread is admitted within one second in a paused-clock fixture; no response-latency promise is asserted.

- [ ] **Step 6: Render typed job provenance and register tools**

```rust
pub struct JobEventContextFragment {
    pub subscription_id: String,
    pub process_id: String,
    pub stream: Stream,
    pub matched_text: String,
    pub suppressed_count: u32,
    pub exited: Option<i32>,
}

impl ContextualUserFragment for JobEventContextFragment {
    fn context_kind(&self) -> ContextKind { ContextKind::JobEvent }
}
```

Register list/cancel and subscription tools with exact ownership errors. Event activation uses the thread input admission/generation gate and never `UserInput::Text`. Status surfaces show coalesced/suppressed counts.

- [ ] **Step 7: Refresh lock, run tests, and format**

Run: `cd codex-rs && just bazel-lock-update`

Run: `cd codex-rs && just test -p codex-job-monitor && just test -p codex-state job_subscriptions && just test -p codex-core job_subscription`

Expected: PASS for split chunks, stdout/stderr, match-before-exit, flood bounds, one-shot, expiry, cancellation, supersession, ownership, eight-subscription cap, wake-rate exhaustion, and deterministic one-second admission.

Run: `cd codex-rs && just fmt`

- [ ] **Step 8: Commit**

```bash
git add codex-rs/Cargo.toml codex-rs/MODULE.bazel.lock codex-rs/job-monitor codex-rs/state codex-rs/core
git commit -m "feat(exec): wake ready threads from bounded job events"
```

### Task 10: Stage D End-to-End Qualification and Behavior Manifest

**Files:**
- Create: `codex-rs/core/tests/suite/moedex_stage_d.rs`
- Modify: `codex-rs/core/tests/suite/mod.rs`
- Create: `codex-rs/app-server/tests/suite/v2/moedex_stage_d.rs`
- Modify: `codex-rs/app-server/tests/suite/v2/mod.rs`
- Create: `codex-rs/tui/src/app/tests/moedex_stage_d.rs`
- Modify: `codex-rs/tui/src/app/tests.rs`
- Create: `scripts/qualification/stage-d-coordination.sh`
- Modify: `moedex-behavior-manifest.json`

**Interfaces:**
- Consumes: all interfaces produced by Tasks 1-9 plus Stage B/C run/evidence/budget contracts.
- Produces: one packaged Stage D qualification command and manifest mappings for O4/O5/O7-O12/P1/P2/C15.

- [ ] **Step 1: Add the cross-session acceptance journey**

```rust
#[tokio::test(flavor = "multi_thread", worker_threads = 4)]
async fn stage_d_cross_session_message_survives_disconnect_without_touching_draft() -> anyhow::Result<()> {
    let test = two_root_sessions_same_project().await?;
    let room = test.create_room_and_join_both().await?;
    test.target.set_draft("keep this draft α\n").await?;
    test.target.disconnect().await?;
    let receipt = test.sender.send_named(room.id(), test.target.name(), "review ready").await?;
    assert_eq!(receipt.state, ReceiptState::Persisted);
    test.target.reconnect_ready().await?;
    assert_eq!(test.target.draft().await?, "keep this draft α\n");
    assert_eq!(test.target.message_activation_count(receipt.envelope_id()).await?, 1);
    Ok(())
}
```

The suite also covers different-project same names, broadcast unavailable receipts, unauthorized action, task replay, hop/TTL rejection, board multi-membership, concurrent reorder, seen/result race, crash liveness, continuation timer revision, observer cap, completion replay, and final split exec output.

- [ ] **Step 2: Run Stage D core, API, and TUI tests**

Run: `cd codex-rs && just test -p codex-core moedex_stage_d && just test -p codex-app-server moedex_stage_d && just test -p codex-tui moedex_stage_d`

Expected: PASS through public session, JSON-RPC, and TUI paths.

- [ ] **Step 3: Add deterministic qualification script**

```bash
#!/usr/bin/env bash
set -euo pipefail
cd "$(dirname "$0")/../../codex-rs"
just test -p codex-coordination
just test -p codex-job-monitor
just test -p codex-state coordination
just test -p codex-state boards
just test -p codex-state attention
just test -p codex-core moedex_stage_d
just test -p codex-app-server-protocol
just test -p codex-app-server moedex_stage_d
just test -p codex-tui moedex_stage_d
```

- [ ] **Step 4: Record exact manifest evidence mappings**

```json
{
  "stage": "D",
  "requirements": {
    "O4": ["codex-core::supervision"],
    "O5": ["codex-core::progress_observer"],
    "O7": ["codex-coordination::delivery_tests"],
    "O8": ["codex-core::coordination_delivery"],
    "O9": ["codex-coordination::relay_guards"],
    "O10": ["codex-state::boards"],
    "O11": ["codex-app-server::attention"],
    "O12": ["codex-state::followup_proposals"],
    "P1": ["codex-app-server::coordination"],
    "P2": ["codex-tui::moedex_stage_d"],
    "C15": ["codex-job-monitor::matcher_tests"]
  }
}
```

Merge this object into the Stage A-C behavior manifest schema without removing prior requirement evidence.

- [ ] **Step 5: Review and accept remaining TUI snapshots**

Run: `cd codex-rs && cargo insta pending-snapshots -p codex-tui`

Review every pending Stage D snapshot at normal and narrow widths, then run: `cd codex-rs && cargo insta accept -p codex-tui`

Expected: no pending snapshots remain.

- [ ] **Step 6: Run scoped lint fixes and formatting as the final mutation**

Run: `cd codex-rs && just fix -p codex-coordination && just fix -p codex-job-monitor && just fix -p codex-state && just fix -p codex-core && just fix -p codex-app-server-protocol && just fix -p codex-app-server && just fix -p codex-tui`

Run: `cd codex-rs && just fmt`

Expected: commands complete without unresolved lint or formatting changes. Do not rerun tests after `fix` or `fmt` unless those commands report a substantive failure requiring a code change.

- [ ] **Step 7: Commit**

```bash
git add codex-rs/core/tests codex-rs/app-server/tests codex-rs/tui/src/app scripts/qualification/stage-d-coordination.sh moedex-behavior-manifest.json
git commit -m "test(moedex): qualify Stage D coordination"
```
