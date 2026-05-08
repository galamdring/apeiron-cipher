//! Knowledge graph types — concept nodes, edges, and the graph resource for
//! the cross-reference system.
//!
//! The knowledge graph is the player's associative web of discovered concepts.
//! Each concept corresponds to a journal entry (identified by [`JournalKey`]) and
//! carries metadata about when it was discovered and how confident the player is
//! in their understanding of it.
//!
//! This module defines the node-level types, edge types, the [`KnowledgeGraph`]
//! resource backed by `petgraph::Graph`, and the [`KnowledgeGraphPlugin`] that
//! wires the [`update_knowledge_graph`] system into the Bevy app.

use std::collections::{HashMap, HashSet};

use bevy::prelude::*;
use petgraph::graph::NodeIndex;
use petgraph::visit::EdgeRef;
use petgraph::{Direction, Graph};
use serde::{Deserialize, Serialize};

use crate::journal::{JournalKey, RecordObservation};
use crate::observation::Confidence;

// ── Plugin ────────────────────────────────────────────────────────────────

/// Plugin that initialises the [`KnowledgeGraph`] resource and registers the
/// [`update_knowledge_graph`] system.
///
/// Must be added after [`crate::journal::JournalPlugin`] because it reads
/// [`RecordObservation`] messages that the journal plugin registers.
pub struct KnowledgeGraphPlugin;

impl Plugin for KnowledgeGraphPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnowledgeGraph>()
            .add_systems(Update, update_knowledge_graph);
    }
}

// ── System ────────────────────────────────────────────────────────────────

/// Map a [`JournalKey`] to its [`ConceptCategory`].
///
/// Materials map to [`ConceptCategory::Material`], fabrication outputs map to
/// [`ConceptCategory::Fabrication`]. Location keys (when they exist) will map
/// to [`ConceptCategory::Location`]; for now any key passed as a
/// `context_location` is treated as a location concept.
fn category_from_key(key: &JournalKey) -> ConceptCategory {
    match key {
        JournalKey::Material { .. } => ConceptCategory::Material,
        JournalKey::Fabrication { .. } => ConceptCategory::Fabrication,
    }
}

/// System that processes [`RecordObservation`] messages and builds cross-references
/// in the [`KnowledgeGraph`].
///
/// For each observation the system:
///
/// 1. Ensures a concept node exists for the observation subject.
/// 2. If the observation carries a `context_location`, creates a `FoundOn` edge
///    (for materials) or `ObservedAt` edge (for other subjects) between the
///    subject and the location concept.
/// 3. If the observation is for a [`JournalKey::Fabrication`] output and
///    `input_seeds` are provided, creates `DerivedFrom` edges from the output
///    concept to each input material concept, and `CombinedWith` edges between
///    each pair of input materials.
///
/// All edges are bidirectional (enforced by [`KnowledgeGraph::relate`]).
/// No cross-reference is created without an observation event — the system
/// never infers connections the player hasn't made.
pub fn update_knowledge_graph(
    mut observations: MessageReader<RecordObservation>,
    mut graph: ResMut<KnowledgeGraph>,
    time: Res<Time>,
) {
    let tick = time.elapsed().as_millis() as u64;

    for obs in observations.read() {
        let subject_category = category_from_key(&obs.key);
        let subject_node =
            graph.ensure_concept(ConceptId::new(obs.key.clone()), subject_category, tick);

        // ── Location cross-reference ──────────────────────────────────────
        // If the observation has a location context, link the subject to that
        // location. Materials use FoundOn; other subjects use ObservedAt.
        if let Some(location_key) = &obs.context_location {
            let location_node = graph.ensure_concept(
                ConceptId::new(location_key.clone()),
                ConceptCategory::Location,
                tick,
            );

            let relationship = match &obs.key {
                JournalKey::Material { .. } => RelationshipType::FoundOn,
                JournalKey::Fabrication { .. } => RelationshipType::ObservedAt,
            };

            graph.relate(
                subject_node,
                location_node,
                ConceptEdge::new(relationship, obs.observation.confidence, tick),
            );
        }

        // ── Fabrication cross-references ──────────────────────────────────
        // For fabrication outputs, create DerivedFrom edges to each input
        // material and CombinedWith edges between each pair of inputs.
        if let JournalKey::Fabrication { .. } = &obs.key {
            // Collect NodeIndexes for all input materials first so we can
            // create CombinedWith edges between them without borrowing issues.
            let mut input_nodes: Vec<NodeIndex> = Vec::with_capacity(obs.input_seeds.len());

            for &input_seed in &obs.input_seeds {
                let input_key = JournalKey::Material {
                    seed: input_seed,
                    planet_seed: None,
                };
                let input_node = graph.ensure_concept(
                    ConceptId::new(input_key),
                    ConceptCategory::Material,
                    tick,
                );
                input_nodes.push(input_node);

                // Fabrication is directly observed — confidence is 1.0.
                graph.relate(
                    subject_node,
                    input_node,
                    ConceptEdge::new(RelationshipType::DerivedFrom, Confidence(1.0), tick),
                );
            }

            // CombinedWith edges between every pair of input materials.
            // For two inputs A and B: A CombinedWith B (and B CombinedWith A
            // via relate's bidirectionality).
            for i in 0..input_nodes.len() {
                for j in (i + 1)..input_nodes.len() {
                    graph.relate(
                        input_nodes[i],
                        input_nodes[j],
                        ConceptEdge::new(RelationshipType::CombinedWith, Confidence(1.0), tick),
                    );
                }
            }
        }
    }
}

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

// ── KnowledgeGraph resource ───────────────────────────────────────────────

/// The player's knowledge graph — backed by `petgraph::Graph`.
///
/// Every concept the player has discovered is a node; every observed
/// relationship between two concepts is a directed edge. The graph is
/// undirected in spirit (cross-references are bidirectional) but implemented
/// as a directed graph so that each edge carries its own [`RelationshipType`]
/// and the direction encodes the semantic role (e.g., "Material → FoundOn →
/// Location" vs. "Location → FoundOn → Material" would be two separate edges).
///
/// Bidirectionality is enforced by [`KnowledgeGraph::relate`], which always
/// inserts both the forward and reverse edge when a relationship is recorded.
///
/// # Indexes
///
/// Three auxiliary indexes are maintained alongside the graph for O(1) or
/// O(k) lookups that would otherwise require a full graph scan:
///
/// - `concept_index`: maps [`ConceptId`] → [`NodeIndex`] for O(1) node lookup.
/// - `category_index`: maps [`ConceptCategory`] → `Vec<NodeIndex>` for
///   encyclopedia-style listing of all concepts in a category.
/// - `timeline`: ordered list of `(tick, NodeIndex)` pairs recording the
///   discovery order of concepts.
///
/// # Serialization
///
/// The `petgraph::Graph` type serializes its node and edge weights directly
/// when the `serde-1` feature is enabled. The auxiliary indexes are derived
/// from the graph and are therefore re-derived on deserialization rather than
/// stored, keeping the save file compact and avoiding index/graph drift.
#[derive(Resource)]
pub struct KnowledgeGraph {
    /// The underlying directed graph. Nodes are [`ConceptNode`]s; edges are
    /// [`ConceptEdge`]s. Directed so that edge semantics are preserved.
    graph: Graph<ConceptNode, ConceptEdge>,
    /// O(1) lookup: [`ConceptId`] → [`NodeIndex`].
    ///
    /// Not serialized — rebuilt from the graph on load.
    #[allow(clippy::zero_sized_map_values)]
    concept_index: HashMap<ConceptId, NodeIndex>,
    /// Category index for encyclopedia view: category → list of node indexes.
    ///
    /// Not serialized — rebuilt from the graph on load.
    category_index: HashMap<ConceptCategory, Vec<NodeIndex>>,
    /// Timeline of concept discoveries in insertion order: `(tick, NodeIndex)`.
    ///
    /// Not serialized — rebuilt from the graph on load.
    timeline: Vec<(u64, NodeIndex)>,
}

impl Default for KnowledgeGraph {
    fn default() -> Self {
        Self {
            graph: Graph::new(),
            concept_index: HashMap::new(),
            category_index: HashMap::new(),
            timeline: Vec::new(),
        }
    }
}

impl KnowledgeGraph {
    /// Create an empty knowledge graph.
    pub fn new() -> Self {
        Self::default()
    }

    /// Get or create a concept node for the given [`ConceptId`].
    ///
    /// If the concept already exists, its [`NodeIndex`] is returned unchanged
    /// and no new node is inserted. If it does not exist, a new node is created
    /// with the given `category`, an initial confidence of `0.0`, and the
    /// provided `tick` as its discovery time.
    ///
    /// The concept is also registered in the category index and timeline on
    /// first insertion.
    pub fn ensure_concept(
        &mut self,
        id: ConceptId,
        category: ConceptCategory,
        tick: u64,
    ) -> NodeIndex {
        if let Some(&idx) = self.concept_index.get(&id) {
            return idx;
        }

        let node = ConceptNode::new(id.clone(), category.clone(), Confidence(0.0), tick);
        let idx = self.graph.add_node(node);

        self.concept_index.insert(id, idx);
        self.category_index.entry(category).or_default().push(idx);
        self.timeline.push((tick, idx));

        idx
    }

    /// Add or strengthen a typed relationship between two concept nodes.
    ///
    /// Cross-references are **bidirectional**: calling `relate(from, to, edge)`
    /// inserts both a forward edge (`from → to`) and a reverse edge
    /// (`to → from`) with the same relationship type and confidence. This
    /// satisfies the acceptance criterion that "if Material X links to Planet Y,
    /// Planet Y links back to Material X."
    ///
    /// If an edge with the same [`RelationshipType`] already exists between the
    /// two nodes in a given direction, it is **strengthened** (confidence
    /// accumulates) rather than duplicated. This satisfies the criterion that
    /// "cross-references accumulate — the same relationship strengthens with
    /// repeated evidence."
    ///
    /// # Panics
    ///
    /// Panics if `from` or `to` are not valid node indexes in this graph.
    pub fn relate(&mut self, from: NodeIndex, to: NodeIndex, edge: ConceptEdge) {
        // Forward edge: from → to
        Self::upsert_edge(&mut self.graph, from, to, edge.clone());
        // Reverse edge: to → from (same relationship type, same confidence)
        Self::upsert_edge(&mut self.graph, to, from, edge);
    }

    /// Insert a new edge or strengthen an existing one with the same
    /// relationship type between the same pair of nodes.
    fn upsert_edge(
        graph: &mut Graph<ConceptNode, ConceptEdge>,
        from: NodeIndex,
        to: NodeIndex,
        new_edge: ConceptEdge,
    ) {
        // Search for an existing edge with the same relationship type.
        let existing = graph
            .edges_directed(from, Direction::Outgoing)
            .find(|e| e.target() == to && e.weight().relationship == new_edge.relationship)
            .map(|e| e.id());

        if let Some(edge_id) = existing {
            graph[edge_id].strengthen(new_edge.confidence);
        } else {
            graph.add_edge(from, to, new_edge);
        }
    }

    /// Get all relationships for a concept node — returns `(neighbor NodeIndex,
    /// &ConceptEdge)` pairs for every outgoing edge from this node.
    ///
    /// Because [`KnowledgeGraph::relate`] always inserts both a forward and a
    /// reverse edge, iterating outgoing edges is sufficient to enumerate all
    /// connections: every relationship the node participates in appears as an
    /// outgoing edge in at least one direction. Callers that need to display
    /// "all connections" for a concept should call this method; the result
    /// already includes the reverse direction because `relate` inserted it.
    pub fn relationships(&self, node: NodeIndex) -> Vec<(NodeIndex, &ConceptEdge)> {
        self.graph
            .edges_directed(node, Direction::Outgoing)
            .map(|e| (e.target(), e.weight()))
            .collect()
    }

    /// Bounded BFS from a concept node, returning all reachable nodes within
    /// `depth` hops along with their hop distance from the center.
    ///
    /// The center node itself is **not** included in the result. If
    /// `category_filter` is `Some`, only nodes whose category matches are
    /// included in the result (but the BFS still traverses through nodes of
    /// other categories to find matching ones within the depth limit).
    ///
    /// This method is the data-model foundation for the future associative web
    /// view. It does not perform any rendering or UI work.
    pub fn neighborhood(
        &self,
        center: NodeIndex,
        depth: usize,
        category_filter: Option<&ConceptCategory>,
    ) -> Vec<(NodeIndex, usize)> {
        if depth == 0 {
            return Vec::new();
        }

        // BFS state: visited set and queue of (node, current_depth).
        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue: std::collections::VecDeque<(NodeIndex, usize)> =
            std::collections::VecDeque::new();
        let mut result: Vec<(NodeIndex, usize)> = Vec::new();

        visited.insert(center);
        queue.push_back((center, 0));

        while let Some((current, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            // Traverse all neighbors (both directions) from the current node.
            let neighbors: Vec<NodeIndex> = self
                .graph
                .edges_directed(current, Direction::Outgoing)
                .map(|e| e.target())
                .chain(
                    self.graph
                        .edges_directed(current, Direction::Incoming)
                        .map(|e| e.source()),
                )
                .collect();

            for neighbor in neighbors {
                if visited.contains(&neighbor) {
                    continue;
                }
                visited.insert(neighbor);
                let hop = current_depth + 1;

                // Apply category filter to the result set, but always enqueue
                // the neighbor so BFS can traverse through it.
                let node_data = &self.graph[neighbor];
                let passes_filter = category_filter
                    .map(|cat| &node_data.category == cat)
                    .unwrap_or(true);

                if passes_filter {
                    result.push((neighbor, hop));
                }

                queue.push_back((neighbor, hop));
            }
        }

        result
    }

    /// All concept nodes in a given category, in insertion order.
    ///
    /// Returns an empty slice if no concepts of that category have been
    /// discovered yet.
    pub fn by_category(&self, category: &ConceptCategory) -> &[NodeIndex] {
        self.category_index
            .get(category)
            .map(Vec::as_slice)
            .unwrap_or(&[])
    }

    /// Timeline of concept discoveries: `(tick, NodeIndex)` pairs in
    /// insertion order (earliest discovery first).
    pub fn timeline(&self) -> &[(u64, NodeIndex)] {
        &self.timeline
    }

    /// Look up a concept node by its [`ConceptId`].
    ///
    /// Returns `None` if the concept has not been added to the graph yet.
    pub fn lookup(&self, id: &ConceptId) -> Option<NodeIndex> {
        self.concept_index.get(id).copied()
    }

    /// Borrow the [`ConceptNode`] data for a given [`NodeIndex`].
    ///
    /// Returns `None` if the index is not valid (e.g., after a node was
    /// removed, though the current implementation never removes nodes).
    pub fn node(&self, idx: NodeIndex) -> Option<&ConceptNode> {
        self.graph.node_weight(idx)
    }

    /// Mutably borrow the [`ConceptNode`] data for a given [`NodeIndex`].
    pub fn node_mut(&mut self, idx: NodeIndex) -> Option<&mut ConceptNode> {
        self.graph.node_weight_mut(idx)
    }

    /// Serialize the knowledge graph to a JSON string for save/load.
    ///
    /// The auxiliary indexes (`concept_index`, `category_index`, `timeline`)
    /// are **not** serialized — they are rebuilt from the graph on
    /// deserialization via [`KnowledgeGraph::from_serializable`].
    pub fn to_serializable(&self) -> SerializableKnowledgeGraph {
        SerializableKnowledgeGraph {
            graph: self.graph.clone(),
        }
    }

    /// Reconstruct a `KnowledgeGraph` from its serialized form, rebuilding
    /// all auxiliary indexes from the graph data.
    pub fn from_serializable(serializable: SerializableKnowledgeGraph) -> Self {
        let graph = serializable.graph;

        let mut concept_index: HashMap<ConceptId, NodeIndex> = HashMap::new();
        let mut category_index: HashMap<ConceptCategory, Vec<NodeIndex>> = HashMap::new();
        let mut timeline: Vec<(u64, NodeIndex)> = Vec::new();

        for idx in graph.node_indices() {
            let node = &graph[idx];
            concept_index.insert(node.id.clone(), idx);
            category_index
                .entry(node.category.clone())
                .or_default()
                .push(idx);
            timeline.push((node.discovered_at, idx));
        }

        // Sort timeline by tick to restore discovery order after round-trip.
        timeline.sort_by_key(|(tick, _)| *tick);

        Self {
            graph,
            concept_index,
            category_index,
            timeline,
        }
    }
}

/// Serializable form of [`KnowledgeGraph`] for save/load.
///
/// The auxiliary indexes are omitted and rebuilt on deserialization via
/// [`KnowledgeGraph::from_serializable`].
#[derive(Serialize, Deserialize)]
pub struct SerializableKnowledgeGraph {
    /// The underlying petgraph graph with all concept nodes and edges.
    pub graph: Graph<ConceptNode, ConceptEdge>,
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
        let node = ConceptNode::new(id, ConceptCategory::Material, Confidence(0.2), 100);
        assert!(node.revealed_properties.is_empty());
        assert!(!node.has_property("density"));
    }

    #[test]
    fn concept_node_reveal_property_is_idempotent() {
        let id = ConceptId::new(material_key(2));
        let mut node = ConceptNode::new(id, ConceptCategory::Material, Confidence(0.5), 200);
        node.reveal_property("density");
        node.reveal_property("density"); // second call is a no-op
        assert!(node.has_property("density"));
        assert_eq!(node.revealed_properties.len(), 1);
    }

    #[test]
    fn concept_node_multiple_properties() {
        let id = ConceptId::new(material_key(3));
        let mut node = ConceptNode::new(id, ConceptCategory::Material, Confidence(0.8), 300);
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
        let node = ConceptNode::new(id.clone(), ConceptCategory::Location, Confidence(0.6), 999);
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
        assert_ne!(
            RelationshipType::CombinedWith,
            RelationshipType::DerivedFrom
        );
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

    // ── KnowledgeGraph tests ──────────────────────────────────────────────

    fn location_key(seed: u64) -> JournalKey {
        // JournalKey has no Location variant yet; use a Material with a
        // planet_seed to represent a location concept in tests.
        JournalKey::Material {
            seed,
            planet_seed: Some(seed),
        }
    }

    fn make_graph() -> KnowledgeGraph {
        KnowledgeGraph::new()
    }

    #[test]
    fn ensure_concept_creates_new_node() {
        let mut graph = make_graph();
        let id = ConceptId::new(material_key(1));
        let idx = graph.ensure_concept(id.clone(), ConceptCategory::Material, 10);
        assert_eq!(graph.lookup(&id), Some(idx));
    }

    #[test]
    fn ensure_concept_is_idempotent() {
        let mut graph = make_graph();
        let id = ConceptId::new(material_key(2));
        let idx1 = graph.ensure_concept(id.clone(), ConceptCategory::Material, 10);
        let idx2 = graph.ensure_concept(id.clone(), ConceptCategory::Material, 20);
        // Same node returned both times.
        assert_eq!(idx1, idx2);
        // Timeline should only have one entry.
        assert_eq!(graph.timeline().len(), 1);
    }

    #[test]
    fn lookup_returns_none_for_unknown_concept() {
        let graph = make_graph();
        let id = ConceptId::new(material_key(99));
        assert_eq!(graph.lookup(&id), None);
    }

    #[test]
    fn by_category_returns_inserted_nodes() {
        let mut graph = make_graph();
        let mat_id = ConceptId::new(material_key(1));
        let loc_id = ConceptId::new(location_key(2));
        let mat_idx = graph.ensure_concept(mat_id, ConceptCategory::Material, 1);
        let loc_idx = graph.ensure_concept(loc_id, ConceptCategory::Location, 2);

        let materials = graph.by_category(&ConceptCategory::Material);
        assert_eq!(materials, &[mat_idx]);

        let locations = graph.by_category(&ConceptCategory::Location);
        assert_eq!(locations, &[loc_idx]);
    }

    #[test]
    fn by_category_returns_empty_for_unknown_category() {
        let graph = make_graph();
        assert!(graph.by_category(&ConceptCategory::Fabrication).is_empty());
    }

    #[test]
    fn timeline_records_discovery_order() {
        let mut graph = make_graph();
        let id1 = ConceptId::new(material_key(1));
        let id2 = ConceptId::new(material_key(2));
        let idx1 = graph.ensure_concept(id1, ConceptCategory::Material, 5);
        let idx2 = graph.ensure_concept(id2, ConceptCategory::Material, 10);

        let tl = graph.timeline();
        assert_eq!(tl.len(), 2);
        assert_eq!(tl[0], (5, idx1));
        assert_eq!(tl[1], (10, idx2));
    }

    #[test]
    fn relate_creates_bidirectional_edges() {
        let mut graph = make_graph();
        let mat_id = ConceptId::new(material_key(1));
        let loc_id = ConceptId::new(location_key(2));
        let mat_idx = graph.ensure_concept(mat_id, ConceptCategory::Material, 1);
        let loc_idx = graph.ensure_concept(loc_id, ConceptCategory::Location, 2);

        let edge = ConceptEdge::new(RelationshipType::FoundOn, Confidence(0.5), 3);
        graph.relate(mat_idx, loc_idx, edge);

        // Forward: material → location
        let mat_rels = graph.relationships(mat_idx);
        assert!(
            mat_rels
                .iter()
                .any(|(n, e)| { *n == loc_idx && e.relationship == RelationshipType::FoundOn })
        );

        // Reverse: location → material
        let loc_rels = graph.relationships(loc_idx);
        assert!(
            loc_rels
                .iter()
                .any(|(n, e)| { *n == mat_idx && e.relationship == RelationshipType::FoundOn })
        );
    }

    #[test]
    fn relate_strengthens_existing_edge_on_repeat() {
        let mut graph = make_graph();
        let id1 = ConceptId::new(material_key(1));
        let id2 = ConceptId::new(material_key(2));
        let idx1 = graph.ensure_concept(id1, ConceptCategory::Material, 1);
        let idx2 = graph.ensure_concept(id2, ConceptCategory::Material, 2);

        let edge1 = ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.3), 5);
        let edge2 = ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.4), 10);
        graph.relate(idx1, idx2, edge1);
        graph.relate(idx1, idx2, edge2);

        // relationships() returns both outgoing and incoming edges.
        // Filter to only edges pointing toward idx2 (the forward direction).
        let rels = graph.relationships(idx1);
        let forward: Vec<_> = rels
            .iter()
            .filter(|(n, e)| *n == idx2 && e.relationship == RelationshipType::SimilarTo)
            .collect();
        // Exactly one forward edge (strengthened, not duplicated).
        assert_eq!(forward.len(), 1);
        // 0.3 + 0.4 = 0.7
        assert!((forward[0].1.confidence.0 - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn neighborhood_returns_nodes_within_depth() {
        let mut graph = make_graph();
        // A — B — C — D (linear chain)
        let a = graph.ensure_concept(
            ConceptId::new(material_key(1)),
            ConceptCategory::Material,
            1,
        );
        let b = graph.ensure_concept(
            ConceptId::new(material_key(2)),
            ConceptCategory::Material,
            2,
        );
        let c = graph.ensure_concept(
            ConceptId::new(material_key(3)),
            ConceptCategory::Material,
            3,
        );
        let d = graph.ensure_concept(
            ConceptId::new(material_key(4)),
            ConceptCategory::Material,
            4,
        );

        let edge = || ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.5), 1);
        graph.relate(a, b, edge());
        graph.relate(b, c, edge());
        graph.relate(c, d, edge());

        // From A with depth=2: should reach B (hop 1) and C (hop 2), not D.
        let neighbors = graph.neighborhood(a, 2, None);
        let nodes: Vec<NodeIndex> = neighbors.iter().map(|(n, _)| *n).collect();
        assert!(nodes.contains(&b));
        assert!(nodes.contains(&c));
        assert!(!nodes.contains(&d));
        assert!(!nodes.contains(&a)); // center excluded
    }

    #[test]
    fn neighborhood_depth_zero_returns_empty() {
        let mut graph = make_graph();
        let a = graph.ensure_concept(
            ConceptId::new(material_key(1)),
            ConceptCategory::Material,
            1,
        );
        let b = graph.ensure_concept(
            ConceptId::new(material_key(2)),
            ConceptCategory::Material,
            2,
        );
        graph.relate(
            a,
            b,
            ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.5), 1),
        );

        assert!(graph.neighborhood(a, 0, None).is_empty());
    }

    #[test]
    fn neighborhood_depth_one_returns_only_direct_neighbors() {
        let mut graph = make_graph();
        // A — B — C — D (linear chain)
        let a = graph.ensure_concept(
            ConceptId::new(material_key(1)),
            ConceptCategory::Material,
            1,
        );
        let b = graph.ensure_concept(
            ConceptId::new(material_key(2)),
            ConceptCategory::Material,
            2,
        );
        let c = graph.ensure_concept(
            ConceptId::new(material_key(3)),
            ConceptCategory::Material,
            3,
        );
        let d = graph.ensure_concept(
            ConceptId::new(material_key(4)),
            ConceptCategory::Material,
            4,
        );

        let edge = || ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.5), 1);
        graph.relate(a, b, edge());
        graph.relate(b, c, edge());
        graph.relate(c, d, edge());

        // depth=1: only direct neighbors of A (i.e., B). C and D are too far.
        let neighbors = graph.neighborhood(a, 1, None);
        let nodes: Vec<NodeIndex> = neighbors.iter().map(|(n, _)| *n).collect();
        assert_eq!(nodes.len(), 1, "depth=1 must return exactly one direct neighbor");
        assert!(nodes.contains(&b), "B must be in depth=1 neighborhood of A");
        assert!(!nodes.contains(&c), "C must not be in depth=1 neighborhood of A");
        assert!(!nodes.contains(&d), "D must not be in depth=1 neighborhood of A");
        assert!(!nodes.contains(&a), "center node must not appear in its own neighborhood");

        // Verify hop distance is reported as 1.
        let hop = neighbors.iter().find(|(n, _)| *n == b).map(|(_, h)| *h);
        assert_eq!(hop, Some(1), "B must be reported at hop distance 1");
    }

    #[test]
    fn neighborhood_category_filter_excludes_non_matching() {
        let mut graph = make_graph();
        let mat = graph.ensure_concept(
            ConceptId::new(material_key(1)),
            ConceptCategory::Material,
            1,
        );
        let loc = graph.ensure_concept(
            ConceptId::new(location_key(2)),
            ConceptCategory::Location,
            2,
        );
        let mat2 = graph.ensure_concept(
            ConceptId::new(material_key(3)),
            ConceptCategory::Material,
            3,
        );

        let edge = || ConceptEdge::new(RelationshipType::FoundOn, Confidence(0.5), 1);
        graph.relate(mat, loc, edge());
        graph.relate(loc, mat2, edge());

        // From mat with depth=2, filter to Material only: should see mat2 (hop 2) but not loc.
        let neighbors = graph.neighborhood(mat, 2, Some(&ConceptCategory::Material));
        let nodes: Vec<NodeIndex> = neighbors.iter().map(|(n, _)| *n).collect();
        assert!(!nodes.contains(&loc));
        assert!(nodes.contains(&mat2));
    }

    #[test]
    fn neighborhood_disconnected_node_returns_empty() {
        // A node with no edges should have an empty neighborhood at any depth.
        let mut graph = make_graph();
        let isolated = graph.ensure_concept(
            ConceptId::new(material_key(99)),
            ConceptCategory::Material,
            1,
        );
        assert!(
            graph.neighborhood(isolated, 3, None).is_empty(),
            "disconnected node must have no neighbors"
        );
    }

    #[test]
    fn neighborhood_handles_cycles_without_infinite_loop() {
        // Build a cycle: A — B — C — A (triangle).
        // BFS must visit each node at most once and terminate cleanly.
        let mut graph = make_graph();
        let a = graph.ensure_concept(
            ConceptId::new(material_key(1)),
            ConceptCategory::Material,
            1,
        );
        let b = graph.ensure_concept(
            ConceptId::new(material_key(2)),
            ConceptCategory::Material,
            2,
        );
        let c = graph.ensure_concept(
            ConceptId::new(material_key(3)),
            ConceptCategory::Material,
            3,
        );

        let edge = || ConceptEdge::new(RelationshipType::SimilarTo, Confidence(0.5), 1);
        // A → B → C → A forms a directed cycle; relate() also adds the reverse edge,
        // so the undirected traversal sees a fully-connected triangle.
        graph.relate(a, b, edge());
        graph.relate(b, c, edge());
        graph.relate(c, a, edge());

        // With depth=10 (well beyond the 3-node cycle), BFS must still return exactly
        // the two other nodes (B and C) and must not loop or panic.
        let neighbors = graph.neighborhood(a, 10, None);
        let nodes: Vec<NodeIndex> = neighbors.iter().map(|(n, _)| *n).collect();

        assert_eq!(
            nodes.len(),
            2,
            "cycle graph must yield exactly 2 unique neighbors for A (B and C), got {nodes:?}"
        );
        assert!(nodes.contains(&b), "B must be reachable from A");
        assert!(nodes.contains(&c), "C must be reachable from A");
        assert!(!nodes.contains(&a), "center node A must not appear in its own neighborhood");

        // Hop distances: B is 1 hop away, C is 1 hop away (direct edge via relate's reverse).
        // Both must be ≤ depth and must be the shortest path distance.
        let hop_b = neighbors.iter().find(|(n, _)| *n == b).map(|(_, h)| *h);
        let hop_c = neighbors.iter().find(|(n, _)| *n == c).map(|(_, h)| *h);
        assert_eq!(hop_b, Some(1), "B must be at hop distance 1 from A");
        assert_eq!(hop_c, Some(1), "C must be at hop distance 1 from A");
    }

    #[test]
    fn node_accessor_returns_concept_data() {
        let mut graph = make_graph();
        let id = ConceptId::new(material_key(42));
        let idx = graph.ensure_concept(id.clone(), ConceptCategory::Material, 7);
        let node = graph.node(idx).expect("node must exist");
        assert_eq!(node.id, id);
        assert_eq!(node.category, ConceptCategory::Material);
        assert_eq!(node.discovered_at, 7);
    }

    #[test]
    fn serialization_round_trip_preserves_graph() {
        let mut graph = make_graph();
        let mat_id = ConceptId::new(material_key(1));
        let loc_id = ConceptId::new(location_key(2));
        let mat_idx = graph.ensure_concept(mat_id.clone(), ConceptCategory::Material, 10);
        let loc_idx = graph.ensure_concept(loc_id.clone(), ConceptCategory::Location, 20);
        graph.relate(
            mat_idx,
            loc_idx,
            ConceptEdge::new(RelationshipType::FoundOn, Confidence(0.6), 15),
        );

        // Serialize to JSON bytes and deserialize back to exercise serde.
        let serializable = graph.to_serializable();
        let json = serde_json::to_string(&serializable).expect("serialization must succeed");
        let restored_serializable: SerializableKnowledgeGraph =
            serde_json::from_str(&json).expect("deserialization must succeed");
        let restored = KnowledgeGraph::from_serializable(restored_serializable);

        // Indexes are rebuilt.
        let restored_mat = restored.lookup(&mat_id).expect("material must be found");
        let restored_loc = restored.lookup(&loc_id).expect("location must be found");

        // Relationships are preserved.
        let rels = restored.relationships(restored_mat);
        assert!(
            rels.iter().any(|(n, e)| {
                *n == restored_loc && e.relationship == RelationshipType::FoundOn
            })
        );

        // Timeline is rebuilt.
        assert_eq!(restored.timeline().len(), 2);
    }

    // ── update_knowledge_graph system tests ──────────────────────────────

    use crate::journal::{Observation, ObservationCategory, RecordObservation};

    /// Build a minimal Bevy App with the message channel and system under test.
    fn build_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<RecordObservation>();
        app.init_resource::<KnowledgeGraph>();
        app.add_systems(Update, update_knowledge_graph);
        app
    }

    /// Inject a single [`RecordObservation`] message via a one-shot system.
    fn inject_observation(app: &mut App, obs: RecordObservation) {
        fn write_obs(
            input: bevy::ecs::system::In<RecordObservation>,
            mut writer: MessageWriter<RecordObservation>,
        ) {
            writer.write(input.0.clone());
        }
        app.world_mut()
            .run_system_cached_with(write_obs, obs)
            .expect("one-shot system must run");
    }

    /// Construct a minimal [`RecordObservation`] for a material with no
    /// cross-reference metadata.
    fn material_obs(seed: u64) -> RecordObservation {
        RecordObservation {
            key: JournalKey::Material {
                seed,
                planet_seed: None,
            },
            name: format!("Mat-{seed}"),
            observation: Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: Confidence(0.5),
                description: "test".to_string(),
                recorded_at: 0,
            },
            input_seeds: vec![],
            context_location: None,
        }
    }

    #[test]
    fn material_observation_creates_concept_node() {
        let mut app = build_test_app();
        inject_observation(&mut app, material_obs(42));
        app.update();

        let graph = app.world().resource::<KnowledgeGraph>();
        let id = ConceptId::new(JournalKey::Material {
            seed: 42,
            planet_seed: None,
        });
        assert!(
            graph.lookup(&id).is_some(),
            "concept node must be created for observed material"
        );
    }

    #[test]
    fn material_with_context_location_creates_found_on_edge() {
        let mut app = build_test_app();

        let location_key = JournalKey::Material {
            seed: 999,
            planet_seed: Some(999),
        };

        let obs = RecordObservation {
            key: JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            name: "Mat-1".to_string(),
            observation: Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: Confidence(0.5),
                description: "test".to_string(),
                recorded_at: 0,
            },
            input_seeds: vec![],
            context_location: Some(location_key.clone()),
        };

        inject_observation(&mut app, obs);
        app.update();

        let graph = app.world().resource::<KnowledgeGraph>();
        let mat_id = ConceptId::new(JournalKey::Material {
            seed: 1,
            planet_seed: None,
        });
        let loc_id = ConceptId::new(location_key);

        let mat_node = graph.lookup(&mat_id).expect("material concept must exist");
        let loc_node = graph.lookup(&loc_id).expect("location concept must exist");

        // Forward edge: material → FoundOn → location
        let rels = graph.relationships(mat_node);
        assert!(
            rels.iter()
                .any(|(n, e)| *n == loc_node && e.relationship == RelationshipType::FoundOn),
            "material must have FoundOn edge to location"
        );

        // Reverse edge: location → FoundOn → material (bidirectional)
        let loc_rels = graph.relationships(loc_node);
        assert!(
            loc_rels
                .iter()
                .any(|(n, e)| *n == mat_node && e.relationship == RelationshipType::FoundOn),
            "location must have reverse FoundOn edge back to material"
        );
    }

    #[test]
    fn fabrication_observation_creates_derived_from_edges() {
        let mut app = build_test_app();

        let obs = RecordObservation {
            key: JournalKey::Fabrication { output_seed: 100 },
            name: "Output-100".to_string(),
            observation: Observation {
                category: ObservationCategory::FabricationResult,
                confidence: Confidence(1.0),
                description: "fabricated".to_string(),
                recorded_at: 0,
            },
            input_seeds: vec![10, 20],
            context_location: None,
        };

        inject_observation(&mut app, obs);
        app.update();

        let graph = app.world().resource::<KnowledgeGraph>();
        let output_id = ConceptId::new(JournalKey::Fabrication { output_seed: 100 });
        let input_a_id = ConceptId::new(JournalKey::Material {
            seed: 10,
            planet_seed: None,
        });
        let input_b_id = ConceptId::new(JournalKey::Material {
            seed: 20,
            planet_seed: None,
        });

        let output_node = graph.lookup(&output_id).expect("output concept must exist");
        let input_a_node = graph.lookup(&input_a_id).expect("input A concept must exist");
        let input_b_node = graph.lookup(&input_b_id).expect("input B concept must exist");

        // Output → DerivedFrom → each input
        let rels = graph.relationships(output_node);
        assert!(
            rels.iter()
                .any(|(n, e)| *n == input_a_node && e.relationship == RelationshipType::DerivedFrom),
            "output must have DerivedFrom edge to input A"
        );
        assert!(
            rels.iter()
                .any(|(n, e)| *n == input_b_node && e.relationship == RelationshipType::DerivedFrom),
            "output must have DerivedFrom edge to input B"
        );
    }

    #[test]
    fn fabrication_observation_creates_combined_with_edges_between_inputs() {
        let mut app = build_test_app();

        let obs = RecordObservation {
            key: JournalKey::Fabrication { output_seed: 200 },
            name: "Output-200".to_string(),
            observation: Observation {
                category: ObservationCategory::FabricationResult,
                confidence: Confidence(1.0),
                description: "fabricated".to_string(),
                recorded_at: 0,
            },
            input_seeds: vec![30, 40],
            context_location: None,
        };

        inject_observation(&mut app, obs);
        app.update();

        let graph = app.world().resource::<KnowledgeGraph>();
        let input_a_id = ConceptId::new(JournalKey::Material {
            seed: 30,
            planet_seed: None,
        });
        let input_b_id = ConceptId::new(JournalKey::Material {
            seed: 40,
            planet_seed: None,
        });

        let input_a_node = graph.lookup(&input_a_id).expect("input A concept must exist");
        let input_b_node = graph.lookup(&input_b_id).expect("input B concept must exist");

        // Input A → CombinedWith → Input B (and reverse via bidirectionality)
        let rels_a = graph.relationships(input_a_node);
        assert!(
            rels_a.iter().any(|(n, e)| {
                *n == input_b_node && e.relationship == RelationshipType::CombinedWith
            }),
            "input A must have CombinedWith edge to input B"
        );

        let rels_b = graph.relationships(input_b_node);
        assert!(
            rels_b.iter().any(|(n, e)| {
                *n == input_a_node && e.relationship == RelationshipType::CombinedWith
            }),
            "input B must have reverse CombinedWith edge to input A"
        );
    }

    #[test]
    fn repeated_observation_strengthens_edge_not_duplicates() {
        let mut app = build_test_app();

        let location_key = JournalKey::Material {
            seed: 777,
            planet_seed: Some(777),
        };

        // Send the same material+location observation twice.
        for _ in 0..2 {
            let obs = RecordObservation {
                key: JournalKey::Material {
                    seed: 5,
                    planet_seed: None,
                },
                name: "Mat-5".to_string(),
                observation: Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: Confidence(0.3),
                    description: "test".to_string(),
                    recorded_at: 0,
                },
                input_seeds: vec![],
                context_location: Some(location_key.clone()),
            };
            inject_observation(&mut app, obs);
            app.update();
        }

        let graph = app.world().resource::<KnowledgeGraph>();
        let mat_id = ConceptId::new(JournalKey::Material {
            seed: 5,
            planet_seed: None,
        });
        let loc_id = ConceptId::new(location_key);

        let mat_node = graph.lookup(&mat_id).expect("material concept must exist");
        let loc_node = graph.lookup(&loc_id).expect("location concept must exist");

        // There must be exactly one FoundOn edge from material to location
        // (strengthened, not duplicated).
        let rels = graph.relationships(mat_node);
        let found_on_edges: Vec<_> = rels
            .iter()
            .filter(|(n, e)| *n == loc_node && e.relationship == RelationshipType::FoundOn)
            .collect();
        assert_eq!(
            found_on_edges.len(),
            1,
            "repeated observation must strengthen the edge, not create a duplicate"
        );
        // Confidence must be higher than a single observation (0.3 + 0.3 = 0.6).
        assert!(
            found_on_edges[0].1.confidence.0 > 0.3,
            "confidence must accumulate across repeated observations"
        );
    }

    #[test]
    fn observation_without_context_creates_no_location_edge() {
        let mut app = build_test_app();
        inject_observation(&mut app, material_obs(99));
        app.update();

        let graph = app.world().resource::<KnowledgeGraph>();
        let id = ConceptId::new(JournalKey::Material {
            seed: 99,
            planet_seed: None,
        });
        let node = graph.lookup(&id).expect("concept must exist");

        // No edges should exist — no context_location was provided.
        assert!(
            graph.relationships(node).is_empty(),
            "observation without context_location must not create any edges"
        );
    }
}
