//! Procedural name generation for seed-derived materials.
//!
//! Both the fabricator (combining two materials into a new one) and the
//! seed-based material derivation pipeline need to turn a `u64` seed into a
//! human-readable mineral-ish name.  This module owns the shared vocabulary
//! tables and the deterministic mapping so neither module depends on the other.

/// Syllable prefixes — evocative, vaguely scientific.  16 entries so a 4-bit
/// window selects one without bias.
pub const PREFIXES: &[&str] = &[
    "Neo", "Aur", "Vex", "Cor", "Nyx", "Zel", "Pyr", "Lux", "Thal", "Kyn", "Ven", "Dra", "Sol",
    "Mor", "Cyn", "Vir",
];

/// Root syllables — connecting body of the name.  16 entries, selected via a
/// separate 4-bit window so the same prefix/suffix pair can still produce
/// distinct names.
pub const ROOTS: &[&str] = &[
    "an", "el", "or", "is", "um", "ax", "on", "ir", "et", "ul", "ar", "os", "en", "ix", "al", "ur",
];

/// Syllable suffixes — mineral / chemical flavour.  16 entries, same reasoning.
pub const SUFFIXES: &[&str] = &[
    "ite", "ium", "ite", "ane", "ene", "oid", "ate", "ide", "yne", "ase", "ose", "ine", "ile",
    "ore", "ux", "al",
];

/// Deterministically produce a mineral-style name from a seed.
///
/// The name is built by selecting one prefix, one root, and one suffix from
/// fixed vocabulary tables using different bit windows of the seed.  This
/// 3-part scheme yields 16 × 16 × 16 = 4 096 unique names (vs 256 with the
/// previous 2-part approach), substantially reducing collision probability.
///
/// The mapping is intentionally simple and stable — changing it would rename
/// every procedurally generated material across all saved worlds.
pub fn procedural_name(seed: u64) -> String {
    // Hash the seed so that small sequential values (e.g. well-known seeds
    // 1001..1010) spread across the full bit range.  Uses the same
    // splitmix64 finaliser employed elsewhere in the codebase for
    // deterministic mixing.
    let h = {
        let mut x = seed;
        x ^= x >> 30;
        x = x.wrapping_mul(0xbf58476d1ce4e5b9);
        x ^= x >> 27;
        x = x.wrapping_mul(0x94d049bb133111eb);
        x ^= x >> 31;
        x
    };
    let prefix_idx = ((h) as usize) % PREFIXES.len();
    let root_idx = ((h >> 16) as usize) % ROOTS.len();
    let suffix_idx = ((h >> 32) as usize) % SUFFIXES.len();
    format!(
        "{}{}{}",
        PREFIXES[prefix_idx], ROOTS[root_idx], SUFFIXES[suffix_idx]
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn deterministic_same_seed_same_name() {
        assert_eq!(procedural_name(42), procedural_name(42));
    }

    #[test]
    fn varies_by_seed() {
        assert_ne!(procedural_name(1000), procedural_name(999_999));
    }

    #[test]
    fn prefix_index_stays_in_bounds() {
        // Exercise edge-case seeds: 0, max, and a handful of arbitrary values.
        for seed in [0u64, 1, u64::MAX, 0xDEAD_BEEF, 0xCAFE_BABE_1234_5678] {
            let name = procedural_name(seed);
            assert!(
                !name.is_empty(),
                "name must be non-empty for seed {seed:#X}"
            );
        }
    }

    #[test]
    fn tables_have_power_of_two_length() {
        // Not strictly required, but keeps the modulo unbiased for small bit windows.
        assert!(PREFIXES.len().is_power_of_two());
        assert!(ROOTS.len().is_power_of_two());
        assert!(SUFFIXES.len().is_power_of_two());
    }

    #[test]
    fn name_contains_three_parts() {
        // Verify the name is longer than any single prefix or suffix, confirming
        // the root syllable is present.
        let name = procedural_name(0xABCD_1234_5678_9ABC);
        // Shortest possible: 2-char prefix + 2-char root + 2-char suffix = 6
        assert!(
            name.len() >= 6,
            "3-part name should be at least 6 chars, got {name:?}"
        );
    }

    #[test]
    fn expanded_namespace_reduces_collisions() {
        // With 4096 possible names, sampling 200 random-ish seeds should yield
        // very few (ideally zero) collisions.
        use std::collections::HashSet;
        let names: HashSet<String> = (0u64..200).map(|i| procedural_name(i * 7919)).collect();
        // Allow up to 5% collisions as a generous tolerance.
        assert!(
            names.len() > 190,
            "expected >190 unique names from 200 seeds, got {}",
            names.len()
        );
    }
}
