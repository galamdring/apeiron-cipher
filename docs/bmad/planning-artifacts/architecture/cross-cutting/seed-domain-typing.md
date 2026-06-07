# Cross-Cutting Concern: Seed Domain Typing

## Decision

Every value that represents a deterministic generation seed must have an enforced Rust type for its domain. Bare `u64` seed plumbing is not an acceptable long-term architecture, even for derived sub-seeds.

This rule applies to root seeds, derived sub-seeds, per-chunk generation keys, material seeds, biome/climate seeds, placement seeds, object-identity seeds, elevation seeds, solar-system seeds, and any future deterministic generation seed domain.

## Why This Exists

Procedural output can be wrong while still looking plausible. A placement seed passed where an object-identity seed is expected will still produce deterministic numbers, but those numbers belong to the wrong generation domain. If every seed is a bare `u64`, Rust cannot protect the architecture from cross-domain mistakes.

Seed newtypes make that mistake a compile-time error:

```rust
pub struct MaterialSeed(pub u64);
pub struct PlacementDensitySeed(pub u64);
pub struct ObjectIdentitySeed(pub u64);
```

Each type can still carry the same deterministic numeric value internally, but a function that expects `MaterialSeed` cannot accidentally receive `PlacementDensitySeed`.

## Rule

If a value semantically represents a seed, it gets a domain-specific type.

- No struct field named `*_seed` should be a bare `u64`.
- No function parameter representing a seed should be a bare `u64`.
- No registry, cache, observation key, or generated identifier should key seed-domain data by a bare `u64`.
- Derived sub-seeds do not shed type safety after derivation. `PlanetSeed` deriving a placement seed must produce a placement-seed type, not a raw integer.
- Per-chunk or per-candidate keys derived from seed domains also need explicit key types when they cross helper boundaries or are stored.

The narrow exception is seed utility internals: hashing, mixing, serialization adapters, and conversion code may operate on raw integers at the boundary where bits are actually mixed or persisted. Those boundaries must be explicit and local.

## Naming Guidance

Use names that describe the generation domain, not merely the storage shape:

```rust
pub struct SolarSystemSeed(pub u64);
pub struct PlanetSeed(pub u64);
pub struct MaterialSeed(pub u64);
pub struct PlacementDensitySeed(pub u64);
pub struct PlacementVariationSeed(pub u64);
pub struct ObjectIdentitySeed(pub u64);
pub struct BiomeClimateSeed(pub u64);
pub struct ElevationSeed(pub u64);
pub struct TerrainTextureSeed(pub u64);
pub struct GiantFloraSeed(pub u64);
pub struct FloraMeshSeed(pub u64);
pub struct ShipDamageSeed(pub u64);
```

When a derived value is no longer a root seed but a scoped generation key, name it as a key:

```rust
pub struct ChunkPlacementDensityKey(pub u64);
pub struct ChunkPlacementVariationKey(pub u64);
pub struct ChunkObjectIdentityKey(pub u64);
```

The story or issue must specify new public seed type names. If implementation requires a public seed domain name that the story does not provide, follow the autonomy rule: stop and ask rather than inventing architectural vocabulary.

## Derivation Pattern

Seed derivation should preserve type information at every boundary:

```rust
impl PlanetSeed {
    pub fn placement_density_seed(self) -> PlacementDensitySeed {
        PlacementDensitySeed(mix_seed(self.0, PLACEMENT_DENSITY_CHANNEL))
    }
}

impl PlacementDensitySeed {
    pub fn for_chunk(self, chunk_key: u64) -> ChunkPlacementDensityKey {
        ChunkPlacementDensityKey(mix_seed(self.0, chunk_key))
    }
}
```

The exact helper shape can vary by module, but the architectural invariant cannot: callers should receive and pass typed seed values, not anonymous integers.

A further example of visual-layer separation: `MaterialSeed` derives a `TerrainTextureSeed` via a `TERRAIN_TEXTURE_CHANNEL` constant. The visual layer and the gameplay material layer share a root seed but are separated by domain type, so visual-parameter generation and material-identity generation cannot silently cross-contaminate.

```rust
impl MaterialSeed {
    pub fn terrain_texture_seed(self) -> TerrainTextureSeed {
        TerrainTextureSeed(mix_seed(self.0, TERRAIN_TEXTURE_CHANNEL))
    }
}
```

## Material Seeds Are Still Not Type Identifiers

`MaterialSeed` gives compile-time safety to the procedural generation input. It does not turn the seed into a material type identifier.

Material identity remains emergent and query-time: observed generated properties are compared against asset-defined classification ranges. A material seed is a deterministic input used to produce world facts; it is not the stored name of a substance such as "iron".

## TerrainTextureSeed Is Not a Visual Style Identifier

`TerrainTextureSeed` is not a visual style identifier. It is a deterministic input that produces texture parameters communicating material properties; it does not name a visual style. Selecting "which texture set to use" is not the role of this seed — the seed drives parameter generation, and the resulting parameters are what determine how the surface looks and what it communicates about the underlying material.

## Serialization and Configuration Boundaries

Configuration files, save files, and debug tools may expose numeric seed values because users and serialized formats need stable primitives. Convert those values into domain types immediately after loading or parsing.

Acceptable boundary pattern:

```rust
let planet_seed = PlanetSeed(config.planet_seed);
let material_seed = MaterialSeed(saved.material_seed);
```

Unacceptable propagation pattern:

```rust
let planet_seed = config.planet_seed;
world_profile.placement_density_seed = mix_seed(planet_seed, PLACEMENT_DENSITY_CHANNEL);
```

Raw integers should not leak beyond the boundary where they are loaded, saved, displayed, or mixed.

## Testing Expectation

Tests for deterministic generation should assert not only that outputs are stable, but that APIs preserve seed-domain separation. Compile-time type safety is the primary enforcement mechanism; integration tests then verify that typed derivation still produces deterministic output for identical typed inputs.
