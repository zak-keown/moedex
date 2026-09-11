# Operator experience and identity-adjacent requirements

Part of [SPEC.md](SPEC.md). All shared limits, authority and compatibility contracts apply. These are proposed behavior requirements, not existing implementation claims.

## U1 — Live session status

Show the active model/effort, thread working directory, Git branch/divergence/dirty state, capacity and available usage attribution. Reuse the existing status surface and let users select fields. An unavailable field is unknown, not zero or a stale value presented as current. Polling must be bounded, cancellable and shared rather than multiplied by each view. Proposed default Git refresh is 15 seconds with a 2-second collection timeout; refreshing current work is independent of long-running tools.

**Acceptance:** switch working directory while a Git refresh is pending; the old result cannot update the new display. Narrow-terminal snapshots remain readable. Provider/capacity failure leaves the composer responsive and marks affected data unavailable. Status-only ticks satisfy O6.

## U2 — Temporary and saved model settings

Expose clearly distinct “this session” and “save as default” actions. Session changes propagate to the next safe turn, including selected reasoning settings; they do not rewrite user configuration. Persist only the fields the user chose, using the existing config editor. Nested pickers must preserve the selected persistence scope. Do not change active child models implicitly.

**Acceptance:** change model and effort for a session, restart another session and confirm defaults are unchanged; explicitly save and confirm only intended defaults change. Cancellation from any picker produces no persistence. A current turn retains its original provider identity.

## U3 — Subagent context policy

Add policy enforcement to existing `fork_turns`: proposed default one recent user turn, maximum three, with full-history inheritance disabled unless explicitly enabled. These are Moedex defaults for new configurations; migration previews the difference from Codex. An explicit disallowed request fails before allocating a child or sending a model request, reporting the effective policy. Policy applies consistently to roots, forks and children; role settings can narrow but not exceed a parent-enforced ceiling.

Turn count alone is insufficient. Proposed inherited-history budget is 8,000 tokens total, preserving existing individual item limits and instruction scope. If a requested history cannot fit, reject with a smaller-history/fresh-spawn alternative rather than silently removing turns. Applicable system/developer instructions must still be assembled normally; copied parent-specific instructions must not duplicate or override the child's effective instructions. Full-history model/effort restrictions remain compatible with existing runtime contracts.

**Acceptance:** no/all/one/three/over-limit/invalid requests behave deterministically; invalid defaults fail config loading; a single oversized turn cannot bypass the token limit. Rejected spawns incur no request. Inherited goal budgets remain shared as designed. Imported historical rollouts remain readable without reinterpretation of old turns.

## U4 — Independent installation identity

Implement the [BRAND.md](BRAND.md) coexistence, home and update contracts as part of REBRAND, not a separate conflicting mechanism.

**Acceptance:** run Codex and Moedex simultaneously with separate default homes, auth stores, daemon identity and update channels; neither process writes the other's state. Explicit shared-home hazards produce a clear diagnostic before conflicting ownership is acquired.

## U5 — Custom behavior manifest

Maintain a versioned manifest of enabled Moedex requirements, upstream base and verifying tests/artifacts. Upstream adoption must distinguish behavior preserved, superseded by upstream, intentionally changed and broken. Run packaged CLI/helper smoke checks in addition to source compilation; record deliberate exclusions. An upgrade does not earn a “compatible” label because merges or structural scripts pass.

**Acceptance:** deliberately remove a required companion executable from a release fixture; qualification fails. Rebase/merge a selected upstream fixture and demonstrate that missing Moedex behavior fails its mapped regression gate. Record an explicit decision when upstream replaces custom code.

## U6 — Display translation

Offer opt-in translation of user-visible summaries into a selected language through an explicitly configured provider. Preserve the original text and identity, show translated/failed/pending state, and leave primary execution unblocked by translation. Do not expose or request hidden reasoning. Translated display text does not replace original model history. Translation requests count toward usage and obey existing data/provider permissions. Default is off.

**Acceptance:** failed translation leaves the original readable and input active; toggling off cancels pending translation and prevents late replacement. Source and translated text can be inspected independently. Provider selection clearly identifies where text is sent.

## U7 — Resume-picker cleanup

Expose archive/restore through existing thread lifecycle primitives. Removing a session from a board is only membership removal. Permanent deletion is a separate deliberate action with existing confirmation semantics; reject deletion of active-owned state until it is stopped or ownership is resolved. Preserve provenance references as tombstones rather than dangling links.

**Acceptance:** archive hides from the default picker and remains restorable; restore retains identity/history. Removing a board card leaves the conversation intact. Deleting a referenced thread leaves an intelligible unavailable-source record. No active turn loses its underlying files through picker cleanup.

## U8 — Copy draft

Provide a discoverable copy-draft action distinct from interrupt/quit. Copy only user-authored draft text; preserve the draft, queued input and current turn. Use existing clipboard and terminal capabilities, including remote fallbacks where available. If the terminal cannot distinguish a proposed key chord, retain a menu/action alternative.

**Acceptance:** copy a multiline Unicode draft during an active turn and verify exact clipboard content without interruption or input mutation. Empty draft and unavailable clipboard produce harmless feedback. Snapshot help text for the actual supported binding.

## U9 — Voice dictation

Optional dictation produces an editable draft; default behavior never sends it automatically. Show recording/transcribing state, selected local/remote transcription backend and cancellation. Existing text input remains available. Proposed session recording limit is 120 seconds; no retained audio by default after transcription/cancellation. Use existing permissions for microphone/provider access and report unsupported platforms explicitly.

**Acceptance:** cancel recording or failed transcription produces no submitted turn. A transcript can be corrected and then sent by the user. Audio cleanup occurs on success, failure and cancellation. Disabling voice removes its background resources and does not affect ordinary startup.

## U10 — Android/Termux distribution

Optional target; do not weaken desktop contracts to obtain a build. Qualify native execution, PTY behavior, login launch, runtime libraries, file locking and code-mode helper availability on a declared Android/ABI matrix. Unsupported OS facilities require an explicit capability result rather than a silent no-op or blanket security downgrade. Fork-owned updates and branding apply equally.

**Acceptance:** retain on-device evidence for login, a real authorized file edit, PTY execution, code-mode invocation, resume, cancellation and update. Cross-compilation alone is insufficient. Unsupported operations fail descriptively; desktop regression gates still pass.

## U11 — Rich queued input

Extend existing queues with immutable image/file attachment references, captured at admission with type, size and content identity. Proposed limits: 100 pending inputs per thread, 10 attachments and 20 MiB total attachment bytes per input, 200 MiB pending attachment bytes per thread; a stricter provider limit wins. Shared text/context limits also apply. File contents must not change when the original path changes later. Validate permission and available capacity before acknowledging acceptance. List, edit, remove and inspect queued inputs through consistent TUI/CLI semantics.

**Acceptance:** enqueue an image, modify/delete its original file, and verify delivery uses the admitted content. Restart retains queue order and attachments. Full storage rejects a new admission visibly without losing earlier items. Editing a queued input creates a new version; an in-flight version cannot be retroactively changed. Unsupported attachment delivery preserves the input and explains a recovery action rather than discarding it.

## P1 — Named cross-session messaging

Build the user action on O7/O8 room membership and stable IDs. A name is resolved within the selected room/project; ambiguous names require explicit disambiguation. Direct messages target one stable identity; broadcast snapshots membership at acceptance and reports per-target outcomes. Persist the sender and original scope. Suggested UI language may use “message”; no donor-specific character/personality naming is required.

**Acceptance:** two projects with identically named agents do not receive each other's traffic. Broadcast yields correlated receipts, including unavailable targets. A renamed agent keeps identity. Untrusted message body text cannot acquire the sender's control permissions.

## P2 — Respect the active draft

Separate message receipt from model activation. Receipt is durable immediately; ordinary incoming coordination messages appear in an inbox/badge while the user is composing or using a modal. They must not overwrite, submit or discard the draft. An explicit authorized interrupt uses the existing interruption path and preserves recoverable draft state. Agent-origin messages stay typed as agent-origin context and never impersonate a human turn.

**Acceptance:** receive direct and broadcast messages while editing a draft, opening a picker and running a turn; draft bytes remain unchanged. After the thread is ready, each accepted message activates at most once under O8's reconciliation contract. Cancellation of a sender's task does not revoke an already accepted human draft.
