//! Knowledge graph types — concept nodes and categories for the cross-reference system.
//!
//! The knowledge graph is the player's associative web of discovered concepts.
//! Each concept corresponds to a journal entry (identified by [`JournalKey`]) and
//! carries metadata about when it was discovered and how confident the player is
//! in their understanding of it.
//!
//! This module defines the node-level types. The graph resource itself
//! (`KnowledgeGraph`) and edge types (`ConceptEdge`, `RelationshipType`) are
//! defined in subsequent stories.

use std::collections::HashSet;

use serde::{Deserialize, Serialize};

use crate::journal::JournalKey;
use crate::observation::Confidence;

// ── Concept identity ─────────────────────────────────────────────────────

/// Unique concept identifier — wraps a [`JournalKey`] so every journal entry
/// has a corresponding concept node in the knowledge graph.
///
/// The one-to-one mapping between `ConceptId` and `JournalKey` means that
/// creating a concept node for a journal entry is always unambiguous: the
/// concept's identity *is* the journal key. This avoids a separate ID space
/// that could drift out of sync with the journal.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConceptId(pub JournalKey);

impl ConceptId {
    /// Create a new concept identifier from a journal key.
    pub fn new(key: JournalKey) -> Self {
        Self(key)
    }

    /// Borrow the underlying journal key.
    pub fn key(&self) -> &JournalKey {
        &self.0
    }
}

impl From<JournalKey> for ConceptId {
    fn from(key: JournalKey) -> Self {
        Self(key)
    }
}

// ── Concept category ─────────────────────────────────────────────────────

/// Encyclopedia-style grouping for concept nodes.
///
/// Categories allow the journal's encyclopedia view to group related concepts
/// together (all materials, all locations, etc.) and let the bounded BFS
/// traversal optionally filter results to a single category.
///
/// **Extensibility:** new categories (Language, Culture, Trade, Species, …)
/// are added as variants here when their underlying game systems are
/// implemented. Existing match arms do not need to change because the
/// category is used for grouping and filtering, not for exhaustive dispatch.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConceptCategory {
    /// Raw or fabricated materials the player has encountered.
    Material,
    /// Planets, biomes, and other spatial locations.
    Location,
    /// Outputs and processes from the fabrication system.
    Fabrication,
    // Future: Language, Culture, Trade, Species, etc.
}

// ── Concept node ─────────────────────────────────────────────────────────

/// A node in the knowledge graph — represents a concept the player has
/// discovered and accumulated knowledge about.
///
/// Each node corresponds to exactly one [`JournalEntry`] (via [`ConceptId`]).
/// The node carries a snapshot of the player's current understanding:
/// how confident they are overall, when they first encountered the concept,
/// and which properties they have personally revealed through observation.
///
/// `revealed_properties` is a set of string keys rather than a typed enum
/// so that new game systems can add property names without modifying this
/// struct. The strings match the property names used by the observation
/// system (e.g., `"thermal_resistance"`, `"density"`).
///
/// [`JournalEntry`]: crate::journal::JournalEntry
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptNode {
    /// The unique identifier for this concept, linking it to its journal entry.
    pub id: ConceptId,
    /// Encyclopedia category for grouping and BFS filtering.
    pub category: ConceptCategory,
    /// Overall confidence in this concept, aggregated from all observations.
    ///
    /// Starts at the confidence of the first observation and accumulates
    /// as the player gathers more evidence. Used by the `SimilarTo` edge
    /// creation logic: similarity edges are only created when both concepts
    /// are at `Observed` tier or above (confidence ≥ 0.3).
    pub confidence: Confidence,
    /// Game-time tick when the player first encountered this concept.
    pub discovered_at: u64,
    /// Set of property keys the player has personally revealed through
    /// observation (e.g., `"thermal_resistance"`, `"density"`).
    ///
    /// Only properties the player has *observed* appear here — the system
    /// never pre-populates this set from hidden material data. This enforces
    /// the acceptance criterion that no cross-reference is created without
    /// an observation event.
    pub revealed_properties: HashSet<String>,
}

impl ConceptNode {
    /// Create a new concept node with no revealed properties.
    ///
    /// The `confidence` is set to the initial observation's confidence value.
    /// `revealed_properties` starts empty and is populated as the player
    /// makes observations.
    pub fn new(
        id: ConceptId,
        category: ConceptCategory,
        confidence: Confidence,
        discovered_at: u64,
    ) -> Self {
        Self {
            id,
            category,
            confidence,
            discovered_at,
            revealed_properties: HashSet::new(),
        }
    }

    /// Mark a property as revealed by the player.
    ///
    /// Idempotent — calling this multiple times with the same key is safe.
    pub fn reveal_property(&mut self, property: impl Into<String>) {
        self.revealed_properties.insert(property.into());
    }

    /// Whether the player has revealed the named property.
    pub fn has_property(&self, property: &str) -> bool {
        self.revealed_properties.contains(property)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::JournalKey;

    fn material_key(seed: u64) -> JournalKey {
        JournalKey::Material {
            seed,
            planet_seed: None,
        }
    }

    #[test]
    fn concept_id_wraps_journal_key() {
        let key = material_key(42);
        let id = ConceptId::new(key.clone());
        assert_eq!(id.key(), &key);
    }

    #[test]
    fn concept_id_from_journal_key() {
        let key = material_key(7);
        let id: ConceptId = key.clone().into();
        assert_eq!(id.0, key);
    }

    #[test]
    fn concept_node_starts_with_no_revealed_properties() {
        let id = ConceptId::new(material_key(1));
        let node = ConceptNode::new(
            id,
            ConceptCategory::Material,
            Confidence(0.2),
            100,
        );
        assert!(node.revealed_properties.is_empty());
        assert!(!node.has_property("density"));
    }

    #[test]
    fn concept_node_reveal_property_is_idempotent() {
        let id = ConceptId::new(material_key(2));
        let mut node = ConceptNode::new(
            id,
            ConceptCategory::Material,
            Confidence(0.5),
            200,
        );
        node.reveal_property("density");
        node.reveal_property("density"); // second call is a no-op
        assert!(node.has_property("density"));
        assert_eq!(node.revealed_properties.len(), 1);
    }

    #[test]
    fn concept_node_multiple_properties() {
        let id = ConceptId::new(material_key(3));
        let mut node = ConceptNode::new(
            id,
            ConceptCategory::Material,
            Confidence(0.8),
            300,
        );
        node.reveal_property("density");
        node.reveal_property("thermal_resistance");
        assert!(node.has_property("density"));
        assert!(node.has_property("thermal_resistance"));
        assert!(!node.has_property("reactivity"));
    }

    #[test]
    fn concept_category_equality() {
        assert_eq!(ConceptCategory::Material, ConceptCategory::Material);
        assert_ne!(ConceptCategory::Material, ConceptCategory::Location);
        assert_ne!(ConceptCategory::Location, ConceptCategory::Fabrication);
    }

    #[test]
    fn concept_node_stores_metadata() {
        let id = ConceptId::new(material_key(99));
        let node = ConceptNode::new(
            id.clone(),
            ConceptCategory::Location,
            Confidence(0.6),
            999,
        );
        assert_eq!(node.id, id);
        assert_eq!(node.category, ConceptCategory::Location);
        assert_eq!(node.confidence.0, 0.6);
        assert_eq!(node.discovered_at, 999);
    }
}
