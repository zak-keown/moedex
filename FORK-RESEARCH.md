# Codex fork idea survey

Research snapshot: 2026-09-10. Purpose: collect design references for Moedex while its requirements are being written.

Follow-up: [50-candidate enhancement catalog](ENHANCEMENT-CATALOG.md), with source/test evidence, effort, limitations and a recommended starting portfolio. See also the [local baseline overlap audit](FORK-BASELINE.md).

## Scope and confidence

GitHub's API reported 18,947 forks during this survey. Screened metadata for the 100 most-starred and 100 newest forks, supplemented by web discovery; read nine candidate READMEs and Codext's change inventory. This is a targeted first pass, not an exhaustive census or runtime validation. Stars and push dates are discovery signals, not evidence of quality or fork-specific activity. Feature descriptions below are maintainer claims unless a narrower implementation reference is supplied.

Reproduce discovery with `gh api 'repos/openai/codex/forks?sort=stargazers&per_page=100'` and the same request with `sort=newest`. Parent metadata comes from `gh api repos/OWNER/REPO`. API results are time-sensitive.

## Priority references

| Project | What to study | Design question for Moedex |
|---|---|---|
| [Every Code](https://github.com/just-every/code) — 4,022 stars | Auto Drive orchestration, background review in separate worktrees, browser feedback, bounded long-session state | Can execution and independent review proceed concurrently while the user retains immediate control? |
| [Codext](https://github.com/Loongphy/codext) — 149 stars | Status header, preserving queued input during quota exhaustion, bounded overload recovery, documented upstream reapplication | Can interrupted work recover predictably, and can custom behavior remain cheap to maintain? |
| [Weave](https://github.com/rosem/codex-weave) — 47 stars | Persistent coordination rooms, named agents, lead-controlled relays and scoped interrupt/compact commands | Should collaboration span independent CLI sessions rather than only a parent/child task tree? |
| [ecodex](https://github.com/EmpiricaAI/ecodex) — 4 stars | Lifecycle hooks, work transactions, comparing agent assessments with deterministic results, separate integration/translator crates | Which workflow guarantees belong in runtime enforcement rather than instructions? |
| [Ata](https://github.com/Agents2AgentsAI/ata) — 92 stars | Paper/patent search, Zotero, dedicated research reading view, LSP/Tree-sitter, multiple providers | Should research have its own artifacts and navigation surface within the harness? |
| [Codex RLM](https://github.com/sudoblockio/codex-rlm) — 7 stars | Embedded Python for selective document access, recursive analysis and coverage tracking | How should agents inspect large material without copying it all into model context? |
| [Codex Infinity](https://github.com/lee101/codex-infinity) — 96 stars | Separate continuation modes for next steps, new ideas and next goals | How should continuing authorized work differ from proposing or starting new work? |
| [Cometix](https://github.com/Haleclipse/codex) — 547 stars | Session status, reasoning translation, CJK cursor handling and resume-picker deletion | Which small interface improvements make long daily sessions easier to operate? |
| [HsinCLI](https://github.com/hsincode/hsincli) — 0 stars in newest sample | Independent configuration home and configurable subagent history | How much history should delegation inherit, and how should a fork coexist with stock Codex? |

Ata identifies itself as built on Codex, but GitHub currently reports `fork: false`; include it as a derivative discovered outside the fork network. HsinCLI demonstrates why a stars-only search misses relevant new work.

## Strongest concrete leads

Every Code's [60727b068203cec9c1ed7ae97236962ff3c9a84f](https://github.com/just-every/code/commit/60727b068203cec9c1ed7ae97236962ff3c9a84f) is titled “fix(auto-drive): decouple auto review and cap long-session growth.” Its changed-file inventory includes the coordinator, agent tools, session/streaming code and TUI. This verifies an implementation reference exists; the patch has not been audited here. Start there to examine bounded queues and review responsiveness.

Codext's [CHANGED.md](https://github.com/Loongphy/codext/blob/main/CHANGED.md) records detailed recovery invariants: stale retry timers must not submit late continuation messages after user intervention; queued user input can supersede synthetic recovery prompts; git polling must reject stale results after changing working directory. Those are useful acceptance-test scenarios independently of whether its code is reused.

Codext's README describes recreating custom behavior on a fresh upstream release rather than merging old fork history. Treat this as an alternative maintenance model to evaluate, not an established recommendation. A behavioral inventory plus regression evidence would be essential to detect lost customizations.

ecodex explicitly labels itself alpha. Its hook and architecture claims merit source verification, particularly where its README simultaneously describes new lifecycle wiring and a mostly unchanged upstream foundation. The interesting idea is comparing claimed confidence with actual outcomes; model self-assessment alone does not establish correctness.

## Suggested next investigation

My proposed reading order is Every Code, Codext, then ecodex/Weave. This prioritizes execution reliability, fork maintenance and orchestration contracts. Ata and RLM are the strongest research/context references if those become central to the product.

Before adopting a feature, identify its fork-only commits relative to the shared ancestor, compare with current upstream behavior, inspect its tests and failure semantics, and estimate the recurring integration cost. Several projects advertise capabilities that may also exist in today's upstream; this survey does not establish novelty.

Additional metadata leads, not yet inspected: `duo121/codex-kanban`, `garyfpga/codex-compact-fix`, `yuguorui/codex-plus`, and `DioNanos/codex-termux`. A broader pass should inspect custom commits on older low-star forks, since newest-only sampling mostly captures freshly created copies.
