# Material Identity Encoding

**Status:** DRAFT — pending owner review before implementation begins
**Authored for:** Epic #342 — Material identity and combination system refactor
**Prerequisite for:** WellKnownMaterial ID constants (Story), combination system
refactor (Story), compositional naming (Story)

---

## Overview

This document defines the compact integer encoding scheme for material identities
in Apeiron Cipher. It specifies:

- The 16-bit `MaterialId` type for base materials
- The 32-bit `CombinedMaterialId` bit layout for fabricated materials
- ID allocation bands and future-expansion strategy
- Compile-time uniqueness enforcement
- Rationale for every non-obvious choice

---

## Concept Separation: Seed vs ID

Two concepts that must not be conflated:

| Concept | Type | What it is |
|---------|------|-----------|
| `MaterialSeed(u64)` | Generation input | Deterministic input to the property-generation algorithm. Same seed = same properties. Not an identity. |
| `MaterialId(u16)` | Stable logical identity | Compact integer constant assigned to a well-known base material. Used for combination encoding and catalog lookup. Not a generation parameter. |

A `MaterialSeed` tells the generator *what properties to produce*.
A `MaterialId` names *what substance was produced* at the logical level.

These are orthogonal. Seeds 1001–1010 remain stable generation inputs forever.
IDs 1–10 are stable logical names forever. They move in lockstep (the same
`WellKnownMaterial` variant carries both) but serve different callers.

---

## Base Material ID — 16-bit (`MaterialId`)

### Bit Layout

```
 15                              0
┌────────────────────────────────┐
│  unsigned 16-bit integer value │
└────────────────────────────────┘
```

No internal bit-field subdivision. The full value is the ID.

### Allocation Bands

| Range | Decimal | Meaning |
|-------|---------|---------|
| `0x0000` | 0 | Reserved — null / "no material" sentinel |
| `0x0001`–`0x00FF` | 1–255 | **Well-known materials** — assigned in Rust source as `WellKnownMaterial::id()` constants; immutable after publication |
| `0x0100`–`0xFFFF` | 256–65535 | Future expansion — IDs assigned at runtime or by future story work |

The well-known band provides 255 slots; 10 are currently occupied, leaving 245
for future built-in materials before the band limit needs revisiting.

### Current WellKnownMaterial Assignments

| Variant    | `MaterialId` (hex) | `MaterialId` (dec) | `MaterialSeed` |
|------------|-------------------|--------------------|---------------|
| Ferrite    | `0x0001`          | 1                  | 1001          |
| Calcium    | `0x0002`          | 2                  | 1002          |
| Sulfurite  | `0x0003`          | 3                  | 1003          |
| Prismate   | `0x0004`          | 4                  | 1004          |
| Verdant    | `0x0005`          | 5                  | 1005          |
| Osmium     | `0x0006`          | 6                  | 1006          |
| Volatite   | `0x0007`          | 7                  | 1007          |
| Cobaltine  | `0x0008`          | 8                  | 1008          |
| Silite     | `0x0009`          | 9                  | 1009          |
| Phosphite  | `0x000A`          | 10                 | 1010          |

IDs are small sequential integers starting at 1. The hex form highlights the
band structure; decimal equivalents are equally valid and preferred in debug
output.

---

## Combined Material ID — 32-bit (`CombinedMaterialId`)

A combined material is produced by the fabricator from two base material inputs.
The 32-bit ID encodes both constituent base IDs directly, with no hash, no salt,
and no information loss.

### Bit Layout

```
 31              16 15               0
┌──────────────────┬─────────────────┐
│  min(id_a, id_b) │ max(id_a, id_b) │
│  (canonical low) │ (canonical high) │
└──────────────────┴─────────────────┘
```

The two 16-bit halves hold the constituent base IDs sorted in ascending order.
The numerically smaller ID always occupies the high 16 bits; the larger ID
always occupies the low 16 bits.

### Encoding Formula

```
sorted_lo = min(id_a, id_b)
sorted_hi = max(id_a, id_b)
combined  = (sorted_lo as u32) << 16 | (sorted_hi as u32)
```

### Decoding Formula

```
id_lo = (combined >> 16) as u16    // the smaller constituent ID
id_hi = (combined & 0xFFFF) as u16 // the larger constituent ID
```

### Examples

| Input A (hex) | Input B (hex) | `CombinedMaterialId` (hex) | Decodes to |
|--------------|--------------|---------------------------|-----------|
| Ferrite `0x0001` | Calcium `0x0002` | `0x0001_0002` | Ferrite + Calcium |
| Calcium `0x0002` | Ferrite `0x0001` | `0x0001_0002` | identical — order-independent |
| Ferrite `0x0001` | Phosphite `0x000A` | `0x0001_000A` | Ferrite + Phosphite |
| Osmium `0x0006` | Silite `0x0009` | `0x0006_0009` | Osmium + Silite |

Reading the hex value left-to-right gives the constituent pair in ascending ID
order. `0x0001_0002` is "material 1 combined with material 2" — unambiguous
at a glance in debug output.

### Order Independence

Because inputs are sorted before packing, the combined ID is identical regardless
of which slot the player used:

```
pack(Ferrite, Calcium) = 0x0001_0002
pack(Calcium, Ferrite) = 0x0001_0002   ← same
```

This eliminates review finding 1.2 (order-dependent combination pair keys) at
the encoding layer with zero runtime cost (one min/max compare).

---

## Rust Type Sketch

These are the intended newtype definitions. The implementing story must use these
exact types and implement the const methods shown. The `id()` method on
`WellKnownMaterial` is added alongside the existing `seed()` method.

```rust
/// Stable 16-bit identity for a base material.
///
/// Distinct from [`MaterialSeed`] — this is a logical name constant, not a
/// procedural generation input. Well-known materials have IDs in 0x0001–0x00FF;
/// 0x0000 is reserved as the null sentinel.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct MaterialId(pub u16);

impl MaterialId {
    /// Null sentinel — indicates "no material" in optional contexts.
    pub const NONE: Self = Self(0);
}

/// Packed 32-bit identity for a material produced by combining two base materials.
///
/// Bit layout:
///   - High 16 bits: `min(id_a, id_b)` (the smaller constituent ID)
///   - Low  16 bits: `max(id_a, id_b)` (the larger constituent ID)
///
/// Packing is order-independent: `CombinedMaterialId::new(a, b)` ==
/// `CombinedMaterialId::new(b, a)` for any `a`, `b`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct CombinedMaterialId(pub u32);

impl CombinedMaterialId {
    /// Pack two base material IDs into a canonical combined ID.
    /// Input order does not affect the result.
    pub const fn new(a: MaterialId, b: MaterialId) -> Self {
        let lo = a.0.min(b.0) as u32; // smaller ID → high 16 bits
        let hi = a.0.max(b.0) as u32; // larger ID  → low  16 bits
        Self((lo << 16) | hi)
    }

    /// Recover the two constituent base material IDs.
    /// Always returns them in ascending order: `result.0.0 <= result.1.0`.
    pub const fn constituents(self) -> (MaterialId, MaterialId) {
        let lo = (self.0 >> 16) as u16;
        let hi = (self.0 & 0xFFFF) as u16;
        (MaterialId(lo), MaterialId(hi))
    }
}

impl WellKnownMaterial {
    /// Stable 16-bit identity for this base material.
    ///
    /// Complement to [`Self::seed`] — the seed is the generation input; the ID
    /// is the logical name constant for combination encoding and catalog lookup.
    /// Both are forever stable.
    pub const fn id(self) -> MaterialId {
        match self {
            Self::Ferrite    => MaterialId(1),
            Self::Calcium    => MaterialId(2),
            Self::Sulfurite  => MaterialId(3),
            Self::Prismate   => MaterialId(4),
            Self::Verdant    => MaterialId(5),
            Self::Osmium     => MaterialId(6),
            Self::Volatite   => MaterialId(7),
            Self::Cobaltine  => MaterialId(8),
            Self::Silite     => MaterialId(9),
            Self::Phosphite  => MaterialId(10),
        }
    }
}
```

### Compile-Time Uniqueness Assertion

The implementing story must add a const assertion that validates all
`WellKnownMaterial` IDs are unique, mirroring the existing seed-uniqueness
assertion:

```rust
const fn validate_well_known_material_id_uniqueness(ids: &[u16]) {
    let mut i = 0;
    while i < ids.len() {
        let mut j = i + 1;
        while j < ids.len() {
            if ids[i] == ids[j] {
                panic!(
                    "duplicate WellKnownMaterial id detected; every \
                     WellKnownMaterial::id() value must be unique",
                );
            }
            j += 1;
        }
        i += 1;
    }
}

const _: () = validate_well_known_material_id_uniqueness(&[
    WellKnownMaterial::Ferrite.id().0,
    WellKnownMaterial::Calcium.id().0,
    // ... all 10 variants
]);
```

---

## What Stays the Same

- `MaterialSeed(u64)` — generation input, unchanged. Seeds 1001–1010 are stable.
- `GameMaterial::seed` field — generation identity, unchanged.
- `MaterialCatalog` primary index — `seed → GameMaterial`, unchanged.
- Biome palette TOML files — they reference seeds, not IDs. No migration.
- `property_combine()` — combination algorithm unchanged. Only the key used to
  store/look up combination results changes from name-pair to
  `CombinedMaterialId`.

## What Changes

| Site | Old | New |
|------|-----|-----|
| `WellKnownMaterial` | `seed(self) -> u64` only | Gains `id(self) -> MaterialId` |
| `MaterialCatalog` | No ID index | Gains `get_by_id(MaterialId) -> Option<&GameMaterial>` |
| Combination pair keys | String name pairs (order-dependent) | `CombinedMaterialId` (order-independent) |
| `WELL_KNOWN_MATERIAL_SEEDS` (deprecated) | `&[(&str, u64)]` | Eventually removed; replaced by `WellKnownMaterial::all()` |

---

## Rationale

**Why 16-bit IDs instead of reusing the u64 seed?**
Seeds are generation inputs and are 64 bits. Encoding two 64-bit seeds into a
combined identity would require 128 bits or a hash. The hash loses the
constituent information (you cannot recover which two materials were combined
from a hash). 16-bit IDs are compact (two fit in 32 bits), losslessly
recoverable, and human-readable in debug logs.

**Why sorted (min, max) packing?**
Sorting eliminates order-dependent keys at the encoding layer with a single
integer compare. The caller does not need to normalize; the type is always
canonical. This directly and completely fixes review finding 1.2 with no
runtime overhead.

**Why put the smaller ID in the high 16 bits?**
Reading the hex value left-to-right presents the constituents in ascending ID
order, which matches natural sorting. `0x0001_0002` reads "material 1 combined
with material 2". This makes debug output self-documenting.

**Why the 1–255 well-known band?**
Well-known materials are authoritatively enumerated in Rust source as compile-
time constants. Runtime-registered materials (new biome discoveries, future mod-
added materials) must not occupy the same range, or a runtime assignment could
silently alias a well-known material. The band boundary at 256 is an explicit
guard rail enforced by the allocation strategy, not the type system — the
implementing story must document this boundary in code.

**Why not use enum discriminants as IDs?**
Rust enum discriminants are not guaranteed stable across recompilation or
refactor. An explicit `const fn id()` method is the stable source of truth that
survives variant reordering.

**Why `Option<MaterialId>` vs `MaterialId::NONE`?**
Both are valid. `NONE` allows the type to be used in contexts where `Option` is
inconvenient (bitfield structs, TOML config). The recommendation is to prefer
`Option<MaterialId>` in Rust APIs and reserve `MaterialId::NONE` for
serialization boundaries where `Option` cannot be expressed.

---

## Out of Scope / Future Work

**Multi-stage combination ("tier 2 compounds"):** Combining a `CombinedMaterialId`
with a `MaterialId` (or two `CombinedMaterialId`s) cannot be encoded in 32 bits
with this scheme. If multi-stage combination is needed, a separate story must
define a 64-bit or opaque hash identity for higher-order products. This design
does not block that work — it simply does not define it.

**Mod-registered base materials:** Assigning IDs from the `0x0100–0xFFFF`
expansion band to mod-authored materials is a future story. The band is
reserved; the allocation mechanism is not defined here.

---

## Open Questions for Owner

1. **Multi-stage combination:** Can a combined material be combined with another
   material (base or combined) to produce a "tier 2" compound? If yes, this
   story should extend the encoding before the combination refactor begins.

2. **Well-known band size:** Is 255 slots (IDs 1–255) for built-in materials
   acceptable, or should the boundary be wider (e.g. 1–1023)?

3. **Null sentinel vs Option:** Should APIs use `MaterialId::NONE` (0x0000) as
   the sentinel, or `Option<MaterialId>` everywhere? Either is implementable;
   this document recommends `Option<MaterialId>` for Rust APIs but defers to
   owner preference.
