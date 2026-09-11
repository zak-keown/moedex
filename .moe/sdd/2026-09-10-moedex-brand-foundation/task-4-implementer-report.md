# Task 4 implementer report

Implemented the explicit, preview-first `moedex import codex` flow.

- Added immutable in-process previews keyed by a content-derived ID, with canonical source/destination equality rejection including symlink aliases.
- Added typed `config.toml` validation and a non-secret sanitizer. Project trust, credential-like values, absolute/home-bound paths, notifications, hooks, commands, and executable settings are omitted and identified for manual review.
- Added native rollout discovery and record validation. Imports copy validated JSONL rollouts and never copy live SQLite files. Duplicate destination thread IDs are retained and reported as incompatible.
- Added conflict-skip defaults, explicit replacement backups, source revalidation after preview, atomic sibling writes with fsync where supported, a per-item import ledger, and idempotent recovery after partial application.
- Kept credential import default-off and represented an explicit credential selection as a redacted unsupported item pending Task 5's namespace-aware auth adapter.
- Registered `moedex import codex` with `--dry-run`, `--settings`, `--sessions`, `--credentials`, `--replace`, and explicit home overrides.
- With parent approval, expanded ownership to correct four stale CLI expectations exposed by the full regression: the reserved `import` prompt case, app-server binary lookup, `moedex cloud` spelling, and Moedex version output under sandbox execution. Provider/service naming remains unchanged.

Validation:

- `just bazel-lock-update` completed; no `MODULE.bazel.lock` change was needed.
- `just test -p codex-external-agent-migration -p codex-cli`: 628 passed, 0 failed (one pre-existing slow test and one leaky-process annotation).
- `just fix -p codex-external-agent-migration`: clean.
- `just fix -p codex-cli`: clean.
- `just fmt`: clean and run last, with no test rerun afterward per repository instructions.

The complete workspace suite was not run.

## Review fix round

Resolved all six findings from `task-4-review.md`:

- Credential-free settings now structurally remove typed MCP and model-provider credential/header fields, including literal `Authorization` values and arbitrary header maps.
- Each preview item is validated, hashed, and materialized from one bounded immutable snapshot. Apply revalidates that hash, rejects a concurrently changing capture, and includes a LocalThreadStore read-through regression.
- Session conflicts retain the destination for matching thread IDs at any path and for any different session already occupying the target path, regardless of `--replace`.
- Ledger writes use the repository's cross-platform atomic replacement helper, with multi-record and idempotent-rerun coverage.
- Preview admission and the process registry have hard item, per-item byte, aggregate byte, and preview-count bounds; applied and explicitly cancelled previews are removed and oldest previews are evicted.
- Apply returns bounded, sanitized per-item outcomes with kind, path, disposition, hash, and reason.

Expanded test-harness ownership by changing the managed MCP OAuth integration fixture to `--no-browser` pasteback input. The previous package-wide command opened a loopback browser tab because `login_and_logout_persist_only_cloud_managed_mcp_oauth_credentials` exercised browser-based login. The revised fixture preserves its token exchange without launching the user's browser.

Review-fix validation, in execution order:

- `just bazel-lock-update`: completed with no `MODULE.bazel.lock` delta.
- `just test -p codex-external-agent-migration -p codex-cli`: 637 passed before the OAuth harness correction; this broad selection was not rerun.
- `just test -p codex-external-agent-migration source_codex`: 17 passed.
- `just test -p codex-cli import_codex`: 2 passed.
- `just test -p codex-cli login_and_logout_persist_only_cloud_managed_mcp_oauth_credentials`: 1 passed with the non-browser fixture.
- `just fix -p codex-external-agent-migration`: completed and applied two Clippy fixes.
- `just fix -p codex-cli`: completed cleanly.
- `just fmt`: completed last; tests were not rerun afterward as required.
