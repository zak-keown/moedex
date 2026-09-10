# Reliable execution and orchestration

Part of [SPEC.md](SPEC.md). Shared contracts and limits apply.

## Reliable execution and orchestration

This contract covers R1–R7 and O1–O12 from the enhancement catalog. These are proposed product behaviors, not claims about donor implementations. Defaults below are design defaults to validate during implementation; they are configurable within installation policy limits. Existing retry, queue, goal, messaging, hook, and session mechanisms remain the starting point.

### Shared execution contract

Each logical run has a durable run ID, objective revision, execution generation, and ordered event sequence. Each queued instruction has an immutable ID and explicit revision. Each model attempt, tool operation, scheduled retry, review, and coordination action refers to its owning run and generation. Editing a pending instruction retains its identity and increments its revision; replacing the objective advances the generation. Late results remain inspectable but cannot advance a superseded run.

The run states are `ready`, `active`, `waiting_capacity`, `waiting_backoff`, `paused`, `reconciling`, and terminal `completed`, `cancelled`, or `failed`. Waiting preserves pending work. Cancellation invalidates scheduled continuation immediately and requests cancellation of active operations; it does not falsely report that an external effect has been reversed. After a crash, an active run enters reconciliation before dispatch resumes.

Instruction states distinguish pending, durably accepted into a turn, cancelled, and superseded. Queue removal and turn acceptance must share a durable transaction or equivalent recoverable protocol. A transport retry retains its logical attempt identity; starting a new turn is a distinct operation. The product must not claim exactly-once external execution. If a tool might have executed but its result is missing, record `effect_unknown`, query existing operation status or use its supported idempotency mechanism before deciding whether replay is safe. An unresolved non-idempotent operation pauses dependent work with the missing evidence identified; independent authorized work may continue.

Existing product authorization and sandbox rules apply unchanged. Recovery, observers, countdowns, and messages do not grant new authority. Already authorized actions do not acquire another approval solely because they were retried or resumed. Agent-originated information remains agent-originated and cannot impersonate user instructions.

### R1 — Durable follow-up queue

**Requirement.** Capacity exhaustion must park pending follow-ups without consuming them. Users can append, edit, reorder, cancel, or supersede pending entries while parked. Capacity recovery dispatches one eligible instruction, waits for that turn's acceptance/completion boundary, then evaluates the next entry. The default order is FIFO. Queue-full rejection must occur before reporting acceptance; no oldest-item eviction is permitted.

**Design defaults.** Maximum 100 pending entries per thread; maximum 64 KiB stored text per entry, with bounded model-context rendering governed separately by the global fragment limits. Attachments depend on U11.

**Acceptance.** Enqueue A/B/C, exhaust capacity, edit B, cancel C, and crash between dispatch and its acknowledgement. Restart and recover capacity: A is represented by one accepted logical turn, revised B follows, C never dispatches, and all operations remain visible in queue history. If A's external effect is uncertain, reconciliation prevents blind replay.

### R2 — Cancellable overload recovery

**Requirement.** Classify transient overload separately from permanent request errors and capacity-reset waits. Backoff must display its reason and next attempt time, support cancellation, and apply jitter without exceeding the run deadline. A timer must compare generation and pending instruction revision before submitting work. Existing client-level retries and outer run recovery must share accounting so nested retries cannot multiply invisibly.

**Design defaults.** Exponential backoff begins at 2 seconds, caps at 60 seconds, and allows at most 8 total recovery attempts within O3's elapsed budget.

**Acceptance.** Inject overload, change the objective while a timer is pending, then deliver that timer twice. No continuation for the old generation occurs. A permanent malformed-request response fails immediately with preserved pending input; repeated transient failures stop at the documented attempt or elapsed limit.

### R3 — Safe credential refresh

**Requirement.** Apply an explicit identity/workspace change only at a logical turn boundary. Bind an active turn and its retries to one credential generation. Validate a replacement credential record before atomic activation; discard cached transport identity when switching generations. A transient partial read must not overwrite a valid credential snapshot, but explicit logout/revocation must take effect rather than silently retaining access. Pending instructions must show the identity selected for their next dispatch.

**Acceptance.** Replace credentials during an active streamed turn: that turn retains its identity and the next turn uses the validated new identity with refreshed transport state. A partial-file read does not erase valid state. Logout stops subsequent authenticated dispatch. If credentials cease working mid-turn, surface the failure rather than switching accounts silently.

### R4 — Unified compaction recovery

**Requirement.** Manual and automatic compaction must use one policy for remote timeout, eligible local fallback, cancellation, and checkpoint publication. The policy must retain the baseline's established context/checkpoint semantics. An interrupted compaction must not trigger fallback. Publish a replacement checkpoint only after the complete selected compaction path succeeds; concurrent compactions for the same generation must coalesce or serialize. Failed or stale candidates cannot overwrite the active checkpoint.

**Design defaults.** Remote deadline 120 seconds; one local fallback attempt on eligible timeout or transient service failure; no fallback on explicit cancellation or invalid input.

**Acceptance.** Run the same timeout/cancellation/failure scenarios through manual and automatic entry points and obtain equivalent policy outcomes. Cancel during remote compaction and assert zero local attempts. Crash before checkpoint publication and recover the previously committed checkpoint without a partially installed summary.

### R5 — Independent compaction resources

**Requirement.** Compaction may have its own timeout and supported service-tier setting, separate from ordinary task requests. Effective settings must be inspectable. Unsupported explicit tiers must produce a clear configuration/capability error instead of silently selecting a more expensive tier. Unconfigured tier selection inherits current provider behavior.

**Acceptance.** Configure a compaction tier and deadline, capture ordinary and compaction requests, and verify that only compaction receives the override. A provider without the tier rejects the override clearly; ordinary task settings remain unchanged.

### R6 — Repetition and non-progress detection

**Requirement.** Observe a bounded window of normalized actions, results, and meaningful state changes. Repeated arguments alone are insufficient evidence of non-progress. Recognize declared waits, job subscriptions, and reruns following actual code changes. When repeated unsuccessful behavior exceeds the threshold, provide evidence and pause automatic continuation; permit an explicitly revised approach or user continuation without rewriting the objective. Detection is advisory for interactive work by default.

**Design defaults.** Inspect the last 12 completed actions; trigger after 3 equivalent unsuccessful action/outcome pairs without a relevant state change. Store normalized fingerprints and bounded evidence, not unlimited output.

**Acceptance.** Three unchanged failing repair attempts trigger a visible pause. Identical polls of an active job do not trigger merely because their arguments repeat. Re-running a test after a changed artifact resets that sequence; changing irrelevant argument whitespace does not evade detection.

### R7 — Explain and reconcile stalled work

**Requirement.** Show authoritative lifecycle state, last meaningful event time, active operation identity, pending input count, and whether process/tool liveness is known. Distinguish silence from confirmed failure. Recovery uses the latest completed durable checkpoint and reconciles outstanding effects before continuation; it must never imply rollback of external side effects.

**Design defaults.** Flag an unexplained silent active run after 120 seconds; a live declared long-running operation displays its own status instead of being labeled failed.

**Acceptance.** Kill the client while a tool is active and reconnect. The UI initially reports reconciliation, then resolves live/completed/unknown tool status. A missing result remains uncertain and does not turn into an automatic duplicate tool invocation. A healthy quiet build is shown as active with its last event time.

### O1 — Reviews bound to captured content

**Requirement.** Every review must identify immutable captured content, comparison base, selected scope, and relevant configuration. Include tracked modifications and selected untracked files; report exclusions. Review through a stable snapshot or validated read-only content view. Serialize overlapping review entry points per workspace and scope. Before displaying findings as current or applying repairs, compare actual content identity, including external edits, rather than relying only on internal mutation counters.

**Acceptance.** Start a review, externally edit a reviewed file, and finish the review. Findings remain attached to their original snapshot and are labeled stale. Two competing review entry points cannot launch competing repair writers. An untracked selected source file appears in the captured scope; an excluded file is disclosed.

### O2 — Bounded review and repair

**Requirement.** A repair cycle consumes findings for a specific snapshot, produces a new snapshot, runs declared verification, and requests fresh review. Completion requires explicit review status and required check evidence; reviewer silence is inconclusive. Stop on exhausted limits, stale inputs, repeated findings without meaningful change, cancellation, or unmet authorization. Preserve remaining findings and termination reason.

**Design defaults.** At most 3 repair iterations, 30 minutes elapsed, and 2 consecutive unchanged finding fingerprints. Apply configured run token/spend limits when available; absent pricing cannot mean unlimited approved spend.

**Acceptance.** A reviewer returning identical findings after unchanged repairs terminates with unresolved findings. Passing review with failed required tests remains incomplete. External edits invalidate the affected repair basis and require recapture before another repair, without overwriting those edits.

### O3 — Visible autonomous recovery deadline

**Requirement.** Display the outer recovery deadline separately from the next attempt and provider reset time. Waiting time counts toward the elapsed recovery budget. An individual request also has a deadline; no hung request can defeat the outer bound. If a provider reset falls beyond the limit, park the run with a clear resumable state instead of silently extending it.

**Design defaults.** Ten minutes total recovery wait; request deadlines inherit the configured provider timeout but cannot exceed remaining recovery time.

**Acceptance.** With a frozen test clock, combine several transient attempts and a hanging request: recovery terminates at its original deadline. A capacity reset an hour away parks the queue with that timestamp. Cancelled deadline callbacks cannot revive the run.

### O4 — Optional supervised continuation

**Requirement.** Support existing automatic behavior, preview-with-countdown, and manual continuation modes. Preview includes the proposed action, objective revision, and reason. The user can advance, edit, pause, or cancel. Manual mode is explicitly selected; enabling this feature must not add approval prompts to established automatic behavior. Preview acceptance does not substitute for existing action authorization.

**Design defaults.** Automatic mode retains current semantics; preview mode uses a 5-second countdown.

**Acceptance.** Edit a proposal during countdown and deliver its old timeout event: no old action executes. Manual mode waits without consuming a timer. An automatically authorized continuation proceeds without a newly invented confirmation step.

### O5 — Advisory progress observer

**Requirement.** An optional observer examines bounded progress evidence for objective drift, missing verification, and repetitive failure. Findings cite observed events and remain advisory. The observer cannot edit goals, dispatch tools, or extend budgets; its usage is attributable to the run.

**Design defaults.** Disabled until enabled; inspect after 10 completed tool operations, at most once per 2 minutes, maximum 3 observations per run unless configured otherwise.

**Acceptance.** An observer flags an omitted required test with event references while leaving execution authority and objective unchanged. Repeated event delivery cannot create extra observations beyond the budget. Disabling it cancels pending observation scheduling.

### O6 — Long-session rendering

**Requirement.** Status-only events must update affected UI regions without invalidating finalized transcript layout. Width, theme, or content changes may invalidate the relevant caches. Cached data and live-event backlog must remain bounded.

**Acceptance.** In a 300-turn transcript regression fixture, repeated countdown/usage/status updates perform zero finalized-cell re-layouts. Resizing correctly reflows history once; subsequent status updates reuse it. Snapshot tests verify the visible states. This is a deterministic work bound, not an unmeasured latency claim.

### O7–O9 — Durable rooms, receipts, and relay guards

**O7 requirement.** Independent root sessions can join a durable project-scoped room using explicit membership and session identity. Cross-platform clients share the same semantics; disconnected members are distinguishable from ready ones. Room membership alone does not authorize a participant to interrupt or control another session.

**O8 requirement.** Typed envelopes carry room, sender, target, message, task, action, reply, generation, and sequence identities as applicable. Per-target receipts distinguish persisted, delivered, accepted, started, completed, rejected, and expired. Durable deduplication suppresses repeated logical deliveries and task starts; transport acknowledgement never implies task completion. Actions have explicit target-side authority checks.

**O9 requirement.** Queue delivery until the target advertises readiness; never automatically relay a reply as another request. Preserve provenance, enforce bounded hops and backpressure, and expire undeliverable actions visibly. Agent messages must use typed agent-origin context, not synthetic user-role prompts.

**Design defaults.** 32 members per room, 100 pending envelopes per target, 24-hour pending TTL, 4 relay hops, and 30-day deduplication retention. Expired envelopes are rejected after retention so old replay cannot become a new task. Stored payloads cap at 16 KiB; model-visible rendering obeys the stricter global context limits.

**Acceptance.** O7: disconnect/reconnect two independent sessions and recover their room memberships without widening project scope. O8: crash after durable receipt and replay the envelope; one task starts and per-target status remains correlated. An unauthorized control action is rejected while an authorized message is delivered. O9: send before readiness and route a reply through a loop; delivery waits for readiness, the reply does not start a new relay chain, and over-limit/expired deliveries receive explicit outcomes.

### O10–O11 — Boards and attention

**O10 requirement.** Boards reference existing thread IDs and maintain names, card order, membership, and search metadata independently of conversation storage or scheduling. Removing a card does not delete its thread. Concurrent edits use revision checks or deterministic conflict handling.

**O11 requirement.** Aggregate actual pending approvals, errors, active work, paused recovery, and unread results from durable events plus live reconciliation. Seen state is per board membership and event sequence; reading one board does not silently clear another. A stored running flag alone is insufficient to claim current liveness.

**Acceptance.** O10: put one thread on two boards, rename/reorder/remove one card, restart, and verify the other membership and complete transcript remain intact. Concurrent reorder is resolved explicitly. O11: crash an active session, reconnect, and show reconciliation/unknown until authoritative status resolves it. A result arriving while the user marks an older event seen remains unread. Approvals already resolved elsewhere disappear after synchronization.

### O12 — Completion follow-up proposals

**Requirement.** Generate at most one follow-up proposal for each completed objective revision by default. Distinguish a proposal from a runnable task. Automatic dispatch is allowed only within an explicitly authorized backlog runner's scope and remaining limits; completion alone grants no new task scope. Persist the completion-event identity and proposal decision to suppress replay.

**Acceptance.** Replay a completion event after restart: one proposal exists and no unsolicited task starts. An authorized backlog runner selects only an eligible scoped item, stops at its budget, and preserves user edits over a late generated proposal.

### Dependencies and delivery boundaries

Deliver the shared durable run/queue generation contract with R1/R2/O3 first; R7 builds on its reconciliation events. R3 can proceed independently with integration at dispatch boundaries. R4/R5 share one compaction policy. O1 must precede O2; evidence-backed completion (C13) can extend its check records. R6 informs O5 but neither requires an additional planner. O4 and O12 consume generation-safe continuation events. O6 is a prerequisite for richer frequently updated UI. O10's organizational store can ship before O7 rooms; O11 consumes authoritative lifecycle events rather than assuming room presence. O7–O9 are one coherent coordination subsystem and must not ship an unbounded or provenance-free intermediate transport. Rebranding (U4 plus the dedicated identity specification) determines homes, daemon namespaces, and durable storage placement for every subsystem.
