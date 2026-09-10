# Requirement traceability

Part of [SPEC.md](SPEC.md). Each catalog ID has a normative behavior and acceptance scenario in the linked contract. REBRAND adds the 51st workstream; B1–B13 are its detailed requirements. Stage names are defined in SPEC.md. No row asserts implementation is complete.

| ID | Enhancement | Contract | Stage |
|---|---|---|---|
| REBRAND | Actual Moedex product/distribution identity (B1–B13) | [BRAND.md](BRAND.md) | A |
| R1 | Preserve follow-ups through quota exhaustion | [EXECUTION.md](EXECUTION.md) | B |
| R2 | Bounded recovery after model overload | [EXECUTION.md](EXECUTION.md) | B |
| R3 | Reload credentials at safe request boundaries | [EXECUTION.md](EXECUTION.md) | B |
| R4 | One compaction fallback policy | [EXECUTION.md](EXECUTION.md) | B |
| R5 | Independent compaction resource policy | [EXECUTION.md](EXECUTION.md) | B |
| R6 | Detect repetitive autonomous work | [EXECUTION.md](EXECUTION.md) | C |
| R7 | Explain stalled work and recover from a completed checkpoint | [EXECUTION.md](EXECUTION.md) | B |
| U1 | Live session dashboard near the composer | [EXPERIENCE.md](EXPERIENCE.md) | B |
| U2 | Temporary versus saved model selection | [EXPERIENCE.md](EXPERIENCE.md) | B |
| U3 | Explicit subagent history policy | [EXPERIENCE.md](EXPERIENCE.md) | C |
| U4 | Independent Moedex installation identity | [EXPERIENCE.md](EXPERIENCE.md) | A |
| U5 | Fork behavior manifest and upgrade checks | [EXPERIENCE.md](EXPERIENCE.md) | A |
| U6 | Selectable translation of displayed reasoning summaries | [EXPERIENCE.md](EXPERIENCE.md) | F |
| U7 | Session cleanup from the resume picker | [EXPERIENCE.md](EXPERIENCE.md) | B |
| U8 | Copy the current draft without interrupting work | [EXPERIENCE.md](EXPERIENCE.md) | B |
| U9 | Voice dictation for task instructions | [EXPERIENCE.md](EXPERIENCE.md) | F |
| U10 | Native Android/Termux packaging | [EXPERIENCE.md](EXPERIENCE.md) | G |
| U11 | Queue screenshots and file attachments | [EXPERIENCE.md](EXPERIENCE.md) | B |
| P1 | Address another active session by name | [EXPERIENCE.md](EXPERIENCE.md) | D |
| P2 | Deliver coordination messages without stealing the draft | [EXPERIENCE.md](EXPERIENCE.md) | D |
| P3 | Link code changes to the task that produced them | [RESEARCH.md](RESEARCH.md) | F |
| P4 | Cross-agent handoff packages | [RESEARCH.md](RESEARCH.md) | F |
| P5 | Stored response checkpoints as an experimental branch backend | [RESEARCH.md](RESEARCH.md) | G |
| O1 | Review the exact code snapshot | [EXECUTION.md](EXECUTION.md) | C |
| O2 | Bounded review–repair cycles | [EXECUTION.md](EXECUTION.md) | C |
| O3 | A visible retry deadline for autonomous runs | [EXECUTION.md](EXECUTION.md) | B |
| O4 | Supervised continuation | [EXECUTION.md](EXECUTION.md) | D |
| O5 | Independent progress observer | [EXECUTION.md](EXECUTION.md) | D |
| O6 | A long-session rendering contract | [EXECUTION.md](EXECUTION.md) | B |
| O7 | Persistent rooms across root sessions | [EXECUTION.md](EXECUTION.md) | D |
| O8 | Typed coordination actions and receipts | [EXECUTION.md](EXECUTION.md) | D |
| O9 | Relay loop and readiness guards | [EXECUTION.md](EXECUTION.md) | D |
| O10 | Boards over existing threads | [EXECUTION.md](EXECUTION.md) | D |
| O11 | Attention inbox for parallel work | [EXECUTION.md](EXECUTION.md) | D |
| O12 | Propose the next task on completion | [EXECUTION.md](EXECUTION.md) | D |
| C1 | Federated paper search with visible coverage | [RESEARCH.md](RESEARCH.md) | E |
| C2 | Citation-neighborhood exploration | [RESEARCH.md](RESEARCH.md) | E |
| C3 | Research-library search across notes and annotations | [RESEARCH.md](RESEARCH.md) | E |
| C4 | Canonical document resolution with traceable fallbacks | [RESEARCH.md](RESEARCH.md) | E |
| C5 | Section-oriented document reader with contextual questions | [RESEARCH.md](RESEARCH.md) | E |
| C6 | Large-context handles and selective fetch | [RESEARCH.md](RESEARCH.md) | E |
| C7 | Explicit context-coverage reports | [RESEARCH.md](RESEARCH.md) | E |
| C8 | Hierarchical documentation routing | [RESEARCH.md](RESEARCH.md) | E |
| C9 | Bounded structured scratchpad | [RESEARCH.md](RESEARCH.md) | E |
| C10 | Per-model cost and cache-savings ledger | [RESEARCH.md](RESEARCH.md) | C |
| C11 | Native alternate provider protocols with a behavioral contract suite | [RESEARCH.md](RESEARCH.md) | F |
| C12 | Model-appropriate editing tools with stale-read checks | [RESEARCH.md](RESEARCH.md) | C |
| C13 | Evidence ledger and post-task calibration | [RESEARCH.md](RESEARCH.md) | C |
| C14 | Configurable workflow policy checks, with explicit enforcement mode | [RESEARCH.md](RESEARCH.md) | C |
| C15 | Event-driven wake from long-running tools | [RESEARCH.md](RESEARCH.md) | D |

## Shared prerequisites

All workstreams inherit the SPEC.md context, admission, cancellation, compatibility and verification contracts. U4/U5 are implemented jointly with REBRAND rather than duplicating its home/update policy. O1/O2 share snapshot and evidence identities with C12/C13/P3. R1/R2/O3/U11 share durable input admission. O7/O8/O9/P1/P2 share one coordination protocol. C6/C7/C9 supply storage and coverage for the research reader and handoff packages.

Stage G is split: U10 is an optional platform deliverable; P5 is an experiment whose valid completed outcome may be no-go. The catalog remains the source map for donor references, while this set defines intended Moedex behavior.
