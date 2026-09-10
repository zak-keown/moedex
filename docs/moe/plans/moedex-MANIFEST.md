# Moedex Plan Set

This manifest sequences implementation of the approved [Moedex product specification](../../../specs/moedex/SPEC.md). Each plan must land and be marked `done` with its commit range before its dependents become runnable. The manifest permits independent ready sets, while the v1 sequencing workflow executes one plan at a time.

```yaml
plans:
  - id: brand-foundation
    plan: docs/moe/plans/2026-09-10-moedex-brand-foundation.md
    depends_on: []
    status: pending
  - id: runtime-reliability
    plan: docs/moe/plans/2026-09-10-moedex-runtime-reliability.md
    depends_on: [brand-foundation]
    status: pending
  - id: verification-controls
    plan: docs/moe/plans/2026-09-10-moedex-verification-controls.md
    depends_on: [runtime-reliability]
    status: pending
  - id: coordination
    plan: docs/moe/plans/2026-09-10-moedex-coordination.md
    depends_on: [verification-controls]
    status: pending
  - id: research-context
    plan: docs/moe/plans/2026-09-10-moedex-research-context.md
    depends_on: [brand-foundation]
    status: pending
  - id: integrations
    plan: docs/moe/plans/2026-09-10-moedex-integrations.md
    depends_on: [verification-controls, research-context]
    status: pending
  - id: android-termux
    plan: docs/moe/plans/2026-09-10-moedex-android-termux.md
    depends_on: [brand-foundation]
    status: pending
  - id: stored-checkpoint-experiment
    plan: docs/moe/plans/2026-09-10-moedex-stored-checkpoint-experiment.md
    depends_on: [runtime-reliability]
    status: pending
```

`brand-foundation` is the initial runnable plan. After it lands, runtime reliability, research/context work, and Android qualification become independent ready candidates. The sequencing workflow selects one at a time. The stored-checkpoint experiment may complete with a documented no-go decision when its predeclared criteria are not met; that is a valid completed experimental result and does not change the production fork backend.
