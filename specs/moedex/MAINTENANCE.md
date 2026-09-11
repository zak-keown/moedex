# Moedex maintenance realignment

**Status:** Proposed specification, 2026-09-11. Six priorities specified; implementation and hosted qualification have not started. This is repository and distribution maintenance, not a plugin.

## Purpose and scope

Make Moedex independently maintainable while preserving automated evidence for agent behavior, platform safety, compatibility, and packaged execution. This specification expands the six numbered recommendations from the fork assessment. It does not include the separate suggestions for a doctor command, change-aware test selector, or capability-report UI.

The inspected source revision is `e772a65de55fa84e9faebf786cd1e2c6b50e447c`; its upstream baseline is `8e2afc09126c0cea4c282725fe68af43adad73d7`. Source anchors describe that checkout. Proposed commands, policies, and acceptance cases below are requirements, not claims that implementations or passing evidence already exist.

| Priority | Contract | Outcome |
| --- | --- | --- |
| P1 | [Release ownership](maintenance/01-release-ownership.md) | GitHub-first fork distribution with qualified artifacts and no upstream publishing fallback |
| P2 | [Portable CI](maintenance/02-portable-ci.md) | Verification runs on provisioned fork-accessible runners without OpenAI credentials |
| P3 | [Primary verification path](maintenance/03-verification-path.md) | Cargo/nextest is the proposed primary PR path; Bazel compatibility remains exercised |
| P4 | [Advisory argument-comment lint](maintenance/04-advisory-lint.md) | The readability convention remains, with enforcement outside the blocking correctness path |
| P5 | [Telemetry defaults](maintenance/05-telemetry-defaults.md) | Local diagnostics remain available; outbound observability requires explicit configuration |
| P6 | [Community automation](maintenance/06-community-automation.md) | Inherited community bots remain inactive without weakening build or dependency checks |

## Design choice

Three approaches were considered. Keeping upstream automation unchanged minimizes the initial diff but retains unavailable release identities and runner assumptions. Aggressive removal of build systems, crates, and compatibility machinery reduces the visible surface but expands future merge conflicts and verification gaps. The selected proposal is to replace operational dependencies and simplify default workflows while retaining source compatibility and reusable checks.

Cargo-first is a proposed policy choice, not a measured speed claim. P2/P3 require timing, coverage, and runner-feasibility evidence before replacing the required gate. An unsuccessful qualification leaves the working gate in place. Runtime feature disabling and compilation/package reduction are different changes; removing V8, code mode, SDKs, or platform helpers is outside these six priorities.

## Shared requirements

- Preserve [SPEC.md](SPEC.md) and [BRAND.md](BRAND.md) identity, home isolation, provider authentication, protocol, rollout, and sandbox contracts. Retain internal `codex-*` names and attribution.
- Never modify code related to `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR`.
- Keep integration coverage for changed agent logic, reviewed UI snapshots, schema drift checks, dependency checks, and package qualification. Tests must exercise behavior; do not add tests that merely assert static constants or deleted code is absent.
- Preserve Linux, macOS, and Windows core support. Qualify each advertised release target; an unavailable runner is missing evidence, not a passing or silently excluded platform.
- Retain Cargo/Bazel lock synchronization and Bazel compile-time resource declarations. Run `just bazel-lock-update` when changing Rust dependencies. P3 does not repeal these requirements.
- During implementation, use scoped `just test`, required schema regeneration, scoped `just fix` where applicable, and `just fmt` in the repository-prescribed order. Local full-suite execution still requires the existing user approval; configured CI runs do not require an interactive approval prompt.
- Keep independent changes reviewable, normally under 800 changed lines and complex logic under 500. Split implementations by coherent behavior rather than mixing all six priorities into one patch.
- Changes to repository policy, required checks, release permissions, and service destinations must be explicit in implementation diffs. These draft documents do not themselves change AGENTS.md, GitHub rulesets, configuration, or workflows.
- Acceptance evidence records the tested source revision, job/command, platform, result, and relevant artifact digest. Skipped, cancelled, unavailable, and untested cases remain distinguishable from success. Use existing CI reports and package provenance rather than introducing a second evidence service.

## Ownership and sequencing

Priority expresses value, not a strictly serial execution order. P2 establishes runner capability; P3 consumes that capability to qualify a new gate. P4 can be prepared independently, but P3 owns its placement in the final required-check graph. P6 can land independently. P5 can be implemented independently and must be included in qualification of the first maintenance-realigned release. P1 consumes P2's target evidence and P3's release verification policy.

| Existing artifact | Required reconciliation before implementation |
| --- | --- |
| [Brand foundation plan](../../docs/moe/plans/2026-09-10-moedex-brand-foundation.md) | Extend Tasks 6–7 with P1 distribution/qualification requirements and P5 release acceptance; reuse identity/provenance and package helpers rather than creating competing implementations. The existing plan excludes publication and remains so. |
| [Plan manifest](../../docs/moe/plans/moedex-MANIFEST.md) | Leave statuses and dependency edges unchanged during specification. At implementation planning, attach P1 to brand-foundation and add separately scoped P2–P6 plans with explicit dependencies and file ownership. Do not mark a runtime plan done for completing these documents. |
| AGENTS.md and workflow strategy | P3/P4 implementation must update applicable policy text together with workflows. Until that lands, current local instructions remain authoritative. |
| BRAND.md B8–B10/B13 | P1 supplies operational detail under the existing identity and distribution contract. It does not rename providers or redirect model traffic. |

Re-check the manifest with `plan-set next --manifest docs/moe/plans/moedex-MANIFEST.md` when implementation starts; the inspected manifest reports `brand-foundation`. This specification set is not a runnable task plan.

## Decisions and prerequisites

GitHub Releases first, advisory custom lint, disabled inherited bots, and explicit outbound telemetry are the proposed defaults. Public npm/Homebrew/WinGet/PyPI publication, operating a telemetry collector, changing external contribution acceptance, and provisioning signing accounts are outside the initial scope.

Before activating P3, record successful hosted-runner trials and comparative CI costs; retain the old gate if the proposed one cannot meet coverage. Before claiming a release target, record its native package qualification and required signing state. Before any future public publication, resolve the actual credentials, release version, and publication authorization. None of these external prerequisites prevents reviewing or implementing locally testable portions of this specification.

## Completion

Each child contract contains independently reviewable acceptance scenarios and rollback behavior. The six-priority realignment is complete only when all six have implementation evidence and the combined release candidate passes P1/P2/P3/P5 qualification. Writing these specifications satisfies the current documentation request only.
