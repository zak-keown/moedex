# P5: Remove remote telemetry

Part of [MAINTENANCE.md](../MAINTENANCE.md). Status: user-directed specification, revised 2026-09-11; implementation remains pending. This supersedes the earlier default-off/custom-exporter proposal.

## Outcome and boundary

Remove Moedex's remote telemetry capability and its settings. No built-in or custom destination can receive product analytics, diagnostic metrics, traces, logs, crash reports, or feedback through a Moedex telemetry facility. Remove the senders and exporters rather than retaining dormant implementations behind disabled defaults.

Preserve bounded local diagnostics and explicit export to a local file. Authentication, inference, model discovery, cloud tasks, and connected apps retain their functional service requests. This does not prohibit ordinary user-authorized network tools or claim control over provider-side logging. Functional RPC responses to a connected client remain supported; diagnostic streaming/export facilities are in scope for removal even if transported over an existing connection.

## Requirements

| ID | Contract |
| --- | --- |
| P5-01 | Remove product analytics senders, remote diagnostic exporters, upload queues/retries/flush paths, and associated built-in destination credentials. No remote telemetry implementation remains available through a configuration choice or build feature. |
| P5-02 | Remove telemetry enablement, destination, authentication, headers, TLS, and remote payload-selection fields from supported typed configuration, schemas, examples, CLI/environment controls, and client configuration surfaces. Do not retain an `enabled = false`, exporter `none`, or custom collector option as a supported setting. Local logging controls remain where they serve only local diagnostics. |
| P5-03 | Remove custom OTLP HTTP/GRPC, Statsig, Sentry upload, and other remote diagnostics transports, including WebSocket diagnostic export and remote crash reporting where present. Inventory every shipped entrypoint and dependency initialization path before declaring coverage complete. |
| P5-04 | Remove remote feedback submission code, UI actions, and API capabilities. Preserve report preparation and explicit local-file export with redaction and bounded attachments. No consent prompt, hidden flag, environment variable, or host-supplied setting can restore uploading. |
| P5-05 | Remove direct dependencies and build/package resources used solely for remote telemetry. Retain local tracing/instrumentation libraries only where needed for local behavior. Where a shared dependency also serves functional traffic, remove its telemetry integration without breaking that functional use. |
| P5-06 | Preserve local logs, diagnostic snapshots, error reporting, redaction, retention/size limits, and attachment selection. No automatic collection is retained solely to populate removed remote event pipelines. |
| P5-07 | Apply removal across CLI, exec, app-server, daemon, code-mode/voice hosts, and other shipped helpers in debug and release builds. Startup, failure handling, shutdown, subprocess initialization, and old persisted queues cannot initiate diagnostic transmission. |
| P5-08 | Existing diagnostic/config output may state that remote telemetry is unsupported; it must not advertise an off/on state or remote destination. Keep credentials out of local errors and migration reports. No new model-visible context is introduced. |
| P5-09 | Inventory intentional removals from external config/API surfaces and regenerate affected schemas/SDK fixtures. Preserve unrelated functional protocol events and provider requests. Record targeted compatibility changes explicitly rather than using compatibility as a reason to retain an upload route. |

## Configuration and import

Fresh configuration exposes no remote telemetry setting. Direct configuration or API attempts to use removed controls produce an actionable unsupported-setting/capability error, without echoing values containing secrets. Detect legacy keys only for migration/error reporting; do not keep them in the supported configuration model. Neither true nor false is a supported value for a removed setting.

Typed settings import under [BRAND.md](../BRAND.md) B3–B7 lists remote telemetry fields as unsupported and excludes them from the destination. Preview reports key/category only, never collector credentials. Local logging and functional provider settings retain their existing semantics. No imported value, profile, or higher-precedence override can enable remote telemetry.

## Migration and rollback

Existing Moedex configs containing removed controls require their removal, with actionable key-only guidance. Do not rewrite stock/shared configuration automatically, including intentional `CODEX_HOME` compatibility use. Explicit import produces compatible destination settings while leaving source files unchanged. Old upload queues are never drained; any eventual cleanup follows the existing explicit data-removal policy.

Recovery within this contract must retain remote telemetry removal. Restoring an older executable that contains telemetry restores an excluded capability and is not a conforming rollback; explain that limitation in release recovery guidance. Preserve local diagnostics and stock state. This contract cannot prevent a user from separately installing different software or manually sharing a saved report.

## Acceptance scenarios

1. Retain packaged debug/release traffic-observation evidence through startup, a completed turn, an error, and shutdown, including helpers. Functional auth/inference/RPC flows succeed with no diagnostic transmission. Inventory observations against all shipped remote-diagnostics paths.
2. Exercise the supported migration/error behavior: legacy telemetry keys yield key-only unsupported guidance; typed import excludes them and preserves local/provider settings. Generated configuration and API artifacts accurately represent the remaining supported surface.
3. Produce and inspect a local diagnostic report. Exercise bounds, redaction, file errors, and cancellation. Existing local diagnostics remain useful after dependency removal.
4. Review the implementation and dependency/build diff for surviving upload transports, initialization hooks, queues, configuration controls, and diagnostic metadata sent through shared functional clients. Document which metadata is functionally necessary and which telemetry fields were removed.
5. Update relevant integration and UI snapshot coverage and regenerate config/app-server schemas as affected. Remove obsolete remote-telemetry tests with their implementations; do not add negative tests solely asserting deleted logic or static names are absent. Use existing behavioral qualification and migration tests for the new contract. Do not alter sandbox environment constants.

## Source anchors

- [Core config](../../../codex-rs/core/src/config/mod.rs): analytics/feedback default resolution.
- [Analytics client](../../../codex-rs/analytics/src/client.rs): default queue enablement and derived event destination.
- [Config types](../../../codex-rs/config/src/types.rs), [OTEL initialization](../../../codex-rs/core/src/otel_init.rs), and [exporter config](../../../codex-rs/otel/src/config.rs): Statsig default and analytics/metrics coupling.
- [Feedback](../../../codex-rs/feedback/src/lib.rs): embedded Sentry destination and local snapshot/capture APIs.

Primary risks are senders embedded in shared clients, helper-specific initialization, and confusing local instrumentation with remote export. Completion requires removal of capability and settings, not only evidence that a default configuration sends nothing.
