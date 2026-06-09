//! Knowledge graph — the player's associative web of discovered concepts.
//!
//! This module implements the `KnowledgeGraph` resource backed by
//! [`petgraph::Graph`] as specified in the data-architecture decision.
//! The graph stores concept nodes ([`ConceptNode`]) and typed relationship
//! edges ([`ConceptEdge`]) that are accumulated as the player observes
//! the world.
//!
//! # Design principles
//!
//! * **Observation-gated:** No edge or node is ever created without an
//!   observation event. The graph never infers connections the player
//!   hasn't personally made.
//! * **Deterministic:** BFS traversal order follows petgraph's stable
//!   internal edge order. Timeline entries are appended in tick order,
//!   which is guaranteed monotone by the game clock.
//! * **Serializable:** The full graph round-trips through serde via
//!   petgraph's `serde-1` feature plus hand-implemented index maps.

use std::collections::{BTreeMap, HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use petgraph::Direction;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::journal::JournalKey;
use crate::materials::MaterialSeed;
use crate::observation::{Confidence, ConfidenceConfig};
use crate::world_generation::PlanetSeed;

// ── Plugin ────────────────────────────────────────────────────────────────

/// Registers the [`KnowledgeGraph`] resource and the
/// [`update_knowledge_graph`] system that populates it from
/// [`crate::observation::RecordObservation`] messages.
pub struct KnowledgeGraphPlugin;

impl Plugin for KnowledgeGraphPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnowledgeGraph>().add_systems(
            Update,
            (
                update_knowledge_graph.in_set(crate::journal::JournalSet::Navigate),
                detect_similar_on_observation
                    .in_set(crate::journal::JournalSet::Navigate)
                    .after(update_knowledge_graph),
            ),
        );
    }
}

// ── Concept identity ──────────────────────────────────────────────────────

/// Unique concept identifier — wraps a [`JournalKey`] so the graph and the
/// journal share the same identity space with no mapping step.
///
/// Equality and hashing match those of the wrapped key, giving O(1) map
/// lookups in [`KnowledgeGraph::concept_index`].
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ConceptId(pub JournalKey);

// ── Concept categories ────────────────────────────────────────────────────

/// High-level classification used for encyclopedia-style grouping and BFS
/// category filtering.
///
/// Variants map 1-to-1 onto [`JournalKey`] discriminants — [`ConceptId`]
/// creation always supplies the correct category, so the graph never
/// stores an inconsistent category for a node.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum ConceptCategory {
    /// A raw or discovered material.
    Material,
    /// A planetary or regional location.
    Location,
    /// A fabrication output.
    Fabrication,
    // Future: Language, Culture, Trade, Species, etc.
}

// ── Graph node ────────────────────────────────────────────────────────────

/// A node in the knowledge graph — represents a known concept.
///
/// Each node is created by [`KnowledgeGraph::ensure_concept`] on first
/// encounter and is never removed (the graph accumulates monotonically).
///
/// As of Story 387 this node is the **sole storage location** for all
/// player knowledge about a concept: the display name, every observation
/// ever recorded, confidence, timestamps, and planet provenance all live
/// here. The `Journal` component is now a pure query/UI layer — it holds
/// no data between frames.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptNode {
    /// Stable identity linking this node to the corresponding journal key.
    pub id: ConceptId,
    /// Coarse classification for encyclopedia grouping and BFS filtering.
    pub category: ConceptCategory,
    /// Player-facing display name for this concept (e.g. "Ferrite").
    ///
    /// Set on first observation and never overwritten — first observer wins,
    /// matching the behaviour of the old `Journal::ensure_entry`.
    pub name: String,
    /// Overall confidence that this concept is real and correctly identified.
    ///
    /// Accumulated from every observation's confidence value. Used by the
    /// similarity system to gate `SimilarTo` edge creation (must be at
    /// `Observed` tier or above before cross-material comparisons fire).
    pub confidence: Confidence,
    /// Game-time tick (whole seconds elapsed) when this concept was first
    /// encountered. Immutable after creation.
    pub first_observed_at: u64,
    /// Game-time tick of the most recent observation recorded for this node.
    /// Updated every time [`Self::add_observation`] or its accumulating
    /// variants are called.
    pub last_updated_at: u64,
    /// Planet on which this concept was first observed.
    ///
    /// Populated from [`crate::observation::RecordObservation::planet_seed`] on
    /// the first observation that carries a planet seed. `None` for fabricated
    /// materials and concepts recorded without planetary context. Used by the
    /// `CurrentPlanet` journal filter.
    pub origin_planet_seed: Option<PlanetSeed>,
    /// Which named properties the player has directly observed on this concept.
    ///
    /// Keys match [`crate::journal::ObservationCategory::display_label`] so
    /// the inspect panel and encyclopedia can check revealed status without
    /// additional mapping.
    pub revealed_properties: HashMap<crate::journal::ObservationCategory, f32>,
    /// All observations recorded about this concept, grouped by category.
    ///
    /// Each category group is in chronological insertion order. Observations
    /// within a group are deduplicated by description — a second identical
    /// observation strengthens confidence rather than appending a duplicate.
    ///
    /// `BTreeMap` gives deterministic iteration order (important for
    /// save/load reproducibility and test stability).
    pub observations:
        BTreeMap<crate::journal::ObservationCategory, Vec<crate::journal::Observation>>,
}

// ── ConceptNode observation methods ──────────────────────────────────────

impl ConceptNode {
    /// Construct a bare `ConceptNode` with no observations.
    ///
    /// Used by tests that need a `ConceptNode` value without going through
    /// the full `KnowledgeGraph` machinery.
    pub fn new(key: JournalKey, name: &str, tick: u64) -> Self {
        let category = key.concept_category();
        ConceptNode {
            id: ConceptId(key),
            category,
            name: name.to_string(),
            confidence: Confidence::new(0.3),
            first_observed_at: tick,
            last_updated_at: tick,
            origin_planet_seed: None,
            revealed_properties: HashMap::new(),
            observations: BTreeMap::new(),
        }
    }

    /// Record an observation on this node, deduplicating by description within
    /// the same category.
    ///
    /// When an observation with the same category **and** the same description
    /// already exists, the duplicate is not appended — instead the existing
    /// observation's confidence is upgraded to the higher of the two values and
    /// `last_updated_at` is advanced. This prevents the node from bloating when
    /// systems repeatedly report the same finding (e.g. picking up the same
    /// material multiple times).
    ///
    /// When the observation is genuinely new (different category or different
    /// description), it is appended to the appropriate category group and
    /// `last_updated_at` is advanced unconditionally.
    pub fn add_observation(&mut self, observation: crate::journal::Observation) {
        self.last_updated_at = observation.recorded_at;

        let group = self
            .observations
            .entry(observation.category.clone())
            .or_default();

        if let Some(existing) = group
            .iter_mut()
            .find(|o| o.description == observation.description)
        {
            // Upgrade confidence if the new evidence is stronger.
            if observation.confidence.value() > existing.confidence.value() {
                existing.confidence = observation.confidence;
            }
            return;
        }

        group.push(observation);
    }

    /// Record an observation with confidence accumulation for existing
    /// observations.
    ///
    /// When an observation with the same category and description already
    /// exists, [`Confidence::accumulate`] is called with
    /// `base_observation_weight * recovery_multiplier`. This gives diminishing
    /// returns as evidence builds, matching Story 10.4's confidence model.
    ///
    /// `recovery_multiplier` should be:
    /// - `> 1.0` when the player is engaging with the domain they died in
    ///   (faster recovery)
    /// - `< 1.0` for unrelated domains after a death
    /// - `= 1.0` for normal play without death context
    ///
    /// When the observation is genuinely new it is appended as-is.
    pub fn add_observation_with_accumulation(
        &mut self,
        observation: crate::journal::Observation,
        config: &crate::observation::ConfidenceConfig,
    ) {
        self.add_observation_with_domain_weighted_accumulation(observation, config, 1.0);
    }

    /// When the observation is genuinely new it is appended as-is.
    pub fn add_observation_with_domain_weighted_accumulation(
        &mut self,
        observation: crate::journal::Observation,
        config: &crate::observation::ConfidenceConfig,
        recovery_multiplier: f32,
    ) {
        self.last_updated_at = observation.recorded_at;

        let group = self
            .observations
            .entry(observation.category.clone())
            .or_default();

        if let Some(existing) = group
            .iter_mut()
            .find(|o| o.description == observation.description)
        {
            let adjusted_weight = config.base_observation_weight * recovery_multiplier;
            existing.confidence.accumulate(adjusted_weight);
            return;
        }

        group.push(observation);
    }

    /// Return all observations for a given category, in insertion order.
    ///
    /// Returns an empty slice when no observations for that category exist yet.
    pub fn observations_by_category(
        &self,
        category: &crate::journal::ObservationCategory,
    ) -> &[crate::journal::Observation] {
        self.observations
            .get(category)
            .map_or(&[], |v| v.as_slice())
    }

    /// Total observation count across all categories.
    pub fn observation_count(&self) -> usize {
        self.observations.values().map(|v| v.len()).sum()
    }

    /// Iterator over all observations across all categories, in deterministic
    /// category order (driven by `BTreeMap`) then insertion order within each
    /// category.
    pub fn all_observations(&self) -> impl Iterator<Item = &crate::journal::Observation> {
        self.observations.values().flat_map(|v| v.iter())
    }
}

// ── Relationship types ────────────────────────────────────────────────────

/// Types of relationships between journal subjects.
///
/// Every edge in the knowledge graph is one of these typed relationships.
/// The set is exhaustive for Story 10.5 and deliberately extensible —
/// future systems (language, trade, culture) add variants here without
/// touching existing match arms.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub enum RelationshipType {
    /// A material was found on a specific planet or location.
    FoundOn,
    /// Two materials were combined in the fabricator.
    CombinedWith,
    /// An output material was derived from one or more input materials.
    DerivedFrom,
    /// Two materials share similar measured properties above the similarity
    /// threshold. Only created when both materials are at `Observed` tier
    /// or above so the player has earned the insight.
    SimilarTo,
    /// An observation was made at a specific location (weaker than `FoundOn`
    /// — records a connection without implying the subject originated there).
    ObservedAt,
    // Future: SpokenBy, TradedAt, UsedIn, etc.
}

impl RelationshipType {
    /// Player-facing label used in the journal "Related" section header
    /// for each cross-reference link.
    ///
    /// These labels are deliberately factual and terse — no flavour text.
    /// The player draws their own conclusions from the connection.
    pub fn display_label(&self) -> &'static str {
        match self {
            RelationshipType::FoundOn => "Found on",
            RelationshipType::CombinedWith => "Combined with",
            RelationshipType::DerivedFrom => "Derived from",
            RelationshipType::SimilarTo => "Similar to",
            RelationshipType::ObservedAt => "Observed at",
        }
    }
}

// ── Graph edge ────────────────────────────────────────────────────────────

/// A typed, confidence-bearing relationship between two concept nodes.
///
/// Edges are directional in petgraph but are interpreted as bidirectional
/// by [`KnowledgeGraph::relationships`], which walks both incoming and
/// outgoing edges. This keeps storage at half the edges while giving the
/// UI the "bidirectional links" behaviour specified in the acceptance
/// criteria.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptEdge {
    /// The nature of the relationship between the two concepts.
    pub relationship: RelationshipType,
    /// How certain the player is that this relationship is real, accumulated
    /// each time a new observation confirms the same connection.
    pub confidence: Confidence,
    /// Game-time tick (whole seconds elapsed) when this relationship was
    /// first established.
    pub discovered_at: u64,
}

// ── Knowledge graph resource ──────────────────────────────────────────────

/// The player's knowledge graph — per architecture decision, backed by
/// [`petgraph::Graph`].
///
/// The graph is a directed multigraph: each concept is a node and each
/// observed relationship is an edge. Most queries treat edges as
/// undirected (relationships are symmetric in the player's mental model)
/// but petgraph stores them directed so we can reach them efficiently with
/// `edges_directed` in both directions.
///
/// Three indexes give O(1) lookups on top of the O(log n) petgraph
/// internal structures:
///
/// * `concept_index` — `ConceptId → NodeIndex` for idempotent node creation
///   and edge wiring.
/// * `category_index` — `ConceptCategory → Vec<NodeIndex>` for encyclopedia
///   view ("all known materials") and BFS filtering.
/// * `timeline` — `(tick, NodeIndex)` pairs in discovery order for the event
///   log view.
///
/// **Serialization strategy:** `petgraph::Graph` serializes cleanly with the
/// `serde-1` feature. The three indexes are rebuilt from the graph on
/// deserialization — they are not stored explicitly — ensuring the indexes
/// always stay consistent with the canonical graph data.
#[derive(Resource, Clone, Debug, Default)]
pub struct KnowledgeGraph {
    /// Core petgraph directed graph with typed nodes and edges.
    graph: Graph<ConceptNode, ConceptEdge>,
    /// Primary index: `ConceptId → NodeIndex` for O(1) lookups.
    concept_index: HashMap<ConceptId, NodeIndex>,
    /// Category index: `ConceptCategory → Vec<NodeIndex>` for encyclopedia view.
    category_index: HashMap<ConceptCategory, Vec<NodeIndex>>,
    /// Timeline of discoveries in insertion order (monotone by tick).
    timeline: Vec<(u64, NodeIndex)>,
}

impl KnowledgeGraph {
    // ── Node operations ───────────────────────────────────────────────────

    /// Get or create a concept node and return its [`NodeIndex`].
    ///
    /// Idempotent: calling this twice with the same `id` returns the same
    /// `NodeIndex` and does NOT overwrite the existing node's data. The
    /// node's `discovered_at` and `confidence` are set only on creation.
    ///
    /// Maintains [`Self::concept_index`], [`Self::category_index`], and
    /// [`Self::timeline`] atomically — all three are always in sync.
    pub fn ensure_concept(
        &mut self,
        id: ConceptId,
        category: ConceptCategory,
        tick: u64,
    ) -> NodeIndex {
        if let Some(&existing) = self.concept_index.get(&id) {
            return existing;
        }

        let node = ConceptNode {
            id: id.clone(),
            category: category.clone(),
            name: String::new(),
            confidence: Confidence::new(0.3), // Start at Tentative/Observed boundary
            first_observed_at: tick,
            last_updated_at: tick,
            origin_planet_seed: None,
            revealed_properties: HashMap::new(),
            observations: BTreeMap::new(),
        };

        let idx = self.graph.add_node(node);
        self.concept_index.insert(id, idx);
        self.category_index.entry(category).or_default().push(idx);
        self.timeline.push((tick, idx));
        idx
    }

    /// Look up a concept by its [`ConceptId`].
    ///
    /// Returns `None` when the concept has not yet been observed. Callers
    /// use [`ensure_concept`](Self::ensure_concept) to create on first
    /// encounter and `lookup` only when they need to assert existence.
    pub fn lookup(&self, id: &ConceptId) -> Option<NodeIndex> {
        self.concept_index.get(id).copied()
    }

    /// Get a reference to the node data for a given index.
    ///
    /// Returns `None` if the index is invalid (should not happen during
    /// normal gameplay since indexes are never removed).
    pub fn node(&self, idx: NodeIndex) -> Option<&ConceptNode> {
        self.graph.node_weight(idx)
    }

    /// Mutable access to a concept node by index.
    ///
    /// Used by test helpers and save-migration code that need to directly
    /// patch a node's fields. Production write paths go through the message
    /// pipeline and `update_knowledge_graph`.
    pub fn node_mut(&mut self, idx: NodeIndex) -> Option<&mut ConceptNode> {
        self.graph.node_weight_mut(idx)
    }

    /// Count of nodes that have a non-empty name.
    ///
    /// Location nodes and any concept created before its first `RecordObservation`
    /// arrives have an empty `name`; the journal list panel skips those.
    /// This count mirrors the entry count the old `journal.entries.len()` produced.
    pub fn named_node_count(&self) -> usize {
        self.graph
            .node_indices()
            .filter_map(|idx| self.graph.node_weight(idx))
            .filter(|n| !n.name.is_empty())
            .count()
    }

    /// Count of named nodes that pass a journal filter.
    pub fn named_node_count_filtered(&self, filter: &crate::journal::JournalFilter) -> usize {
        self.graph
            .node_indices()
            .filter_map(|idx| self.graph.node_weight(idx))
            .filter(|n| !n.name.is_empty())
            .filter(|n| crate::journal::matches_filter_node(n, filter))
            .count()
    }

    /// Record a named observation on a concept node.  Test helper for any test
    /// that used to call `journal.record(key, name, obs)`.
    ///
    /// Creates the concept if it doesn't exist, stamps the name on first call,
    /// and delegates to [`ConceptNode::add_observation`].
    pub fn record(
        &mut self,
        key: crate::journal::JournalKey,
        name: &str,
        observation: crate::journal::Observation,
    ) {
        let category = key.concept_category();
        let tick = observation.recorded_at;
        let id = ConceptId(key);
        let idx = self.ensure_concept(id, category, tick);
        let node = self.graph.node_weight_mut(idx).expect("just created");
        if node.name.is_empty() {
            node.name = name.to_string();
        }
        node.add_observation(observation);
    }

    /// Convenience: record with domain-weighted accumulation. Test helper.
    pub fn record_with_accumulation(
        &mut self,
        key: crate::journal::JournalKey,
        name: &str,
        observation: crate::journal::Observation,
        config: &crate::observation::ConfidenceConfig,
    ) {
        self.record_with_domain_weighted_accumulation(key, name, observation, config, 1.0);
    }

    /// Convenience: record with domain-weighted accumulation. Test helper.
    pub fn record_with_domain_weighted_accumulation(
        &mut self,
        key: crate::journal::JournalKey,
        name: &str,
        observation: crate::journal::Observation,
        config: &crate::observation::ConfidenceConfig,
        recovery_multiplier: f32,
    ) {
        let category = key.concept_category();
        let tick = observation.recorded_at;
        let id = ConceptId(key);
        let idx = self.ensure_concept(id, category, tick);
        let node = self.graph.node_weight_mut(idx).expect("just created");
        if node.name.is_empty() {
            node.name = name.to_string();
        }
        node.add_observation_with_domain_weighted_accumulation(
            observation,
            config,
            recovery_multiplier,
        );
    }

    /// Remove a concept node by key. Returns `true` if the node existed.
    ///
    /// Used by test helpers that simulate deletion. The internal indexes
    /// are updated; edges connected to the removed node are dropped by petgraph.
    pub fn remove(&mut self, key: &crate::journal::JournalKey) {
        let id = ConceptId(key.clone());
        let Some(idx) = self.concept_index.remove(&id) else {
            return;
        };
        // Remove from category index.
        if let Some(cat_vec) = self
            .category_index
            .get_mut(&self.graph[idx].category.clone())
        {
            cat_vec.retain(|&i| i != idx);
        }
        // Remove from timeline.
        self.timeline.retain(|(_, i)| *i != idx);
        // Remove from graph (also removes connected edges).
        self.graph.remove_node(idx);
        // NOTE: removing a node invalidates the NodeIndex values stored in
        // concept_index for nodes with higher internal indices.  Rebuild the
        // concept_index from scratch to keep it consistent.
        let mut new_index = std::collections::HashMap::new();
        for (id_key, &stored_idx) in &self.concept_index {
            new_index.insert(id_key.clone(), stored_idx);
        }
        // Actually petgraph's `remove_node` uses swap_remove semantics, so
        // re-scan the graph to rebuild.
        let mut rebuilt: std::collections::HashMap<ConceptId, NodeIndex> =
            std::collections::HashMap::new();
        let mut rebuilt_cat: std::collections::HashMap<ConceptCategory, Vec<NodeIndex>> =
            std::collections::HashMap::new();
        let mut rebuilt_tl: Vec<(u64, NodeIndex)> = Vec::new();
        for ni in self.graph.node_indices() {
            let n = &self.graph[ni];
            rebuilt.insert(n.id.clone(), ni);
            rebuilt_cat.entry(n.category.clone()).or_default().push(ni);
            rebuilt_tl.push((n.first_observed_at, ni));
        }
        rebuilt_tl.sort_by_key(|(t, _)| *t);
        self.concept_index = rebuilt;
        self.category_index = rebuilt_cat;
        self.timeline = rebuilt_tl;
    }

    // ── Edge operations ───────────────────────────────────────────────────

    /// Add or strengthen a relationship between two concepts.
    ///
    /// If an edge of the same [`RelationshipType`] already exists between
    /// `from` and `to`, its confidence is accumulated (diminishing returns)
    /// rather than a duplicate edge being added. This satisfies the
    /// acceptance criterion that "cross-references accumulate — the same
    /// relationship strengthens with repeated evidence".
    ///
    /// Direction: `from → to`. [`Self::relationships`] exposes both
    /// directions to callers, so the UI sees the edge regardless of which
    /// side it queries from.
    pub fn relate(&mut self, from: NodeIndex, to: NodeIndex, edge: ConceptEdge) {
        // Look for an existing edge of the same type to strengthen.
        let existing = self
            .graph
            .edges_connecting(from, to)
            .find(|e| e.weight().relationship == edge.relationship)
            .map(|e| e.id());

        if let Some(edge_id) = existing {
            // Accumulate evidence rather than duplicating.
            if let Some(existing_edge) = self.graph.edge_weight_mut(edge_id) {
                existing_edge.confidence.accumulate(0.2);
            }
        } else {
            self.graph.add_edge(from, to, edge);
        }
    }

    /// All relationships for a concept, in both directions.
    ///
    /// Returns `(neighbor_index, edge)` pairs for every edge incident on
    /// `node` whether it points outward (the concept is the `from` side)
    /// or inward (it is the `to` side). This gives the caller a
    /// bidirectional view without storing duplicate edges in the graph.
    ///
    /// The order follows petgraph's stable edge-list iteration, which is
    /// insertion order within each direction for the same node.
    pub fn relationships(&self, node: NodeIndex) -> Vec<(NodeIndex, &ConceptEdge)> {
        let outgoing = self
            .graph
            .edges_directed(node, Direction::Outgoing)
            .map(|e| (e.target(), e.weight()));

        let incoming = self
            .graph
            .edges_directed(node, Direction::Incoming)
            .map(|e| (e.source(), e.weight()));

        outgoing.chain(incoming).collect()
    }

    // ── Category queries ──────────────────────────────────────────────────

    /// All concept node indexes in a given category.
    ///
    /// Returns an empty slice when no concepts of that category have been
    /// registered yet. Insertion order within each category matches the
    /// order in which [`ensure_concept`](Self::ensure_concept) was first
    /// called.
    pub fn by_category(&self, category: &ConceptCategory) -> &[NodeIndex] {
        self.category_index
            .get(category)
            .map_or(&[], |v| v.as_slice())
    }

    // ── Timeline ─────────────────────────────────────────────────────────

    /// Look up a material concept node by seed alone, ignoring `planet_seed`.
    ///
    /// Returns the first [`NodeIndex`] found whose [`ConceptId`] wraps a
    /// [`crate::journal::JournalKey::Material`] with the given `seed`, regardless
    /// of what `planet_seed` that node was stored with.
    ///
    /// This exists to resolve the "identity mismatch" where a material is first
    /// observed on a planet (key = `Material { seed: X, planet_seed: Some(Y) }`) but
    /// is later referenced via fabrication input with `planet_seed: None`. Using
    /// `lookup` directly would create a second, disconnected node — this method
    /// prevents that by finding the existing node by seed.
    ///
    /// Returns `None` when no material with that seed exists in the graph yet.
    pub fn lookup_material_by_seed(
        &self,
        seed: crate::materials::MaterialSeed,
    ) -> Option<NodeIndex> {
        self.concept_index.iter().find_map(|(id, &idx)| {
            if matches!(&id.0, crate::journal::JournalKey::MaterialInstance { seed: s } if *s == seed.0) {
                Some(idx)
            } else {
                None
            }
        })
    }

    /// Returns the concept node at the given index, or `None` if the index
    /// is invalid.  Exposes read-only access to node data without making
    /// the internal `petgraph::Graph` field public.
    pub fn node_weight(&self, idx: NodeIndex) -> Option<&ConceptNode> {
        self.graph.node_weight(idx)
    }

    /// Mark a property as revealed on the given concept node.
    ///
    /// Called each time the player makes an observation of a specific category
    /// on this concept. The `property_name` should be the
    /// Record that the player has observed a specific property on this concept
    /// and store the raw measured value.
    ///
    /// `category` is the typed observation kind; `value` is the underlying
    /// float from [`crate::materials::GameMaterial`] at the moment of
    /// revelation — stored so the classification system can compare against
    /// asset-defined ranges without reaching back into world entities.
    ///
    /// No-ops when the node index is invalid.
    pub fn reveal_property(
        &mut self,
        node: NodeIndex,
        category: crate::journal::ObservationCategory,
        value: f32,
    ) {
        if let Some(n) = self.graph.node_weight_mut(node) {
            n.revealed_properties.insert(category, value);
        }
    }

    /// Mutable access to the underlying petgraph `Graph` — exposed only for
    /// test code that needs to directly populate node fields (e.g. seeding
    /// confidence or observation data without going through the message
    /// pipeline). Production code should use the typed accessors instead.
    ///
    /// Marked `pub` because the observation module's tests are in a sibling
    /// module and need to seed graph state for death-degradation assertions.
    pub fn graph_mut(&mut self) -> &mut Graph<ConceptNode, ConceptEdge> {
        &mut self.graph
    }
    /// Timeline of all discovered concepts in chronological order.
    ///
    /// Each entry is `(discovered_at_tick, node_index)`. Because
    /// [`ensure_concept`](Self::ensure_concept) only appends to this
    /// vec and the game clock is monotonically increasing, the timeline
    /// is guaranteed to be in non-decreasing tick order.
    pub fn timeline(&self) -> &[(u64, NodeIndex)] {
        &self.timeline
    }

    /// All concept nodes sorted by name, for stable alphabetical display in the journal.
    ///
    /// Returns an empty vec when the graph has no nodes yet. Nodes with empty
    /// names (location concepts that have never had a name stamped) sort to the
    /// front — the UI skips them via `name.is_empty()` checks.
    pub fn nodes_sorted_by_name(&self) -> Vec<NodeIndex> {
        let mut pairs: Vec<(NodeIndex, &str)> = self
            .graph
            .node_indices()
            .filter_map(|idx| self.graph.node_weight(idx))
            .filter(|n| !n.name.is_empty())
            // Location nodes are not player-facing journal entries — they exist
            // only as cross-reference targets for FoundOn/ObservedAt edges.
            // Exclude them from the sorted list so they don't appear in the
            // main journal entry panel.
            .filter(|n| n.category != ConceptCategory::Location)
            .map(|n| {
                let idx = self.concept_index[&n.id];
                (idx, n.name.as_str())
            })
            .collect();
        pairs.sort_by_key(|(_, a)| *a);
        pairs.into_iter().map(|(idx, _)| idx).collect()
    }

    /// All concept nodes in a given category, sorted by name.
    ///
    /// Convenience wrapper over [`by_category`](Self::by_category) +
    /// [`node`](Self::node) that also sorts alphabetically — the same
    /// ordering used by the journal list panel.
    pub fn nodes_in_category_sorted_by_name(&self, category: &ConceptCategory) -> Vec<NodeIndex> {
        let mut pairs: Vec<(NodeIndex, &str)> = self
            .by_category(category)
            .iter()
            .filter_map(|&idx| self.graph.node_weight(idx).map(|n| (idx, n.name.as_str())))
            .collect();
        pairs.sort_by_key(|(_, a)| *a);
        pairs.into_iter().map(|(idx, _)| idx).collect()
    }

    /// Apply death confidence degradation to every observation and node
    /// confidence in the graph.
    ///
    /// Called by the observation system's `handle_player_death` handler.
    /// Iterates all nodes and degrades both the per-observation confidence
    /// and the overall node confidence using the same factor/floor values
    /// loaded from `assets/config/confidence.toml`.
    pub fn degrade_all(&mut self, factor: f32, floor: f32) {
        for idx in self.graph.node_indices().collect::<Vec<_>>() {
            if let Some(node) = self.graph.node_weight_mut(idx) {
                node.confidence.degrade(factor, floor);
                for obs_group in node.observations.values_mut() {
                    for obs in obs_group.iter_mut() {
                        obs.confidence.degrade(factor, floor);
                    }
                }
            }
        }
    }

    /// Bounded BFS from a center node, returning `(node_index, depth)`
    /// pairs for all reachable nodes within `depth` hops.
    ///
    /// The center node itself is NOT included in the result — callers
    /// already have it. Edges are traversed in both directions (the graph
    /// is treated as undirected for BFS purposes), so the result is a
    /// neighbourhood in the graph-theoretic sense rather than a directed
    /// reachability set.
    ///
    /// # Arguments
    ///
    /// - `center` — Starting node index.
    /// - `depth` — Maximum hop count. `depth = 1` returns only direct neighbours;
    ///   `depth = 0` returns an empty vec (the center is excluded).
    /// - `category_filter` — When `Some(category)`, only nodes that belong to that
    ///   category are included in the result. Filtered-out nodes still participate in
    ///   BFS traversal so their neighbours can be reached — the filter is applied at
    ///   result collection time, not during graph traversal.
    ///
    /// # Cycle safety
    ///
    /// BFS uses a `visited` set to prevent re-queuing already-seen nodes,
    /// so cycles in the graph never cause infinite loops.
    pub fn neighborhood(
        &self,
        center: NodeIndex,
        depth: usize,
        category_filter: Option<&ConceptCategory>,
    ) -> Vec<(NodeIndex, usize)> {
        if depth == 0 {
            return Vec::new();
        }

        let mut visited: HashSet<NodeIndex> = HashSet::new();
        let mut queue: VecDeque<(NodeIndex, usize)> = VecDeque::new();
        let mut result: Vec<(NodeIndex, usize)> = Vec::new();

        visited.insert(center);
        queue.push_back((center, 0));

        while let Some((node, current_depth)) = queue.pop_front() {
            if current_depth >= depth {
                continue;
            }

            // Walk both outgoing and incoming edges (undirected BFS).
            let neighbors: Vec<NodeIndex> = self
                .graph
                .edges_directed(node, Direction::Outgoing)
                .map(|e| e.target())
                .chain(
                    self.graph
                        .edges_directed(node, Direction::Incoming)
                        .map(|e| e.source()),
                )
                .collect();

            for neighbor in neighbors {
                if visited.contains(&neighbor) {
                    continue;
                }
                visited.insert(neighbor);
                let neighbor_depth = current_depth + 1;
                queue.push_back((neighbor, neighbor_depth));

                // Apply category filter at collection time, not traversal time.
                let include = category_filter.is_none_or(|cat| {
                    self.graph
                        .node_weight(neighbor)
                        .is_some_and(|n| &n.category == cat)
                });

                if include {
                    result.push((neighbor, neighbor_depth));
                }
            }
        }

        result
    }
}

// ── Serialization ─────────────────────────────────────────────────────────

/// Serializable snapshot of the graph, used for save/load.
///
/// We serialize the full node and edge data from petgraph directly (via the
/// `serde-1` feature), then rebuild the three in-memory indexes on
/// deserialization. This avoids storing redundant index data that could
/// drift out of sync with the graph.
///
/// `petgraph::Graph` serializes as `{ nodes: [...], edges: [...] }` which
/// is stable across petgraph minor versions, matching the versioning
/// contract in the asset-pipeline architecture decision.
impl Serialize for KnowledgeGraph {
    fn serialize<S: serde::Serializer>(&self, serializer: S) -> Result<S::Ok, S::Error> {
        // Delegate entirely to petgraph's built-in serde support.
        self.graph.serialize(serializer)
    }
}

impl<'de> Deserialize<'de> for KnowledgeGraph {
    fn deserialize<D: serde::Deserializer<'de>>(deserializer: D) -> Result<Self, D::Error> {
        // Restore the petgraph graph from saved data.
        let graph: Graph<ConceptNode, ConceptEdge> = Graph::deserialize(deserializer)?;

        // Rebuild all three indexes from the restored graph — they are
        // derivable from the node data so we never store them separately,
        // eliminating any risk of index/graph divergence across save files.
        let mut concept_index = HashMap::new();
        let mut category_index: HashMap<ConceptCategory, Vec<NodeIndex>> = HashMap::new();
        let mut timeline: Vec<(u64, NodeIndex)> = Vec::new();

        for idx in graph.node_indices() {
            if let Some(node) = graph.node_weight(idx) {
                concept_index.insert(node.id.clone(), idx);
                category_index
                    .entry(node.category.clone())
                    .or_default()
                    .push(idx);
                timeline.push((node.first_observed_at, idx));
            }
        }

        // Restore chronological order — petgraph node iteration order is
        // insertion order (stable), but after deserialization we cannot
        // assume the original tick order was preserved without sorting.
        timeline.sort_by_key(|(tick, _)| *tick);

        Ok(KnowledgeGraph {
            graph,
            concept_index,
            category_index,
            timeline,
        })
    }
}

// ── Automatic cross-reference system ─────────────────────────────────────

/// Processes [`crate::observation::RecordObservation`] messages and populates
/// the [`KnowledgeGraph`] with typed relationship edges.
///
/// As of Story 387 this is the **sole** observation write path.
///
/// # Edges created
///
/// * **`FoundOn`** — when a `Material` observation carries a `planet_seed`
///   in its key, a `material → location` edge is created.
/// * **`DerivedFrom`** — for `Fabrication` observations, an edge is created
///   from the output concept to each input material listed in
///   [`RecordObservation::input_seeds`].
/// * **`ObservedAt`** — when the observation message includes a
///   [`RecordObservation::context_location`], an `ObservedAt` edge is
///   created from subject → location.
///
/// Processes [`crate::observation::RecordObservation`] messages, populates the
/// [`KnowledgeGraph`] with observation data, and wires typed relationship edges.
///
/// This system is the **sole write path** for observation storage as of Story 387.
/// The journal's `apply_observations` system has been removed; all data lives here.
///
/// Runs in [`crate::journal::JournalSet::Navigate`].
///
/// # What it does per message
///
/// 1. Ensures the subject concept node exists.
/// 2. Stamps the display `name` on first encounter (first-observer-wins).
/// 3. Stamps `origin_planet_seed` on first planet-seeded observation.
/// 4. Records the observation onto the node with domain-weighted confidence
///    accumulation (mirrors the old `JournalEntry` accumulation logic).
/// 5. Marks the property category as revealed on the node.
/// 6. Wires graph edges: `FoundOn`, `DerivedFrom`, `CombinedWith`, `ObservedAt`.
fn update_knowledge_graph(
    mut reader: MessageReader<crate::observation::RecordObservation>,
    mut graph: ResMut<KnowledgeGraph>,
    time: Res<Time>,
    config: Res<ConfidenceConfig>,
    catalog: Option<Res<crate::materials::MaterialCatalog>>,
) {
    let tick = time.elapsed().as_millis() as u64;

    // Drain messages into a vec first so we can release the reader borrow
    // before mutably borrowing the graph.
    let messages: Vec<_> = reader.read().cloned().collect();

    for obs in messages {
        // Determine category for the observed subject.
        let subject_category = category_from_key(&obs.key);

        // Ensure the subject node exists.
        let subject_node = graph.ensure_concept(ConceptId(obs.key.clone()), subject_category, tick);

        // ── Stamp name on first encounter ──────────────────────────────
        // First-observer-wins: never overwrite an existing name.
        if let Some(node) = graph.graph.node_weight_mut(subject_node) {
            if node.name.is_empty() {
                node.name = obs.name.clone();
            }

            // ── Stamp origin_planet_seed on first planet observation ───
            if node.origin_planet_seed.is_none()
                && let Some(ps) = obs.planet_seed
            {
                node.origin_planet_seed = Some(ps);
            }

            // ── Record observation with domain-weighted accumulation ───
            // Using accumulation (not simple add) so repeated identical
            // observations strengthen confidence with diminishing returns,
            // matching the Story 10.4 confidence model.
            // Recovery multiplier is 1.0 here; DeathContext-adjusted writes
            // come from the observation sites that know the death domain.
            let mut observation = obs.observation.clone();
            observation.recorded_at = tick;
            node.add_observation_with_domain_weighted_accumulation(observation, &config, 1.0);

            // ── Accumulate overall node confidence ────────────────────
            node.confidence
                .accumulate(obs.observation.confidence.value());

            // ── Mark property as revealed, storing the raw float value ──
            // Look up the material's property value from the catalog so the
            // classification system can compare against asset-defined ranges
            // without reaching back into world entities at query time.
            let category = obs.observation.category.clone();
            let prop_value: f32 = obs
                .material_seed
                .and_then(|seed| catalog.as_ref()?.get_by_seed(seed))
                .map(|mat| property_value_for_category(mat, &category))
                .unwrap_or(0.0);
            node.revealed_properties.insert(category, prop_value);
        }

        // ── FoundOn edge ──────────────────────────────────────────────
        // If the observation carries a planet_seed, wire a FoundOn edge
        // from the material to the location concept.
        if let Some(planet_seed) = obs.planet_seed {
            let location_key = crate::journal::JournalKey::Location { planet_seed };
            let location_node =
                graph.ensure_concept(ConceptId(location_key), ConceptCategory::Location, tick);
            graph.relate(
                subject_node,
                location_node,
                ConceptEdge {
                    relationship: RelationshipType::FoundOn,
                    confidence: obs.observation.confidence,
                    discovered_at: tick,
                },
            );
        }

        // ── DerivedFrom edges ────────────────────────────────────────
        // For fabrication outputs, link the output concept back to each
        // input material that the player put into the fabricator.
        if matches!(&obs.key, crate::journal::JournalKey::Fabrication { .. }) {
            for &input_seed in &obs.input_seeds {
                let input_node = graph
                    .lookup_material_by_seed(input_seed)
                    .unwrap_or_else(|| {
                        let input_key =
                            crate::journal::JournalKey::MaterialInstance { seed: input_seed.0 };
                        graph.ensure_concept(ConceptId(input_key), ConceptCategory::Material, tick)
                    });
                graph.relate(
                    subject_node,
                    input_node,
                    ConceptEdge {
                        relationship: RelationshipType::DerivedFrom,
                        // Fabrication is directly observed — full confidence.
                        confidence: Confidence::new(1.0),
                        discovered_at: tick,
                    },
                );
            }
        }

        // ── CombinedWith edges ───────────────────────────────────────
        // For fabrication outputs with exactly 2 inputs, wire a symmetric
        // CombinedWith edge between the two input materials so the player
        // can see that these materials were used together, independent of
        // what they produced.
        if matches!(&obs.key, crate::journal::JournalKey::Fabrication { .. })
            && obs.input_seeds.len() == 2
        {
            let seed_a = obs.input_seeds[0];
            let seed_b = obs.input_seeds[1];

            let node_a = graph.lookup_material_by_seed(seed_a).unwrap_or_else(|| {
                let key = crate::journal::JournalKey::MaterialInstance { seed: seed_a.0 };
                graph.ensure_concept(ConceptId(key), ConceptCategory::Material, tick)
            });
            let node_b = graph.lookup_material_by_seed(seed_b).unwrap_or_else(|| {
                let key = crate::journal::JournalKey::MaterialInstance { seed: seed_b.0 };
                graph.ensure_concept(ConceptId(key), ConceptCategory::Material, tick)
            });

            let edge = ConceptEdge {
                relationship: RelationshipType::CombinedWith,
                confidence: Confidence::new(1.0),
                discovered_at: tick,
            };
            graph.relate(node_a, node_b, edge);
        }

        // ── ObservedAt edge ──────────────────────────────────────────
        // When the observation message supplies an explicit context
        // location (e.g. a biome landmark), wire a subject→location edge.
        if let Some(context_location) = &obs.context_location {
            let location_category = category_from_key(context_location);
            let location_node =
                graph.ensure_concept(ConceptId(context_location.clone()), location_category, tick);
            graph.relate(
                subject_node,
                location_node,
                ConceptEdge {
                    relationship: RelationshipType::ObservedAt,
                    confidence: obs.observation.confidence,
                    discovered_at: tick,
                },
            );
        }
    }
}

/// Map a [`JournalKey`] discriminant to the corresponding
/// [`ConceptCategory`].
///
/// This mapping is the single place where the journal and knowledge-graph
/// category systems are joined. Adding a new `JournalKey` variant will
/// produce a non-exhaustive match error here, forcing the developer to
/// declare the correct category.
fn category_from_key(key: &crate::journal::JournalKey) -> ConceptCategory {
    match key {
        crate::journal::JournalKey::MaterialInstance { .. } => ConceptCategory::Material,
        crate::journal::JournalKey::Material { .. } => ConceptCategory::Material,
        crate::journal::JournalKey::Fabrication { .. } => ConceptCategory::Fabrication,
        crate::journal::JournalKey::Location { .. } => ConceptCategory::Location,
    }
}

/// Extract the raw property float for a given observation category from a
/// [`crate::materials::GameMaterial`].
///
/// This is the bridge between the typed `ObservationCategory` enum and the
/// named fields of `GameMaterial` — used by `update_knowledge_graph` to store
/// observed property values on `ConceptNode::revealed_properties` without
/// string keys. `SurfaceAppearance`, `FabricationResult`, and `LocationNote`
/// have no direct float analogue, so they return `0.0`.
fn property_value_for_category(
    mat: &crate::materials::GameMaterial,
    category: &crate::journal::ObservationCategory,
) -> f32 {
    use crate::journal::ObservationCategory;
    match category {
        ObservationCategory::Weight => mat.density.value(),
        ObservationCategory::ThermalBehavior => mat.thermal_resistance.value(),
        // Reactivity, conductivity, and toxicity observation systems are not
        // wired yet (Story N.4+). Return 0.0 as a safe sentinel — the value
        // will be overwritten when those systems fire a RecordObservation.
        ObservationCategory::SurfaceAppearance => mat.reactivity.value(),
        ObservationCategory::FabricationResult => 0.0,
        ObservationCategory::LocationNote => 0.0,
        // Exploitation observations are about vehicle/tool usage events — no
        // direct float property from GameMaterial applies.
        ObservationCategory::Exploitation => 0.0,
    }
}

// ── SimilarTo detection ───────────────────────────────────────────────────

/// Detect materials with similar property profiles and wire `SimilarTo`
/// edges in the knowledge graph when both materials are known with at least
/// `Observed` confidence.
///
/// Called by the material catalog after a new material is registered
/// (Story 10.5, Phase 4). The caller passes in the full catalog so we can
/// compare the new material against every previously-known entry.
///
/// # Arguments
///
/// - `new_seed` — The seed of the newly registered material.
/// - `new_material` — Reference to the [`crate::materials::GameMaterial`] that was just registered.
/// - `catalog` — The full [`crate::materials::MaterialCatalog`] for comparison. The new material is
///   already in it.
/// - `journal` — The player's [`crate::journal::Journal`] — used to check confidence tiers on both
///   materials.
/// - `graph` — Mutable reference to the [`KnowledgeGraph`] where `SimilarTo` edges are added.
/// - `threshold` — Cosine similarity threshold above which two materials are considered "similar"
///   (loaded from config, typically ~0.85).
/// - `tick` — Current game-time tick for edge timestamps.
pub fn detect_and_wire_similar_materials(
    new_seed: MaterialSeed,
    new_material: &crate::materials::GameMaterial,
    catalog: &crate::materials::MaterialCatalog,
    graph_read: &KnowledgeGraph,
    graph: &mut KnowledgeGraph,
    threshold: f32,
    tick: u64,
) {
    let new_vec = new_material.property_vector();

    for existing in catalog.values() {
        if existing.seed == new_seed {
            continue;
        }

        let sim = cosine_similarity(&new_vec, &existing.property_vector());
        if sim < threshold {
            continue;
        }

        let new_key = crate::journal::JournalKey::MaterialInstance { seed: new_seed.0 };
        let existing_key = crate::journal::JournalKey::MaterialInstance {
            seed: existing.seed.0,
        };

        let new_confident = is_at_least_observed(graph_read, &new_key);
        let existing_confident = is_at_least_observed(graph_read, &existing_key);

        if !new_confident || !existing_confident {
            continue;
        }

        let new_node =
            graph.ensure_concept(ConceptId(new_key.clone()), ConceptCategory::Material, tick);
        let existing_node =
            graph.ensure_concept(ConceptId(existing_key), ConceptCategory::Material, tick);

        let edge = ConceptEdge {
            relationship: RelationshipType::SimilarTo,
            confidence: Confidence::new(sim),
            discovered_at: tick,
        };
        graph.relate(new_node, existing_node, edge);
    }
}

/// Compute the cosine similarity between two property vectors.
///
/// Returns a value in [-1.0, 1.0] — in practice always [0.0, 1.0] for
/// non-negative property vectors derived from material seeds. A return
/// value of `1.0` means identical profiles; `0.0` means orthogonal
/// (no property overlap).
///
/// Returns `0.0` when either vector has zero magnitude (degenerate case
/// that cannot arise from [`crate::materials::GameMaterial::property_vector`]
/// since property values are in [0.0, 1.0] and seeds span the full range).
pub fn cosine_similarity(a: &[f32], b: &[f32]) -> f32 {
    debug_assert_eq!(
        a.len(),
        b.len(),
        "cosine_similarity: vectors must have the same length"
    );

    let dot: f32 = a.iter().zip(b.iter()).map(|(x, y)| x * y).sum();
    let mag_a: f32 = a.iter().map(|x| x * x).sum::<f32>().sqrt();
    let mag_b: f32 = b.iter().map(|x| x * x).sum::<f32>().sqrt();

    if mag_a == 0.0 || mag_b == 0.0 {
        return 0.0;
    }

    (dot / (mag_a * mag_b)).clamp(-1.0, 1.0)
}

/// Returns `true` when the graph contains a concept node for `key` with
/// overall confidence at or above [`crate::observation::ConfidenceTier::Observed`]
/// (i.e. confidence ≥ 0.3).
///
/// "At least Observed" is the threshold at which the player has
/// accumulated enough evidence to treat an observation as factual
/// rather than tentative. `SimilarTo` edges are only wired when both
/// materials meet this bar, ensuring the connection is earned.
///
/// Reads confidence from the KnowledgeGraph node — no Journal access needed.
fn is_at_least_observed(graph: &KnowledgeGraph, key: &crate::journal::JournalKey) -> bool {
    let id = ConceptId(key.clone());
    graph
        .lookup(&id)
        .and_then(|idx| graph.node(idx))
        .is_some_and(|node| node.confidence.value() >= 0.3)
}

/// Detects and wires `SimilarTo` edges for newly observed materials.
///
/// Runs in [`crate::journal::JournalSet::Navigate`] after
/// [`update_knowledge_graph`]. For each [`RecordObservation`] message that
/// carries a [`crate::journal::JournalKey::Material`] key, compares the
/// material against all others in the catalog and wires `SimilarTo` edges
/// when both materials are at or above `Observed` confidence and their
/// property vectors exceed the configured similarity threshold.
///
/// Uses an independent [`MessageReader`] cursor — both this system and
/// [`update_knowledge_graph`] receive all messages without consuming each
/// other's reads.
fn detect_similar_on_observation(
    mut reader: MessageReader<crate::observation::RecordObservation>,
    mut graph: ResMut<KnowledgeGraph>,
    catalog: Res<crate::materials::MaterialCatalog>,
    time: Res<Time>,
    config: Res<crate::observation::ConfidenceConfig>,
) {
    let tick = time.elapsed().as_secs();

    for obs in reader.read() {
        let crate::journal::JournalKey::MaterialInstance { seed } = obs.key else {
            continue;
        };
        let Some(material) = catalog.get_by_seed(crate::materials::MaterialSeed(seed)) else {
            continue;
        };
        detect_and_wire_similar_materials(
            MaterialSeed(seed),
            material,
            &catalog,
            &graph.as_ref().clone(),
            &mut graph,
            config.similarity_threshold,
            tick,
        );
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::journal::JournalKey;

    fn mat_id(seed: u64) -> ConceptId {
        ConceptId(JournalKey::MaterialInstance { seed })
    }

    fn loc_id(planet_seed: u64) -> ConceptId {
        ConceptId(JournalKey::Location {
            planet_seed: PlanetSeed(planet_seed),
        })
    }

    // ── Phase 1 tests ─────────────────────────────────────────────────

    #[test]
    fn ensure_concept_is_idempotent() {
        let mut graph = KnowledgeGraph::default();
        let id = mat_id(42);
        let idx1 = graph.ensure_concept(id.clone(), ConceptCategory::Material, 100);
        let idx2 = graph.ensure_concept(id.clone(), ConceptCategory::Material, 200);
        assert_eq!(idx1, idx2, "same ID must always return the same NodeIndex");
        assert_eq!(
            graph.timeline().len(),
            1,
            "idempotent call must not append to timeline"
        );
    }

    #[test]
    fn relate_creates_new_edge() {
        let mut graph = KnowledgeGraph::default();
        let a = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 0);
        let b = graph.ensure_concept(loc_id(999), ConceptCategory::Location, 0);
        graph.relate(
            a,
            b,
            ConceptEdge {
                relationship: RelationshipType::FoundOn,
                confidence: Confidence::new(0.5),
                discovered_at: 0,
            },
        );
        let rels = graph.relationships(a);
        assert_eq!(rels.len(), 1);
        assert_eq!(rels[0].1.relationship, RelationshipType::FoundOn);
    }

    #[test]
    fn relate_strengthens_existing_edge() {
        let mut graph = KnowledgeGraph::default();
        let a = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 0);
        let b = graph.ensure_concept(loc_id(999), ConceptCategory::Location, 0);

        let initial_edge = ConceptEdge {
            relationship: RelationshipType::FoundOn,
            confidence: Confidence::new(0.3),
            discovered_at: 0,
        };
        graph.relate(a, b, initial_edge.clone());

        // Second relate with same type should strengthen, not duplicate.
        graph.relate(a, b, initial_edge);
        let rels = graph.relationships(a);
        assert_eq!(rels.len(), 1, "no duplicate edge");
        assert!(
            rels[0].1.confidence.value() > 0.3,
            "confidence must increase on repeated observation"
        );
    }

    #[test]
    fn by_category_returns_correct_nodes() {
        let mut graph = KnowledgeGraph::default();
        let m1 = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 0);
        let m2 = graph.ensure_concept(mat_id(2), ConceptCategory::Material, 1);
        let _l1 = graph.ensure_concept(loc_id(1), ConceptCategory::Location, 2);

        let materials = graph.by_category(&ConceptCategory::Material);
        assert_eq!(materials.len(), 2);
        assert!(materials.contains(&m1));
        assert!(materials.contains(&m2));

        let locations = graph.by_category(&ConceptCategory::Location);
        assert_eq!(locations.len(), 1);
    }

    #[test]
    fn timeline_is_ordered_by_discovery_tick() {
        let mut graph = KnowledgeGraph::default();
        graph.ensure_concept(mat_id(10), ConceptCategory::Material, 100);
        graph.ensure_concept(mat_id(20), ConceptCategory::Material, 50);
        graph.ensure_concept(mat_id(30), ConceptCategory::Material, 200);

        // Timeline should be in insertion order (ticks 100, 50, 200 for
        // in-memory; deserialized would be sorted).
        let ticks: Vec<u64> = graph.timeline().iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![100, 50, 200]);
    }

    // ── Phase 2 tests ─────────────────────────────────────────────────

    #[test]
    fn bfs_depth_one_returns_only_direct_neighbors() {
        let mut graph = KnowledgeGraph::default();
        let center = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 0);
        let neighbor = graph.ensure_concept(mat_id(2), ConceptCategory::Material, 0);
        let far = graph.ensure_concept(mat_id(3), ConceptCategory::Material, 0);

        graph.relate(
            center,
            neighbor,
            ConceptEdge {
                relationship: RelationshipType::SimilarTo,
                confidence: Confidence::new(0.9),
                discovered_at: 0,
            },
        );
        graph.relate(
            neighbor,
            far,
            ConceptEdge {
                relationship: RelationshipType::SimilarTo,
                confidence: Confidence::new(0.9),
                discovered_at: 0,
            },
        );

        let results = graph.neighborhood(center, 1, None);
        let indices: Vec<NodeIndex> = results.iter().map(|(idx, _)| *idx).collect();
        assert!(
            indices.contains(&neighbor),
            "direct neighbor must be included"
        );
        assert!(
            !indices.contains(&far),
            "2-hop node must be excluded at depth=1"
        );
    }

    #[test]
    fn bfs_with_category_filter_excludes_non_matching() {
        let mut graph = KnowledgeGraph::default();
        let center = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 0);
        let mat_neighbor = graph.ensure_concept(mat_id(2), ConceptCategory::Material, 0);
        let loc_neighbor = graph.ensure_concept(loc_id(42), ConceptCategory::Location, 0);

        let edge = || ConceptEdge {
            relationship: RelationshipType::FoundOn,
            confidence: Confidence::new(0.5),
            discovered_at: 0,
        };
        graph.relate(center, mat_neighbor, edge());
        graph.relate(center, loc_neighbor, edge());

        let results = graph.neighborhood(center, 1, Some(&ConceptCategory::Location));
        assert_eq!(results.len(), 1);
        assert_eq!(results[0].0, loc_neighbor);
    }

    #[test]
    fn bfs_on_disconnected_node_returns_empty() {
        let mut graph = KnowledgeGraph::default();
        let island = graph.ensure_concept(mat_id(99), ConceptCategory::Material, 0);
        let results = graph.neighborhood(island, 5, None);
        assert!(results.is_empty());
    }

    #[test]
    fn bfs_handles_cycles_without_infinite_loop() {
        let mut graph = KnowledgeGraph::default();
        let a = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 0);
        let b = graph.ensure_concept(mat_id(2), ConceptCategory::Material, 0);
        let c = graph.ensure_concept(mat_id(3), ConceptCategory::Material, 0);

        let edge = || ConceptEdge {
            relationship: RelationshipType::SimilarTo,
            confidence: Confidence::new(0.9),
            discovered_at: 0,
        };
        // Create a cycle: a → b → c → a
        graph.relate(a, b, edge());
        graph.relate(b, c, edge());
        graph.relate(c, a, edge());

        // Should terminate and return b and c (depth=5 more than covers the cycle).
        let results = graph.neighborhood(a, 5, None);
        assert_eq!(results.len(), 2);
    }

    // ── Phase 3 tests (integration-level — graph wiring via system) ───

    #[test]
    fn no_edge_when_no_planet_seed() {
        // Material key without planet_seed must not create a FoundOn edge.
        let mut graph = KnowledgeGraph::default();
        let key = JournalKey::MaterialInstance { seed: 42 };
        let subject = graph.ensure_concept(ConceptId(key.clone()), ConceptCategory::Material, 0);
        // Simulate what update_knowledge_graph does: only wire FoundOn when
        // planet_seed is Some. With None, no location node is created.
        let rels = graph.relationships(subject);
        assert!(rels.is_empty(), "no FoundOn edge without planet_seed");
    }

    // ── Phase 4 tests ─────────────────────────────────────────────────

    #[test]
    fn cosine_similarity_identical_vectors_is_one() {
        let v = vec![0.3, 0.7, 0.5, 0.1, 0.9];
        let sim = cosine_similarity(&v, &v);
        assert!(
            (sim - 1.0).abs() < 1e-5,
            "identical vectors → similarity 1.0"
        );
    }

    #[test]
    fn cosine_similarity_orthogonal_vectors_is_zero() {
        let a = vec![1.0, 0.0, 0.0, 0.0, 0.0];
        let b = vec![0.0, 1.0, 0.0, 0.0, 0.0];
        let sim = cosine_similarity(&a, &b);
        assert!(sim.abs() < 1e-5, "orthogonal vectors → similarity ~0.0");
    }

    // ── Phase 6 tests ─────────────────────────────────────────────────

    #[test]
    fn knowledge_graph_round_trips_via_serde() {
        let mut graph = KnowledgeGraph::default();
        let a = graph.ensure_concept(mat_id(1), ConceptCategory::Material, 10);
        let b = graph.ensure_concept(loc_id(99), ConceptCategory::Location, 20);
        graph.relate(
            a,
            b,
            ConceptEdge {
                relationship: RelationshipType::FoundOn,
                confidence: Confidence::new(0.6),
                discovered_at: 10,
            },
        );

        let json = serde_json::to_string(&graph).expect("serialize");
        let restored: KnowledgeGraph = serde_json::from_str(&json).expect("deserialize");

        // Indexes are rebuilt: concept lookup should work.
        assert!(restored.lookup(&mat_id(1)).is_some());
        assert!(restored.lookup(&loc_id(99)).is_some());

        // Category index is rebuilt.
        assert_eq!(restored.by_category(&ConceptCategory::Material).len(), 1);
        assert_eq!(restored.by_category(&ConceptCategory::Location).len(), 1);

        // Timeline is sorted and contains both nodes.
        assert_eq!(restored.timeline().len(), 2);
        assert!(restored.timeline()[0].0 <= restored.timeline()[1].0);
    }

    #[test]
    fn round_trip_preserves_all_indexes() {
        let mut graph = KnowledgeGraph::default();
        // Add several concepts across categories.
        graph.ensure_concept(mat_id(1), ConceptCategory::Material, 5);
        graph.ensure_concept(mat_id(2), ConceptCategory::Material, 10);
        graph.ensure_concept(loc_id(1), ConceptCategory::Location, 15);

        let json = serde_json::to_string(&graph).expect("serialize");
        let restored: KnowledgeGraph = serde_json::from_str(&json).expect("deserialize");

        assert_eq!(restored.by_category(&ConceptCategory::Material).len(), 2);
        assert_eq!(restored.by_category(&ConceptCategory::Location).len(), 1);
        assert_eq!(restored.timeline().len(), 3);
        // Timeline must be sorted by tick after deserialization.
        let ticks: Vec<u64> = restored.timeline().iter().map(|(t, _)| *t).collect();
        assert_eq!(ticks, vec![5, 10, 15]);
    }

    // ── Integration: observation types → confidence system ────────────────
    //
    // These tests drive the full ECS system path: RecordObservation messages
    // are consumed by `update_knowledge_graph`, which accumulates confidence
    // on the KnowledgeGraph node.  This verifies that both hold-duration-gated
    // weight observations (PR #451) and failed-pickup weight observations
    // (PR #452) flow through `update_knowledge_graph` and converge toward 1.0
    // with the expected diminishing-return formula.

    /// Build a minimal App that contains only the systems and resources
    /// needed to exercise `update_knowledge_graph`.
    ///
    /// The app does not pull in any carry, journal, or UI systems —
    /// only the bare minimum required for `update_knowledge_graph` to run.
    fn build_kg_integration_app() -> App {
        use crate::journal::JournalSet;

        let mut app = App::new();
        // Time is required so `update_knowledge_graph` can stamp `recorded_at`.
        app.add_plugins(bevy::time::TimePlugin);
        // Configure the JournalSet ordering so `in_set(JournalSet::Navigate)` works.
        app.configure_sets(
            Update,
            (JournalSet::Navigate, JournalSet::Compute, JournalSet::Sync).chain(),
        );
        // Message bus for RecordObservation.
        app.add_message::<crate::observation::RecordObservation>();
        // Resources consumed by update_knowledge_graph.
        app.init_resource::<KnowledgeGraph>();
        app.insert_resource(crate::observation::ConfidenceConfig {
            base_observation_weight: 0.2,
            death_degradation_factor: 0.6,
            death_floor: 0.2,
            domain_recovery_multiplier: 2.0,
            passive_recovery_multiplier: 0.7,
            similarity_threshold: 0.85,
            initial_observation_confidence: 0.2,
        });
        // Register update_knowledge_graph itself (the private system under test).
        app.add_systems(
            Update,
            update_knowledge_graph.in_set(crate::journal::JournalSet::Navigate),
        );
        app
    }

    /// Helper: emit a single `RecordObservation` message directly into the
    /// world's message bus so the next `app.update()` will process it.
    fn emit_record_observation(
        app: &mut App,
        material_seed: u64,
        category: crate::journal::ObservationCategory,
        description: &str,
        confidence: f32,
    ) {
        use bevy::prelude::Messages;
        app.world_mut()
            .resource_mut::<Messages<crate::observation::RecordObservation>>()
            .write(crate::observation::RecordObservation {
                key: JournalKey::MaterialInstance {
                    seed: material_seed,
                },
                name: format!("TestMaterial-{material_seed}"),
                observation: crate::journal::Observation {
                    category,
                    confidence: crate::observation::Confidence::new(confidence),
                    description: description.to_string(),
                    recorded_at: 0,
                },
                material_seed: Some(MaterialSeed(material_seed)),
                planet_seed: None,
                input_seeds: Vec::new(),
                context_location: None,
            });
    }

    /// Accumulate formula sanity check (pure, no ECS):
    ///   new = old + (1 - old) * weight, clamped to [0.0, 1.0].
    fn expected_accumulate(old: f32, weight: f32) -> f32 {
        (old + (1.0 - old) * weight).clamp(0.0, 1.0)
    }

    /// A sequence of hold-duration-gated weight observations followed by
    /// failed-pickup weight observations for the same material must all be
    /// processed by `update_knowledge_graph` and cause node confidence to
    /// converge toward 1.0 with diminishing returns.
    ///
    /// Hold-duration obs use confidence=0.2 (standard `record_weight_observation`
    /// default).  Failed-pickup obs use confidence=0.1 (`CarryConfig` default
    /// `failed_pickup_observation_confidence`).
    ///
    /// The test verifies:
    /// 1. Both observation kinds are stored in the KnowledgeGraph.
    /// 2. Node overall confidence increases monotonically.
    /// 3. Numeric values match the accumulate formula exactly.
    #[test]
    fn hold_duration_and_failed_pickup_observations_converge_confidence() {
        use crate::journal::ObservationCategory;

        const MATERIAL_SEED: u64 = 42;
        // Confidence values matching real carry.rs defaults:
        // record_weight_observation uses 0.2; failed-pickup uses 0.1.
        const HOLD_DURATION_CONF: f32 = 0.2;
        const FAILED_PICKUP_CONF: f32 = 0.1;

        let mut app = build_kg_integration_app();

        // ── Step 1: first hold-duration observation ────────────────────────
        // This is what `record_weight_observation` emits via PR #451 (after
        // the hold-timer threshold is satisfied).
        emit_record_observation(
            &mut app,
            MATERIAL_SEED,
            ObservationCategory::Weight,
            "Somewhat heavy",
            HOLD_DURATION_CONF,
        );
        app.update();

        let node_idx = {
            let kg = app.world().resource::<KnowledgeGraph>();
            let key = JournalKey::MaterialInstance {
                seed: MATERIAL_SEED,
            };
            kg.lookup(&ConceptId(key))
                .expect("node must exist after first obs")
        };

        // After first obs, node.confidence starts at 0.3 (initial value in
        // `ensure_concept`) and is accumulated with the observation's confidence
        // value (0.2):
        //   0.3 + (1.0 - 0.3) * 0.2 = 0.3 + 0.14 = 0.44
        let expected_after_first = expected_accumulate(0.3, HOLD_DURATION_CONF);
        {
            let kg = app.world().resource::<KnowledgeGraph>();
            let node = kg.node(node_idx).unwrap();
            assert!(
                (node.confidence.value() - expected_after_first).abs() < 1e-4,
                "after first hold-duration obs: expected {expected_after_first:.4}, got {:.4}",
                node.confidence.value()
            );
            let weight_obs = node.observations_by_category(&ObservationCategory::Weight);
            assert_eq!(
                weight_obs.len(),
                1,
                "one weight observation after first hold-duration obs"
            );
        }

        // ── Step 2: failed-pickup observation ─────────────────────────────
        // This is what `process_failed_pickup_observation` emits via PR #452.
        // Different description ("Too heavy to lift") → appended as a new entry.
        emit_record_observation(
            &mut app,
            MATERIAL_SEED,
            ObservationCategory::Weight,
            "Too heavy to lift",
            FAILED_PICKUP_CONF,
        );
        app.update();

        let expected_after_second = expected_accumulate(expected_after_first, FAILED_PICKUP_CONF);
        {
            let kg = app.world().resource::<KnowledgeGraph>();
            let node = kg.node(node_idx).unwrap();
            assert!(
                (node.confidence.value() - expected_after_second).abs() < 1e-4,
                "after failed-pickup obs: expected {expected_after_second:.4}, got {:.4}",
                node.confidence.value()
            );
            let weight_obs = node.observations_by_category(&ObservationCategory::Weight);
            assert_eq!(
                weight_obs.len(),
                2,
                "two distinct weight entries (hold-duration + failed-pickup)"
            );
        }

        // ── Step 3: second hold-duration observation (same description) ───
        // Repeat the hold-duration obs with the same description.  This hits
        // the "existing description" branch in
        // `add_observation_with_domain_weighted_accumulation`, accumulating
        // with `base_observation_weight` (0.2) rather than the message's
        // confidence value.  The total Weight entries must not grow.
        emit_record_observation(
            &mut app,
            MATERIAL_SEED,
            ObservationCategory::Weight,
            "Somewhat heavy",
            HOLD_DURATION_CONF,
        );
        app.update();

        let expected_after_third = expected_accumulate(expected_after_second, HOLD_DURATION_CONF);
        {
            let kg = app.world().resource::<KnowledgeGraph>();
            let node = kg.node(node_idx).unwrap();
            assert!(
                (node.confidence.value() - expected_after_third).abs() < 1e-4,
                "after second hold-duration obs: expected {expected_after_third:.4}, got {:.4}",
                node.confidence.value()
            );
            // Description matched → no new entry appended; still 2 entries.
            let weight_obs = node.observations_by_category(&ObservationCategory::Weight);
            assert_eq!(
                weight_obs.len(),
                2,
                "repeated same-description hold-duration obs must not append a new entry"
            );
        }

        // ── Step 4: verify monotonic convergence over a longer sequence ──
        // Send N more alternating hold-duration and failed-pickup observations
        // and confirm confidence increases strictly monotonically.
        let mut prev = app
            .world()
            .resource::<KnowledgeGraph>()
            .node(node_idx)
            .unwrap()
            .confidence
            .value();

        for i in 0..6u32 {
            let (desc, conf) = if i % 2 == 0 {
                ("Somewhat heavy", HOLD_DURATION_CONF)
            } else {
                ("Too heavy to lift", FAILED_PICKUP_CONF)
            };
            emit_record_observation(
                &mut app,
                MATERIAL_SEED,
                ObservationCategory::Weight,
                desc,
                conf,
            );
            app.update();

            let current = app
                .world()
                .resource::<KnowledgeGraph>()
                .node(node_idx)
                .unwrap()
                .confidence
                .value();
            assert!(
                current > prev,
                "iteration {i}: confidence must increase monotonically ({prev:.4} → {current:.4})"
            );
            assert!(
                current <= 1.0,
                "confidence must never exceed 1.0 (got {current:.4})"
            );
            prev = current;
        }
    }

    /// Verifies that hold-duration weight observations and failed-pickup
    /// observations each carry the correct initial confidence value when
    /// they reach the knowledge graph.  This pins the contract between
    /// carry.rs and the confidence system without requiring the full carry
    /// ECS stack.
    ///
    /// - Hold-duration obs: confidence = 0.2  (standard `record_weight_observation`)
    /// - Failed-pickup obs: confidence = 0.1  (`failed_pickup_observation_confidence` default)
    #[test]
    fn hold_duration_obs_has_higher_confidence_than_failed_pickup_obs() {
        use crate::journal::ObservationCategory;

        const SEED: u64 = 99;
        let mut app = build_kg_integration_app();

        // Emit hold-duration obs then failed-pickup obs.
        emit_record_observation(
            &mut app,
            SEED,
            ObservationCategory::Weight,
            "Somewhat heavy",
            0.2,
        );
        emit_record_observation(
            &mut app,
            SEED,
            ObservationCategory::Weight,
            "Too heavy to lift",
            0.1,
        );
        app.update();

        let kg = app.world().resource::<KnowledgeGraph>();
        let node_idx = kg
            .lookup(&ConceptId(JournalKey::MaterialInstance { seed: SEED }))
            .expect("node must exist");
        let node = kg.node(node_idx).unwrap();
        let weight_obs = node.observations_by_category(&ObservationCategory::Weight);

        assert_eq!(weight_obs.len(), 2, "two distinct weight observations");

        let hold_obs = weight_obs
            .iter()
            .find(|o| o.description == "Somewhat heavy")
            .expect("hold-duration observation must be stored");
        let failed_obs = weight_obs
            .iter()
            .find(|o| o.description == "Too heavy to lift")
            .expect("failed-pickup observation must be stored");

        assert!(
            hold_obs.confidence.value() > failed_obs.confidence.value(),
            "hold-duration obs ({:.2}) must have higher confidence than failed-pickup obs ({:.2})",
            hold_obs.confidence.value(),
            failed_obs.confidence.value()
        );
    }
}
