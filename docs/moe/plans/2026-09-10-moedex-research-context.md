# Moedex Research and Context Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Deliver Stage E's bounded, traceable research workspace, covering C1–C9.

**Architecture:** Put content storage, routing and source adapters in a new `codex-research` crate; keep core integration limited to contextual fragments and existing extension registration. A separately owned TUI reader consumes immutable handles and version-checked document updates. Research remains disabled until configured and never becomes a coding startup dependency.

**Tech Stack:** Rust workspace, existing HTTP client factory/permissions, serde, Tokio, SQLite or atomic versioned files using existing workspace storage dependencies, ratatui, insta, Cargo/Just/Bazel.

**Spec:** `specs/moedex/SPEC.md`, `specs/moedex/RESEARCH.md` C1–C9.

## Global Constraints

- “1,000 tokens per fragment and 4,000 new tokens per turn across these enhancements”. Direct human instructions are never silently shortened.
- “1 GiB/project, 10,000 documents, 20 MiB/document”; pin refusal must be explicit.
- “256 keys/session, 64 KiB/value, 4 MiB/session, 32 operations/batch”; atomic batch conflict leaves all keys unchanged.
- “20 results/page, 100 maximum/page, 15-second deadline/source”; traversal depth 1, 100 nodes, 20 requests by default.
- Linux, macOS and Windows, including supported mixed-OS server configurations; host-native path permissions remain authoritative.
- Keep modules private with deliberate exports; target fewer than 500 non-test lines/module. New unit test modules use explicit sibling `_tests.rs` files.
- Use `just test`, never direct `cargo test`. Run Rust `just` and `cargo insta` commands from `codex-rs`. Core changes require the complete `just test` only after the repository-required approval; record that gate before declaring integration complete.
- Any Cargo dependency change includes `just bazel-lock-update` and `MODULE.bazel.lock`. Compile-time fixtures need `BUILD.bazel` data declarations. Run scoped `just fix -p PACKAGE` then `just fmt` after successful tests; do not rerun tests after fix/fmt.

## Open Decisions

This plan is not dispatchable until D1 is resolved. Stage A home isolation is an implementation prerequisite.

- **D1 — Live research API contracts** · `research` · AFK
  - **Question:** Which current API endpoints, authentication modes, quotas and pagination contracts support arXiv, Semantic Scholar, OpenAlex and Zotero read-only discovery?
  - **Options:** Official supported endpoint / explicitly unsupported adapter capability.
  - **Recommendation:** Verify primary documentation at execution time; retain partial-source errors and avoid unofficial scraping fallbacks.
  - **Blocked by:** —
  - **Blocks:** Task 3, Task 4.
  - **Resolution:** Unresolved; commit dated links, endpoint examples and credential requirements to `research/moedex/source-capabilities.md` before dispatch. Missing credentials block live qualification, not permission-mocked fixtures.

## Not Yet Specified

None beyond D1. Exact wire method spelling is deliberately avoided: first delivery uses extension tools and an internal reader API, with no new public RPC.

## Out of Scope

- Alternate inference providers, voice and translation: Stage F.
- Editing source code through document-reader patches: C12 owns code edits; this reader updates its admitted research report.
- Library mutation and automatic downloading outside existing access policy.

## File Structure

Create `codex-rs/research/{Cargo.toml,BUILD.bazel,src/lib.rs}` with private `store`, `coverage`, `routing`, `scratchpad`, `papers`, `zotero`, `resolve`, `tools` modules and sibling tests. Modify workspace membership/dependencies and `app-server/src/extensions.rs` (existing `thread_extensions` registration). Create `core/src/context/research.rs` and a focused `tui/src/bottom_pane/research_reader/` module. Existing anchors inspected: `ext/goal/{Cargo.toml,BUILD.bazel,src/lib.rs}`, `app-server/src/extensions.rs`, `http-client`, and `tui/src/bottom_pane/mod.rs`.

### Task 1: Immutable scoped content and coverage (C6/C7)

**Files:**
- Create: `codex-rs/research/Cargo.toml`, `codex-rs/research/BUILD.bazel`, `codex-rs/research/src/lib.rs`, `codex-rs/research/src/store.rs`, `codex-rs/research/src/store_tests.rs`, `codex-rs/research/src/coverage.rs`.
- Modify: `codex-rs/Cargo.toml`, `codex-rs/Cargo.lock`, `MODULE.bazel.lock`.
- Test: `codex-rs/research/src/store_tests.rs`.

**Interfaces:**
- Consumes: None; caller supplies already-authorized bytes and project identity.
- Produces: `ContentStore::admit(&mut self, project: &str, bytes: &[u8]) -> Result<String, StoreError>` and `fetch(&self, project: &str, handle: &str, range: std::ops::Range<usize>) -> Result<Vec<u8>, StoreError>`; String handles are opaque immutable content-version IDs. `StoreError` exhaustively distinguishes `Missing`, `Expired`, `Denied`, `InvalidRange`, `Capacity` and `Io(String)`. `Coverage` contains `discovered: Option<u64>`, `loaded: u64`, `excluded: BTreeMap<String,u64>` and bounded `examples: Vec<String>`.

- [ ] Write a failing test using a temporary store, admitting `b"old"`, changing the source fixture to `b"new"`, then asserting the old handle still returns `b"old"`; cross-project fetch must return `Denied`. Include UTF-8 split-range rejection, pin refusal and unknown denominator.

```rust
let handle = store.admit("project-a", b"old").unwrap();
assert_eq!(store.fetch("project-a", &handle, 0..3).unwrap(), b"old");
assert_eq!(store.fetch("project-b", &handle, 0..3), Err(StoreError::Denied));
```

- [ ] Run `just test -p codex-research`; expect the missing store API test to fail after registering the crate, before implementation.
- [ ] Implement atomic content admission and manifests under the resolved host's Moedex project storage. Do not read arbitrary paths inside `admit`; filesystem enumeration uses approved callers and reports every excluded file. Evict only unpinned LRU records; retain expired-handle tombstones within the store's finite budget. Implement the permission branch before content lookup:

```rust
if owner_project != requested_project {
    return Err(StoreError::Denied);
}
```

- [ ] Run `just test -p codex-research`, `just bazel-lock-update`, scoped lint and formatting. Confirm independent source mutation cannot alter retained bytes.
- [ ] Commit the listed files: `git commit -m "feat(research): add scoped immutable content and coverage"` after explicit `git add` of this task's paths.

### Task 2: Routing and durable scratchpad (C8/C9)

**Files:**
- Create: `codex-rs/research/src/routing.rs`, `codex-rs/research/src/routing_tests.rs`, `codex-rs/research/src/scratchpad.rs`, `codex-rs/research/src/scratchpad_tests.rs`.
- Modify: `codex-rs/research/src/lib.rs`.
- Test: the two sibling test files.

**Interfaces:**
- Consumes: Task 1 opaque content handles and `Coverage`.
- Produces: `route(topic: &str, manifests: &[RouteManifest]) -> RouteSelection`; `RouteManifest { id: String, parent: Option<String>, topics: Vec<String>, handles: Vec<String> }`; `RouteSelection { handles: Vec<String>, diagnostics: Vec<String> }`. `Scratchpad::apply(&mut self, session: &str, batch: &[ScratchWrite]) -> Result<Vec<u64>, ScratchError>` with `ScratchWrite { key: String, expected_version: Option<u64>, value: Option<Vec<u8>> }` (None value deletes; None expected version creates only). `ScratchError` distinguishes conflict, capacity, missing session and I/O.

- [ ] Add failing cycle/no-match routing tests and a scratchpad conflict test: create keys A/B, issue one batch with current A/stale B, and compare the complete store snapshot before/after.

```rust
let before = pad.get("s", "a").unwrap();
assert_eq!(pad.apply("s", &conflicting_batch), Err(ScratchError::Conflict));
assert_eq!(pad.get("s", "a").unwrap(), before);
```

Implement `list(&self, session: &str, cursor: Option<&str>, limit: usize) -> Result<Vec<(String,u64)>,ScratchError>` and `get(&self, session: &str, key: &str) -> Result<(u64,Vec<u8>),ScratchError>` for production retrieval. The test assembles `before` and the post-write value map from these methods; do not introduce a test-only production snapshot method. Both methods enforce shared retrieval caps.
- [ ] Run `just test -p codex-research`; expect cycle termination/version-conflict assertions to fail.
- [ ] Implement validate-all-then-commit scratchpad transactions; persist with schema version and atomic replacement. Route with a visited set and finite manifests/edges; missing links become diagnostics. Instruction loading stays in the existing scope resolver and is never filtered by route score.

```rust
let mut visited = std::collections::BTreeSet::new();
if !visited.insert(manifest.id.clone()) {
    diagnostics.push(format!("routing cycle at {}", manifest.id));
}
```

- [ ] Run `just test -p codex-research`; verify restart retention, delete, oversize-value rejection, bounded list and ancestor-instruction integration fixtures; then scoped lint/fmt.
- [ ] Stage only task paths and commit `feat(research): add bounded routing and transactional scratchpad`.

### Task 3: Federated papers and citation traversal (C1/C2)

**Blocked by:** D1.

**Files:**
- Create: `codex-rs/research/src/papers.rs`, `codex-rs/research/src/papers/arxiv.rs`, `codex-rs/research/src/papers/semantic_scholar.rs`, `codex-rs/research/src/papers/openalex.rs`, `codex-rs/research/src/papers_tests.rs`, `research/moedex/source-capabilities.md`.
- Modify: `codex-rs/research/src/lib.rs`, `codex-rs/research/Cargo.toml`, `codex-rs/research/BUILD.bazel` and lockfiles if needed.
- Test: `codex-rs/research/src/papers_tests.rs` using local mock HTTP endpoints.

**Interfaces:**
- Consumes: Existing `codex_http_client::HttpClientFactory`, task cancellation and resolved D1 source contracts.
- Produces: `PaperSearch::search(&self, query: &str, cursor: Option<&str>, limit: usize) -> impl Future<Output=Result<SearchPage,SearchError>> + Send`; `SearchPage { items: Vec<Paper>, sources: Vec<SourceOutcome>, next_cursor: Option<String> }`; `Paper { id: String, title: String, source_ids: BTreeMap<String,String>, retrieved_at: i64 }`; `SourceOutcome { source: String, status: String, reported_total: Option<u64> }`. Relation lookup returns the same page type plus typed `CitationEdge { from: String, to: String, relation: Relation, source: String, retrieved_at: i64 }`, where `Relation` is `Reference`, `Citation`, or `Recommendation`.

- [ ] Build mocked source responses with two equal DOIs, one uncertain title-only match, one timeout and a citation cycle. Assert merged IDs, preserved uncertain records and all source outcomes.

```rust
assert_eq!(page.sources.iter().map(|s| s.source.as_str()).collect::<Vec<_>>(),
           vec!["arxiv", "openalex", "semanticScholar"]);
assert!(page.sources.iter().any(|s| s.status == "timeout"));
```

- [ ] Run `just test -p codex-research`; expect missing adapters then behavior failures.
- [ ] Implement each adapter from D1's recorded endpoint contracts, independent deadline/cancellation and deterministic sort. Cursor state records per-source cursors plus emitted canonical IDs within bounded traversal state; never use an infinite seen-ID collection. Emit a continuation-exhausted result if the state budget is reached. Recommendations never dispatch tasks.

```rust
let deadline = std::time::Duration::from_secs(15);
let outcome = tokio::time::timeout(deadline, source_request).await;
```

- [ ] Run `just test -p codex-research`; add pagination replay, source rate-limit and cancellation assertions. Record an opt-in live smoke only with configured credentials; mock success is not live certification. Run lock update for dependencies, scoped lint/fmt.
- [ ] Commit `feat(research): add federated papers and bounded citation traversal` with listed paths only.

### Task 4: Read-only Zotero and canonical resolution (C3/C4)

**Blocked by:** D1.

**Files:**
- Create: `codex-rs/research/src/zotero.rs`, `codex-rs/research/src/zotero_tests.rs`, `codex-rs/research/src/resolve.rs`, `codex-rs/research/src/resolve_tests.rs`.
- Modify: `codex-rs/research/src/lib.rs`.
- Test: sibling tests with permission-aware local fixtures and mocked library scopes.

**Interfaces:**
- Consumes: Task 1 `ContentStore`, Task 3 source-status semantics, existing filesystem/network access mediation.
- Produces: `LibraryHit { library: String, item: String, origin: HitOrigin, excerpt: String, truncated: bool }` with `HitOrigin::{Metadata,Note,Annotation,FullText}`; `ResolvedDocument { handle: String, bibliographic_id: String, source: DocumentSource, fallback_reasons: Vec<String> }` and `DocumentSource::{Canonical,Attachment,LocalFile,IndexedText}`. `resolve(item: &str) -> impl Future<Output=Result<ResolvedDocument,ResolveError>> + Send` belongs to a configured resolver holding the approved stores/transports.

- [ ] Write failures for one inaccessible shared library alongside accessible hits, an old arXiv ID, symlink escape, and missing attachment falling back to indexed text.

```rust
assert_eq!(resolved.source, DocumentSource::IndexedText);
assert_eq!(resolved.fallback_reasons, vec!["attachment unavailable"]);
assert!(hits.iter().any(|hit| hit.truncated));
```

- [ ] Run `just test -p codex-research`; expect resolution/provenance failures.
- [ ] Implement read-only requests and canonicalization through permitted path operations. Preserve source scope in every hit, admit resolved bytes to Task 1, record origin and fallback explicitly. Reject unsafe local resolution before opening bytes; never implement library-write commands.

```rust
let source = if indexed_text_used {
    DocumentSource::IndexedText
} else {
    selected_source
};
```

- [ ] Run `just test -p codex-research`; verify changed content produces a new handle and retained prior citations survive source deletion; scoped lint/fmt.
- [ ] Commit `feat(research): add library provenance and document resolution`.

### Task 5: Bounded extension integration (C1–C4/C6–C9)

**Files:**
- Create: `codex-rs/research/src/tools.rs`, `codex-rs/core/src/context/research.rs`, `codex-rs/app-server/tests/suite/v2/research_extension.rs`.
- Modify: `codex-rs/app-server/src/extensions.rs`, `codex-rs/app-server/Cargo.toml`, `codex-rs/core/src/context/mod.rs`, `codex-rs/app-server/tests/suite/v2/mod.rs`, dependency lockfiles.
- Test: `codex-rs/app-server/tests/suite/v2/research_extension.rs`.

**Interfaces:**
- Consumes: Tasks 1–4 services; existing `ExtensionRegistryBuilder<Config>` registration pattern from `ext/goal` and existing user config feature selection.
- Produces: Disabled-by-default `research_search`, `research_relations`, `research_library`, `research_resolve`, `context_fetch`, `context_coverage`, `context_route`, `scratchpad_read` and `scratchpad_write` tool registrations. `ResearchFragment` implements the actual `ContextualUserFragment` contract and carries source kind, handle, bounded excerpt and truncation/coverage metadata.

- [ ] Write public app-server integration fixtures with auto-env builder: disabled research emits no tool definitions and makes zero source calls; enabled search returns partial failure with bounded typed provenance.

```rust
assert_eq!(source_call_count.load(std::sync::atomic::Ordering::SeqCst), 0);
assert!(serialized_fragment_token_count <= 1_000);
assert!(new_fragment_tokens_in_turn <= 4_000);
```

- [ ] Run `just test -p codex-app-server`; expect absent enabled tools. Use request-capture helpers for actual model request assertions, not string-only registration tests.
- [ ] Register thin handlers through `thread_extensions`; keep transport/store ownership outside core. Apply conservative token admission before constructing `ResearchFragment`; return handle/pagination on overflow. If new `ConfigToml` fields are needed, define a focused nested config type and run `just write-config-schema`; never hide config schema changes in later commits.
- [ ] Run `just test -p codex-research` and `just test -p codex-app-server`; request required approval for full `just test` because core changed. Refresh Bazel lock, scoped lint/fmt after tests. No complete claim without the approved full-suite result.
- [ ] Commit `feat(research): connect bounded research tools to sessions`.

### Task 6: Section reader and version-checked updates (C5)

**Files:**
- Create: `codex-rs/tui/src/bottom_pane/research_reader/mod.rs`, `codex-rs/tui/src/bottom_pane/research_reader/document.rs`, `codex-rs/tui/src/bottom_pane/research_reader/reader_tests.rs` and resulting insta snapshots in that module's `snapshots/` directory.
- Modify: `codex-rs/tui/src/bottom_pane/mod.rs`, `codex-rs/tui/src/app.rs` only for event dispatch, `codex-rs/tui/src/app_event.rs` for reader events.
- Test: `codex-rs/tui/src/bottom_pane/research_reader/reader_tests.rs`.

**Interfaces:**
- Consumes: Task 1 handle identity, Task 5 bounded question submission and ordinary TUI events.
- Produces: `ReportDocument { version: String, sections: Vec<ReportSection> }`, `ReportSection { id: String, heading: String, body: String }`; `apply_section(&mut self, expected_version: &str, section_id: &str, replacement: &str) -> Result<(),SectionConflict>`. No standalone reader orchestration methods added to `chatwidget.rs`.

- [ ] Write navigation/question/apply/exit and stale-version tests; capture narrow-terminal, folded match and 500-section snapshots.

```rust
let before = document.clone();
assert_eq!(document.apply_section("old-version", "intro", "replacement"), Err(SectionConflict));
assert_eq!(document, before);
```

- [ ] Run `just test -p codex-tui`; expect missing reader behavior/snapshot failures.
- [ ] Implement stable section IDs and position restoration to the selected section or nearest surviving neighbor. Questions include selected section version and bounded excerpt; patches target the admitted report only. Use existing wrapping helpers and Stylize, with no provider call from render.
- [ ] Run `just test -p codex-tui`; inspect `cargo insta pending-snapshots -p codex-tui` from `codex-rs`, read each new snapshot, accept only intentional reader snapshots. Then scoped lint/fmt; do not retest after formatting.
- [ ] Commit `feat(tui): add versioned research report reader` with reader files, narrow event integration and reviewed snapshots.
