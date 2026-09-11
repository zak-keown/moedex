# P5: Explicit outbound diagnostics

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: proposed; no collector or submission service will be operated by this initial change.

## Outcome and boundary

Retain useful bounded local diagnostics while sending no analytics, metrics, traces, logs, or feedback by default. Explicit custom OTLP remains supported. Authentication, inference, model discovery, cloud tasks, and connected apps keep their normal service requests. This is a diagnostics policy, not an offline mode or a claim about provider-side data handling.

Reuse existing configuration resolution, exporters, and diagnostic buffers. Do not introduce a telemetry service, consent database, or generic destination framework.

## Requirements

| ID | Contract |
| --- | --- |
| P5-01 | Fresh installs and existing configurations with unspecified telemetry settings resolve to outbound diagnostics disabled in all entrypoints and build profiles. Constructors/helpers cannot substitute upstream-enabled defaults after resolution. |
| P5-02 | Preserve config-layer precedence; resolve explicit values before applying fork defaults. Effective `analytics.enabled = false` disables product analytics and metrics. Omission disables built-in analytics but permits an explicitly configured custom metrics exporter; omission differs from an explicit metrics veto. |
| P5-03 | Built-in product analytics has no enabled destination in this release. `analytics.enabled = true` does not authorize a destination derived from `chatgpt_base_url`; report a bounded local notice that product analytics is unavailable. It may permit explicit custom metrics. No implicit collector fallback exists. |
| P5-04 | Default log, trace, and metrics exporters are `none`. Explicit custom OTLP HTTP/GRPC endpoints, headers, and TLS keep their existing semantics. Explicit `none` disables its signal. Legacy `statsig` remains parseable but resolves disabled with a local notice. Export failure never selects an alternate destination. |
| P5-05 | Explicit custom log/trace exporters remain independent of the analytics switch. Do not describe `analytics.enabled = false` as an all-exporter switch. Prompt inclusion remains off unless explicitly selected through its existing control. |
| P5-06 | Feedback upload defaults off. Omission and legacy `feedback.enabled = true` do not authorize the embedded OpenAI Sentry destination. Keep local report preparation/export accessible; enabling the workflow creates no upload destination. A future submission integration requires destination/category/attachment preview and per-submission consent, and is outside this implementation. |
| P5-07 | Preserve local logs, bounded diagnostic capture, snapshots, errors, redaction, size limits, and attachment selection with outbound diagnostics disabled. Reuse local capture APIs; add local report export only where the existing feedback flow cannot provide it without uploading. |
| P5-08 | Apply resolution to CLI, exec, app-server, daemon, and shipped helpers. Startup, retries, shutdown, and exit flush cannot revive disabled exporters. Events collected while a signal was disabled cannot later enter automatic upload queues. Explicitly selected local report attachments remain a separate user action. |
| P5-09 | Existing diagnostic/config output reports each signal's effective state, source, and sanitized destination. Do not expose tokens, headers, URL credentials/query secrets, or Sentry key material. No new model-visible context or status stream is introduced. |

## Effective behavior examples

| Effective configuration | Product analytics | Metrics | Logs/traces |
| --- | --- | --- | --- |
| All unspecified | Off | Off | Off |
| Analytics false; custom exporters specified | Off | Off | Explicit custom log/trace exporters only |
| Analytics omitted; custom metrics specified | Off | Custom metrics | Explicit custom log/trace exporters only |
| Analytics true; exporters unspecified | Off, notice | Off | Off |
| Legacy Statsig selected | Off unless separately described above, never an implicit product sink | Disabled, notice | Explicit custom log/trace exporters only |

This proposed omission/false distinction deliberately changes the current default coupling. Cover typed resolution and serialization so a resolved default does not become indistinguishable from a user's explicit veto.

## Import, migration, and rollback

[BRAND.md](../BRAND.md) B3–B7 applies. Typed import preview identifies diagnostic settings/destinations separately from provider credentials. Preserve supported explicit custom OTLP settings and explicit false. Omitted values receive fork defaults; imported analytics true, feedback true, and Statsig resolve as specified above. Do not rewrite stock/shared configuration automatically, including intentional `CODEX_HOME` compatibility use.

Rollback restores a prior executable/config backup without deleting diagnostics or stock state. An older executable may restore its own upload defaults; record this limitation in rollback guidance. No migration silently opts users into a new collector.

## Acceptance scenarios

1. Capture requests with fresh and imported configs in debug and release builds through startup, a completed turn, error handling, and exit flushing. No diagnostic upload occurs by default; expected auth/inference behavior remains functional.
2. Exercise omission, explicit false/true, custom OTLP, none, and legacy Statsig across conflicting config layers. Compare effective policy and observed requests. Include a same-host helper and app-server/exec-server split configuration.
3. Explicit collectors receive only selected signals. Failed collectors produce no fallback requests. Re-enabling a signal does not send its disabled-period queue.
4. Prepare a local diagnostic report with uploads disabled. Verify bounded capture, redaction, and selected attachments. Preview/cancel sends nothing; the initial implementation has no remote feedback submission target.
5. Review snapshots for changed feedback/status copy, use integration coverage for changed agent paths, and regenerate config schemas if typed config changes. Tests intercept diagnostic clients; do not alter sandbox environment constants to obtain evidence.

## Source anchors

- [Core config](../../../codex-rs/core/src/config/mod.rs): analytics/feedback default resolution.
- [Analytics client](../../../codex-rs/analytics/src/client.rs): default queue enablement and derived event destination.
- [Config types](../../../codex-rs/config/src/types.rs), [OTEL initialization](../../../codex-rs/core/src/otel_init.rs), and [exporter config](../../../codex-rs/otel/src/config.rs): Statsig default and analytics/metrics coupling.
- [Feedback](../../../codex-rs/feedback/src/lib.rs): embedded Sentry destination and local snapshot/capture APIs.

Primary risks are inconsistent entrypoint defaults, overinterpreting imported consent, and flush-time uploads hidden by debug-only tests. Observe requests rather than relying only on static default assertions.
