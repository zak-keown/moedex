# P4: Advisory argument-comment lint

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: proposed; current repository instructions remain in force.

## Outcome and boundary

Keep readable Rust call sites while removing the custom compiler lint from the routine blocking PR path. Preserve the lint source and explicit invocation for contributors who want it. This changes enforcement scheduling, not Rust semantics or the recommendation to use self-documenting APIs.

The baseline [lint README](../../../tools/argument-comment-lint/README.md) describes Dylint, a pinned nightly toolchain, and repo wrappers that promote findings to errors. [rust-ci.yml](../../../.github/workflows/rust-ci.yml) runs package tests and a three-platform lint matrix and includes them in its result aggregation. [The terminal gate](../../../.github/workflows/blocking-ci.yml) depends on that aggregation, so changing only the job label would not make it advisory.

## Requirements

| ID | Contract |
| --- | --- |
| P4-01 | Retain the exact `/*param_name*/` convention, existing exemptions, and preference for enums/named APIs. Do not perform bulk comment rewrites or relax ordinary Clippy rules. |
| P4-02 | Move repository-wide custom lint execution to an explicitly dispatched advisory workflow. Default PR, push, scheduled correctness, and release gates do not require its results or install its pinned nightly solely for this lint. |
| P4-03 | Preserve `just argument-comment-lint -p <crate>` and the existing source/prebuilt paths. A direct command retains its truthful nonzero exit on a finding or tool failure. Advisory means excluded from the merge dependency graph, not silently converted to success. |
| P4-04 | An advisory run reports its revision, platform, completion state, and findings through ordinary Actions logs/artifacts. Toolchain/download failure is reported as unavailable/error, never zero findings. It has no permission to post comments or modify code. |
| P4-05 | Keep focused tests of the custom lint and wrappers required when those components change. These component tests are separate from applying the lint to the entire workspace; unrelated changes must not install their toolchain. Include changes to their workflow/action wiring in the selection. |
| P4-06 | P3 owns terminal gate membership. Remove advisory invocation from every required dependency chain, including postmerge/release aggregators, while retaining the ordinary Rust, schema, security, and package checks. Do not apply `continue-on-error` to a combined correctness job. |
| P4-07 | Update AGENTS.md enforcement guidance, the lint README, and workflow strategy in the same coherent change. Explain the manual command and preserve the source-level readability agreement. Update stale individually required GitHub check names only as a separately authorized repository-settings operation. |

## Acceptance scenarios

1. **Ordinary Rust PR:** evaluate workflow selection for a source-only change. Correctness checks run; no custom lint toolchain setup or advisory dependency is selected. The required terminal check reports the actual correctness results.
2. **Manual finding:** run the existing lint against a fixture with a mismatched argument comment. It emits the diagnostic and fails its advisory job; the terminal correctness gate has no dependency on that job.
3. **Tool failure:** make the advisory toolchain unavailable. The job reports failure with actionable setup information; it does not publish a clean result or block an unrelated PR.
4. **Lint implementation change:** a regression in an existing wrapper or lint fixture fails the focused required component test. This remains true after removing workspace-wide enforcement.
5. **Gate isolation:** exercise failure/cancellation of an ordinary Rust check alongside successful or failed advisory lint results. The correctness failure still blocks. Reuse gate/workflow-selection tests rather than adding a text assertion that a deleted job name is absent.

## Migration and rollback

First separate lint component testing from workspace-wide enforcement. Then wire the advisory workflow and update aggregators/policy together. P3 must demonstrate that changed membership cannot produce a false green or leave an obsolete check permanently pending. No application config or user state migrates.

Rollback restores the former blocking job and required-check membership with the same revision semantics. Historical advisory failures remain visible. Loss of automatic readability enforcement is the accepted tradeoff; no build-time saving is claimed until measured.
