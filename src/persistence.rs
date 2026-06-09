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

pub mod mutation;
pub mod schema;

use bevy::prelude::*;

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
        app.init_resource::<mutation::MutationBus>();
    }
}
