# Moedex Optional Integrations Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver optional alternate protocols, commit provenance, handoffs, display translation and voice dictation (Stage F: C11, P3, P4, U6, U9).

**Architecture:** Extend existing codex-api/provider and migration seams instead of replacing the request engine. Keep provenance and handoff data in versioned local records referencing Stage C evidence and Stage E immutable content. Translation and dictation are optional presentation/input services with cancellation generations, usage attribution and no authority to submit new human turns.

**Tech Stack:** Rust, existing `codex-api`, `codex-client`, HTTP/auth factories, migration and attribution crates, Tokio, serde, ratatui/insta and the current realtime input implementation.

**Spec:** `specs/moedex/RESEARCH.md` C11/P3/P4; `specs/moedex/EXPERIENCE.md` U6/U9; `specs/moedex/SPEC.md`.

## Global Constraints

- “1,000 tokens per fragment and 4,000 new tokens per turn across these enhancements”.
- “10 MiB/package, bounded summary plus attachment references; reject oversize creation/import.”
- “Optional research connectors MUST remain optional and must not make ordinary coding depend on their availability.” Apply the same startup isolation to these integrations.
- Voice recording limit: “120 seconds”; “no retained audio by default after transcription/cancellation.” Translation defaults off.
- Preserve Linux/macOS/Windows and cross-OS app-server/exec-server behavior. Provider identity/auth remains truthful after rebranding.
- Keep native RPITIT with explicit Send future bounds for new traits; no async_trait. Focused modules and separate `_tests.rs` files.
- Use `just test -p PACKAGE`, no direct `cargo test`. Run Rust `just` and `cargo insta` commands from `codex-rs`. Run scoped lint/fmt after tests without retesting. Dependency changes require `just bazel-lock-update`; config types require `just write-config-schema`; public v2 changes require `just write-app-server-schema` and experimental fixtures as appropriate.
- Core/protocol integration requires the approved complete-suite gate before completion. No live credentials or publishing authorization is implied by this plan.

## Open Decisions

This plan is not dispatchable until D1–D3 resolve. Stage C ledger/evidence and Stage E scoped content/handoff storage are prior implementation dependencies.

- **D1 — Provider certification matrix** · `research` · AFK
  - **Question:** Which current model/API versions offer required Chat Completions and Anthropic Messages streaming/tool/usage semantics?
  - **Options:** Official endpoint/model with documented capabilities / unsupported capability.
  - **Recommendation:** Select one documented model per protocol and pin dated primary-source contracts; do not silently adapt unsupported reasoning/tool forms.
  - **Blocked by:** —
  - **Blocks:** Task 1.
  - **Resolution:** Unresolved; record a concrete matrix and request examples in `research/moedex/provider-contracts.md`.
- **D2 — First handoff adapter target** · `conversation` · HITL
  - **Question:** Which external harness should the first interoperable handoff adapter target?
  - **Options:** Claude Code / Cursor Agent.
  - **Recommendation:** Claude Code, because the local migration crate already parses CLA records; target capability and loss still need explicit qualification.
  - **Blocked by:** —
  - **Blocks:** Task 3.
  - **Resolution:** Unresolved; the portable Moedex envelope is specified, but no external adapter may be declared supported until the target is chosen.
- **D3 — Translation and dictation backends** · `conversation` · HITL
  - **Question:** Which configured backend receives translated summaries and microphone recordings for the first supported release?
  - **Options:** Explicit remote provider / supported local model.
  - **Recommendation:** Reuse an explicitly configured provider for translation and an explicitly selected transcription backend; leave both disabled until selected. Existing realtime functionality is not proof of a standalone dictation API.
  - **Blocked by:** —
  - **Blocks:** Task 4, Task 5.
  - **Resolution:** Unresolved; record chosen endpoint/model, privacy display text, capability contract and credential setup before implementing backend transport.

## Not Yet Specified

None beyond the explicit decisions. Backend endpoint details become concrete through D1/D3; unsupported capability results are acceptable, silent semantic substitution is not.

## Out of Scope

- Changing the default inference provider merely because protocol mocks pass.
- Full-duplex voice conversation or exposing hidden reasoning.
- Implicit transcript export, remote memory sync or rewriting Git history.

## File Structure

Protocol adapters live under `codex-rs/codex-api/src/endpoint/`, with thin core dispatch in a new module. Extend `ext/git-attribution` for local links and `external-agent-migration` for bounded handoffs. New TUI translation/dictation modules own lifecycle logic; `app.rs` and `chatwidget.rs` receive only necessary event dispatch. Confirmed anchors: `codex-api/src/provider.rs`, `codex-api/src/endpoint/responses.rs`, `external-agent-migration/src/sessions/export.rs`, `ext/git-attribution/src/lib.rs`, `tui/src/chatwidget/realtime/recording_controls.rs`.

### Task 1: Optional protocol adapters with contract tests (C11)

**Blocked by:** D1.

**Files:**
- Create: `codex-rs/codex-api/src/endpoint/chat_completions.rs`, `codex-rs/codex-api/src/endpoint/anthropic_messages.rs`, corresponding `chat_completions_tests.rs` and `anthropic_messages_tests.rs`, `codex-rs/core/src/client/provider_protocol.rs`, `codex-rs/core/tests/suite/provider_protocols.rs`, `research/moedex/provider-contracts.md`.
- Modify: `codex-rs/codex-api/src/endpoint/mod.rs`, `codex-rs/codex-api/src/provider.rs`, `codex-rs/core/src/client.rs`, `codex-rs/core/tests/suite/mod.rs`; config/schema files only for required explicit protocol selection.
- Test: adapter sibling tests and core public agent integration suite.

**Interfaces:**
- Consumes: Existing `codex_client::Request`, `codex_api::Provider`, configured auth, retry/cancellation and Stage C request usage identity.
- Produces: `ProviderProtocol::{Responses,ChatCompletions,AnthropicMessages}` and `ProtocolCapabilities { parallel_tools: bool, namespaced_tools: bool, usage: bool, reasoning: bool }`. New adapters translate into the existing response event stream; no new parallel session engine or second retry loop.

- [ ] Add captured request/stream fixtures using D1 exact versions. Include interleaved tool fragments, failed tool result, cancellation, namespaced calls, malformed final frame, missing usage and compaction-triggering usage.

```rust
let capabilities = ProtocolCapabilities {
    parallel_tools: true, namespaced_tools: false, usage: true, reasoning: false,
};
assert!(!capabilities.namespaced_tools);
```

The behavioral test requests a namespaced tool when unsupported and asserts explicit pre-dispatch rejection with zero HTTP calls; do not add a test merely asserting these static flags.
- [ ] Run `just test -p codex-api` and `just test -p codex-core`; expect missing adapter dispatch/contract failures.
- [ ] Implement capability validation before request serialization. Preserve tool-call identities across chunk boundaries and preserve errors as errors; emit usage once with the original request ID. Reuse existing permission/auth/retry services. Reject unsupported reasoning settings instead of discarding them.

```rust
match selected_protocol {
    ProviderProtocol::Responses => responses_request,
    ProviderProtocol::ChatCompletions => chat_request,
    ProviderProtocol::AnthropicMessages => messages_request,
}
```

Each branch must yield the existing request/event type; define conversions within the adapter files rather than changing shared wire meanings.
- [ ] Run the two targeted suites; complete schema updates, then approved full `just test` for core. Run explicitly configured live cases and record model/date/results separately from mocks. Apply scoped lint/fmt after checks.
- [ ] Stage listed paths and commit `feat(api): add qualified optional provider protocols`.

### Task 2: Local commit-to-task links (P3)

**Files:**
- Create: `codex-rs/ext/git-attribution/src/local_provenance.rs`, `codex-rs/ext/git-attribution/src/local_provenance_tests.rs`.
- Modify: `codex-rs/ext/git-attribution/src/lib.rs` and its extension integration module discovered via current exports.
- Test: `codex-rs/ext/git-attribution/src/local_provenance_tests.rs` with temporary repositories and existing Git utilities.

**Interfaces:**
- Consumes: Stage C durable task/snapshot/evidence IDs and existing Git attribution installation in `app-server/src/extensions.rs`.
- Produces: `CommitLink { commit: String, snapshot: String, tasks: Vec<String>, evidence: Vec<String>, relation: AttributionRelation }`; `AttributionRelation::{Exact,ManyToOne,Uncertain}`. `record_link(&self, link: &CommitLink) -> Result<(),std::io::Error>` persists local schema-versioned records; `links_for_commit(&self, commit: &str) -> Result<Vec<CommitLink>,std::io::Error>` returns bounded pages in production.

- [ ] Test a squash of two contributing commits, a cherry-pick and missing session artifacts; assert ambiguity and artifact unavailability rather than choosing one author/task.

```rust
assert_eq!(squashed.relation, AttributionRelation::ManyToOne);
assert_eq!(squashed.tasks, vec!["task-a", "task-b"]);
assert_eq!(git_head_after_lookup, git_head_before_lookup);
```

- [ ] Run `just test -p codex-git-attribution`; expect missing provenance storage behavior.
- [ ] Implement local append/atomic records with immutable original IDs; treat patch/content evidence as a mapping signal, not proof of sole authorship. Expose bounded lookup through the existing attribution service. No Git notes push or commit-message mutation; exports include references by default.

```rust
let relation = match matching_originals.len() {
    0 => AttributionRelation::Uncertain,
    1 => AttributionRelation::Exact,
    _ => AttributionRelation::ManyToOne,
};
```

Use `Exact` only when both content and task association establish the match; one candidate alone is insufficient.
- [ ] Run the scoped suite, including rebase, changed commit message and dirty-tree evidence; scoped lint/fmt.
- [ ] Commit `feat(provenance): link local commits to task evidence`.

### Task 3: Inspectable handoff packages (P4)

**Blocked by:** D2.

**Files:**
- Create: `codex-rs/external-agent-migration/src/handoff.rs`, `codex-rs/external-agent-migration/src/handoff_tests.rs`, `codex-rs/external-agent-migration/src/handoff_adapter.rs`, `codex-rs/core/src/context/handoff.rs`, `research/moedex/handoff-capabilities.md`.
- Modify: `codex-rs/external-agent-migration/src/lib.rs`, `codex-rs/core/src/context/mod.rs`, `codex-rs/cli/src/main.rs` only for CLI dispatch; create `codex-rs/cli/src/handoff_cmd.rs`.
- Test: `codex-rs/external-agent-migration/src/handoff_tests.rs`, `codex-rs/cli/tests/handoff.rs`.

**Interfaces:**
- Consumes: Stage C evidence IDs, Stage E immutable artifact handles, existing migration parsers; adapter chosen in D2.
- Produces: `HandoffPackage { schema_version: u32, package_id: String, goal: String, scope: Vec<String>, decisions: Vec<String>, failures: Vec<String>, outstanding: Vec<String>, artifact_handles: Vec<String> }`; `encode(&HandoffPackage) -> Result<Vec<u8>,HandoffError>` and `decode(&[u8]) -> Result<HandoffPackage,HandoffError>`; errors cover unsupported version, size, malformed data and access. Adapter loss report records preserved/dropped fields. Import summary uses typed historical context, not a user instruction.

- [ ] Add round-trip and size-limit tests, duplicate import detection, changed/missing artifact references and hostile historical instructions.

```rust
assert_eq!(decode(&encode(&package).unwrap()).unwrap(), package);
assert_eq!(decode(&vec![b' '; 10 * 1024 * 1024 + 1]), Err(HandoffError::Size));
```

- [ ] Run `just test -p codex-external-agent-migration` and `just test -p codex-cli`; expect absent encode/import behavior.
- [ ] Implement strict schema validation before import, non-secret selection preview and immutable package identity. Keep large artifacts external by handle; reject oversized package files. Expose explicit `moedex handoff export` and `moedex handoff import` with file target/source and preview; never auto-export on commit. Read target adapter formats only under D2's selected capability contract.

```rust
if bytes.len() > 10 * 1024 * 1024 {
    return Err(HandoffError::Size);
}
```

- [ ] Run scoped migration/CLI/core tests and approved complete-suite gate for new context; verify imported instructions cannot expand authority using a request-capture integration fixture. Record real target round-trip/loss report; scoped lint/fmt.
- [ ] Commit `feat(handoff): add bounded inspectable cross-harness packages`.

### Task 4: Optional summary translation (U6)

**Blocked by:** D3.

**Files:**
- Create: `codex-rs/tui/src/chatwidget/translation.rs`, `codex-rs/tui/src/chatwidget/translation_tests.rs` and snapshots.
- Modify: `codex-rs/tui/src/chatwidget.rs` for module/event dispatch only, `codex-rs/tui/src/app_event.rs`.
- Test: `codex-rs/tui/src/chatwidget/translation_tests.rs`.

**Interfaces:**
- Consumes: Public displayed summary text only, configured D3 backend, Stage C usage request identity.
- Produces: `TranslationState::{Off,Pending,Ready(String),Failed(String)}` and `TranslationJob { generation: u64, original: String, language: String, provider: String }`; completion event carries generation and Result text. The original remains stored separately and unchanged.

- [ ] Write a test toggling off while translation is pending, then deliver a successful old generation. Assert original display and no translated replacement; snapshot pending/failed/provider disclosure.

```rust
controller.disable();
controller.complete(old_generation, Ok("translated".to_owned()));
assert_eq!(controller.state(), &TranslationState::Off);
assert_eq!(controller.original(), original_summary);
```

Define these controller methods inside the new module; `complete` is generation-checked and never mutates conversation history.
- [ ] Run `just test -p codex-tui`; expect lifecycle test failure.
- [ ] Implement off-by-default explicit provider/language selection; account for requests through the common usage path. Send no hidden reasoning; failure restores readable original without blocking composer. Add conservative request/output limits inherited from shared context/store contracts.

```rust
if generation != self.generation {
    return;
}
```

- [ ] Run scoped TUI tests, review/accept only task snapshots, verify zero backend calls with feature off, then lint/fmt.
- [ ] Commit `feat(tui): add opt-in displayed summary translation`.

### Task 5: Dictation into an editable draft (U9)

**Blocked by:** D3.

**Files:**
- Create: `codex-rs/tui/src/chatwidget/dictation.rs`, `codex-rs/tui/src/chatwidget/dictation_tests.rs` and snapshots.
- Modify: `codex-rs/tui/src/chatwidget/realtime/recording_controls.rs`, `codex-rs/tui/src/chatwidget.rs` for narrow event integration, `codex-rs/tui/src/app_event.rs`.
- Test: `codex-rs/tui/src/chatwidget/dictation_tests.rs`.

**Interfaces:**
- Consumes: Existing microphone permissions/recording lifecycle, explicit D3 transcription backend, current user draft revision.
- Produces: `DictationState::{Idle,Recording,Transcribing,Ready(String),Failed(String)}`; `DraftTranscript { recording_id: String, draft_revision: u64, text: String }`. Only a human submit action sends a turn; late transcript with changed draft opens a preview rather than overwriting typed text.

- [ ] Add fake-audio/time tests for 120-second cutoff, cancel, backend failure, draft changed during transcription and successful editable text. Assert no submitted model turn before human submit.

```rust
assert_eq!(submitted_turns.len(), 0);
assert_eq!(retained_audio_bytes, 0);
assert_eq!(composer_text, corrected_transcript);
```

- [ ] Run `just test -p codex-tui`; expect absent dictation lifecycle.
- [ ] Reuse recording devices but separate dictation state from conversational realtime. Enforce duration and byte-rate bound before accepting frames; clear audio on every terminal state and cancellation. Show configured backend and recording status. Preserve ordinary text input and capability-error UI on unsupported hosts.

```rust
if elapsed >= std::time::Duration::from_secs(120) {
    recording.stop();
}
```

- [ ] Run scoped TUI tests and perform explicit on-device microphone/backend acceptance on supported OSes. Retain redacted transcript/status evidence, not audio. Review snapshots, lint/fmt.
- [ ] Commit `feat(tui): add cancellable draft-only dictation`.
