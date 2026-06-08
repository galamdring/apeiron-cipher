# Core Principles

Rules that apply to every line of code, every system, every story. No exceptions.

1. **Knowledge is the only progression system.** There are no levels, no XP, no skill trees, no unlocks. The player progresses by understanding the world better. Every system serves this.
2. **Accretion over reward.** Every player action must teach something new through consequence, never through confirmation.
3. **Diegetic only.** The game never tells — it reveals through world behavior. No UI text explaining state. No popups. No progress bars. This applies to **reactive/feedback UI** — the game must never use text to explain why an action failed or what the player should do next. Physical in-world objects that display text as part of their nature (journal datapad, inscriptions, fabricator readouts) are *the diegetic mechanism*, not a violation of it.
4. **Deterministic.** Same seed + same inputs = same outputs. Always. Seeded RNG only. Explicit system ordering. No float ambiguity.
5. **Server-authoritative.** Intent is untrusted. Simulation decides. Even in single-player.
6. **Data-driven.** All tuning lives in asset files. If a tuning value is hardcoded in Rust source, it's wrong. Only truly fixed values (mathematical constants, protocol versions) are `const`.
7. **No `unsafe`. No `.unwrap()` in production (`.expect("reason")` where panicking is correct). No `pub(crate)`. No exceptions.**
8. **Default posture: stop and ask.** If the story doesn't specify the name, type, field, event, or architectural choice — stop and ask. Do not infer. Do not invent. Correctness over speed. See `implementation-patterns-consistency-rules.md` for the precise "stop" vs "proceed" breakdown.
   - **Entity types always have their required components.** The enforcement mechanism is Bevy's `#[require(...)]` attribute on marker components: required components are declared on the type definition, not repeated at every spawn site. If a spawn site lists components beyond the marker, the marker definition is missing `#[require]`. See the ECS Patterns section of `implementation-patterns-consistency-rules.md`.
9. **Document like Cave Johnson is reading.** Every public type, every system, every non-obvious line. Over-documentation is not a failure mode.
10. **Every system emits observations.** The Mirror System observes player behavior across all gameplay systems. If a system produces player-observable actions, it must emit observation hooks. No silent systems.
11. **Visual representation is functional, not decorative.** Terrain texture, surface detail, and material appearance are outputs of the same property parameters that drive gameplay simulation. No visual element exists for aesthetics alone. Appearance is always a readable signal about physical reality.
12. **Collision geometry matches the visible surface for any traversable or inhabitable space.** No bounding box approximations. No convex hull simplification. If the visual mesh has an opening, the collision space has the same opening. What the player can see, they can navigate. This is a hard constraint equivalent in force to "No unsafe." An implementing agent who cannot generate mesh-fidelity collision must stop and ask — not fall back to a bounding box.
