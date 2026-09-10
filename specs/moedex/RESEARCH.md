# Research, context, editing and provenance

Part of [SPEC.md](SPEC.md). Shared contracts and limits apply.

## Research, context, editing, providers and provenance

This section specifies catalog items C1–C15 and P3–P5. Each ID resolves to the corresponding row and pinned donor references in [ENHANCEMENT-CATALOG.md](../../ENHANCEMENT-CATALOG.md). Those references establish inspiration and inspected evidence, not donor runtime qualification or a requirement to copy code. The [baseline audit](../../FORK-BASELINE.md) remains authoritative about existing foundations: hooks, memory, code mode, compaction, model selection and usage reporting already exist. These requirements extend those foundations.

### Shared contracts

A **content handle** identifies a particular immutable content version, its origin, media type, size and access scope. A **snapshot identity** identifies the actual material checked, including applicable uncommitted changes; a Git commit alone is insufficient for a dirty tree. An **evidence record** connects a claim to a check, its outcome, snapshot, timestamp and artifact pointers. These are conceptual interfaces shared with review and recovery; concrete wire APIs belong in implementation planning.

New model-visible fragments MUST be typed in `core/context` and implement `ContextualUserFragment`, while storage and domain logic should live outside the already-large core crate. Each fragment defaults to at most 1,000 tokens. Any design permitting an individual fragment above 1,000 tokens requires the repository's P0 manual review, and no fragment may exceed 10,000 tokens. Every listing, excerpt and event stream MUST have count/byte/token limits and explicit truncation. Persisted artifacts may be larger; retrieving them does not waive these limits. Corrections and refreshed versions MUST append context instead of rewriting prior history. Optional research connectors MUST remain optional and must not make ordinary coding depend on their availability.

### C1 — Federated paper search

**Behavior.** An optional research integration MUST query configured arXiv, Semantic Scholar and OpenAlex sources concurrently, with independent deadlines and cancellation. Results MUST retain source identifiers, retrieval dates and per-source status. DOI/arXiv normalization may merge confidently equivalent records; uncertain matches remain distinct. Ordering must be deterministic for a recorded response set. Totals MUST distinguish provider-reported counts from the deduplicated records actually retrieved.

**Acceptance.** Fixtures with duplicate IDs, conflicting metadata, one timeout and one rate limit produce useful bounded results plus all source statuses. Cancellation stops pending requests. Pagination neither silently loses nor duplicates records. A partial response cannot be labeled exhaustive.

**Trace:** Catalog C1, Ata paper-search implementation.

### C2 — Citation-neighborhood exploration

**Behavior.** From a stable paper identity, users MUST be able to retrieve paginated references and citations and optionally queue selected papers for further reading. Recommendations remain labeled recommendations. Edges retain relationship, provider and retrieval time; citation counts do not become quality scores. Automatic traversal MUST require explicit depth, node and request bounds.

**Acceptance.** A cyclic citation fixture terminates within its declared bounds, deduplicates identities and preserves edge provenance. Missing papers and unsupported relations produce distinct statuses. No queued reading task starts merely because a recommendation appeared.

**Trace:** Catalog C2, Ata relation-search request types.

### C3 — Search personal research libraries

**Behavior.** A Zotero connector MUST search explicitly configured personal/shared library scopes and identify whether a match came from metadata, notes, annotation comments or indexed full text. Results carry library/item identity and bounded excerpts. Discovery MUST be read-only; any future writes use separately authorized operations. Authentication and local-library failures remain visible per scope.

**Acceptance.** Mixed accessible/inaccessible library fixtures retain accessible hits and report failures. Identically titled items in different libraries remain traceable. Large notes and full-text results are capped with truncation notices; a match outside the cap is not represented as fully inspected.

**Trace:** Catalog C3, Ata Zotero tests and output-budget implementation.

### C4 — Traceable document resolution

**Behavior.** Resolution MUST return a content handle and explain which source was selected: canonical document, attachment, permitted local file or indexed text. It MUST preserve bibliographic identity and record fallback reasons. Local paths remain subject to current filesystem permissions and canonical path validation. An index extract MUST never masquerade as the original PDF.

**Acceptance.** Old-style arXiv IDs, malformed URLs, absent attachments, escaping local paths and symlink escapes have explicit outcomes. A changed source creates a new content version. The same resolved version can be cited after the source disappears if its retained artifact remains accessible.

**Trace:** Catalog C4, Ata resolver assertions; donor resolver was not independently audited.

### C5 — Section-oriented report reader

**Behavior.** A persistent TUI reader MUST support section navigation, search, folding and a question tied to the selected section/version. Proposed section updates MUST target that version, preserve unrelated sections and retain a meaningful reading position. A concurrent document edit MUST cause conflict presentation instead of silently applying a stale patch. Leaving the reader returns to the existing conversation.

**Acceptance.** Snapshot and interaction tests cover navigate → ask → proposed update → apply → exit, folded matches, removed sections and stale updates. The question includes bounded section context plus a handle. Rendering and navigation remain usable with a 500-section fixture without injecting the full document.

**Trace:** Catalog C5, Ata reader interaction tests; use focused modules instead of the donor's large implementation.

### C6 — Selective large-context access

**Behavior.** Files and documentation trees MAY be loaded into an external content store. Agents receive immutable handles and bounded metadata, then request selected documents or ranges. Handles MUST retain session/project access scope; another session cannot acquire access merely by guessing an identifier. Refresh produces a new version, and eviction is reported explicitly.

**Acceptance.** Tests cover changed files, missing handles, eviction, authorization failures, binary data and UTF-8 range boundaries. Each fetch respects output bounds and reports its returned range. Repeated retrieval of one retained handle returns the same bytes despite source edits.

**Trace:** Catalog C6, codex-rlm context store.

### C7 — Context coverage reports

**Behavior.** Every bounded indexing/load operation MUST produce coverage counts and exclusion reasons: ignored, oversized, binary, count-limited, unreadable and other documented failures. Reports MUST distinguish discovered, loaded and unexamined material; unavailable enumeration means total coverage is unknown. Representative examples are bounded, with a handle for additional detail.

**Acceptance.** A mixed fixture accounts for every discovered file exactly once in the outcome totals. Hitting a discovery limit reports unexamined scope instead of inventing a denominator. Result wording never converts partial coverage into a claim of complete repository inspection.

**Trace:** Catalog C7, codex-rlm exclusion summaries.

### C8 — Hierarchical documentation routing

**Behavior.** Small manifests MAY route a topic to relevant document branches. Routing MUST disclose the selected branch and support explicit fallback browsing. Applicable ancestor repository instructions MUST be resolved by their actual scope regardless of routing score. Referenced content and manifests retain version identity; missing or cyclic links yield bounded diagnostics.

**Acceptance.** Fixtures verify correct ancestor instructions, cyclic manifests, missing targets, no match and changed manifests. A low relevance score cannot suppress applicable instructions. Measure useful-source recall on a fixed task set before enabling routing by default.

**Trace:** Catalog C8, codex-rlm lexical routing implementation.

### C9 — Bounded structured scratchpad

**Behavior.** A session-scoped scratchpad MUST support versioned put/get/list/delete and bounded batches. Configuration MUST bound stored bytes, keys, per-value size and retrieved output. Concurrent replacement MUST use expected-version checks. Durability across restart must be explicitly advertised; this feature MUST NOT silently replace existing long-term memory.

**Acceptance.** Concurrent writes against the same version have one winner and an explicit conflict. Oversized writes leave previous values intact. Batch failures report each operation's outcome according to a documented atomicity rule. Pagination and retrieval cannot bypass caps; session isolation and deletion are verified.

**Trace:** Catalog C9, codex-rlm memory handlers; version conflict semantics are Moedex's addition.

### C10 — Attributable usage and estimated cost

**Behavior.** The ledger MUST attribute reported input, output and cached tokens to request, model, parent/child agent and task. Estimated prices carry source, currency and effective date. Unknown pricing MUST remain unknown, never zero. Corrections and late usage MUST reconcile without double counting. Any cache-savings number MUST identify its counterfactual estimate.

Spend enforcement is an optional second slice: concurrent admissions MUST atomically reserve a conservative amount against the same parent budget before dispatch. Unknown-priced requests require a configured conservative reservation or are refused under an enforcing monetary budget. Settlement releases unused reservation and records overage. Admission enforcement MUST NOT be described as a guaranteed provider billing ceiling: in-flight usage, delayed reporting and provider billing differences remain distinguishable.

**Acceptance.** Parallel requests cannot over-admit the available reservation budget. Duplicate/late usage, cancellation, missing prices, retries and child aggregation are tested. The displayed total separates known estimated cost from unpriced usage; failed requests with reported usage remain charged to their task.

**Trace:** Catalog C10, codex-rlm cost code; correcting unknown-to-zero behavior and atomic enforcement are Moedex requirements.

### C11 — Alternate provider protocol contracts

**Behavior.** Optional Chat Completions and Anthropic Messages adapters MUST declare supported capabilities rather than imply full compatibility. Tool identities/results, parallel calls, cancellation, errors, usage and compaction behavior MUST preserve the harness's contracts. Unsupported reasoning fields or tool forms MUST fail clearly or use a documented explicitly selected adaptation; no silent semantic loss.

**Acceptance.** Contract fixtures cover streaming boundaries, parallel and failed tools, namespaced tools, malformed events, usage-driven compaction and provider-specific settings. A provider/version matrix separates mock-tested, live-qualified and unsupported behavior. Live qualification requires explicit credentials/configuration and recorded model/version; no provider becomes the default solely from passing mocks.

**Trace:** Catalog C11, Codex++ protocol integration suites.

### C12 — Stale-safe structured editing

**Behavior.** An optional model-selected editing tool MUST bind edits to previously read content and reject stale or ambiguous targets without mutation. Failure returns bounded fresh nearby context. Successful changes MUST emit ordinary file-change/diff events and respect the same permissions as existing editing. Fuzzy relocation is excluded from the initial slice.

**Acceptance.** External writes between read and edit, repeated identical lines, range conflicts, newline variants and Unicode fixtures cannot cause unintended changes. A rejected multi-edit request leaves its declared transactional scope unchanged. A fixed per-model benchmark compares completed correct edits, retries and tokens against existing patches before any default changes.

**Trace:** Catalog C12, Codex++ hashline implementation/tests; short donor hashes are not assumed sufficient content identity.

### C13 — Evidence-backed completion

**Behavior.** Claims MUST link to observed checks/artifacts rather than recycled confidence ratings. Evidence distinguishes passed, failed, skipped, interrupted and unavailable checks and identifies the tested snapshot and environment. Changed relevant content marks prior evidence stale; unknown dependency scope conservatively invalidates snapshot-wide evidence. Completion reports expose unsupported claims and retain historical results.

**Acceptance.** A passing check followed by a source edit cannot substantiate current completion. Skipped tests cannot appear passed. Artifact loss becomes visible. Repeated delivery does not duplicate evidence, and a model's self-assessment alone cannot produce a verified result. Reports separate claim coverage from any optional confidence calibration.

**Trace:** Catalog C13, ecodex hook integration; snapshot invalidation and evidence semantics are proposed Moedex behavior.

### C14 — Explicit workflow policy modes

**Behavior.** Existing hooks MAY implement selected prerequisites with declared advisory or enforcing mode. Each decision MUST identify policy/version, evaluated action, prerequisite evidence, result and applicable exception. Failure/timeout behavior MUST be configured and visible. Policies cannot grant permissions beyond the existing authorization system or claim to be its security boundary.

**Acceptance.** Missing prerequisites, stale evidence, hook timeout, explicit exception and conflicting policies have deterministic recorded results. Advisory failure does not block; enforcing failure follows its configured error policy. The user sees the concrete reason before the blocked action could execute.

**Trace:** Catalog C14, ecodex sentinel source; donor strictness claims are not adopted.

### C15 — Event-driven job wakeups

**Behavior.** An agent MAY subscribe to output from an already-authorized exec session; subscription never starts an independent command. Subscriptions MUST have bounded patterns, event size/count, lifetime, coalescing and explicit cancel/list behavior. Events carry tool/job provenance as typed fragments, never synthetic user instructions. Cancellation and task supersession invalidate pending wakeups. On normal process exit, drain final output, preserve eligible accepted matches, then close the subscription; exit alone must not discard a final completion event.

**Acceptance.** Output floods stay within configured limits and report dropped/coalesced counts. A late match after cancellation starts no turn. One-shot subscriptions fire once; persistent subscriptions expire. Split chunks and stdout/stderr matching are tested. An eligible, unsaturated notification for a ready thread is admitted within one second of event ingestion in a deterministic fixture, without model-side polling; this is not a model-response latency guarantee. Test match followed immediately by exit, including a split final output chunk. Exhausted wake budgets produce visible coalesced/suppressed status without starting another turn, and do not promise one-second activation.

**Trace:** Catalog C15, ecodex monitor; authorized plumbing and bounded injection require redesign.

### P3 — Commit-to-task provenance

**Behavior.** Local provenance MUST link a commit/content snapshot to task identity, contributing sessions and verification pointers without modifying Git history. Rebase/squash reconciliation MUST preserve original identities and label many-to-one or uncertain attribution. Exporting transcripts or sensitive tool output is a separate action, never an implicit consequence of committing or pushing.

**Acceptance.** Tests cover dirty-tree evidence, cherry-pick, squash, rebase and missing session artifacts. Ambiguous attribution stays ambiguous; historical links survive commit-message changes where content mapping provides evidence. Export defaults include references and selected summaries rather than raw conversation/tool logs.

**Trace:** Catalog P3, Atlas documentation; implementation design remains local-first.

### P4 — Inspectable handoff packages

**Behavior.** A versioned handoff MUST carry the authorized goal, scope, decisions, failed approaches, outstanding work and artifact/evidence pointers with origins. Users can inspect the package. Import MUST validate its version, access scope and broken references, and append a bounded summary plus handles. Imported prose is historical context, not authority to expand the task or override current instructions.

**Acceptance.** Cross-session round trips preserve selected facts and attribution. Changed/missing artifacts are flagged; duplicate imports are recognized. Package files over the 10 MiB admission limit are rejected intact. An admitted package may reference larger separately bounded artifacts for selective retrieval; test oversized package rejection separately from referenced-artifact retrieval. An imported instruction to exceed current permissions has no effect. Sensitive fields are omitted unless explicitly selected for export.

**Trace:** Catalog P4, Atlas documentation plus Moedex synthesis.

### P5 — Experimental stored-response branching

**Behavior.** This is a research spike, not a promised production backend. Before live experiments, document current provider storage, retention, deletion, tool-prefix, model and account compatibility from primary sources. Experiments require explicit opt-in to provider retention and a fixed baseline using ordinary history forks. Branch execution MUST remain independent from the mainline.

**Acceptance / decision.** Run matched tasks with identical model/settings and comparable system/tool prefixes; record quality, branch correctness, latency, billed/reported usage and uncertainty across repeated trials. Exercise unavailable checkpoints, retention expiry, deletion, incompatible prefixes and provider rejection. **Go** only if all semantic/retention requirements hold and a predeclared material improvement threshold is met without quality regression. Otherwise record **no-go** and retain ordinary forks. No savings or compatibility claim may be inferred from the donor benchmark alone.

**Trace:** Catalog P5, Nanocodex benchmark with documented confounders.
