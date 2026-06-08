# Crystal Lichen — Example Species Mod for Apeiron Cipher

This is the **reference example for organism/species mods** that ships with
Apeiron Cipher's modding documentation. It adds Crystal Lichen — a glassy,
blue-green photosynthetic crust that colonises cold-wet rock faces — as a
fully playable entity in the material and biome systems.

---

## What this mod does (today)

Flora and fauna as interactive entities are implemented in a future epic.
What the game can do **right now** is:

1. **Classify and name the organism's material substrate.** The fabricator
   and journal will identify Crystal-Lichen Substrate as a distinct material
   type once the player observes and handles a sample.
2. **Generate a biome where the lichen dominates.** Crystal-Lichen Fields
   appear in cold-wet regions of the temperature × moisture map. Exploration
   on the right planet type will surface this biome.
3. **Define fabrication reactions** between the lichen substrate and other
   materials, giving it a distinct role at the workbench.

Together these three hooks give Crystal Lichen a real presence in the game
today — the player can find it, study it in the journal, and experiment with
it at the fabricator — even before entity-level species spawning exists.

---

## Material properties

| Property           | Derived value (seed 9002) | What it signals |
|--------------------|--------------------------|-----------------|
| Density            | 0.3183                   | Porous; lighter than most minerals |
| Thermal resistance | 0.4121                   | Moderate tolerance; cold climate native |
| Reactivity         | 0.6054                   | Chemically active metabolic substrate |
| Conductivity       | 0.9418                   | Exceptional — glassy crystal matrix |
| Toxicity           | 0.3551                   | Mildly toxic; handle cautiously |

Color: cool blue-green (`[0.41, 0.73, 0.85]`)

The material appears in the player's journal once they observe an instance
whose **density falls in 0.28–0.37** and **thermal resistance falls in
0.34–0.48**. These ranges are the only gap-free zone in that density band
relative to base game materials.

---

## Biome: Crystal-Lichen Fields

Crystal-Lichen Fields occupy the **cold-wet** corner of temperature ×
moisture space (temperature 0.0–0.25, moisture 0.5–0.85). Visually: a
teal-grey crystalline crust under subdued lighting.

Material palette for this biome:

| Material                  | Seed | Selection weight |
|---------------------------|------|-----------------|
| Crystal-Lichen Substrate  | 9002 | 4.0 (dominant)  |
| Silite (rock substrate)   | 1009 | 2.0             |
| Calcium (matrix remnant)  | 1002 | 1.5             |
| Prismate (rare crystals)  | 1004 | 0.8             |

> **Load-order note (Epic 23):** The base game's `frost_shelf` biome covers a
> partially overlapping region. Until mod load-order control is implemented
> (Epic 23), `frost_shelf` may win in overlap zones because it loads first.
> This mod's biome range is designed to complement rather than conflict; the
> unique region (temp 0.0–0.25, moisture 0.5–0.85) produces Crystal-Lichen
> Fields wherever frost_shelf does not already claim the chunk.

---

## Fabrication reactions

Crystal-Lichen Substrate is a **biological catalyst** at the workbench. Its
high conductivity and reactivity amplify heat-related properties in partner
materials:

| Pair                            | Notable output |
|---------------------------------|----------------|
| Crystal-Lichen + Ferrite        | Amplified density and thermal resistance; combined conductivity wins |
| Crystal-Lichen + Silite         | Natural pairing; conductivity dominates; reactivity neutralised |
| Crystal-Lichen + Prismate       | Crystalline resonance; thermal resistance spiked ×1.5 |

---

## Mod layout

```
example.crystal-lichen/
├── mod.toml                                <- manifest (required)
├── README.md                               <- this file
└── assets/
    ├── materials/
    │   └── classifications.toml           <- Crystal-Lichen Substrate classification
    └── config/
        ├── biomes.toml                    <- Crystal-Lichen Fields biome
        └── combinations.toml             <- fabricator reaction rules
```

---

## How to install

1. Copy the `example.crystal-lichen/` directory into the game data `mods/`
   folder (next to the base game `assets/`).
2. Launch the game. The mod is discovered automatically — no registration
   step is needed.
3. Explore a cold, wet planet. Once you find a surface deposit whose density
   falls in the 0.28–0.37 band and thermal resistance in the 0.34–0.48 band,
   the journal labels it **Crystal-Lichen Substrate**.
4. Bring a sample to the fabricator and combine it with Ferrite, Silite, or
   Prismate to observe the catalytic reactions.

---

## How to adapt this template

### Adapting the material

1. Pick a seed value **above 9000**. Base game: 1001–1999. Reserved: 2000–8999.
2. Compute derived properties with the formula in
   [`docs/modding/data-formats.md`](../../docs/modding/data-formats.md).
3. Choose density and thermal ranges that do not overlap with any existing
   classification entry (base game or installed mods).
4. Edit `assets/materials/classifications.toml` with your new entry.

### Adapting the biome

1. Choose a temperature × moisture region not already covered by base game
   entries (`scorched_flats`, `mineral_steppe`, `frost_shelf`).
2. Edit `assets/config/biomes.toml`. Set `biome_type` to a new snake_case
   identifier.
3. Set your material's seed in `[[biomes.material_palette]]`.
4. Pick a `ground_color` that reads coherently with your material's fiction.

### Adapting fabrication rules

1. Write `[[rules]]` entries in `assets/config/combinations.toml`.
2. Use `material_seed_a` / `material_seed_b` for the pair. Order is irrelevant.
3. Choose a rule type for each property:
   - `Blend { weight_a, weight_b }` — predictable weighted average
   - `Max` / `Min` — takes the higher or lower of the two inputs
   - `Catalyze { multiplier }` — max of inputs × multiplier (emergent)
   - `Inert` — produces waste (all properties → 0.1)

---

## What's next for species mods (Epic 23+)

The game's flora and fauna entity systems are planned for a later epic. When
those systems ship, species mods will gain:

- **`assets/flora/`** — structural definitions (morphology, scale, seasonal
  state, collision geometry) for giant flora organisms
- **Spawn rules** — per-biome probability weights controlling how often the
  organism appears during world generation
- **Behaviour parameters** — seasonal opening/closing, microclimate modifiers,
  fauna affinity tables

This mod is structured to receive those additions without breaking changes:
the material and biome hooks it installs today will connect naturally to the
entity layer when it arrives.

---

## Compatibility notes

- **No source code changes required.** This mod is 100% data-only.
- **No conflicts with base game materials.** Crystal-Lichen Substrate's
  density range (0.28–0.37) sits entirely between Prismate's max (0.27)
  and Silite's min (0.38).
- **Biome overlap is handled gracefully.** The Crystal-Lichen Fields range
  is designed to minimise overlap with `frost_shelf`. Where overlap occurs,
  first-match load ordering determines which biome wins until Epic 23 adds
  explicit override control.
- **Hot-reload supported.** In debug builds, saving any of the three asset
  files while the game is running updates the relevant registry immediately.

---

## License

CC-BY-4.0 — free to use, adapt, and redistribute with attribution.
