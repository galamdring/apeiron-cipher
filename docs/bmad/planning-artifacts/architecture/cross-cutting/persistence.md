# Cross-Cutting Concern: Persistence / Save Architecture

Knowledge accumulation is the player's only progression; save corruption or loss is catastrophic and unrecoverable. The save system must: serialize deterministically, support version migration as schemas evolve, handle the full knowledge spectrum (not just checkpoints), and anticipate multiplayer save authority (distributed consensus problem in Ring 5). Persistence is not a late-stage feature — it is core infrastructure from Ring 1.

**Required persistence scope (non-exhaustive, grows with each feature):**

- **Giant flora base state:** player-placed structures inside flora interiors, flora seasonal open/closed state, and interior environmental state (atmospheric composition, light levels, chemical concentrations) must persist. Seasonal state is the product of simulation time accumulation — it must NOT be regenerated from seed on load. Regenerating from seed would erase the history of what happened to any player base inside.
- **Found ship repair state:** per-component repair status, structural integrity, and flight capability must persist. This is the player's first fabrication history. Losing it on reload would destroy the meaning of the repair progression.

---

## Generation Versioning Policy

### What `GENERATION_VERSION` tracks

`GENERATION_VERSION` (defined in `src/world_generation.rs`) is a `u32` constant that
represents the world-generation algorithm version. It is embedded in every save file's
`SaveHeader.generation_version` field (see `src/persistence/schema.rs`).

It tracks the **deterministic generation pipeline** — not the save-file binary format
(that is `SAVE_SCHEMA_VERSION` in the same schema module). The two are intentionally
separate: a save-format change does not imply the algorithm changed, and vice versa.

### When to bump `GENERATION_VERSION`

Increment this constant (by 1 — never reset to 0 or 1 after the first release) when
**any** change would alter what the deterministic generation pipeline produces for an
existing seed. This includes:

- Noise parameters: octaves, frequency, amplitude, gain, lacunarity, etc.
- Biome classification rules or biome weight modifiers
- Derivation order for any seeded value (seeds, channel tags, mix order)
- Threshold or rounding changes in placement or surface logic
- Adding or removing a generation step that touches per-chunk outputs

Changes that do **not** require a bump: rendering code, UI, save-format encoding (that
bumps `SAVE_SCHEMA_VERSION`), or anything that does not affect what content appears at
a given coordinate for a given seed.

When in doubt, bump it. A spurious version increment means players see a one-time
warning; a missed increment means their visited chunks silently mismatch the world.

### Mismatch warning behavior

On save load the game compares `SaveHeader.generation_version` (from the file) against
the compiled `GENERATION_VERSION`:

- **Exact match** — load proceeds normally.
- **Save version lower than compiled version** — warn the player that world generation
  has been updated. Chunks the player has **already visited** were baked into the save
  and are safe to load. Chunks the player has **not yet visited** will be regenerated
  using the new algorithm, which may produce different content near old-visit boundaries.
- **Save version higher than compiled version** — the save was written by a newer binary.
  Refuse to load. Regenerating chunks under a downgraded algorithm would silently corrupt
  the world.

The warning is surfaced to the player before any state is applied, giving them the option
to decline the load. The implementation site is the save-loader / startup system
(see `src/persistence/schema.rs` for the `SaveHeader` type; the actual load-time check
lives in the save loader when that story lands).

### Migration strategies (for future consideration)

Three strategies exist for handling a generation-version mismatch. None is implemented
yet; the choice should be deferred until a concrete migration need arises. They are
documented here so the design space is understood before the decision is forced.

**Strategy A — Bake visited chunks into the save**

Serialise the fully-generated content (tiles, objects, surface geometry) for every
chunk the player visits, not just the seed and mutation log. On load with a version
mismatch, restore baked chunks verbatim and only re-generate unvisited chunks using the
new algorithm.

*Pros:* visited content is perfectly preserved regardless of algorithm changes.
*Cons:* save files grow proportionally with world exploration; serialisation and
deserialization complexity is much higher.

**Strategy B — Maintain legacy generation codepaths**

Keep older generation algorithm versions compiled alongside the current one. When
loading a mismatched save, regenerate chunks on demand using the generation version
stored in `SaveHeader.generation_version` rather than the current algorithm.

*Pros:* pixel-perfect preservation; no data growth.
*Cons:* code complexity multiplies with each version; testing surface explodes; older
algorithm versions must remain compilable indefinitely.

**Strategy C — Accept breaking saves**

Treat a generation-version bump as a soft save break. Warn the player, offer them a
choice to proceed, and accept that unvisited chunks may differ. Visited chunks that
were baked into mutations (visited objects, player actions) are preserved; the terrain
scaffold beneath them may differ if it was not explicitly saved.

*Pros:* zero extra code; scales to any number of algorithm versions.
*Cons:* players experience world changes near visit boundaries; appropriate only for
early access / pre-release where this expectation is set.

The current implementation (save-generation constant + header field) is deliberately
strategy-agnostic — it provides the version information needed by any of the three
without committing to a migration mechanism.

---

## Source Links

- Generation version constant: `src/world_generation.rs` — `pub const GENERATION_VERSION: u32`
- Save header type with embedded generation version: `src/persistence/schema.rs` — `SaveHeader`
- Schema version constant (save-format version, separate from generation version):
  `src/persistence/schema.rs` — `pub const SAVE_SCHEMA_VERSION: u32`
- Load-time mismatch check: save-loader / startup system (not yet implemented; will
  live in `src/persistence/` when that story lands)
