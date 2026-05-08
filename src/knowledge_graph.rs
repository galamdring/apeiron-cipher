//! Knowledge graph types — concept nodes and categories for the cross-reference system.
//!
//! The knowledge graph is the player's associative web of discovered concepts.
//! Each concept corresponds to a journal entry (identified by [`JournalKey`]) and
//! carries metadata about when it was discovered and how confident the player is
//! in their understanding of it.
//!
//! This module defines the node-level types and edge types. The graph resource
//! itself (`KnowledgeGraph`) is defined in a subsequent story.

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

// ── Relationship type ─────────────────────────────────────────────────────

/// The typed relationship between two concept nodes in the knowledge graph.
///
/// Each variant describes *how* one concept relates to another. Relationships
/// are directional: the edge goes from a source concept to a target concept,
/// and the variant names are written from the source's perspective
/// (e.g., `FoundOn` means "source was found on target").
///
/// **Extensibility:** new relationship types (SpokenBy, TradedAt, UsedIn, …)
/// are added as variants here when their underlying game systems are
/// implemented. The `ConceptEdge` struct is unchanged by new variants.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationshipType {
    /// Source material was found on the target location.
    ///
    /// Created automatically when a material observation is recorded with a
    /// planet/location context.
    FoundOn,
    /// Source material was combined with the target material in the fabricator.
    ///
    /// Created when a fabrication event records both input materials.
    CombinedWith,
    /// Source material was derived from the target input material.
    ///
    /// Created for fabrication outputs: the output concept links back to each
    /// input material via `DerivedFrom`.
    DerivedFrom,
    /// Source material has similar properties to the target material.
    ///
    /// Created automatically when cosine similarity between property vectors
    /// meets or exceeds the configured threshold, but only when both concepts
    /// are at `Observed` confidence tier or above. The system never surfaces
    /// this connection before the player has earned it.
    SimilarTo,
    /// Source observation was made at the target location.
    ///
    /// More general than `FoundOn` — used when the observation subject is not
    /// a material (e.g., a fabrication event observed at a specific outpost).
    ObservedAt,
    // Future: SpokenBy, TradedAt, UsedIn, etc.
}

// ── Concept edge ──────────────────────────────────────────────────────────

/// A typed, weighted edge in the knowledge graph between two concept nodes.
///
/// Edges are directional (from source to target) and carry a [`RelationshipType`]
/// that describes the nature of the connection. The `confidence` field
/// accumulates as the player gathers repeated evidence for the same
/// relationship — the same edge strengthens rather than duplicating.
///
/// `discovered_at` records the game-time tick of the *first* observation that
/// established this relationship. Subsequent observations that strengthen the
/// edge do not update this field, preserving the discovery timeline.
///
/// Edges are serializable so the full knowledge graph can be saved and
/// restored across play sessions.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptEdge {
    /// The nature of the relationship from source concept to target concept.
    pub relationship: RelationshipType,
    /// Accumulated confidence in this relationship.
    ///
    /// Starts at the confidence of the first observation that created the edge
    /// and increases as the player makes additional observations that confirm
    /// the same connection. Capped at `1.0`.
    pub confidence: Confidence,
    /// Game-time tick when this relationship was first observed.
    pub discovered_at: u64,
}

impl ConceptEdge {
    /// Create a new edge with the given relationship type, confidence, and
    /// discovery tick.
    pub fn new(relationship: RelationshipType, confidence: Confidence, discovered_at: u64) -> Self {
        Self {
            relationship,
            confidence,
            discovered_at,
        }
    }

    /// Strengthen this edge by incorporating additional evidence.
    ///
    /// The new confidence is the maximum of the current value and the incoming
    /// value, capped at `1.0`. This ensures that repeated observations of the
    /// same relationship monotonically increase (or maintain) confidence and
    /// never decrease it due to a weaker subsequent observation.
    ///
    /// `discovered_at` is intentionally *not* updated — the edge retains the
    /// tick of its first observation.
    pub fn strengthen(&mut self, additional_confidence: Confidence) {
        let combined = (self.confidence.0 + additional_confidence.0).min(1.0);
        self.confidence = Confidence(combined);
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

    // ── ConceptEdge / RelationshipType tests ─────────────────────────────

    #[test]
    fn concept_edge_new_stores_fields() {
        let edge = ConceptEdge::new(RelationshipType::FoundOn, Confidence(0.4), 50);
        assert_eq!(edge.relationship, RelationshipType::FoundOn);
        assert_eq!(edge.confidence.0, 0.4);
        assert_eq!(edge.discovered_at, 50);
    }

    #[test]
    fn concept_edge_strengthen_accumulates_confidence() {
        let mut edge = ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.3), 10);
        edge.strengthen(Confidence(0.4));
        // 0.3 + 0.4 = 0.7
        assert!((edge.confidence.0 - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn concept_edge_strengthen_caps_at_one() {
        let mut edge = ConceptEdge::new(RelationshipType::DerivedFrom, Confidence(0.8), 20);
        edge.strengthen(Confidence(0.5));
        // 0.8 + 0.5 = 1.3, capped at 1.0
        assert_eq!(edge.confidence.0, 1.0);
    }

    #[test]
    fn concept_edge_strengthen_does_not_update_discovered_at() {
        let mut edge = ConceptEdge::new(RelationshipType::ObservedAt, Confidence(0.2), 100);
        edge.strengthen(Confidence(0.3));
        assert_eq!(edge.discovered_at, 100);
    }

    #[test]
    fn relationship_type_equality() {
        assert_eq!(RelationshipType::FoundOn, RelationshipType::FoundOn);
        assert_ne!(RelationshipType::FoundOn, RelationshipType::ObservedAt);
        assert_ne!(RelationshipType::CombinedWith, RelationshipType::DerivedFrom);
        assert_ne!(RelationshipType::SimilarTo, RelationshipType::FoundOn);
    }

    #[test]
    fn relationship_type_all_variants_constructible() {
        // Ensure all five required variants exist and are distinct.
        let variants = [
            RelationshipType::FoundOn,
            RelationshipType::CombinedWith,
            RelationshipType::DerivedFrom,
            RelationshipType::SimilarTo,
            RelationshipType::ObservedAt,
        ];
        // All five must be pairwise distinct.
        for i in 0..variants.len() {
            for j in 0..variants.len() {
                if i == j {
                    assert_eq!(variants[i], variants[j]);
                } else {
                    assert_ne!(variants[i], variants[j]);
                }
            }
        }
    }
}
