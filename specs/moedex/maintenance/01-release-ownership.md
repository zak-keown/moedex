# P1: Fork-owned release qualification

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: proposed; no publication or credential provisioning is authorized by this document.

## Outcome and boundary

Produce complete, independently identifiable Moedex installation artifacts through a lean GitHub-first pipeline. Extend [BRAND.md](../BRAND.md) B8–B10/B13 and consume its product identity and provenance seam. P2 owns the target/runner inventory; P3 owns verification scheduling. P1 owns distribution and final artifact acceptance.

## Requirements

| ID | Contract |
| --- | --- |
| P1-01 | Initial distribution and update metadata belong to `zak-keown/moedex` GitHub Releases. Installers, updater actions, wrapper recovery messages, and support guidance consume the shared fork identity. npm, Homebrew, WinGet, PyPI publication, R2 mirrors, website deployment, and external repository updates remain disabled until separately specified. Missing configuration or unavailable releases produce fork guidance without upstream distribution fallback. |
| P1-02 | The default pipeline builds, stages, and qualifies artifacts without public release mutations. Consume P2's target matrix and evidence; do not maintain competing support claims. Results distinguish passing, failed, and unavailable targets. |
| P1-03 | Each artifact has an independently ordered Moedex version and a manifest recording fork commit, actual upstream base, channel, target, feature inventory, packaged files, and cryptographic checksums. Diagnostics agree with the manifest. Distribution versions do not replace protocol compatibility fields. An upstream-only release is never a Moedex update. |
| P1-04 | Include the same-release entrypoint and all components required by shipped features: app-server components, code-mode host, proxies, platform sandbox helpers, and voice resources where included. Helpers resolve within the validated package layout despite conflicting stock binaries on PATH. Private `codex-*` filenames may remain. Do not require a separate executable for a component embedded in the entrypoint. |
| P1-05 | Validate final packaged bytes, provenance, checksum integrity, platform launch, and required helper behavior. Signing or packaging transformations invalidate earlier checksums and require evidence covering the resulting artifact. Retain manifest, target results, and limitations identifying exactly what passed. |
| P1-06 | Unsigned artifacts are allowed for local/CI qualification and are explicitly labeled unsigned. They do not establish publisher identity or notarization. A signed channel requires verified fork-owned signing identity, applicable platform signing/notarization verification, and evidence bound to final bytes. Missing credentials or verification block that channel; never silently downgrade it. |
| P1-07 | A future publication path must validate exact repository, channel, version, source commit, qualified manifest, and artifact digests before mutation. Dry-run reports destination and assets without release creation, upload, registry publication, or deployment. Reject arbitrary/upstream destinations and mismatched existing assets. Retry may reuse identical assets; replacing released bytes requires a separately authorized recovery decision. |
| P1-08 | Validate downloads and package layout before activation. Failed download, integrity verification, extraction, missing helper, or activation leaves the prior installation usable. Interrupted activation has deterministic recovery and retains the prior validated release until completion. Fresh-install failure leaves no partially usable installation advertised as complete. |
| P1-09 | Classify endpoints by purpose. Distribution endpoints are fork-owned; pinned build dependencies such as V8 and ripgrep may retain verified upstream sources. Record their origins and integrity checks. OAuth, inference, dependency registries, and protocol endpoints retain existing behavior. A global OpenAI-string replacement is prohibited. |
| P1-10 | Installation/support guidance lists only qualified targets, actual artifact names, signing state, update/rollback limitations, and checksums. Uninstall preserves user state. Explicit rollback selects a validated Moedex package and checks state-format compatibility before launch; unsupported newer state produces a clear stop rather than destructive migration. |

## Acceptance scenarios

1. **Install and coexist:** install a qualified artifact with stock Codex and conflicting helpers on PATH. Public command and required helpers resolve to the intended package; reported provenance matches the manifest.
2. **Altered package:** remove a required resource, mix helper releases, or alter an archive. Qualification/installation fails before activation and identifies the invalid component. Treat archive traversal or paths escaping the package as invalid layout.
3. **Unavailable update:** malformed/unavailable fork metadata or an unsupported target leaves the installation usable and provides fork recovery guidance. No stock Codex update is selected.
4. **Interrupted update:** interrupt download and activation separately, then retry. Recover to a usable validated installation. Explicit rollback preserves sessions/configuration and detects incompatible state formats.
5. **Destination isolation:** exercise reachable release operations through a mocked/dry-run publisher with wrong repository, version, and digest. Invalid inputs fail validation; every dry-run records zero remote mutations.
6. **Signing:** an unsigned candidate can pass its declared local qualification. Missing or invalid signing evidence prevents signed-channel qualification. Signatures/checksums refer to the final bytes, not a pre-signing build.

## Integration and rollback

Extend brand-foundation Tasks 6–7 with these acceptance requirements; reuse their update/package/behavior-manifest ownership. Packaging and qualification can be implemented before public distribution exists. Initial operation produces local/CI artifacts only. Public publication remains separate from the existing plan's scope.

Keep the previous validated package and immutable release evidence available for recovery. A pipeline rollback must retain fork destination isolation. Reverting workflow code must not reactivate OpenAI publishing jobs or require rewriting released assets.

## Source anchors

- [Release workflow](../../../.github/workflows/rust-release.yml): signing, npm, R2, deployment jobs.
- [Windows release](../../../.github/workflows/rust-release-windows.yml) and [preparation](../../../.github/workflows/rust-release-prepare.yml): platform and upstream scheduling assumptions.
- [Unix installer](../../../scripts/install/install.sh), [Windows installer](../../../scripts/install/install.ps1), [updater](../../../codex-rs/tui/src/updates.rs), and [update actions](../../../codex-rs/tui/src/update_action.rs): destination selection.
- [Package builder](../../../scripts/codex_package/README.md): companion resources and external dependency inputs.

Risks are incomplete companion inventories, platform-specific activation semantics, and qualification of bytes that differ from delivered bytes. No build-size reduction or working signing credentials are assumed.
