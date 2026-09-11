# Moedex product specification

Version: 0.1, 2026-09-10. Status: reviewable product design; implementation has not started.

## Objective and scope

Moedex is a personally maintained Codex-derived harness for sustained engineering and research. It must preserve user intent through interruption, coordinate parallel work visibly, and connect completion claims to inspectable evidence. Its public identity is **Moedex**, invoked as **`moedex`**, distributed from **`zak-keown/moedex`**.

This specification covers all 50 entries in the [enhancement catalog](../../ENHANCEMENT-CATALOG.md), plus **REBRAND**, the 51st workstream. Requirements retain catalog identifiers. Closely related requirements share components; they are not 51 separate implementations. The optional and experimental designations below describe delivery and activation, not silently removed scope.

This request authorizes specification work. Package publication, product implementation, and external release are separate work. Names and numeric defaults proposed here are design choices, not claims about available registry names or measured performance.

## Specification set

| Document | Contract |
|---|---|
| [BRAND.md](BRAND.md) | REBRAND: naming, coexistence, migration, packaging, versioning and updates; expands U4/U5. |
| [EXECUTION.md](EXECUTION.md) | R1–R7 and O1–O12: recovery, review, autonomy, coordination, boards. |
| [EXPERIENCE.md](EXPERIENCE.md) | U1–U11 and P1–P2: operator controls, history policy, attachments, messaging. |
| [RESEARCH.md](RESEARCH.md) | C1–C15 and P3–P5: context, research, editing, providers, evidence and provenance. |
| [TRACEABILITY.md](TRACEABILITY.md) | Complete feature-to-contract and delivery-stage map. |
| [MAINTENANCE.md](MAINTENANCE.md) | Proposed six-priority maintenance supplement: release ownership, portable CI, verification policy, advisory lint, telemetry, and community automation. Adds no completed implementation stages. |

The [baseline audit](../../FORK-BASELINE.md) is pinned to `8e2afc09126c0cea4c282725fe68af43adad73d7`. Existing goals, hooks, messages, partial-history spawning, queues, compaction, code mode, worktrees, plugins, themes and memory are foundations to extend. The catalog supplies donor evidence, not normative behavior. Where a donor's behavior conflicts with this specification, this specification controls the proposed Moedex design. Repository/user instructions remain authoritative over it.

## Product decisions

1. **Extend the existing runtime.** Reuse current session, permission, tool, goal, history and extension mechanisms. A wholesale donor-fork merge would import unrelated behavior and multiply upgrade costs; detached sidecars alone would lack authoritative runtime state. Small owned modules/extensions with narrow integration points are the selected approach.
2. **Rebrand first.** An independently installable product must coexist with stock Codex before daily-driver enhancements ship. Keep upstream internal crate names and protocol compatibility wherever possible; change the user-facing distribution identity deliberately.
3. **Preserve authorized scope.** Retrying, observing or finishing a goal must not silently create new objectives. Already authorized work should not acquire repetitive confirmation gates. Supervised countdowns are opt-in.
4. **Make evidence accessible.** Users can inspect the code revision, check result, source coverage and delivery state behind a conclusion. Confidence scores and reviewer prose are not substitutes for observations.
5. **Keep optional integrations optional.** Research providers, translation, voice, alternate model protocols and Android must not become startup dependencies for ordinary coding.

## Core user journeys

**Install and coexist:** install Moedex, see its identity, sign in or explicitly import selected settings, and run alongside Codex without shared daemon/state mutation. An update remains on the chosen Moedex channel.

**Run sustained work:** submit a goal and queue follow-ups. A capacity interruption parks the queue, explains the wait and resumes within configured bounds. Cancellation invalidates delayed actions. After a crash, reconcile uncertain effects before deciding whether any work can safely continue.

**Delegate and supervise:** allocate context and budgets, create children, see their status and costs, and inspect results in an attention inbox. Independent sessions can exchange typed messages in a project room without pretending those messages are new human instructions.

**Verify and finish:** capture the workspace state, run review/checks, attach results to that state, and repair within a bounded budget. A later edit marks old evidence stale. The completion report distinguishes verified, failed, stale and unverified claims.

**Research while working:** search sources with visible partial failures, open a document/report by section, ask contextual questions, and insert bounded, versioned excerpts. Coverage reports identify material not examined.

## Shared contracts

### Identity, state and ownership

These are domain contracts, not finalized wire schemas. Use opaque stable IDs; display names are not routing keys.

| Record | Required meaning |
|---|---|
| Work item | Goal/task ID, owning thread, authorized objective, lifecycle state, budget policy and generation. |
| Input envelope | Input ID, source kind, target thread, immutable attachment references, enqueue order, delivery state and cancellation generation. |
| Workspace snapshot | Worktree identity, base commit when applicable, tracked/untracked manifest, content identities and declared exclusions. Capturing must detect concurrent mutation or report an unstable capture. |
| Evidence record | Claim/check ID, executor/provider, input snapshot, start/end, exit/result state, bounded artifact references and applicability scope. |
| Coordination envelope | Message/action ID, sender identity, project/room scope, target IDs, correlation/reply IDs, hop limit, expiry and delivery receipt. |
| Context handle | Content identity/version, origin and retrieval date, access scope, coverage/exclusions and bounded retrieval contract. |
| Usage record | Provider/model, request ID, thread/parent attribution, actual or estimated tokens, price source/version/currency and reservation reconciliation. |

Persist durable state before acknowledging its acceptance. A replay must not consume one accepted input twice. This is not a promise of exactly-once external side effects: after ambiguous failure, the state must remain uncertain until reconciled. One owner coordinates state changes for a thread; readers receive versioned events. Events delivered late must not revive a cancelled generation.

### Context and permissions

New model-visible fragments must be typed, provenance-bearing `ContextualUserFragment` implementations in the existing context system, with bounded serialized size. Proposed default maximum: **1,000 tokens per fragment and 4,000 new tokens per turn across these enhancements**. Smaller applicable provider/context limits win. Token estimation must be conservative; page or return artifact handles instead of silently dropping essential data. No single item may exceed the repository's 10K-token ceiling. Any proposed individual item over 1K tokens requires the repository's P0/manual context review before adoption.

These new-fragment budgets govern harness-generated metadata, retrieved excerpts and notifications. They do not authorize truncating or summarizing a human's submitted instructions. Direct user input retains the baseline input-admission rules; if an input cannot be accepted intact, reject before acknowledging acceptance with an actionable limit explanation. Imported/copied history obeys both its own limits and the established history semantics.

Do not rewrite prior model-visible messages to install status changes or evidence updates. Append bounded deltas and retrieve detail on demand. Preserve existing explicit compaction semantics; these enhancements do not introduce another history-rewriting mechanism. Treat documents, tool output and agent messages according to their actual source; none acquires human authority through formatting or role substitution.

All new file/network/process actions use existing permission and sandbox paths. Monitoring subscribes to authorized execution rather than spawning an unmediated command. Workflow policies may impose explicitly configured prerequisites, but do not replace the sandbox or invent universal approval ceremonies. The prohibited sandbox environment-variable code remains untouched.

### Budgets and limits

Every new queue/store/subscription must have a finite documented count, byte and retention policy. Defaults in domain documents are proposed defaults, subject to measured tuning before implementation approval; there must be no implicit unbounded mode. Backpressure is visible. Accepted user inputs are never silently evicted; reject new admission before acknowledgement when full. Truncation is acceptable for derived previews only and must be reported.

Proposed initial resource defaults fill in the per-domain contracts below. Changes require config validation and focused admission/overflow tests; implementations may choose smaller platform/provider limits and report them.

| Resource | Proposed bounds and overflow behavior |
|---|---|
| Pending input text | 100 entries/thread, 64 KiB/entry, 4 MiB total/thread; reject new admission. Pending accepted input has no automatic expiry; explicit removal/completion releases it. |
| Context cache | 1 GiB/project, 10,000 documents, 20 MiB/document; least-recently-used unpinned eviction with explicit expired-handle result. Active review/evidence pins cannot be silently evicted. Refuse new pins when full. |
| Scratchpad | 256 keys/session, 64 KiB/value, 4 MiB/session, 32 operations/batch; writes are atomic per batch, expected-version conflict fails the entire batch. Persist for session resume until explicit clear/delete; reject overflow. |
| Research request | 20 results/page, 100 maximum/page, 15-second deadline/source; traversal defaults depth 1, 100 nodes, 20 requests. Pagination exposes continuation and coverage. |
| Monitor | 8 subscriptions/thread, 256-byte pattern, 24-hour lifetime, 4 KiB retained event text, at most one wake/second/subscription and 60 wakes/hour/thread. Coalesce/drop derived events visibly; compile bounded-complexity matching. |
| Session boards | 100 boards/workspace, 1,000 memberships/board, 100 items/page; reject additions at cap. Archiving preserves the underlying conversation. |
| New UI caches | 512 finalized rendered cells and 1,024 queued status events/view; coalesce status by identity and reload evicted historical layouts on demand. User/tool completion events use durable storage, not lossy status queues. |
| New artifact/evidence storage | 2 GiB/project default, 30-day automatic cleanup only for unreferenced derived artifacts. Referenced evidence is pinned until explicit removal; full store rejects further captures and marks verification unavailable. |
| Export/handoff package | 10 MiB/package, bounded summary plus attachment references; reject oversize creation/import. Do not automatically bundle secrets or full transcripts. |

Room delivery/dedup retention is specified in EXECUTION.md; attachments in EXPERIENCE.md. Persistent human records are bounded by admission rather than time-based destruction. No cleanup rule deletes user source files or upstream rollout history.

Reserve budgets atomically before concurrent model work. Reconcile actual usage once per request. Unknown prices are unknown, not zero. A configured estimated spend budget controls admission, not a guarantee about a provider's final invoice; expose in-flight reservations and uncertain usage. A budget change does not grant new task scope.

### Compatibility and interfaces

Maintain Linux, macOS and Windows for core behavior, including supported app-server/exec-server combinations across OSes. Android is an explicitly optional target with its own qualification. Respect upstream rollout readers, IDs, sandbox behavior and provider authentication contracts.

New external methods belong in app-server v2 with normal experimental gating, naming, pagination and schema generation. Final method/field names are deferred to implementation plans after an API inventory. Do not add commands duplicating existing controls without a demonstrated behavioral distinction. File formats require schema versions and atomic writes; rollback must detect unsupported newer state and stop without corruption.

## Delivery stages

These are proposed ordering constraints, not dates or a promise that every optional feature belongs in the first release.

The executable task breakdown lives in the [Moedex plan-set manifest](../../docs/moe/plans/moedex-MANIFEST.md). The manifest is the source of truth for cross-plan dependencies and completion ranges; each named plan owns its internal task sequence.

| Stage | Deliverable | Required prior foundation |
|---|---|---|
| A | Moedex-branded install, independent home, import preview, fork-owned update path and behavior manifest. | Existing baseline. |
| B | Queue/retry/compaction reliability; temporary settings, live status, draft copy and rich queue; bounded UI updates. | A, durable input identities. |
| C | Snapshot review, bounded repair, evidence ledger, history policy, usage attribution and structured edits. | A; B recovery for long-running loops. |
| D | Boards, attention inbox, rooms, typed delivery, observers and controlled follow-up proposals. | B/C state, budgets and evidence. |
| E | Selective context, coverage, scratchpad, document/library search and reader. | A and shared context/permission contracts; can proceed alongside D. |
| F | Alternate protocols, cross-harness handoffs, commit provenance, translation and voice. | Relevant C/E contracts; each independently optional. |
| G | Android qualification and stored-response checkpoint experiment. | Stable packaged product for Android; provider-specific feasibility for checkpoints. |

A stage is complete only when its enabled user journeys work end to end, including packaged execution where relevant. Do not declare completion for scaffolding or hidden configuration alone. Features in later stages remain specified, with their dependencies visible in TRACEABILITY.md.

## Verification and release gates

- Every feature's acceptance criteria map to executable tests or a retained manual experiment with inputs, expected result and observed result. Agent-logic changes require integration coverage through the existing public testing helpers.
- Exercise cancellation, restart, duplicate/out-of-order delivery, stale workspace contents, unavailable providers, malformed inputs, finite-budget exhaustion and mixed-OS paths where applicable. Fault tests must demonstrate meaningful behavior, not merely mirror implementation.
- User-visible UI/copy changes require reviewed `insta` snapshots, including narrow terminals and supported character widths. For status-only updates, a 300-turn fixture must avoid full-transcript rebuilds; hardware latency targets are established by a benchmark baseline before claiming improvement.
- Provider/editing/retrieval experiments use fixed task sets, report failures and compare outcome quality as well as latency/tokens. Mock protocol tests alone do not certify live providers.
- Follow AGENTS.md for `just test`, crate scope, schema generation, Bazel resource/dependency lock updates, scoped lint fixes and formatting. The complete Rust suite needs the user's required approval at execution time; this specification does not waive it. No Rust tests are needed merely to write these documents.
- Preserve upstream notices and inspect donor licensing/provenance before code reuse. Feature ideas do not imply permission to copy arbitrary assets or branding.

## Decisions that remain before release

| Decision | Proposed resolution | Blocks |
|---|---|---|
| Registry/package ownership | Prefer `@zak-keown/moedex`; verify availability and publishing authority before choosing a public install command. | npm publication only; source/local releases can proceed. |
| Visual identity | Text-first Moedex wordmark using existing theme accessibility. Original icon/logo can follow; do not copy OpenAI product artwork as Moedex identity. | Custom artwork, not functional rebrand. |
| First alternate-provider/model targets | Select a small explicit certification matrix during C11 planning. | Live-provider qualification and default tool selection. |
| Performance thresholds | Measure baseline on a documented machine and dataset, then set regression thresholds. | Performance claims, not structural contracts. |
| Cross-harness target | Choose one documented adapter first and publish its capability/loss report. | P4 interoperable release. |
| Stored checkpoints | Decide go/no-go from P5 evidence; retain local rollout behavior if unsupported or unjustified. | P5 production rollout only. |

No unresolved item above prevents reviewing the specified behavior. Per-domain proposed defaults are visible choices to tune, not silently assumed user preferences.
