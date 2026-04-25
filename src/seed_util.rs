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
//! - Adding a new channel? Add it here, not in the consuming module.
//! - The uniqueness test at the bottom of this file catches collisions at compile time.

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

// ── Star Generation Channels (prefix 0x57A2_0001) ───────────────────────

/// Channel for selecting the star type via weighted random.
pub const STAR_TYPE_CHANNEL: u64 = 0x57A2_0001_0000_0001;
/// Channel for interpolating luminosity within the selected type's range.
pub const STAR_LUMINOSITY_CHANNEL: u64 = 0x57A2_0001_0000_0002;
/// Channel for interpolating surface temperature within the selected type's range.
pub const STAR_TEMPERATURE_CHANNEL: u64 = 0x57A2_0001_0000_0003;
/// Channel for interpolating stellar mass within the selected type's range.
pub const STAR_MASS_CHANNEL: u64 = 0x57A2_0001_0000_0004;

// ── Orbital Layout Channels (prefix 0x02B1_0001) ────────────────────────

/// Channel for deriving planet count from a system seed.
pub const PLANET_COUNT_CHANNEL: u64 = 0x02B1_0001_0000_0001;
/// Channel for seeding the orbital distance RNG.
pub const ORBITAL_LAYOUT_CHANNEL: u64 = 0x02B1_0001_0000_0002;

// ── World Generation Channels (prefix 0xD3E5_17A1) ─────────────────────

/// Channel for deriving placement density seed from planet seed.
pub const PLACEMENT_DENSITY_CHANNEL: u64 = 0xD3E5_17A1_0000_0001;
/// Channel for deriving placement variation seed from planet seed.
pub const PLACEMENT_VARIATION_CHANNEL: u64 = 0xD3E5_17A1_0000_0002;
/// Channel for deriving object identity seed from planet seed.
pub const OBJECT_IDENTITY_CHANNEL: u64 = 0xD3E5_17A1_0000_0003;
/// Channel for deriving the planet surface radius from the planet seed.
///
/// The planet surface radius (measured in chunks) determines how large the
/// planet is. It is derived deterministically from the planet seed so that
/// each planet has a consistent, reproducible size.
pub const PLANET_SURFACE_RADIUS_CHANNEL: u64 = 0xD3E5_17A1_0000_0004;
/// Channel for deriving the biome climate seed from the planet seed.
///
/// The biome climate seed is mixed with sub-channel constants (temperature and
/// moisture) to produce two independent coherent noise fields that together
/// determine the biome at each chunk position.
pub const BIOME_CLIMATE_CHANNEL: u64 = 0xD3E5_17A1_0000_0005;

// ── Elevation Channels (prefix 0xE1EF_0001) ─────────────────────────────

/// Channel for deriving the elevation seed from the planet seed.
///
/// The elevation seed drives multi-octave value noise that produces terrain
/// height variation across the planet surface.
pub const ELEVATION_CHANNEL: u64 = 0xE1EF_0001_0000_0001;
/// Sub-channel for chunk-level detail noise layered on top of the base
/// elevation field. Derived from the elevation seed (not the planet seed)
/// so it is guaranteed independent of the base octaves.
pub const ELEVATION_DETAIL_CHANNEL: u64 = 0xE1EF_0001_0000_0002;

// ── Material Property Channels (prefix 0xA7E1_0001) ────────────────────

/// Channel for deriving material density from a seed.
pub const MAT_DENSITY_CHANNEL: u64 = 0xA7E1_0001_0000_0001;
/// Channel for deriving material thermal resistance from a seed.
pub const MAT_THERMAL_RESISTANCE_CHANNEL: u64 = 0xA7E1_0001_0000_0002;
/// Channel for deriving material reactivity from a seed.
pub const MAT_REACTIVITY_CHANNEL: u64 = 0xA7E1_0001_0000_0003;
/// Channel for deriving material conductivity from a seed.
pub const MAT_CONDUCTIVITY_CHANNEL: u64 = 0xA7E1_0001_0000_0004;
/// Channel for deriving material toxicity from a seed.
pub const MAT_TOXICITY_CHANNEL: u64 = 0xA7E1_0001_0000_0005;
/// Channel for deriving the red component of material color from a seed.
pub const MAT_COLOR_R_CHANNEL: u64 = 0xA7E1_0001_0000_0006;
/// Channel for deriving the green component of material color from a seed.
pub const MAT_COLOR_G_CHANNEL: u64 = 0xA7E1_0001_0000_0007;
/// Channel for deriving the blue component of material color from a seed.
pub const MAT_COLOR_B_CHANNEL: u64 = 0xA7E1_0001_0000_0008;

// ── Planet Environment Channels (prefix 0xE1E7_0001) ───────────────────

/// Channel for deriving planet surface temperature variation from planet seed.
pub const PLANET_TEMP_VARIATION_CHANNEL: u64 = 0xE1E7_0001_0000_0001;
/// Channel for deriving planet atmosphere density variation from planet seed.
pub const PLANET_ATMOSPHERE_CHANNEL: u64 = 0xE1E7_0001_0000_0002;
/// Channel for deriving planet surface gravity from planet seed.
pub const PLANET_GRAVITY_CHANNEL: u64 = 0xE1E7_0001_0000_0003;

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// Every channel constant in the codebase must be unique. If this test
    /// fails, two channels have the same value and seed derivation will
    /// produce identical outputs for different parameters.
    #[test]
    fn all_channel_constants_are_unique() {
        let channels: &[(&str, u64)] = &[
            // Star generation
            ("STAR_TYPE_CHANNEL", STAR_TYPE_CHANNEL),
            ("STAR_LUMINOSITY_CHANNEL", STAR_LUMINOSITY_CHANNEL),
            ("STAR_TEMPERATURE_CHANNEL", STAR_TEMPERATURE_CHANNEL),
            ("STAR_MASS_CHANNEL", STAR_MASS_CHANNEL),
            // Orbital layout
            ("PLANET_COUNT_CHANNEL", PLANET_COUNT_CHANNEL),
            ("ORBITAL_LAYOUT_CHANNEL", ORBITAL_LAYOUT_CHANNEL),
            // World generation
            ("PLACEMENT_DENSITY_CHANNEL", PLACEMENT_DENSITY_CHANNEL),
            ("PLACEMENT_VARIATION_CHANNEL", PLACEMENT_VARIATION_CHANNEL),
            ("OBJECT_IDENTITY_CHANNEL", OBJECT_IDENTITY_CHANNEL),
            (
                "PLANET_SURFACE_RADIUS_CHANNEL",
                PLANET_SURFACE_RADIUS_CHANNEL,
            ),
            ("BIOME_CLIMATE_CHANNEL", BIOME_CLIMATE_CHANNEL),
            // Elevation
            ("ELEVATION_CHANNEL", ELEVATION_CHANNEL),
            ("ELEVATION_DETAIL_CHANNEL", ELEVATION_DETAIL_CHANNEL),
            // Material properties
            ("MAT_DENSITY_CHANNEL", MAT_DENSITY_CHANNEL),
            (
                "MAT_THERMAL_RESISTANCE_CHANNEL",
                MAT_THERMAL_RESISTANCE_CHANNEL,
            ),
            ("MAT_REACTIVITY_CHANNEL", MAT_REACTIVITY_CHANNEL),
            ("MAT_CONDUCTIVITY_CHANNEL", MAT_CONDUCTIVITY_CHANNEL),
            ("MAT_TOXICITY_CHANNEL", MAT_TOXICITY_CHANNEL),
            ("MAT_COLOR_R_CHANNEL", MAT_COLOR_R_CHANNEL),
            ("MAT_COLOR_G_CHANNEL", MAT_COLOR_G_CHANNEL),
            ("MAT_COLOR_B_CHANNEL", MAT_COLOR_B_CHANNEL),
            // Planet environment
            (
                "PLANET_TEMP_VARIATION_CHANNEL",
                PLANET_TEMP_VARIATION_CHANNEL,
            ),
            ("PLANET_ATMOSPHERE_CHANNEL", PLANET_ATMOSPHERE_CHANNEL),
            ("PLANET_GRAVITY_CHANNEL", PLANET_GRAVITY_CHANNEL),
        ];

        for (i, (name_a, val_a)) in channels.iter().enumerate() {
            for (name_b, val_b) in channels.iter().skip(i + 1) {
                assert_ne!(
                    val_a, val_b,
                    "channel collision: {name_a} ({val_a:#018X}) == {name_b} ({val_b:#018X})"
                );
            }
        }
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
