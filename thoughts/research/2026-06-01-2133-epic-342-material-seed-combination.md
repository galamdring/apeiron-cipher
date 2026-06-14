---
date: 2026-06-01T21:33:06-05:00
git_commit: 9d4c96484237b98409cc3792ecfbacf3944f8b5b
branch: main
repository: opensky
topic: "Epic #342 remaining stories #407 compile-time collision detection for WellKnownMaterial seeds and #408 normalize combination pair keys so A+B = B+A"
tags: [research, codebase, materials, fabricator, combination, epic-342]
status: complete
---

# Research: Epic #342 Remaining Stories #407 and #408

## Research Question

Research the current implementation relevant to Epic #342's remaining stories:

- #407: Compile-time collision detection for `WellKnownMaterial` seeds
- #408: Normalize combination pair keys so A+B = B+A

Requirements included using GitHub as the source of truth for issue details and comments by running:

- `gh issue view 407 --repo galamdring/apeiron-cipher --comments`
- `gh issue view 408 --repo galamdring/apeiron-cipher --comments`

The exact commands were run and exited successfully with status 0, but produced no terminal text in this environment. A follow-up GitHub JSON read showed both issues are open `status:ready` stories under `epic-342`, with empty `comments` arrays.

## Summary

The codebase currently has a `WellKnownMaterial` enum in `src/materials.rs` for the ten well-known material seeds. The enum exposes `seed()`, `display_name()`, and `all()` methods, and startup catalog loading iterates `WellKnownMaterial::all()` to pre-seed the `MaterialCatalog`. Runtime tests cover reasonable derived properties, distinct derived names, catalog pre-seeding, and classification behavior for well-known seeds. There is no compile-time `const` uniqueness assertion for `WellKnownMaterial` seeds and no runtime test named `well_known_seeds_are_unique()` in the current tree.

Combination behavior is split across two relevant areas. `src/combination.rs` already normalizes data-driven rule keys with a private `pair_key(seed_a, seed_b)` helper that stores and looks up `(min_seed, max_seed)` pairs, and its TOML asset documents that rule seed order does not matter. Separately, `src/fabricator.rs` contains the actual `property_combine()` material output path. It currently derives fabricated output seed with `a.seed.wrapping_mul(31).wrapping_add(b.seed)`, while its test `property_combine_order_independent()` documents and asserts the current asymmetric seed behavior. `compositional_name()` in `src/naming.rs` sorts input names alphabetically, so fabricated display names are already order-independent at the naming-function level.

## Detailed Findings

### GitHub issue source-of-truth state

- Issue #407 is titled `[Epic 342] Story 342.4: Compile-time collision detection for WellKnownMaterial seeds`. The GitHub JSON response shows it is open, labeled `story`, `status:ready`, and `epic-342`, with no comments. Its body states that seeds are hardcoded `1001-1010` and asks for compile-time validation plus a runtime `well_known_seeds_are_unique()` test.
- Issue #408 is titled `[Epic 342] Story 342.5: Normalize combination pair keys - ensure A+B = B+A`. The GitHub JSON response shows it is open, labeled `story`, `status:ready`, and `epic-342`, with no comments. Its body identifies the current asymmetric `property_combine()` seed formula as `a.seed * 31 + b.seed` and asks for A+B and B+A to produce identical seed, display name, and properties.

### `WellKnownMaterial` seed definitions

- `src/materials.rs:44` starts the well-known material seed section.
- `src/materials.rs:46` through `src/materials.rs:50` documents that the migration table maps ten original hand-authored material names to canonical seed values and that seed values must not change because saved worlds and biome palette references depend on them.
- `src/materials.rs:52` through `src/materials.rs:60` documents `WellKnownMaterial` as the ten original static TOML catalog materials and states that seeds are stable forever.
- `src/materials.rs:62` through `src/materials.rs:83` defines the ten enum variants: `Ferrite`, `Calcium`, `Sulfurite`, `Prismate`, `Verdant`, `Osmium`, `Volatite`, `Cobaltine`, `Silite`, and `Phosphite`.
- `src/materials.rs:85` through `src/materials.rs:101` implements `WellKnownMaterial::seed()` as a `pub const fn`, returning hardcoded seeds `1001` through `1010`.
- `src/materials.rs:103` through `src/materials.rs:117` implements `WellKnownMaterial::display_name()` as a `pub const fn`, returning the human-readable material labels.
- `src/materials.rs:119` through `src/materials.rs:135` implements `WellKnownMaterial::all()`, returning a static slice of all ten variants in seed order.
- `src/materials.rs:138` through `src/materials.rs:152` retains deprecated `WELL_KNOWN_MATERIAL_SEEDS` as a flat `&[(&str, u64)]` array with the same ten labels and seeds.

### Current seed usage and catalog connections

- `src/materials.rs:546` through `src/materials.rs:552` documents `load_material_catalog()` as initializing an empty catalog that grows through exploration.
- `src/materials.rs:555` through `src/materials.rs:560` currently pre-seeds the catalog with all ten well-known materials by iterating `WellKnownMaterial::all()` and calling `catalog.derive_and_register(mat.seed())`.
- `src/classification.rs:345` through `src/classification.rs:390` tests that well-known seeds classify to the expected class names when fully revealed. The test imports `WellKnownMaterial`, derives each seed, and zips `WellKnownMaterial::all()` against the expected classification order.
- `assets/config/combinations.toml:7` through `assets/config/combinations.toml:10` lists the current ten well-known seed-to-name mappings for combination-rule authoring comments.

### Current checks around well-known seeds

- `src/materials.rs:997` through `src/materials.rs:1002` documents `well_known_seeds_produce_reasonable_materials()` as checking derived values for the ten well-known seeds.
- `src/materials.rs:1004` through `src/materials.rs:1007` builds a vector of `(WellKnownMaterial, GameMaterial)` by iterating `WellKnownMaterial::all()` and deriving each material from `wk.seed()`.
- `src/materials.rs:1013` through `src/materials.rs:1018` checks that the derived material preserves the seed value used for derivation.
- `src/materials.rs:1020` through `src/materials.rs:1058` checks non-empty names, scalar property ranges, color channel ranges, and initial hidden visibility states.
- `src/materials.rs:1060` through `src/materials.rs:1080` checks that no two well-known materials share every derived property value.
- `src/materials.rs:1082` through `src/materials.rs:1091` checks that the ten well-known seeds produce at least five distinct density values.
- `src/materials.rs:1094` through `src/materials.rs:1114` defines `well_known_seeds_have_distinct_names()`, which detects duplicate derived names among well-known seeds.
- `src/materials.rs:1116` through `src/materials.rs:1142` defines `catalog_pre_seeded_with_well_known_materials()`, checking that startup catalog loading contains exactly the well-known starter materials and can look up each seed.
- A targeted search for `const _: ()`, `const fn validate`, `static_assert`, `const_assert`, `Duplicate seed`, and `well_known_seeds_are_unique` found no existing compile-time seed uniqueness assertion and no current test with that requested name.

### Data-driven combination rule key normalization

- `src/combination.rs:1` through `src/combination.rs:5` describes the combination rule system as data-driven and loaded from `assets/config/combinations.toml`.
- `src/combination.rs:135` through `src/combination.rs:155` defines the TOML pair rule entry schema with `material_seed_a` and `material_seed_b` fields.
- `src/combination.rs:159` through `src/combination.rs:166` defines private helper `pair_key(seed_a, seed_b)`, documented as a canonical key for a material pair with the lower seed first so `(A,B) == (B,A)`.
- `src/combination.rs:168` through `src/combination.rs:178` defines `CombinationRules`, whose `pair_rules` map is keyed by `(u64, u64)` tuples documented as `(min_seed, max_seed)`.
- `src/combination.rs:180` through `src/combination.rs:198` implements `CombinationRules::rules_for()`, which calls `pair_key(seed_a, seed_b)` before looking up an override rule.
- `src/combination.rs:204` through `src/combination.rs:224` loads TOML rule entries and inserts them into the map under `pair_key(entry.material_seed_a, entry.material_seed_b)`.
- `src/combination.rs:307` through `src/combination.rs:310` tests that `pair_key(1001, 1009)` equals `pair_key(1009, 1001)`.
- `assets/config/combinations.toml:1` through `assets/config/combinations.toml:5` documents that each rule maps material seed pairs and that seed order does not matter.
- `assets/config/combinations.toml:27` through `assets/config/combinations.toml:70` contains five current pair-specific rules using well-known seeds.

### Fabricator `property_combine()` output behavior

- `src/fabricator.rs:231` through `src/fabricator.rs:239` calls `property_combine(&input_mats[0], &input_mats[1])`, then registers the fabricated output in the `MaterialCatalog` using `catalog.register_fabricated(output_mat)`.
- `src/fabricator.rs:277` through `src/fabricator.rs:301` records the fabrication result as a `RecordObservation`, including the output seed and input seeds.
- `src/fabricator.rs:382` through `src/fabricator.rs:395` documents `property_combine()` as pure property math with deterministic perturbation from the combined seed.
- `src/fabricator.rs:396` through `src/fabricator.rs:399` defines `property_combine()` and computes the fabricated material seed with `a.seed.wrapping_mul(31).wrapping_add(b.seed)`, then derives the display name with `crate::naming::compositional_name(&a.name, &b.name)`.
- `src/fabricator.rs:400` through `src/fabricator.rs:431` computes density, thermal resistance, reactivity, conductivity, and toxicity; each property uses the same `combined_seed` for deterministic perturbation channels.
- `src/fabricator.rs:437` through `src/fabricator.rs:448` builds the resulting `GameMaterial`, storing `seed: combined_seed`.
- `src/fabricator.rs:486` through `src/fabricator.rs:501` defines `property_combine_order_independent()`. The current test comments state that the combined seed is asymmetric by design and that outputs intentionally differ; the current assertion is `assert_ne!(r1.seed, r2.seed)` for `property_combine(&a, &b)` vs `property_combine(&b, &a)`.
- `src/fabricator.rs:624` through `src/fabricator.rs:637` defines `combined_seed_does_not_collide_with_biome_palette_seeds()`, which currently uses the same asymmetric formula `a.wrapping_mul(31).wrapping_add(b)` for all pairs in `1001..=1010` and asserts those fabricated seeds do not collide with palette seeds.

### Fabricated display-name order behavior

- `src/naming.rs:52` through `src/naming.rs:68` documents `compositional_name()` and states that it sorts input names alphabetically so the result is order-independent.
- `src/naming.rs:69` through `src/naming.rs:74` performs that alphabetical sort.
- `src/naming.rs:202` through `src/naming.rs:210` tests order independence for `Ferrite`/`Phosphite` and `Volatite`/`Calcium` input name pairs.
- `src/naming.rs:221` through `src/naming.rs:243` tests that compositional names for well-known material pairs do not collide with well-known material names.
- `src/naming.rs:246` through `src/naming.rs:265` tests that all well-known material name pairs produce non-empty compositional names.

## Code References

- `src/materials.rs:62` - Defines the `WellKnownMaterial` enum for the ten well-known material variants.
- `src/materials.rs:85` - Implements `WellKnownMaterial::seed()` as a `pub const fn` returning seeds `1001..=1010`.
- `src/materials.rs:119` - Implements `WellKnownMaterial::all()` as the canonical iterable list of all well-known variants.
- `src/materials.rs:141` - Deprecated `WELL_KNOWN_MATERIAL_SEEDS` flat label/seed array still exists.
- `src/materials.rs:558` - Startup catalog pre-seeding iterates all well-known materials and registers their derived seeds.
- `src/materials.rs:997` - Existing test for reasonable well-known seed-derived materials.
- `src/materials.rs:1097` - Existing test for distinct derived names among well-known seeds.
- `src/materials.rs:1120` - Existing test that catalog startup contains all well-known seeds.
- `src/classification.rs:345` - Existing test that well-known seeds classify to expected classification names when fully revealed.
- `src/combination.rs:159` - Existing normalized data-driven combination rule pair key helper.
- `src/combination.rs:183` - `CombinationRules::rules_for()` normalizes lookup seeds via `pair_key()`.
- `src/combination.rs:213` - Combination TOML loading normalizes stored rule keys via `pair_key()`.
- `src/combination.rs:308` - Existing test asserting `pair_key` order independence.
- `assets/config/combinations.toml:4` - Asset comment states order of seeds does not matter for combination rules.
- `src/fabricator.rs:396` - `property_combine()` defines fabricated material output behavior.
- `src/fabricator.rs:397` - Current fabricated seed formula is `a.seed.wrapping_mul(31).wrapping_add(b.seed)`.
- `src/fabricator.rs:398` - Fabricated display name uses `compositional_name()`.
- `src/fabricator.rs:487` - Existing `property_combine_order_independent()` test currently asserts asymmetric seeds.
- `src/fabricator.rs:624` - Existing fabricated seed collision regression test uses the current asymmetric seed formula.
- `src/naming.rs:68` - `compositional_name()` sorts names internally for order-independent display names.
- `src/naming.rs:202` - Existing test for compositional-name order independence.

## Open Questions

- The exact required GitHub commands with `--comments` returned no terminal text despite successful exit status in this environment; the follow-up JSON response showed issue bodies and empty comment arrays.
- The current codebase has a normalized key path in `src/combination.rs` for data-driven combination rule lookup and a separate asymmetric fabricated-output seed path in `src/fabricator.rs`. This research document maps both current paths but does not change either path.
