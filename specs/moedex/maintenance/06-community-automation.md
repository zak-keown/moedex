# P6: Inactive inherited community automation

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: proposed; no GitHub settings or community records have been changed.

## Outcome and boundary

Keep the fork's inherited community bots inactive without affecting verification or altering existing issues, PRs, comments, labels, or signatures. Start with the already guarded workflows retained in source; deleting them has little operational benefit and creates avoidable upstream merge churn.

This scope covers [CLA](../../../.github/workflows/cla.yml), [issue labeling](../../../.github/workflows/issue-labeler.yml), [deduplication](../../../.github/workflows/issue-deduplicator.yml), [translation](../../../.github/workflows/issue-translator.yml), and [stale contributor PR closure](../../../.github/workflows/close-stale-contributor-prs.yml). The source already guards their jobs for upstream; CLA uses an owner check, while the others use the canonical repository identity.

## Requirements

| ID | Contract |
| --- | --- |
| P6-01 | All five workflows remain inactive on `zak-keown/moedex` and downstream forks for their declared events, including manual dispatch where offered. Use an exact canonical upstream repository condition consistently; renaming an owner or repository must not opt the bots in. A skipped workflow shell is acceptable; running a bot or allocating its worker is not. |
| P6-02 | Community workflows require no fork-provided API key, environment, signature branch, or write permission setup. Do not provision their secrets or request an OpenAI CLA from Moedex contributors. Preserve existing upstream signature/legal documents as historical material; this specification does not change license or contribution terms. |
| P6-03 | These bots are absent from Moedex's required-check graph. An inherited CLA status in GitHub rulesets is recorded as a settings migration prerequisite, not silently ignored or bypassed. |
| P6-04 | Preserve build/test/schema/dependency checks, dependency-update configuration, release integrity checks, and action revision pinning. Classify by effect and purpose, not by whether a workflow contains `openai` or uses automation. |
| P6-05 | Update the workflow inventory to identify these five as inactive upstream community operations. Explain their owner guards so a future upstream merge does not accidentally activate them. Use existing workflow documentation and review checks, not a new bot-management service. |
| P6-06 | Re-enabling any bot requires a separate explicit policy change specifying repository, trigger, permissions, external services, cost limits, and intended writes. It starts with a read-only preview. No initial automated labeling, comments, closure, translation, CLA enforcement, or cleanup is included. |

## Acceptance scenarios

1. **Fork event evaluation:** inspect/evaluate the existing event and job conditions for issue-opened, labeled, comment-created, PR-target, schedule, and dispatch payloads as applicable. With the Moedex identity, no bot action or credential-consuming step is reachable. Validate workflow behavior using the existing test approach or a retained Actions observation; do not create real issues merely to test this contract.
2. **Downstream identity:** repeat with another owner/repository. No bot becomes active merely because it is a fork or has similarly named secrets.
3. **Correctness independence:** an ordinary source PR still reaches P3's required gate and dependency/security checks with no community-bot result present. A failed required test continues to block.
4. **Preserved records:** an implementation diff and any authorized hosted observation show no mutation of existing community records, signature branches, or repository permissions. No legacy-bot credentials are needed for qualification.
5. **Upstream merge review:** a fixture or retained review of a changed bot guard identifies whether it would activate a mutating path on Moedex. The reviewer records the result in normal review evidence; a new generic policy engine is unnecessary.

## Migration and rollback

Normalize only guards that need it, document inactive status, and audit the required-check graph. If live repository settings require an obsolete status, prepare the exact settings delta for separate application; this document grants no external-write authority. Where guards already satisfy the contract, retain them and record evidence rather than rewriting for appearance.

Rollback restores the previous guarded workflow definitions. It does not run bots retroactively or replay queued community events. Any later activation is a new change. Whether Moedex accepts external PRs is a separate maintainer policy decision; disabling OpenAI's CLA bot does not decide it.
