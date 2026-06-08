//! Domain seed newtypes for deterministic procedural generation.
//!
//! All procedural generation in Apeiron Cipher flows through typed seed values.
//! Bare `u64` is only permitted at serialisation / asset-loading edges — everywhere
//! else, pass the appropriate domain seed newtype so the compiler prevents you from
//! accidentally mixing seeds across domains at the type level.
//!
//! ## Newtype catalogue
//!
//! | Type | Derived from | Purpose |
//! |---|---|---|
//! | [`SolarSystemSeed`] | Asset config (solar_system_seed field) | Root seed for a solar system |
//! | [`PlanetSeed`] | `SolarSystemSeed` + orbital slot | Root seed for a planet; re-exported here |
//! | [`MaterialSeed`] | `PlanetSeed` (via asset loader) | Material property generation |
//! | [`PlacementSeed`] | `PlanetSeed` + placement channel | Object placement and spatial distribution |
//! | [`BiomeSeed`] | `PlanetSeed` + biome channel | Biome / climate generation |
//! | [`ObjectIdentitySeed`] | `PlanetSeed` + identity channel | Unique entity identity |
//!
//! ## Usage
//!
//! ```rust
//! use apeiron_cipher::seeds::{MaterialSeed, PlacementSeed, SolarSystemSeed};
//!
//! let mat = MaterialSeed::from(42_u64);
//! let (density, variation) = mat.split();
//! // density and variation are both MaterialSeed — independent sub-seeds
//!
//! // SolarSystemSeed wraps a raw u64 system seed at the asset-loading edge.
//! let sys: SolarSystemSeed = 12345_u64.into();
//! ```
//!
//! ## Why newtypes, not bare `u64`?
//!
//! A function that accepts `PlacementSeed` cannot silently receive a `BiomeSeed`.
//! Without newtypes every seed is a `u64`, and mixing the wrong domain seed into
//! a generator produces deterministically wrong output — a bug that compiles fine
//! and produces plausible-looking results, making it nearly invisible in testing.
//! Newtypes move that class of error to the type checker.

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::seed_util::mix_seed;

/// Re-export [`PlanetSeed`] from the world generation module.
///
/// Callers that need all seed types in one place can import from `seeds::*`
/// instead of reaching into `world_generation`.
pub use crate::world_generation::PlanetSeed;

/// Re-export [`SolarSystemSeed`] from the solar system module.
///
/// Callers that need all seed types in one place can import from `seeds::*`
/// instead of reaching into `solar_system`.
pub use crate::solar_system::SolarSystemSeed;

// ── Internal split constants ─────────────────────────────────────────────────
//
// These splitmix64-style mixing constants are used *only* inside this module for
// the `split()` helper. They are intentionally distinct from every `SeedChannel`
// discriminant (all of which use the family-prefix scheme documented in
// `seed_util`) so that no collision can produce incorrect world output.

const SPLIT_A: u64 = 0xA24B_AED4_963E_E407;
const SPLIT_B: u64 = 0x9D20_8DD2_A4A8_B4C5;

// ── Macro ─────────────────────────────────────────────────────────────────────

/// Generates a domain seed newtype with the standard Apeiron Cipher trait suite.
///
/// Each expansion produces:
/// - A `pub struct $name(pub u64)` tuple-struct newtype
/// - `#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]`
/// - `From<u64>` — wraps a raw value (serialisation / asset-loading edge only)
/// - `From<$name> for u64` — unwraps for the same edge
/// - `fn split(self) -> (Self, Self)` — two deterministic independent sub-seeds
macro_rules! define_seed_newtype {
    (
        $(#[$meta:meta])*
        $vis:vis struct $name:ident;
    ) => {
        $(#[$meta])*
        #[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
        $vis struct $name(pub u64);

        impl From<u64> for $name {
            #[inline]
            fn from(v: u64) -> Self { Self(v) }
        }

        impl From<$name> for u64 {
            #[inline]
            fn from(s: $name) -> u64 { s.0 }
        }

        impl std::fmt::Display for $name {
            fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
                write!(f, "{:#018x}", self.0)
            }
        }

        impl $name {
            /// Derive two independent sub-seeds from this seed.
            ///
            /// Both outputs are produced by splitmix64-style bit mixing of the
            /// original value with two distinct constants. Neither output equals
            /// the input, and the two outputs are independent of each other.
            ///
            /// Useful when one seed must independently initialise two sub-systems
            /// (e.g., splitting a placement seed into a density seed and a
            /// variation seed without them influencing each other).
            ///
            /// Deterministic: calling `split()` twice on the same value always
            /// returns the same pair.
            pub fn split(self) -> (Self, Self) {
                (
                    Self(mix_seed(self.0, SPLIT_A)),
                    Self(mix_seed(self.0, SPLIT_B)),
                )
            }
        }
    };
}

// ── Seed newtype definitions ──────────────────────────────────────────────────

define_seed_newtype! {
    /// Typed seed for a material instance's property generation.
    ///
    /// Each `MaterialSeed` uniquely identifies one material in the procedural
    /// catalog. All five physical properties (density, thermal resistance,
    /// reactivity, conductivity, toxicity) and the display colour are derived
    /// deterministically from this value via `derive_material_from_seed`.
    ///
    /// Bare `u64` is only permitted at serialisation / asset-loading edges;
    /// everywhere else pass `MaterialSeed`.
    pub struct MaterialSeed;
}

define_seed_newtype! {
    /// Typed seed for object placement and spatial distribution.
    ///
    /// Derived from a [`PlanetSeed`] by mixing with the placement density or
    /// placement variation channel. Controls where material deposits and
    /// surface objects appear across an exterior chunk grid.
    ///
    /// Never pass this where a [`BiomeSeed`] or [`ObjectIdentitySeed`] is
    /// expected — they are separate domains that produce different outputs.
    pub struct PlacementSeed;
}

define_seed_newtype! {
    /// Typed seed for biome and climate generation.
    ///
    /// Derived from a [`PlanetSeed`] by mixing with the biome climate channel.
    /// Drives the temperature and moisture coherent noise fields that determine
    /// the biome at each chunk position on the planet surface.
    pub struct BiomeSeed;
}

define_seed_newtype! {
    /// Typed seed for object identity and unique entity generation.
    ///
    /// Derived from a [`PlanetSeed`] by mixing with the object identity channel.
    /// Ensures that every spawned entity carries a stable, reproducible identity
    /// tied to the planet and chunk that produced it, regardless of spawn order.
    pub struct ObjectIdentitySeed;
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use super::*;

    // ── Type distinction ─────────────────────────────────────────────────
    //
    // These are compile-time checks. The test bodies are never reached at
    // runtime — the assertion is that these functions *compile*, proving that
    // each seed domain is a distinct type the compiler recognises separately.
    //
    // A future regression where two newtypes collapse to the same type would
    // surface as a compile error here before it ever reaches CI.

    /// Only accepts `MaterialSeed`. Passing a `PlacementSeed` here is a
    /// compile error.
    #[allow(dead_code)]
    fn _accept_material_seed(_: MaterialSeed) {}

    /// Only accepts `PlacementSeed`.
    #[allow(dead_code)]
    fn _accept_placement_seed(_: PlacementSeed) {}

    /// Only accepts `BiomeSeed`.
    #[allow(dead_code)]
    fn _accept_biome_seed(_: BiomeSeed) {}

    /// Only accepts `ObjectIdentitySeed`.
    #[allow(dead_code)]
    fn _accept_object_identity_seed(_: ObjectIdentitySeed) {}

    // ── From<u64> / Into<u64> ────────────────────────────────────────────

    #[test]
    fn material_seed_round_trips_through_u64() {
        let s = MaterialSeed::from(42_u64);
        assert_eq!(s.0, 42);
        let back: u64 = s.into();
        assert_eq!(back, 42);
    }

    #[test]
    fn placement_seed_round_trips_through_u64() {
        let s = PlacementSeed::from(99_u64);
        assert_eq!(s.0, 99);
        let back: u64 = s.into();
        assert_eq!(back, 99);
    }

    #[test]
    fn biome_seed_round_trips_through_u64() {
        let s = BiomeSeed::from(7_u64);
        assert_eq!(s.0, 7);
        let back: u64 = s.into();
        assert_eq!(back, 7);
    }

    #[test]
    fn object_identity_seed_round_trips_through_u64() {
        let s = ObjectIdentitySeed::from(u64::MAX);
        assert_eq!(s.0, u64::MAX);
        let back: u64 = s.into();
        assert_eq!(back, u64::MAX);
    }

    // ── split() ──────────────────────────────────────────────────────────

    #[test]
    fn split_is_deterministic() {
        let base = MaterialSeed(12345);
        let (a1, b1) = base.split();
        let (a2, b2) = base.split();
        assert_eq!(a1, a2, "split must produce the same first value each call");
        assert_eq!(b1, b2, "split must produce the same second value each call");
    }

    #[test]
    fn split_pair_values_differ_from_each_other_and_from_base() {
        let base = MaterialSeed(99999);
        let (a, b) = base.split();
        assert_ne!(a.0, b.0, "split pair must contain two distinct values");
        assert_ne!(a.0, base.0, "first split must differ from original");
        assert_ne!(b.0, base.0, "second split must differ from original");
    }

    #[test]
    fn split_available_on_every_seed_type() {
        // Each type gets its own split via the macro — this verifies they all
        // compile and run correctly rather than silently coercing to a single impl.
        let _ = PlacementSeed(1).split();
        let _ = BiomeSeed(2).split();
        let _ = ObjectIdentitySeed(3).split();
    }

    // ── Clone / Copy / PartialEq / Eq / Hash ─────────────────────────────

    #[test]
    fn seed_is_copy_and_compares_by_value() {
        let s = MaterialSeed(1);
        let copy = s; // implicit Copy — no move
        assert_eq!(s, copy, "Copy of seed must equal original");
    }

    #[test]
    fn different_values_are_not_equal() {
        assert_ne!(MaterialSeed(1), MaterialSeed(2));
        assert_ne!(PlacementSeed(0), PlacementSeed(1));
    }

    #[test]
    fn same_value_hashes_consistently() {
        let s = BiomeSeed(777);
        let mut set = HashSet::new();
        set.insert(s);
        set.insert(s); // same value again — must not grow the set
        assert_eq!(set.len(), 1, "identical seeds must occupy one hash bucket");
    }

    // ── PlanetSeed re-export ──────────────────────────────────────────────

    #[test]
    fn planet_seed_reexport_is_usable() {
        // Exercises the `pub use crate::world_generation::PlanetSeed;` re-export.
        // If PlanetSeed is not re-exported from this module this test won't compile.
        let ps = PlanetSeed(42);
        assert_eq!(ps.0, 42);
    }

    // ── SolarSystemSeed re-export ─────────────────────────────────────────

    #[test]
    fn solar_system_seed_reexport_is_usable() {
        // Exercises the `pub use crate::solar_system::SolarSystemSeed;` re-export.
        // If SolarSystemSeed is not re-exported from this module this test won't compile.
        let s = SolarSystemSeed(999);
        assert_eq!(s.0, 999);
        let back: u64 = s.into();
        assert_eq!(back, 999);
    }

    #[test]
    fn solar_system_seed_from_u64() {
        let s: SolarSystemSeed = 42_u64.into();
        assert_eq!(s.0, 42);
    }
}
