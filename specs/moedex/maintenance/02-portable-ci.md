# P2: Portable CI infrastructure

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: proposed; no runners or credentials have been provisioned.

## Outcome and boundary

Execute fork verification without OpenAI runner groups, credentials, or remote execution services. P2 owns environment capability and target coverage. P3 owns which checks block PRs; P1 consumes qualified targets for distribution.

## Requirements

| ID | Contract |
| --- | --- |
| P2-01 | Separate supported verification targets from runner assignments. Default blocking checks to fork-accessible GitHub-hosted Linux, macOS, and Windows runners. Resolve concrete labels during qualification against current official documentation and repository availability. Replacing a runner label does not establish equivalent architecture, disk, memory, privileges, or tooling. |
| P2-02 | Qualify dependency installation, Windows SDK/build-directory setup, executable discovery, Linux user namespaces, sandbox tests, and artifact replay. Existing Dev Drive and `sudo sysctl` assumptions must work or have documented portable alternatives. Unsupported setup fails with an actionable result; it cannot silently suppress behavioral coverage. |
| P2-03 | Ordinary PR checks require no publication secrets, release environment approval, or BuildBuddy key. Preserve local Bazel fallback and the restriction against using OpenAI's tenant for untrusted code. Optional cache failures yield cold verification, not skipped tests. |
| P2-04 | Inventory baseline OS, architecture, GNU/musl, native/cross-compiled, remote-environment, and release-profile coverage. P3 may relocate checks but must account for every retained case. Cross-compilation is not native execution. Qualify a fork-controlled alternative for unavailable hosted targets before claiming equivalent coverage. |
| P2-05 | Record cold/warm-cache evidence for routine Rust and dependency/build-system changes: revision, runner/architecture, queue and execution durations, measurable peak disk use, failures, and retries. Select bounded timeouts/concurrency from those observations. Do not claim an unmeasured numerical saving. |
| P2-06 | Preserve pinned actions, restricted checkout credentials, logs, bounded timeouts, generated-file cleanliness checks, and merge-candidate verification. PR code receives no signing/publishing secrets. Separate trusted release mutations from code-under-test execution. |

## Acceptance scenarios

1. A fork PR without organization runners or secrets completes required verification on qualified environments. Evidence shows actual build/test execution, not only workflow parsing.
2. Empty caches and absent BuildBuddy credentials produce cold builds. Infrastructure failure remains failure rather than success or skipped verification.
3. Windows setup works without a pre-provisioned Dev Drive. Linux sandbox tests demonstrate exercised behavior; early exits are identified and covered on another qualified runner before equivalence is claimed.
4. Coverage inventory distinguishes Windows ARM64 archive production from native replay and identifies every target relocated out of the PR path.
5. Qualification records justify runner sizes and timeouts before activation. Each advertised P1 target has applicable package launch evidence.

## Migration and rollback

Run portable assignments alongside the existing path where usable. Otherwise run manual candidate qualification and disclose the absence of comparable baseline timing. Do not claim cutover readiness from unavailable upstream infrastructure. Keep the last working fork-accessible mapping and workflow revision identifiable; rollback restores it rather than an inaccessible upstream runner group.

Before activation, the coverage inventory must give each case a target, native/cross mode, workflow/job, runner capability, required/secondary/release schedule, and evidence reference. An unqualified case stays visibly pending and blocks claims that depend on it. This may delay a release target without narrowing the core platform contract silently.

## Source anchors

- [Bazel matrix](../../../.github/workflows/bazel.yml) and [Rust CI](../../../.github/workflows/rust-ci.yml): existing runner assignments.
- [Shared setup](../../../.github/actions/setup-ci/action.yml) and [platform nextest](../../../.github/workflows/rust-ci-full-nextest-platform.yml): environment assumptions.
- [BuildBuddy wrapper](../../../.github/scripts/run_bazel_with_buildbuddy.py): `remote_config` and `bazel_args_without_remote_execution` preserve credential-free fallback.
- [Bazel configuration](../../../.bazelrc): optional remote endpoints and build settings.

Main risk: a cheaper runner can report apparent success while platform tests exit early. Acceptance therefore records exercised coverage as well as exit codes.
