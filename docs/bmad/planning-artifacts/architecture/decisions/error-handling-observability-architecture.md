# Error Handling & Observability Architecture

**Decision: Structured JSON logging via tracing layers with errors-as-metrics and crash persistence**

- **Category:** Error Handling / Observability
- **Priority:** Critical (blocks Ring 1)
- **Affects:** Every system, debugging, testing, crash reporting, production monitoring

**Structured JSON logging via tracing layers:**
- Disable Bevy's default `LogPlugin`. Configure `tracing-subscriber` directly with a layer stack:
  - **JSON file layer** — structured JSON output with all span context, timestamps, system name, entity IDs, component data. This is what Loki/Grafana/any log parser ingests.
  - **Console layer** — human-readable format for development. Pretty-printed, colored. Debug builds only (compile-time gated).
  - **Metric emission layer** — every `error!` and `warn!` event also increments a counter in a `DiagnosticsResource`. Errors are metrics, not just text.
- Every `tracing::Span` carries structured fields: system name, phase (`GamePhase`), tick number, entity ID where relevant. These propagate automatically to all events within the span. JSON output gets this context for free.

**Error flow in systems:**
- Early-return-with-log pattern: `let Some(x) = query.get(entity) else { error!(entity = ?entity, "missing component X"); return; };`
- The `error!()` call hits all three layers simultaneously — logged to JSON file, printed to console, counted as metric.
- No panicking in gameplay systems. Ever.

**Error types for non-system code:**
- `thiserror`-derived enums per domain when 3+ variants exist. Each variant carries context fields that serialize into the structured log.
- `Box<dyn Error>` until that threshold. No `anyhow` — not needed.
- `.unwrap()` never in production, allowed in tests. `.expect()` only for startup invariants.

**Test harness integration:**
- Custom `tracing` subscriber layer that captures events into a `Vec<LogEvent>` buffer. Injected during test setup.
- Tests can assert on log output: "this system logged exactly one warning containing these fields when given this input."
- Ties directly into the minimal `App` test pattern — `app.update()` N ticks, then inspect both world state AND captured logs.
- Log output is a first-class test artifact, not stderr noise.

**Crash handler:**
- `std::panic::set_hook` — custom panic hook registered at startup, before Bevy `App::run()`.
- On panic: flush the JSON log buffer to `{user_data}/apeiron-cipher/crash-logs/{timestamp}.json`. Include the panic message, backtrace, last N log events from a ring buffer (configurable size, default 1000), and game state snapshot (current tick, active phase, entity count).
- Crash log format designed for direct paste into GitHub issue body or attachment upload.
- Ring buffer of recent log events kept in memory — this is what gets dumped on crash, providing context leading up to the panic.

**New dependencies:**
- `tracing-subscriber` with `json` and `env-filter` features (Bevy already depends on `tracing`)
- `tracing-appender` for non-blocking file output

**Integration with Decision 2's telemetry contract:**
- The debug telemetry (system entry/exit, event fired, state transitions) uses `tracing::span!` and `tracing::event!` — same infrastructure. The JSON layer captures it all. The metric layer counts it all. One observability stack, not two parallel systems.

**Rationale:** Errors as metrics makes failure rates observable and trendable, not just greppable. Structured JSON logging with full span context means every log line carries enough information to reconstruct what was happening when the error occurred. The crash handler ensures that even fatal failures produce actionable debug artifacts. The test harness integration makes log output assertable, turning observability into a testable contract.
