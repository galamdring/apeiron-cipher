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
//!
//! # Generation-version mismatch detection
//!
//! After deserialising a save file, call [`SaveHeader::check_generation_version`]
//! to compare the saved algorithm version against the currently compiled
//! [`crate::world_generation::GENERATION_VERSION`].  The method returns a
//! [`GenerationVersionCheck`] that tells the loader exactly what to do:
//!
//! | Variant | Meaning | Loader action |
//! |---|---|---|
//! | [`GenerationVersionCheck::Match`] | Versions are identical. | Proceed normally. |
//! | [`GenerationVersionCheck::SavedOlder`] | Save was written with an older generator. Unvisited chunks may look different if regenerated. | Log a `warn!`, surface a player-visible message, allow the player to cancel before any state is applied. |
//! | [`GenerationVersionCheck::SavedNewer`] | Save was written with a *newer* generator than the currently compiled binary. Regenerating chunks would silently corrupt the world. | Refuse to load — return an error to the caller, do not apply any state. |
//!
//! See [`SaveHeader::check_generation_version`] for the exact log messages
//! emitted and the data carried in each variant.

use bevy::log::warn;
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

/// Result of comparing a [`SaveHeader::generation_version`] against the
/// currently compiled [`crate::world_generation::GENERATION_VERSION`].
///
/// Returned by [`SaveHeader::check_generation_version`].  The caller (save
/// loader) uses this to decide whether to proceed, warn, or refuse the load.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum GenerationVersionCheck {
    /// The saved generation version matches the compiled constant exactly.
    ///
    /// The world will regenerate identically for any unvisited chunks — no
    /// player action is required.
    Match,

    /// The save file was written with an *older* world-generation algorithm
    /// (saved version < compiled version).
    ///
    /// Chunks the player has already visited are preserved verbatim.
    /// Unvisited chunks will be regenerated using the newer algorithm and
    /// may look different near visit boundaries.  The loader MUST surface a
    /// player-visible warning before applying any state.
    ///
    /// Fields:
    /// - `saved`: the `generation_version` found in the file.
    /// - `current`: the compiled [`crate::world_generation::GENERATION_VERSION`].
    SavedOlder {
        /// The generation version stored in the save file.
        saved: u32,
        /// The generation version compiled into this binary.
        current: u32,
    },

    /// The save file was written with a *newer* world-generation algorithm
    /// (saved version > compiled version).
    ///
    /// Regenerating chunks using the older (current) algorithm would
    /// silently produce different content than the player originally saw.
    /// **The loader MUST refuse to load and return an error to the caller.**
    ///
    /// Fields:
    /// - `saved`: the `generation_version` found in the file.
    /// - `current`: the compiled [`crate::world_generation::GENERATION_VERSION`].
    SavedNewer {
        /// The generation version stored in the save file.
        saved: u32,
        /// The generation version compiled into this binary.
        current: u32,
    },
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

    /// Compares the header's `generation_version` against the currently
    /// compiled [`crate::world_generation::GENERATION_VERSION`] and returns
    /// a [`GenerationVersionCheck`] describing the result.
    ///
    /// # Side effects
    ///
    /// When the result is [`GenerationVersionCheck::SavedOlder`] or
    /// [`GenerationVersionCheck::SavedNewer`] this method logs a prominent
    /// `warn!` via `bevy::log` so the mismatch is visible in the application
    /// log regardless of whether the caller also surfaces a UI message.
    ///
    /// # What the caller must do
    ///
    /// | Result | Required caller action |
    /// |---|---|
    /// | `Match` | Proceed normally. |
    /// | `SavedOlder` | Surface a player-visible warning (console, dialog, or toast) explaining that the world seed may produce different terrain for unvisited chunks.  Do NOT block loading — the player decides. |
    /// | `SavedNewer` | Refuse to load entirely.  Return an error up the call stack. Do NOT apply any game state. |
    ///
    /// # Example
    ///
    /// ```rust,ignore
    /// let header = decode_header(&bytes)?;
    /// match header.check_generation_version() {
    ///     GenerationVersionCheck::Match => {}
    ///     GenerationVersionCheck::SavedOlder { saved, current } => {
    ///         surface_warning(&format!(
    ///             "Save file uses world generation v{saved}; this binary uses v{current}. \
    ///              Unvisited terrain may look different."
    ///         ));
    ///         // Continue loading — player was warned.
    ///     }
    ///     GenerationVersionCheck::SavedNewer { saved, current } => {
    ///         return Err(LoadError::GenerationVersionTooNew { saved, current });
    ///     }
    /// }
    /// ```
    pub fn check_generation_version(&self) -> GenerationVersionCheck {
        let current = crate::world_generation::GENERATION_VERSION;
        let saved = self.generation_version;

        match saved.cmp(&current) {
            std::cmp::Ordering::Equal => GenerationVersionCheck::Match,
            std::cmp::Ordering::Less => {
                warn!(
                    "Save generation version {} differs from current {}; \
                     world may regenerate differently for unvisited chunks",
                    saved, current
                );
                GenerationVersionCheck::SavedOlder { saved, current }
            }
            std::cmp::Ordering::Greater => {
                warn!(
                    "Save generation version {} differs from current {}; \
                     save was created with a newer binary — refusing to load \
                     to prevent silent world corruption",
                    saved, current
                );
                GenerationVersionCheck::SavedNewer { saved, current }
            }
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

    // ── generation-version mismatch detection ─────────────────────────────────

    /// A header whose `generation_version` matches the compiled constant must
    /// return [`GenerationVersionCheck::Match`].
    #[test]
    fn check_generation_version_exact_match() {
        let header = SaveHeader::new(99);
        assert_eq!(
            header.check_generation_version(),
            GenerationVersionCheck::Match,
            "A fresh header (generation_version == GENERATION_VERSION) must be Match"
        );
    }

    /// A header with `generation_version` *lower* than the compiled constant
    /// must return [`GenerationVersionCheck::SavedOlder`] with the correct
    /// saved/current fields.
    #[test]
    fn check_generation_version_saved_older() {
        let current = crate::world_generation::GENERATION_VERSION;
        // Only run this test if bumping would actually produce a lower version.
        // GENERATION_VERSION is 1 in the shipped codebase; this test uses a
        // header with generation_version = 0 to exercise the SavedOlder arm.
        let header = SaveHeader {
            schema_version: SAVE_SCHEMA_VERSION,
            generation_version: 0,
            world_seed: 1,
        };
        assert_eq!(
            header.check_generation_version(),
            GenerationVersionCheck::SavedOlder { saved: 0, current },
            "A header with generation_version < GENERATION_VERSION must be SavedOlder"
        );
    }

    /// A header with `generation_version` *higher* than the compiled constant
    /// must return [`GenerationVersionCheck::SavedNewer`] with the correct
    /// saved/current fields.
    #[test]
    fn check_generation_version_saved_newer() {
        let current = crate::world_generation::GENERATION_VERSION;
        let future_version = current + 1;
        let header = SaveHeader {
            schema_version: SAVE_SCHEMA_VERSION,
            generation_version: future_version,
            world_seed: 1,
        };
        assert_eq!(
            header.check_generation_version(),
            GenerationVersionCheck::SavedNewer {
                saved: future_version,
                current
            },
            "A header with generation_version > GENERATION_VERSION must be SavedNewer"
        );
    }

    /// Verify that [`GenerationVersionCheck`] carries the correct `saved` and
    /// `current` payload for a multi-version gap — not just a ± 1 delta.
    #[test]
    fn check_generation_version_multi_version_gap() {
        let current = crate::world_generation::GENERATION_VERSION;
        // Simulate a save that is many versions behind.
        let old_version = current.saturating_sub(5);
        let header = SaveHeader {
            schema_version: SAVE_SCHEMA_VERSION,
            generation_version: old_version,
            world_seed: 42,
        };
        match header.check_generation_version() {
            GenerationVersionCheck::SavedOlder { saved, current: c } => {
                assert_eq!(saved, old_version, "saved must equal the header value");
                assert_eq!(c, current, "current must equal GENERATION_VERSION");
            }
            other => panic!("expected SavedOlder, got {other:?}"),
        }
    }
}
