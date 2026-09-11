# P3: Primary Cargo/nextest verification

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: proposed; activation depends on P2 qualification.

## Outcome and boundary

Provide one authoritative Cargo-based PR result while preserving Bazel compatibility and broader platform evidence. This simplifies the default workflow; it does not promise lower elapsed time. An affected-crate selector and new developer command framework are outside this stage.

## Requirements

| ID | Contract |
| --- | --- |
| P3-01 | Default Rust PR verification uses workspace nextest and Cargo Clippy with test targets on one qualified native Linux, macOS, and Windows architecture each. Retain formatting, dependency policy, generated-schema checks, relevant SDK checks, and meaningful behavioral/snapshot coverage. Avoid routine `--all-features`. |
| P3-02 | Required checks validate GitHub's synthetic PR merge revision, including workflows, fixtures, and result helper. Record its SHA in summaries and reusable artifacts. Archive consumers reject candidate, target, or profile mismatch. Postmerge verification uses the pushed main SHA. |
| P3-03 | Preserve the stable `CI required` result and `always()` fan-in. Failed, cancelled, missing, or unexpectedly skipped required dependencies cannot pass. Do not add path-skipping of required workflows at initial cutover. Existing conditional leaf selection must be validated by its controller; never globally accept `skipped` as success. |
| P3-04 | Run maintained Bazel verification daily on main and on manual dispatch for a recorded revision. Preserve broader Cargo architecture, remote-environment, and release-profile checks after merge; keep V8 canary observable separately. Secondary failures remain failed results with logs and an investigation path. |
| P3-05 | Retain Bazel definitions, runfiles support, dependency pins, and required lockfile drift validation. AGENTS.md requirements for `just bazel-lock-update` and compile-time resource declarations remain binding. Moving the broad Bazel matrix does not move lock drift out of required CI. |
| P3-06 | Keep `just test` as the local test entry point and the existing approval requirement for an agent's full local suite. Repository-configured CI may run unattended and reuse its archive-backed nextest machinery. Any new CI helper must be introduced explicitly, not described as already existing. |
| P3-07 | Before cutover, compare identical candidate revisions for test inventory, omissions, resource use, timings, and retry behavior. Resolve coverage gaps. Activate only when the Cargo path meets correctness and operational feasibility; otherwise retain the qualified previous gate. |
| P3-08 | P4's advisory workspace lint is outside the required dependency graph; changed lint-component tests remain required. Preserve ordinary Clippy and other checks independently so advisory treatment cannot hide a correctness failure. |
| P3-09 | Release qualification consumes all applicable P2 coverage at the release candidate revision, including Bazel compatibility and relevant remote/platform checks. A green PR or yesterday's scheduled run is insufficient. A known relevant secondary failure blocks release qualification until fixed or shown inapplicable with recorded evidence. |

## Acceptance scenarios

1. A failing integration test, Clippy error, or schema drift fails `CI required`.
2. Failure, cancellation, or unexpected skipping of a required child cannot yield a green terminal result. Test missing-result handling at the controller boundary as well as dependency aggregation.
3. A passing PR head whose synthetic merge candidate fails remains blocked. Reusing an archive from a different candidate is rejected.
4. A dependency change with missing Bazel lock synchronization fails required verification even though the broad Bazel test matrix is secondary.
5. A scheduled Bazel failure remains visible and diagnosable without changing the PR's Cargo gate result; a release candidate cannot inherit success from an older secondary run.
6. Coverage inventory accounts for every retained postmerge/release case. Advisory lint failure cannot mask or replace these results.

## Migration and rollback

Qualify Cargo as a visible candidate path, reconcile coverage, then change required dependencies while retaining the terminal check identity. Update workflow strategy and applicable AGENTS.md guidance in the same change. Prepare any required GitHub ruleset delta explicitly; changing hosted settings is separate from editing repository files.

Activation evidence includes a deliberately failing candidate that the actual required-check configuration blocks. Rollback restores the previous qualified gate and its fork-accessible runners; never remove checks merely to unblock a merge. During qualification, duplicate runs are temporary measurement overhead, not the permanent default.

## Source anchors

- [Workflow strategy](../../../.github/workflows/README.md): current Bazel PR/Cargo postmerge split.
- [Blocking gate](../../../.github/workflows/blocking-ci.yml), [result helper](../../../.github/scripts/check_ci_results.py), and [postmerge](../../../.github/workflows/postmerge-ci.yml): result aggregation.
- [Full Rust CI](../../../.github/workflows/rust-ci-full.yml) and [justfile](../../../justfile): reusable Cargo/nextest paths.

Main risks are hidden coverage loss and higher Cargo latency on ordinary runners. P2/P3 evidence gates address both without guessing at a faster matrix.
