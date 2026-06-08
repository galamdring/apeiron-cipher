# Solar Array — Example Structure Mod for Apeiron Cipher

This is the **reference example for structure mods** that ships with Apeiron Cipher's
modding documentation. It adds the Solar Array — a buildable photovoltaic structure
that harvests stellar radiation — as a fully defined structure ready to integrate with
the game's construction and power systems.

---

## What this mod does

The Solar Array mod defines a complete, minimal structure from the ground up:

- **New material:** Solite (seed 9101) — a photovoltaic semiconductor found in
  irradiated surface deposits near luminous stars
- **New deposit type:** Solite deposits on exterior surfaces (additive, rare weight 0.6)
- **New structure:** `solar_array` — a 4m×4m exterior-placement power structure built
  from Photovoltaic Panels, an Orientation Frame, and a Power Junction

The structure definition demonstrates every field the construction system will consume:
blueprint footprint, build cost by component, forward-compatible effects, sprite paths,
and deconstruction recovery rules.

---

## What is playable today vs. future epics

The construction and power systems are planned for Epic 8+. Before that:

| Capability | Status |
|---|---|
| Solite material classification loads, classifies observed samples | Ready today — uses existing MaterialPlugin |
| Solite deposit type spawns on exterior surfaces | Ready today — uses existing exterior deposit system |
| Structure definition file loads, logs `AssetEvent::Added` | Ready when structure asset loader ships (Epic 8+) |
| Blueprint editor shows Solar Array in build menu | Requires Epic 8+ construction system |
| Structure builds from Photovoltaic Panel + Orientation Frame + Power Junction | Requires Epic 8+ construction system |
| Power output scales with star luminosity | Requires Epic 8+ power system |
| Environmental modifiers reduce output in corrosive/dim conditions | Requires Epic 8+ simulation layer |
| Deconstruction returns 75% of materials | Requires Epic 8+ construction system |

**Today:** The material classification and deposit files load and function immediately.
Solite samples spawn on terrain, appear in the journal, and are classifiable. The
structure definition file is structurally valid — when the construction system ships,
it discovers this file automatically. No re-authoring needed.

---

## The Solar Array

A photovoltaic array mounted on a motorized orientation frame. Four panels track the
local star across the sky. Output is proportional to stellar luminosity and degrades
slightly over time as panel surfaces weather.

When fully built on a temperate world orbiting a G-type star, a single solar array
produces enough energy to power a small fabrication workspace. On a high-luminosity
hot star, output nearly doubles. On a dim red dwarf, the same array struggles to
power a single appliance.

### Build cost

| Component | Quantity | Materials |
|---|---|---|
| Photovoltaic Panel | 4 | 3× Solite + 1× Silite per panel |
| Orientation Frame | 1 | 6× Ferrite + 2× Osmium |
| Power Junction | 1 | 4× Cobaltine + 2× Ferrite |

### Effects

- **Power output:** 12.0 energy units / game-day at baseline (G-type star, 1 AU)
- **Scaling:** linear with star luminosity, inverse with orbital distance
- **Corrosive atmosphere:** ×0.6 output multiplier
- **Low light (luminosity < 0.3):** ×0.2 output multiplier
- **Degradation:** –0.5% condition / game-day (replace panels periodically)

### Deconstruction

- 75% material recovery at full condition (scales with condition)
- Orientation Frame is guaranteed recovery (precision component, fully disassembles)

---

## Mod layout

```
example.solar-array/
├── mod.toml                                  <- manifest (required)
├── README.md                                 <- this file
└── assets/
    ├── materials/
    │   └── classifications.toml             <- Solite material classification (seed 9101)
    ├── exterior/
    │   └── surface_mineral_deposits.toml    <- Solite deposit type (additive)
    └── structures/
        └── solar_array.toml                 <- structure definition (forward-compatible)
```

---

## How to install

1. Copy the `example.solar-array/` directory into the game data `mods/` folder.
2. Launch the game. Solite deposits begin spawning on terrain immediately.
3. Collect Solite samples — they appear blue-white and slightly translucent in bright
   environments. The journal classifies them once thermal resistance is revealed.
4. When the construction system ships: enter blueprint mode, find Solar Array under the
   Power category, check the build cost, collect the remaining materials, and confirm.

---

## Discovering Solite in-game

Solite is rare. It spawns in irradiated surface layers — look for planets close to
their star or with documented radiation events in the journal. The deposit clusters
are small (4–7 nodes) and tightly grouped. When you find a cluster, collect all of
it — there won't be much nearby.

The journal classifies Solite once you've observed enough samples to reveal its
thermal resistance. Its density profile (very light) and conductivity (high) are
the first tells. Once classified, the journal shows its name.

---

## How to adapt this template

### Defining a new structure

1. Create `assets/structures/your_structure.toml`. Copy from `solar_array.toml`.
2. Set a unique `structure.id` (snake_case). This ID ties the structure to its cost
   entries, effects, and blueprint preview.
3. Set `category` to one of: `"power"`, `"storage"`, `"extraction"`, `"habitat"`,
   `"defense"`, `"decoration"`.
4. Set `blueprint.placement` to `"exterior"`, `"interior"`, or `"both"`.
5. Define `[[cost.components]]` entries — each component is staged separately.
   Each component gets `[[cost.components.materials]]` entries with `material_seed`
   and `quantity`.
6. Define `[effects]` — the simulation layer reads these when the structure is active.
7. Optionally define `[deconstruction]` — if absent, the game uses the default
   (50% material recovery, no guaranteed components).

### Adding a new material for your structure

1. Choose a seed above 9000. Verify no other mod you depend on uses it.
2. Create `assets/materials/classifications.toml` in your mod.
3. Add a `[[classification]]` entry with `name`, `display_name`, and property ranges.
4. Keep the ranges clearly distinct from base game entries
   (see docs/modding/best-practices.md). Run the overlap check:
   `python3 -c "import tomllib; ..."` or use `cargo test` once asset_validation
   is available. Solite's density range was adjusted during development to avoid
   overlapping Calcium's density [0.09, 0.20] at the compound density×tr check.
5. Reference your seed in `[[cost.components.materials]]` in the structure definition.

### Making the material discoverable on terrain

1. Create `assets/exterior/surface_mineral_deposits.toml` in your mod.
2. Add a `[[deposits]]` entry. Choose a low `selection_weight` (0.3–0.8) for rare
   materials so the player has to seek them out.
3. The key is your deposit identifier; it does not need to match the material name.

---

## Forward-compatibility notes

### Why forward-declare the structure if it doesn't build yet?

For the same reason the language example mod forward-declares vocabulary before the
language system ships: when the construction system comes online, it scans all
`assets/structures/*.toml` files — in the base game and in every loaded mod — and
populates the structure registry. If the file exists and is valid, the structure
appears in the build menu automatically.

Authoring the file now means:
- The mod is structurally testable today via `cargo test --test asset_validation`
- Modders have a concrete annotated example to learn from before the system ships
- No re-authoring needed when Epic 8+ lands

### What "forward-compatible" means in practice

Every `structures/*.toml` file must pass the asset validator. That validator checks:
- `schema_version` is present and is a positive integer
- Required fields (`structure.id`, `structure.display_name`, `blueprint.*`) are present
- `placement` is one of the allowed values
- All `material_seed` references are positive integers (existence is not validated
  until the material system ships)
- TOML syntax is well-formed

The structure system will add additional semantic validation (does the footprint
fit on the terrain type? do all referenced material seeds exist?) when it ships.
For now, structural validity is sufficient.

---

## Compatibility notes

- **No source code changes required.** This mod is 100% data-only.
- **Additive content only.** The new deposit type and material classification do not
  replace any base game entry. See docs/modding/best-practices.md.
- **Seed 9101 is claimed.** If you fork this mod, choose a different seed for your
  version of Solite to avoid cross-mod classification conflicts.
- **Forward-compatible schema.** All files carry `schema_version = 1`. The loader
  migrates older files forward automatically when the schema increments.

---

## License

CC-BY-4.0 — free to use, adapt, and redistribute with attribution.
