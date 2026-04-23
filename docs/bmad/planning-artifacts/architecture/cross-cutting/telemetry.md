# Cross-Cutting Concern: Telemetry

Centralized, compile-time toggled (`#[cfg(feature = "telemetry")]`). JSON lines with consistent schema. "Emit by default, prune by evidence." Every system should emit observable events through this single channel.
