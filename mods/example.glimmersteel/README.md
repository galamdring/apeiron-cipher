# Glimmersteel — Example Mod for Apeiron Cipher

This is the **reference example mod** that ships with Apeiron Cipher's modding
documentation. It adds a single new material type — Glimmersteel — to the
material registry. Use it as a starter template for your own material mods.

---

## What this mod does

Glimmersteel is a dense, moderately heat-resistant material with an iridescent
metallic sheen. It occupies the high-density band above the base game's Osmium,
making it the heaviest classifiable material in the journal.

| Property           | Derived value (seed 9001) |
|--------------------|--------------------------|
| Density            | 0.8158                   |
| Thermal resistance | 0.4732                   |
| Reactivity         | 0.0618 (very stable)     |
| Conductivity       | 0.3061                   |
| Toxicity           | 0.5298                   |

The material appears in the player's journal once they observe a material
instance whose density and thermal resistance fall inside Glimmersteel's
classification ranges.

---

## Mod layout

```
example.glimmersteel/
├── mod.toml                              <- manifest (required)
├── README.md                             <- this file
└── assets/
    └── materials/
        └── classifications.toml         <- the single new classification entry
```

---

## How to install

1. Copy the `example.glimmersteel/` directory into the game data `mods/`
   folder (next to the base game `assets/`).
2. Launch the game. The mod is discovered automatically — no registration step
   is needed.
3. Explore. Once you encounter a material instance whose density falls in the
   0.81–0.88 band and thermal resistance in the 0.42–0.53 band, the journal
   will label it **Glimmersteel**.

---

## How to adapt this template

### Adding a new material type

1. Pick a seed value **above 9000**. The base game uses 1001–1999; 2000–8999
   is reserved for future use by the developers.
2. Compute the derived properties for your seed using the formula in
   [`docs/modding/data-formats.md`](../../docs/modding/data-formats.md)
   (or run `derive_material_from_seed` from a debug build).
3. Choose density and thermal_resistance ranges that do not overlap with any
   base game entry (see
   [`assets/materials/classifications.toml`](../../assets/materials/classifications.toml)
   for the full list).
4. Add a `[[classification]]` block to your mod's
   `assets/materials/classifications.toml` with the new entry.
5. Optionally, reference the seed in `assets/config/biomes.toml` to make the
   material appear in biome palettes, or add combination rules in
   `assets/config/combinations.toml`.

### File format reference

See [`docs/modding/data-formats.md`](../../docs/modding/data-formats.md) for
the full schema of every moddable file.

---

## Compatibility notes

- **No source code changes required.** This mod is 100% data-only.
- **No conflicts with base game materials.** Glimmersteel's density range
  (0.81–0.88) starts above Osmium's maximum (0.80), so the two entries never
  match the same observed instance.
- **Hot-reload supported.** In debug builds, saving
  `assets/materials/classifications.toml` while the game is running updates
  the registry immediately.

---

## License

CC-BY-4.0 — free to use, adapt, and redistribute with attribution.
