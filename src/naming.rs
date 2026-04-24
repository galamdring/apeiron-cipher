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

/// Syllable suffixes — mineral / chemical flavour.  16 entries, same reasoning.
pub const SUFFIXES: &[&str] = &[
    "ite", "ium", "ite", "ane", "ene", "oid", "ate", "ide", "yne", "ase", "ose", "ine", "ile",
    "ore", "ux", "al",
];

/// Deterministically produce a mineral-style name from a seed.
///
/// The name is built by selecting one prefix and one suffix from fixed
/// vocabulary tables using different bit windows of the seed.  The mapping is
/// intentionally simple and stable — changing it would rename every
/// procedurally generated material across all saved worlds.
pub fn procedural_name(seed: u64) -> String {
    let prefix_idx = ((seed >> 8) as usize) % PREFIXES.len();
    let suffix_idx = ((seed >> 16) as usize) % SUFFIXES.len();
    format!("{}{}", PREFIXES[prefix_idx], SUFFIXES[suffix_idx])
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
        assert!(SUFFIXES.len().is_power_of_two());
    }
}
