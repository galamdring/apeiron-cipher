//! Persistence layer — write-ahead log, snapshots, and save/load infrastructure.
//!
//! This module defines the canonical mutation vocabulary for the game's
//! persistence system. All durable state changes flow through the
//! [`mutation::MutationBus`] so they can be serialised, replayed, and compacted
//! into snapshots without coupling the ECS systems to a specific storage
//! backend.
//!
//! # Architecture overview
//!
//! ```text
//!  ECS systems
//!      │  emit WorldMutation variants
//!      ▼
//!  MutationBus (crossbeam-channel sender/receiver wrapped as a Bevy resource)
//!      │  lock-free, unbounded, one writer per game frame
//!      ▼
//!  WAL writer  ← future story
//!      │  serialise via bincode 2.x (serde feature)
//!      ▼
//!  WAL file on disk  ← future story
//! ```
//!
//! The WAL writer and compaction logic are **not** implemented here; this story
//! only delivers the enum, the bus resource, and their serialisation tests.

pub mod decay;
pub mod error;
pub mod mutation;
pub mod schema;
pub mod wal;

use bevy::prelude::*;

/// Fired by the save loader after it calls
/// [`schema::SaveHeader::check_generation_version`] and finds that the saved
/// world-generation algorithm version differs from the currently compiled
/// [`crate::world_generation::GENERATION_VERSION`].
///
/// The event is **not** fired on an exact match — only on a version delta.
///
/// # Who emits this
///
/// The save loader (a future story) emits this message after decoding the
/// [`schema::SaveHeader`] and before applying any game state.  For a
/// `SavedNewer` result the loader also refuses the load; even so it fires this
/// event first so any subscribed systems (UI, logging) can capture the
/// diagnosis.
///
/// # Who reads this
///
/// A future HUD or journal system can subscribe to emit in-world feedback to
/// the player.  For now the `warn!` logged inside
/// [`schema::SaveHeader::check_generation_version`] is the canonical
/// player-visible output.
///
/// # Diegetic rule
///
/// Any in-game surface for this warning must comply with the diegetic feedback
/// contract: it may appear as a journal log entry, datapad readout, or console
/// output, but NOT as a raw HUD popup or tooltip.  The player reads the world;
/// the world does not narrate itself.
#[derive(Clone, Debug, Message)]
pub struct OnSaveGenerationMismatchEvent {
    /// The generation version stored in the save file.
    pub saved: u32,
    /// The generation version compiled into this binary.
    pub current: u32,
    /// `true` if the save was created with a *newer* binary and the load must
    /// be refused.  `false` if the save is older and the load continues with a
    /// warning.
    pub is_newer: bool,
}

/// Bevy plugin that registers the persistence resources.
///
/// Add this to your `App` before any systems that need to emit mutations:
///
/// ```rust,ignore
/// app.add_plugins(PersistencePlugin);
/// ```
///
/// Future stories will attach WAL-writer systems here.
pub struct PersistencePlugin;

impl Plugin for PersistencePlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<mutation::MutationBus>()
            // Register the message type so future systems can subscribe.  The
            // save loader (a future story) will write to this channel after
            // deserialising a header with a mismatched generation_version.
            .add_message::<OnSaveGenerationMismatchEvent>();
    }
}
