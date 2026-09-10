# Moedex Android and Termux Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Qualify and package the optional U10 Android/Termux target with native runtime evidence and explicit capability limitations.

**Architecture:** Add a separately selected native package target to Stage A's fork-owned release packaging. Reuse ordinary Moedex binaries, helpers and permissions; Android-specific capability checks remain small platform modules. Native build success is only admission to on-device qualification, never evidence that desktop sandbox or V8 behavior carries over.

**Tech Stack:** Rust workspace/Just, Android NDK target toolchain selected through D1, Termux native environment, Python unittest for package validation, existing release artifact pipeline.

**Spec:** `specs/moedex/EXPERIENCE.md` U10, `specs/moedex/BRAND.md`, shared SPEC platform and release contracts.

## Global Constraints

- “Optional target; do not weaken desktop contracts to obtain a build.”
- “Cross-compilation alone is insufficient.”
- “Unsupported OS facilities require an explicit capability result rather than a silent no-op or blanket security downgrade.”
- Never modify `CODEX_SANDBOX_NETWORK_DISABLED_ENV_VAR` or `CODEX_SANDBOX_ENV_VAR` code.
- Fork release channel and matching companion resources remain required; no unrelated helper lookup through PATH.
- Use `just test` for Rust tests, no direct `cargo test`. Dependency changes include Cargo and Bazel locks. Run required scoped lint/fmt after tests; no tests after fix/fmt.
- Stage A packaged coexistence is required before these tasks. No publication action is part of qualification.

## Open Decisions

This plan is not dispatchable until D1/D2 resolve and the native compatibility frontier below has been made concrete. Resolve external facts from primary sources; do not substitute untested donor claims.

- **D1 — Supported Android matrix** · `research` · AFK
  - **Question:** Which current Termux distribution, Android API floor, ABI and Rust/NDK toolchain can run the existing workspace dependencies and required helpers?
  - **Options:** First qualify arm64 Termux native / expand ABI coverage only after arm64 succeeds.
  - **Recommendation:** One arm64 configuration first; record exact Android/API, Termux origin/version, compiler and device identity.
  - **Blocked by:** —
  - **Blocks:** Task 1, Task 2, Task 3.
  - **Resolution:** Unresolved; write dated primary-source support facts to `research/moedex/android-matrix.json` with `target`, `android_api`, `termux_version`, `rust_toolchain`, `ndk_version`, and `device` values before implementation.
- **D2 — Device and login access** · `task` · HITL
  - **Question:** Which user-controlled Android device/emulator and account may be used for real login, editing and update smoke tests?
  - **Options:** Physical user-controlled device / emulator with the documented required facilities.
  - **Recommendation:** Physical device for microphone/browser/PTY realism; credentials entered by the user and excluded from evidence.
  - **Blocked by:** D1.
  - **Blocks:** Task 3.
  - **Resolution:** Unresolved; record authorized device connection and account setup without secrets.

## Not Yet Specified

Native compilation may expose Bionic/PTY/file-locking or rusty_v8/helper incompatibilities. The exact affected modules cannot be named honestly before the selected target's build evidence exists. D1 must include a compile probe and graduate concrete failures into a reviewed implementation amendment with exact files and tests. A missing code-mode host cannot be waved away as successful U10 qualification. This plan must not be dispatched as a claim that those compatibility patches are already designed.

## Out of Scope

- An Android GUI, phone-to-desktop remote control app or Play Store distribution.
- All-ABI support in the first qualification.
- Disabling security checks or claiming unavailable isolation is present.

## File Structure

Create `scripts/moedex/android_package.py`, `scripts/moedex/test_android_package.py`, `scripts/moedex/android_acceptance.py`, and `research/moedex/android-matrix.json`. Modify Stage A package inventory/workflow only after its exact delivered interface is available; `.github/workflows/rust-release.yml` is the current inspected release anchor. New platform compatibility modules must be enumerated in the D1 amendment before code mutation.

### Task 1: Target-specific package inventory

**Blocked by:** D1.

**Files:**
- Create: `scripts/moedex/android_package.py`, `scripts/moedex/test_android_package.py`, `research/moedex/android-matrix.json`.
- Modify: `.github/workflows/rust-release.yml` only for opt-in Android artifact construction, not publication credentials.
- Test: `scripts/moedex/test_android_package.py`.

**Interfaces:**
- Consumes: Stage A release manifest containing distribution version, build commit and installed companion paths; D1 exact target matrix.
- Produces: `validate_package(root: pathlib.Path, required: list[str]) -> list[str]` returns missing/escaping/non-executable members. CLI `python3 scripts/moedex/android_package.py --root DIR --matrix FILE --manifest FILE` validates and writes a checksummed target manifest; no downloads occur inside validation.

- [ ] Write a temporary-directory test with `moedex` present but required code-mode host absent; assert missing helper, and test symlink escaping the package root.

```python
with tempfile.TemporaryDirectory() as tmp:
    root = pathlib.Path(tmp)
    (root / "moedex").write_bytes(b"fixture")
    errors = validate_package(root, ["moedex", "code-mode-host"])
    self.assertIn("missing:code-mode-host", errors)
```

- [ ] Run `python3 -m unittest discover -s scripts/moedex -p test_android_package.py`; expect import/missing-validation failure.
- [ ] Implement path containment, existence, executable-mode and SHA-256 manifest checks. Preserve exact Stage A companion filenames by consuming its manifest rather than guessing names.

```python
candidate = (root / member).resolve()
if not candidate.is_relative_to(root.resolve()):
    errors.append(f"escaping:{member}")
```

- [ ] Run the Python suite. Run `just fmt` for repository code changes. Document the actual compile probe command in D1 evidence, including target triple and release profile. Build failures amend the plan, not the capability manifest as successes.
- [ ] Stage task paths and commit `build(android): validate isolated native package inventory`.

### Task 2: Native capability smoke and desktop preservation

**Blocked by:** D1.

**Files:**
- Create: `scripts/moedex/android_acceptance.py`, `scripts/moedex/test_android_acceptance.py`.
- Modify: D1 compatibility-amendment files only after that amendment lists exact paths; no implicit broad edits are authorized by this task.
- Test: `scripts/moedex/test_android_acceptance.py`; the specific Rust crates identified in the amendment.

**Interfaces:**
- Consumes: Task 1 validated package and D1 compile/compatibility results.
- Produces: `classify_case(expected: str, exit_code: int, observed: str) -> str`, returning `passed`, `failed` or `unsupported`; unsupported must be explicitly permitted by the matrix and never used for required login/edit/PTY/code-mode/resume/cancellation/update cases.

- [ ] Write classification tests proving unsupported code mode fails the required acceptance and a successful compilation alone leaves runtime cases unexecuted.

```python
self.assertEqual(classify_case("required", 78, "unsupported"), "failed")
self.assertEqual(classify_case("optional", 78, "unsupported"), "unsupported")
```

- [ ] Run `python3 -m unittest discover -s scripts/moedex -p test_android_acceptance.py`; expect missing classifier.
- [ ] Implement JSON result recording with device/OS/toolchain/build IDs and explicit `not_run` initial state. Never synthesize runtime success from process startup. Execute commands through argument arrays and capture redacted results; no credentials in argv or evidence.

```python
result = subprocess.run(command, check=False, capture_output=True, text=True, timeout=120)
```

- [ ] Run Python tests and the amendment's `just test -p` commands on supported execution environments; retain desktop regression evidence. If required native capability fails, keep U10 incomplete and record the failure. Apply scoped lint/fmt after checks.
- [ ] Commit `test(android): add native capability qualification harness` with the explicit task/amendment paths.

### Task 3: On-device release qualification

**Blocked by:** D1, D2.

**Files:**
- Create: `research/moedex/android-acceptance.md`, `research/moedex/android-acceptance.json`.
- Modify: `scripts/moedex/android_acceptance.py` only for documented device command handling.
- Test: the actual device against Task 1 packaged artifact.

**Interfaces:**
- Consumes: Task 2 result schema and D2 authorized device; stock/fork coexistence identity from Stage A.
- Produces: Immutable release evidence with every required case's command/action, expected outcome, observed result, build/device identity and redacted artifact references.

- [ ] Record initially failing/not-run rows for login, authorized file edit, interactive PTY, code-mode call, resume, cancel, and fork-owned update. A row has this concrete shape:

```json
{"case":"code_mode","expected":"returns 2 from 1+1","status":"not_run","observed":"","artifact":null}
```

- [ ] Run packaged `moedex --version`, then each required user journey on device. Use a temporary project; verify the file edit's bytes, resume identity, cancellation behavior and helper release identity. An update test may use a locally hosted fork fixture; do not publish a release merely to exercise updating.
- [ ] Record observed output and mark each row only from evidence; confirm source/home isolation and unsupported capability descriptions. Replace no failed case with a build-only assertion.
- [ ] Run `python3 scripts/moedex/android_acceptance.py --validate research/moedex/android-acceptance.json`; implement this validator to fail unless all required cases pass and metadata is complete. Expected exit 0 only for complete runtime qualification.
- [ ] Commit evidence only after credential redaction: `git commit -m "test(android): retain native release acceptance evidence"`. U10 remains incomplete on any required failure; expand the compatibility amendment and repeat affected cases only.
