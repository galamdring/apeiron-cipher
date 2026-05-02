//! Shared seed-mixing utilities and channel constants for deterministic generation.
//!
//! Every procedurally generated value in Apeiron Cipher is derived by mixing a
//! seed with a channel constant. This module provides the core mixing function,
//! helper conversions, and the authoritative registry of all channel constants
//! across the codebase.
//!
//! ## Channel constant rules
//!
//! - Each constant occupies a unique 64-bit value.
//! - Channel families use distinct prefix spaces to avoid collisions:
//!   - `0x57A2_0001` — star generation (solar system)
//!   - `0x02B1_0001` — orbital layout
//!   - `0xD3E5_17A1` — world generation (placement, biome, surface)
//!   - `0xE1EF_0001` — elevation
//!   - `0xA7E1_0001` — material properties
//!   - `0xE1E7_0001` — planet environment
//! - Adding a new channel? Add it to the `SeedChannel` enum, not as a separate constant.
//! - The compiler enforces unique discriminants automatically.

// ── Core Mixing Functions ────────────────────────────────────────────────

/// Deterministically mix a base seed and a channel into a new 64-bit value.
///
/// SplitMix64-style bit mixer — cheap, deterministic, no external crate.
/// Avalanches nearby integer inputs into well-mixed outputs so that later
/// generation systems do not accidentally treat "similar number" as "similar
/// world feature."
pub fn mix_seed(base: u64, channel: u64) -> u64 {
    let mut z = base.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Seed channel constants for deterministic generation.
///
/// Each variant has a unique discriminant value that serves as the channel
/// constant for `mix_seed`. The compiler enforces uniqueness automatically,
/// eliminating the need for manual collision checking.
///
/// ## Usage
///
/// ```rust
/// use apeiron_cipher::seed_util::{mix_seed, SeedChannel};
/// let base_seed: u64 = 42;
/// let mixed = mix_seed(base_seed, SeedChannel::StarType as u64);
/// ```
///
/// ## Adding new channels
///
/// Add a new variant with an explicit discriminant in the appropriate family
/// prefix space. The discriminant values follow the same prefix organization
/// as the old constants to maintain deterministic compatibility.
#[repr(u64)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum SeedChannel {
    // ── Star Generation Channels (prefix 0x57A2_0001) ───────────────────────
    /// Channel for selecting the star type via weighted random.
    StarType = 0x57A2_0001_0000_0001,
    /// Channel for interpolating luminosity within the selected type's range.
    StarLuminosity = 0x57A2_0001_0000_0002,
    /// Channel for interpolating surface temperature within the selected type's range.
    StarTemperature = 0x57A2_0001_0000_0003,
    /// Channel for interpolating stellar mass within the selected type's range.
    StarMass = 0x57A2_0001_0000_0004,

    // ── Orbital Layout Channels (prefix 0x02B1_0001) ────────────────────────
    /// Channel for deriving planet count from a system seed.
    PlanetCount = 0x02B1_0001_0000_0001,
    /// Channel for seeding the orbital distance RNG.
    OrbitalLayout = 0x02B1_0001_0000_0002,

    // ── World Generation Channels (prefix 0xD3E5_17A1) ─────────────────────
    /// Channel for deriving placement density seed from planet seed.
    PlacementDensity = 0xD3E5_17A1_0000_0001,
    /// Channel for deriving placement variation seed from planet seed.
    PlacementVariation = 0xD3E5_17A1_0000_0002,
    /// Channel for deriving object identity seed from planet seed.
    ObjectIdentity = 0xD3E5_17A1_0000_0003,
    /// Channel for deriving the planet surface radius from the planet seed.
    ///
    /// The planet surface radius (measured in chunks) determines how large the
    /// planet is. It is derived deterministically from the planet seed so that
    /// each planet has a consistent, reproducible size.
    PlanetSurfaceRadius = 0xD3E5_17A1_0000_0004,
    /// Channel for deriving the biome climate seed from the planet seed.
    ///
    /// The biome climate seed is mixed with sub-channel constants (temperature and
    /// moisture) to produce two independent coherent noise fields that together
    /// determine the biome at each chunk position.
    BiomeClimate = 0xD3E5_17A1_0000_0005,

    // ── Elevation Channels (prefix 0xE1EF_0001) ─────────────────────────────
    /// Channel for deriving the elevation seed from the planet seed.
    ///
    /// The elevation seed drives multi-octave value noise that produces terrain
    /// height variation across the planet surface.
    Elevation = 0xE1EF_0001_0000_0001,
    /// Sub-channel for chunk-level detail noise layered on top of the base
    /// elevation field. Derived from the elevation seed (not the planet seed)
    /// so it is guaranteed independent of the base octaves.
    ElevationDetail = 0xE1EF_0001_0000_0002,

    // ── Material Property Channels (prefix 0xA7E1_0001) ────────────────────
    /// Channel for deriving material density from a seed.
    MaterialDensity = 0xA7E1_0001_0000_0001,
    /// Channel for deriving material thermal resistance from a seed.
    MaterialThermalResistance = 0xA7E1_0001_0000_0002,
    /// Channel for deriving material reactivity from a seed.
    MaterialReactivity = 0xA7E1_0001_0000_0003,
    /// Channel for deriving material conductivity from a seed.
    MaterialConductivity = 0xA7E1_0001_0000_0004,
    /// Channel for deriving material toxicity from a seed.
    MaterialToxicity = 0xA7E1_0001_0000_0005,
    /// Channel for deriving the red component of material color from a seed.
    MaterialColorR = 0xA7E1_0001_0000_0006,
    /// Channel for deriving the green component of material color from a seed.
    MaterialColorG = 0xA7E1_0001_0000_0007,
    /// Channel for deriving the blue component of material color from a seed.
    MaterialColorB = 0xA7E1_0001_0000_0008,

    // ── Planet Environment Channels (prefix 0xE1E7_0001) ───────────────────
    /// Channel for deriving planet surface temperature variation from planet seed.
    PlanetTempVariation = 0xE1E7_0001_0000_0001,
    /// Channel for deriving planet atmosphere density variation from planet seed.
    PlanetAtmosphere = 0xE1E7_0001_0000_0002,
    /// Channel for deriving planet surface gravity from planet seed.
    PlanetGravity = 0xE1E7_0001_0000_0003,
}

impl SeedChannel {
    /// Mix a base seed with this channel to produce a deterministic derived seed.
    ///
    /// This is a convenience method that calls `mix_seed(base, self as u64)`.
    /// It provides a more ergonomic API while maintaining the same deterministic
    /// behavior as the original constant-based approach.
    pub fn mix_seed(self, base: u64) -> u64 {
        mix_seed(base, self as u64)
    }
}

/// Convert a mixed `u64` into a `f32` in `[0.0, 1.0)`.
///
/// Takes the lower 32 bits and divides by `2^32`. This gives ~7 decimal
/// digits of granularity — more than enough for interpolating physical
/// parameters that will be displayed to the player as rounded values.
pub fn seed_to_unit_f32(mixed: u64) -> f32 {
    (mixed as u32) as f32 / (u32::MAX as f32 + 1.0)
}

/// Convert an `f32` to a `u64` suitable for seed mixing.
///
/// Uses `f32::to_bits` to get the IEEE-754 bit pattern as a `u32`, then
/// zero-extends to `u64`. This is deterministic and platform-independent for
/// non-NaN values — the same float always produces the same bits.
///
/// Used by orbital layout generation to derive position-based planet seeds:
/// each planet's seed depends on its orbital distance rather than its index,
/// so inserting a planet between two existing ones won't change their seeds.
pub fn f32_to_u64_bits(value: f32) -> u64 {
    value.to_bits() as u64
}

/// Linearly interpolate between `min` and `max` using a `[0, 1)` fraction.
///
/// Returns exactly `min` when `t == 0.0` and approaches `max` as `t → 1.0`.
/// Does not clamp — callers are responsible for providing `t` in range.
///
/// Note: `carry_feedback.rs` has its own `lerp` that clamps `t` to `[0.0, 1.0]`.
/// That is intentionally separate — different contract.
pub fn lerp(min: f32, max: f32, t: f32) -> f32 {
    min + (max - min) * t
}

/// Return the next representable `f32` above `value`.
///
/// Used in separation enforcement to guarantee that the gap between
/// consecutive orbits is never fractionally below `min_separation_au` due to
/// floating-point addition rounding down.
///
/// For positive, finite values this bumps the IEEE 754 significand by one ULP.
/// Special cases (infinity, NaN) pass through unchanged.
///
/// TODO: Replace with `f32::next_up()` when stabilized in std
/// (tracking issue: <https://github.com/rust-lang/rust/issues/91399>).
pub fn f32_next_up(value: f32) -> f32 {
    debug_assert!(
        value.is_finite() && value >= 0.0,
        "f32_next_up expects a finite non-negative value, got {value}"
    );
    if value.is_nan() || value == f32::INFINITY {
        return value;
    }
    if value == f32::NEG_INFINITY {
        return f32::MIN;
    }
    let bits = value.to_bits();
    let next_bits = if value >= 0.0 { bits + 1 } else { bits - 1 };
    f32::from_bits(next_bits)
}

// ── Backward Compatibility Constants ────────────────────────────────────
//
// These constants maintain API compatibility with existing code that uses
// the old constant-based approach. They map directly to the enum discriminants
// to ensure identical deterministic behavior.

/// Channel for selecting the star type via weighted random.
pub const STAR_TYPE_CHANNEL: u64 = SeedChannel::StarType as u64;
/// Channel for interpolating luminosity within the selected type's range.
pub const STAR_LUMINOSITY_CHANNEL: u64 = SeedChannel::StarLuminosity as u64;
/// Channel for interpolating surface temperature within the selected type's range.
pub const STAR_TEMPERATURE_CHANNEL: u64 = SeedChannel::StarTemperature as u64;
/// Channel for interpolating stellar mass within the selected type's range.
pub const STAR_MASS_CHANNEL: u64 = SeedChannel::StarMass as u64;

/// Channel for deriving planet count from a system seed.
pub const PLANET_COUNT_CHANNEL: u64 = SeedChannel::PlanetCount as u64;
/// Channel for seeding the orbital distance RNG.
pub const ORBITAL_LAYOUT_CHANNEL: u64 = SeedChannel::OrbitalLayout as u64;

/// Channel for deriving placement density seed from planet seed.
pub const PLACEMENT_DENSITY_CHANNEL: u64 = SeedChannel::PlacementDensity as u64;
/// Channel for deriving placement variation seed from planet seed.
pub const PLACEMENT_VARIATION_CHANNEL: u64 = SeedChannel::PlacementVariation as u64;
/// Channel for deriving object identity seed from planet seed.
pub const OBJECT_IDENTITY_CHANNEL: u64 = SeedChannel::ObjectIdentity as u64;
/// Channel for deriving the planet surface radius from the planet seed.
pub const PLANET_SURFACE_RADIUS_CHANNEL: u64 = SeedChannel::PlanetSurfaceRadius as u64;
/// Channel for deriving the biome climate seed from the planet seed.
pub const BIOME_CLIMATE_CHANNEL: u64 = SeedChannel::BiomeClimate as u64;

/// Channel for deriving the elevation seed from the planet seed.
pub const ELEVATION_CHANNEL: u64 = SeedChannel::Elevation as u64;
/// Sub-channel for chunk-level detail noise layered on top of the base elevation field.
pub const ELEVATION_DETAIL_CHANNEL: u64 = SeedChannel::ElevationDetail as u64;

/// Channel for deriving material density from a seed.
pub const MAT_DENSITY_CHANNEL: u64 = SeedChannel::MaterialDensity as u64;
/// Channel for deriving material thermal resistance from a seed.
pub const MAT_THERMAL_RESISTANCE_CHANNEL: u64 = SeedChannel::MaterialThermalResistance as u64;
/// Channel for deriving material reactivity from a seed.
pub const MAT_REACTIVITY_CHANNEL: u64 = SeedChannel::MaterialReactivity as u64;
/// Channel for deriving material conductivity from a seed.
pub const MAT_CONDUCTIVITY_CHANNEL: u64 = SeedChannel::MaterialConductivity as u64;
/// Channel for deriving material toxicity from a seed.
pub const MAT_TOXICITY_CHANNEL: u64 = SeedChannel::MaterialToxicity as u64;
/// Channel for deriving the red component of material color from a seed.
pub const MAT_COLOR_R_CHANNEL: u64 = SeedChannel::MaterialColorR as u64;
/// Channel for deriving the green component of material color from a seed.
pub const MAT_COLOR_G_CHANNEL: u64 = SeedChannel::MaterialColorG as u64;
/// Channel for deriving the blue component of material color from a seed.
pub const MAT_COLOR_B_CHANNEL: u64 = SeedChannel::MaterialColorB as u64;

/// Channel for deriving planet surface temperature variation from planet seed.
pub const PLANET_TEMP_VARIATION_CHANNEL: u64 = SeedChannel::PlanetTempVariation as u64;
/// Channel for deriving planet atmosphere density variation from planet seed.
pub const PLANET_ATMOSPHERE_CHANNEL: u64 = SeedChannel::PlanetAtmosphere as u64;
/// Channel for deriving planet surface gravity from planet seed.
pub const PLANET_GRAVITY_CHANNEL: u64 = SeedChannel::PlanetGravity as u64;

#[cfg(test)]
mod tests {
    use super::*;

    /// The enum-based approach automatically ensures unique discriminants.
    /// This test verifies that the backward compatibility constants match
    /// their corresponding enum values.
    #[test]
    fn backward_compatibility_constants_match_enum() {
        assert_eq!(STAR_TYPE_CHANNEL, SeedChannel::StarType as u64);
        assert_eq!(STAR_LUMINOSITY_CHANNEL, SeedChannel::StarLuminosity as u64);
        assert_eq!(
            STAR_TEMPERATURE_CHANNEL,
            SeedChannel::StarTemperature as u64
        );
        assert_eq!(STAR_MASS_CHANNEL, SeedChannel::StarMass as u64);
        assert_eq!(PLANET_COUNT_CHANNEL, SeedChannel::PlanetCount as u64);
        assert_eq!(ORBITAL_LAYOUT_CHANNEL, SeedChannel::OrbitalLayout as u64);
        assert_eq!(
            PLACEMENT_DENSITY_CHANNEL,
            SeedChannel::PlacementDensity as u64
        );
        assert_eq!(
            PLACEMENT_VARIATION_CHANNEL,
            SeedChannel::PlacementVariation as u64
        );
        assert_eq!(OBJECT_IDENTITY_CHANNEL, SeedChannel::ObjectIdentity as u64);
        assert_eq!(
            PLANET_SURFACE_RADIUS_CHANNEL,
            SeedChannel::PlanetSurfaceRadius as u64
        );
        assert_eq!(BIOME_CLIMATE_CHANNEL, SeedChannel::BiomeClimate as u64);
        assert_eq!(ELEVATION_CHANNEL, SeedChannel::Elevation as u64);
        assert_eq!(
            ELEVATION_DETAIL_CHANNEL,
            SeedChannel::ElevationDetail as u64
        );
        assert_eq!(MAT_DENSITY_CHANNEL, SeedChannel::MaterialDensity as u64);
        assert_eq!(
            MAT_THERMAL_RESISTANCE_CHANNEL,
            SeedChannel::MaterialThermalResistance as u64
        );
        assert_eq!(
            MAT_REACTIVITY_CHANNEL,
            SeedChannel::MaterialReactivity as u64
        );
        assert_eq!(
            MAT_CONDUCTIVITY_CHANNEL,
            SeedChannel::MaterialConductivity as u64
        );
        assert_eq!(MAT_TOXICITY_CHANNEL, SeedChannel::MaterialToxicity as u64);
        assert_eq!(MAT_COLOR_R_CHANNEL, SeedChannel::MaterialColorR as u64);
        assert_eq!(MAT_COLOR_G_CHANNEL, SeedChannel::MaterialColorG as u64);
        assert_eq!(MAT_COLOR_B_CHANNEL, SeedChannel::MaterialColorB as u64);
        assert_eq!(
            PLANET_TEMP_VARIATION_CHANNEL,
            SeedChannel::PlanetTempVariation as u64
        );
        assert_eq!(
            PLANET_ATMOSPHERE_CHANNEL,
            SeedChannel::PlanetAtmosphere as u64
        );
        assert_eq!(PLANET_GRAVITY_CHANNEL, SeedChannel::PlanetGravity as u64);
    }

    #[test]
    fn seed_channel_mix_seed_method_works() {
        let base = 12345_u64;
        let channel = SeedChannel::StarType;

        // The method should produce the same result as the function
        assert_eq!(channel.mix_seed(base), mix_seed(base, channel as u64));
    }

    #[test]
    fn mix_seed_deterministic() {
        let a = mix_seed(100, 200);
        let b = mix_seed(100, 200);
        assert_eq!(a, b, "same inputs must produce same output");
    }

    #[test]
    fn mix_seed_different_channels_differ() {
        let a = mix_seed(100, 1);
        let b = mix_seed(100, 2);
        assert_ne!(a, b, "different channels must produce different outputs");
    }

    #[test]
    fn seed_to_unit_f32_in_range() {
        for i in 0..10_000_u64 {
            let val = seed_to_unit_f32(mix_seed(i, 0));
            assert!(
                (0.0..1.0).contains(&val),
                "seed_to_unit_f32 produced {val} outside [0.0, 1.0)"
            );
        }
    }

    #[test]
    fn lerp_endpoints() {
        assert!((lerp(10.0, 20.0, 0.0) - 10.0).abs() < f32::EPSILON);
        assert!((lerp(10.0, 20.0, 0.5) - 15.0).abs() < f32::EPSILON);
    }

    #[test]
    fn f32_to_u64_bits_known_values() {
        assert_eq!(f32_to_u64_bits(0.0_f32), 0x0000_0000_u64);
        assert_eq!(f32_to_u64_bits(1.0_f32), 0x3F80_0000_u64);
        assert_eq!(f32_to_u64_bits(-1.0_f32), 0xBF80_0000_u64);
    }

    #[test]
    fn f32_to_u64_bits_upper_bits_zero() {
        let negative = f32_to_u64_bits(-42.0_f32);
        assert_eq!(
            negative >> 32,
            0,
            "upper 32 bits must be zero even for negative floats"
        );
    }

    #[test]
    fn f32_next_up_increments() {
        let a = 1.0_f32;
        let b = f32_next_up(a);
        assert!(b > a, "f32_next_up must produce a strictly larger value");
        assert_eq!(
            a.to_bits() + 1,
            b.to_bits(),
            "should be exactly one ULP above"
        );
    }
}
