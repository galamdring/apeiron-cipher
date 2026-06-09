//! Save-file schema types — the top-level envelope that wraps every save.
//!
//! # Save file format
//!
//! Every save file begins with a [`SaveHeader`] that allows the loader to
//! determine compatibility before it attempts to deserialise any game-state
//! payload.  The full format is:
//!
//! ```text
//! ┌─────────────────────────────────────────────────────┐
//! │  SaveHeader                                         │
//! │    schema_version:       u32  — save-format version │
//! │    generation_version:   u32  — world-gen algorithm │
//! │    world_seed:           u64  — planetary seed      │
//! └─────────────────────────────────────────────────────┘
//! ┌─────────────────────────────────────────────────────┐
//! │  Payload (sequence of WorldMutation records)        │
//! │  ... (defined in mutation.rs, encoded via bincode)  │
//! └─────────────────────────────────────────────────────┘
//! ```
//!
//! The payload section is defined in [`super::mutation`] and is not part of
//! this module.  WAL writer / loader stories will assemble the two sections.
//!
//! # Version semantics
//!
//! | Field | What it tracks | Bump when |
//! |---|---|---|
//! | `schema_version` | The binary layout of the save file itself. | The header fields, payload encoding, or file layout change in a backward-incompatible way. |
//! | `generation_version` | The deterministic world-generation algorithm. | Any change to noise parameters, biome rules, or derivation order that would produce a different world from the same seed. |
//! | `world_seed` | Which world this save belongs to. | N/A — fixed per save. |
//!
//! A loader that reads `generation_version` **greater than** the currently
//! compiled [`crate::world_generation::GENERATION_VERSION`] must refuse to
//! load (the save was created with a newer generator; regenerated chunks would
//! mismatch).  A save with a **lower** `generation_version` can be loaded but
//! must mark any un-revisited chunks as stale so they are re-generated on next
//! visit.

use serde::{Deserialize, Serialize};

/// Current schema version for the binary save format.
///
/// Increment this any time the binary layout of [`SaveHeader`] or the payload
/// encoding changes in a backward-incompatible way.  It is intentionally
/// separate from [`crate::world_generation::GENERATION_VERSION`] — a save-
/// format change does not imply the world-gen algorithm changed, and vice
/// versa.
pub const SAVE_SCHEMA_VERSION: u32 = 1;

/// Top-level envelope prepended to every save file.
///
/// The loader reads this struct first to decide whether the file is compatible
/// before it attempts to deserialise the (potentially large) payload.
///
/// # Field stability
///
/// Fields MUST NOT be reordered or removed once the format is shipped —
/// `bincode` does not store field names, so positional order is part of the
/// contract.  Add new optional fields at the end only, and bump
/// [`SAVE_SCHEMA_VERSION`] when you do.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub struct SaveHeader {
    /// Version of the save-file binary format.
    ///
    /// The loader rejects files where `schema_version` is higher than
    /// [`SAVE_SCHEMA_VERSION`] (file written by a newer binary) and may
    /// apply migration logic when `schema_version` is lower.
    pub schema_version: u32,

    /// Version of the world-generation algorithm when this save was created.
    ///
    /// Mirrors [`crate::world_generation::GENERATION_VERSION`] at save time.
    /// The loader uses this to detect stale chunks: if the saved value is
    /// lower than the currently compiled generation version, chunks that have
    /// not been revisited by the player must be regenerated on next visit.
    pub generation_version: u32,

    /// Planetary seed used to procedurally generate this world.
    ///
    /// Stored at the serialisation edge as a bare `u64` — the one legitimate
    /// use of a raw seed value outside the domain newtype boundary, per the
    /// architecture rules.  The loader converts this back to
    /// [`crate::world_generation::PlanetSeed`] before passing it to the
    /// world-generation subsystem.
    pub world_seed: u64,
}

impl SaveHeader {
    /// Constructs a new header for a fresh save.
    ///
    /// `world_seed` is the raw `u64` extracted from a
    /// [`crate::world_generation::PlanetSeed`] newtype.
    pub fn new(world_seed: u64) -> Self {
        Self {
            schema_version: SAVE_SCHEMA_VERSION,
            generation_version: crate::world_generation::GENERATION_VERSION,
            world_seed,
        }
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    /// The header must round-trip through bincode without data loss.
    ///
    /// We use `bincode::serde` helpers here (same helpers the WAL will use)
    /// so that the test exercises the real serialisation path, not just Rust's
    /// `Clone`/`Copy`.
    #[test]
    fn save_header_bincode_roundtrip() {
        let original = SaveHeader::new(0xDEAD_BEEF_1234_5678);
        let config = bincode::config::standard();
        let bytes = bincode::serde::encode_to_vec(&original, config)
            .expect("SaveHeader encode should succeed");
        let (decoded, _): (SaveHeader, usize) = bincode::serde::decode_from_slice(&bytes, config)
            .expect("SaveHeader decode should succeed");
        assert_eq!(
            original, decoded,
            "SaveHeader must round-trip through bincode without data loss"
        );
    }

    /// `generation_version` in a freshly constructed header must match the
    /// compile-time constant — a mismatch would mean saves become incompatible
    /// the moment they are written.
    #[test]
    fn save_header_generation_version_matches_constant() {
        let header = SaveHeader::new(42);
        assert_eq!(
            header.generation_version,
            crate::world_generation::GENERATION_VERSION,
            "SaveHeader::generation_version must equal GENERATION_VERSION"
        );
    }

    /// The `world_seed` field must survive the serialisation edge unchanged.
    #[test]
    fn save_header_world_seed_preserved() {
        let seed = 0xCAFE_BABE_0000_0001_u64;
        let header = SaveHeader::new(seed);
        assert_eq!(
            header.world_seed, seed,
            "world_seed must be stored verbatim in SaveHeader"
        );
    }

    /// `schema_version` must equal [`SAVE_SCHEMA_VERSION`] in a freshly
    /// constructed header.
    #[test]
    fn save_header_schema_version_matches_constant() {
        let header = SaveHeader::new(1);
        assert_eq!(
            header.schema_version, SAVE_SCHEMA_VERSION,
            "SaveHeader::schema_version must equal SAVE_SCHEMA_VERSION"
        );
    }
}
