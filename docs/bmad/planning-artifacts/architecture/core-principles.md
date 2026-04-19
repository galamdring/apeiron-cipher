# Core Principles

Rules that apply to every line of code, every system, every story. No exceptions.

1. **Knowledge is the only progression system.** There are no levels, no XP, no skill trees, no unlocks. The player progresses by understanding the world better. Every system serves this.
2. **Accretion over reward.** Every player action must teach something new through consequence, never through confirmation.
3. **Diegetic only.** The game never tells — it reveals through world behavior. No UI text explaining state. No popups. No progress bars.
4. **Deterministic.** Same seed + same inputs = same outputs. Always. Seeded RNG only. Explicit system ordering. No float ambiguity.
5. **Server-authoritative.** Intent is untrusted. Simulation decides. Even in single-player.
6. **Data-driven.** All tuning lives in asset files. If a tuning value is hardcoded in Rust source, it's wrong. Only truly fixed values (mathematical constants, protocol versions) are `const`.
7. **No `unsafe`. No `.unwrap()` in production (`.expect("reason")` where panicking is correct). No `pub(crate)`. No exceptions.**
8. **Default posture: stop and ask.** If the story doesn't specify the name, type, field, event, or architectural choice — stop and ask. Do not infer. Do not invent. Correctness over speed. See `implementation-patterns-consistency-rules.md` for the precise "stop" vs "proceed" breakdown.
9. **Document like Cave Johnson is reading.** Every public type, every system, every non-obvious line. Over-documentation is not a failure mode.
10. **Every system emits observations.** The Mirror System observes player behavior across all gameplay systems. If a system produces player-observable actions, it must emit observation hooks. No silent systems.
