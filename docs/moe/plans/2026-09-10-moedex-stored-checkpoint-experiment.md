# Moedex Stored Checkpoint Experiment Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use `subagent-driven-development` (recommended) or `executing-plans` to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Complete P5 with a reproducible go/no-go report on provider-stored branching, without changing production fork behavior.

**Architecture:** A standalone experiment compares ordinary history forks and provider checkpoints under matched requests. Its adapter uses the selected provider's current documented retention/branching contract; ordinary rollout history remains the production backend. A valid no-go report completes this experiment when semantics, consent, availability or measured value do not justify adoption.

**Tech Stack:** Python 3 experiment/analysis scripts and unittest, JSONL fixtures/results, existing app-server request capture and ordinary fork semantics as the control, explicit provider endpoint after D1.

**Spec:** `specs/moedex/RESEARCH.md` P5; `specs/moedex/SPEC.md` Stage G.

## Global Constraints

- “This is a research spike, not a promised production backend.”
- “Experiments require explicit opt-in to provider retention and a fixed baseline using ordinary history forks.”
- “Branch execution MUST remain independent from the mainline.”
- “No savings or compatibility claim may be inferred from the donor benchmark alone.”
- Do not store credentials, raw sensitive user histories or tokens in committed fixtures. Use synthetic tasks with known outcomes.
- No production protocol/config changes; no direct `cargo test`. If baseline fixtures require Rust changes, use scoped `just test` and required schema/lock/full-suite rules.
- Run `just fmt` after code changes; no Rust tests after fix/fmt.

## Open Decisions

This plan is not dispatchable until D1–D3 resolve. A fact-based early no-go may resolve the experiment without live tasks; do not request retention consent for a provider already proven unsuitable.

- **D1 — Provider semantics feasibility** · `research` · AFK
  - **Question:** Does a currently accessible provider support stored response branches with documented retention/deletion, account/model compatibility and matching system/tool-prefix semantics?
  - **Options:** Supported with qualifying documented constraints / unsupported or insufficiently documented.
  - **Recommendation:** Require primary-source support for every semantic/retention requirement; otherwise record no-go.
  - **Blocked by:** —
  - **Blocks:** Task 1, Task 2.
  - **Resolution:** Unresolved; dated primary-source facts and exact selected API request forms belong in `research/moedex/checkpoint-provider-contract.md`. Unsupported resolves to early no-go and removes live adapter execution from the goal.
- **D2 — Retention and account opt-in** · `conversation` · HITL
  - **Question:** May the selected provider retain these synthetic experiment prompts/responses under D1's documented policy and account configuration?
  - **Options:** Explicit opt-in / no live retained-data experiment.
  - **Recommendation:** Opt in only for synthetic data after reviewing D1; refusal is a legitimate no-go outcome.
  - **Blocked by:** D1.
  - **Blocks:** Task 2.
  - **Resolution:** Unresolved; record consent scope without credentials. No live request before an affirmative answer.
- **D3 — Material improvement threshold** · `conversation` · HITL
  - **Question:** What measured improvement justifies a future production proposal without a correctness regression?
  - **Options:** At least 20% lower median reported/billed token cost with no correctness loss / a separately selected latency threshold.
  - **Recommendation:** Predeclare 20% token-cost reduction, identical model/settings, 20 paired trials across five synthetic tasks and zero branch-isolation failures; report uncertainty rather than overgeneralizing this sample.
  - **Blocked by:** D1.
  - **Blocks:** Task 1, Task 3.
  - **Resolution:** Unresolved; commit the chosen metric/threshold before observing live outcomes. Unknown billing cannot satisfy a cost threshold.

## Not Yet Specified

Production checkpoint fallback, storage UI and rollout migration are intentionally not designed: they require a go decision and a separate approved implementation plan. They are outside this experiment's goal.

## Out of Scope

- Shipping a production checkpoint backend, automatic retention or altering default forks.
- Benchmarking dissimilar system/tool prefixes and presenting the result as checkpoint savings.
- Using private session data for convenience.

## File Structure

Create `scripts/moedex/checkpoint_experiment.py`, `scripts/moedex/test_checkpoint_experiment.py`, `scripts/moedex/checkpoint_provider.py`, and `research/moedex/checkpoints/` containing fixtures, preregistration and redacted results. Baseline anchors inspected: `codex-rs/app-server/tests/suite/v2/thread_fork.rs`, `thread_fork_multi_agent_tests.rs` and `thread-store/src/local/thread_history.rs`.

### Task 1: Preregister matched cases and an offline evaluator

**Blocked by:** D1, D3.

**Files:**
- Create: `scripts/moedex/checkpoint_experiment.py`, `scripts/moedex/test_checkpoint_experiment.py`, `research/moedex/checkpoints/preregistration.json`, `research/moedex/checkpoints/tasks.json`, `research/moedex/checkpoint-provider-contract.md`.
- Test: `scripts/moedex/test_checkpoint_experiment.py`.

**Interfaces:**
- Consumes: D1 contract and D3 threshold.
- Produces: `evaluate(pairs: list[dict], threshold: float) -> dict` returning `decision`, `reason`, paired sample count, metric distribution and invalid-pair count. Each pair records model/settings/prefix digests, correctness and independence, input/output/cached usage, known cost and latency for both modes.

- [ ] Write failing offline tests: mismatched prefixes invalidate a pair; cheaper but incorrect results are no-go; unknown cost cannot satisfy a cost target; a branch touching mainline fails isolation.

```python
result = evaluate([pair_with_changed_mainline], 0.20)
self.assertEqual(result["decision"], "no-go")
self.assertEqual(result["reason"], "branch isolation failed")
```

- [ ] Run `python3 -m unittest discover -s scripts/moedex -p test_checkpoint_experiment.py`; expect import/evaluator failure.
- [ ] Implement evaluator guards before aggregate comparison. The fixture set covers independent branch edits, tool result replay, changed tool prefix, unavailable/expired checkpoint and deletion. Store exact expected synthetic outputs, not subjective model grading alone.

```python
if any(not pair["branch_independent"] for pair in pairs):
    return {"decision": "no-go", "reason": "branch isolation failed"}
```

Then validate equal model/settings/prefix and known metric fields before calculating paired deltas. Add distribution and sample-count fields on every return, including failure paths.
- [ ] Run offline unittest; review committed preregistration before any live run. Run `just fmt`.
- [ ] Commit `test(checkpoints): preregister branching experiment and evaluator`.

### Task 2: Run explicit provider experiments

**Blocked by:** D1, D2.

**Files:**
- Create: `scripts/moedex/checkpoint_provider.py`, `research/moedex/checkpoints/results.jsonl`, `research/moedex/checkpoints/run-metadata.json`.
- Modify: `scripts/moedex/checkpoint_experiment.py`, `scripts/moedex/test_checkpoint_experiment.py`.
- Test: offline transport fixtures plus explicit synthetic live runs.

**Interfaces:**
- Consumes: Task 1 preregistration, D1 exact API contract, D2 consent and credentials passed through the existing approved environment mechanism.
- Produces: `run_pair(task: dict, provider: ProviderAdapter) -> dict`; adapter exposes `ordinary(task: dict) -> dict`, `stored(task: dict) -> dict`, `delete(checkpoint_id: str) -> dict`. ProviderAdapter is defined in `checkpoint_provider.py`; returned mappings match Task 1's evaluator schema. Adapter timeout is 120 seconds/request and total requests are capped by preregistered trial/fault-case counts.

- [ ] Mock provider rejection, expired ID and deletion failure. Assert the result remains explicit and no ordinary fallback result is mislabeled checkpoint success.

```python
self.assertEqual(failed_stored["mode"], "stored")
self.assertEqual(failed_stored["status"], "provider_rejected")
self.assertIsNone(failed_stored["cost"])
```

- [ ] Run the offline unittest; expect missing adapter/error-path behavior.
- [ ] Implement only D1's documented API forms with scoped credentials and bounded responses. Match system/tool prefixes and model settings byte-for-byte where required; record their hashes. Execute ordinary/stored order alternately to reduce ordering bias. Refuse live mode without explicit retained-data consent record and preregistration digest.

```python
if not consent["retention_opt_in"]:
    raise RuntimeError("live checkpoint experiment requires recorded retention opt-in")
```

- [ ] Run `python3 scripts/moedex/checkpoint_experiment.py --preregistration research/moedex/checkpoints/preregistration.json --tasks research/moedex/checkpoints/tasks.json --results research/moedex/checkpoints/results.jsonl --live`. Exercise expiry/deletion/rejection cases and record redacted metadata; delete test checkpoints under the selected provider's documented behavior. Offline fixtures must not be represented as live deletion proof.
- [ ] Run offline unittest for any new adapter parsing changes, then `just fmt`; commit redacted scripts/results as `experiment(checkpoints): record matched provider branch trials`.

### Task 3: Publish the go/no-go artifact

**Blocked by:** D3.

**Files:**
- Create: `research/moedex/checkpoints/DECISION.md`, `research/moedex/checkpoints/evaluation.json`.
- Modify: `scripts/moedex/checkpoint_experiment.py` only if adding the evaluation CLI around Task 1's existing evaluator.
- Test: offline evaluation fixtures and retained real evidence.

**Interfaces:**
- Consumes: Task 1 criteria and Task 2 results; for early no-go, D1 documented incompatibility or D2 refusal replaces live results.
- Produces: A final decision artifact containing scope, exact provider/model/date, source references, retention result, correctness/isolation findings, matched metric distribution, uncertainty and go/no-go rationale.

- [ ] Run `python3 scripts/moedex/checkpoint_experiment.py --evaluate research/moedex/checkpoints/results.jsonl --preregistration research/moedex/checkpoints/preregistration.json --output research/moedex/checkpoints/evaluation.json`; expected non-success/no-go on invalid/missing required evidence. Early no-go records `live_trials: 0` and the concrete blocking fact instead of creating fake results.
- [ ] Write DECISION.md from retained evidence, with no runtime production changes. A go means “eligible for a separate production design”; it does not turn on the feature. An inconclusive sample fails the adoption threshold and records no-go for this trial.

```json
{"decision":"no-go","production_backend_changed":false,"live_trials":0,"reason":"documented prerequisite not satisfied"}
```

Use the actual evidenced reason, not the example wording, in the final artifact.
- [ ] Verify all claimed metrics are reproducible from committed redacted results and all linked sources/artifacts exist. Run Python tests only if code changed; format code if changed.
- [ ] Commit `docs(checkpoints): record branching experiment go-no-go decision`. Either supported go or evidenced no-go completes P5; production rollout remains outside this plan.
