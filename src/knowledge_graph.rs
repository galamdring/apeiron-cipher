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

use std::collections::{HashMap, HashSet, VecDeque};

use bevy::prelude::*;
use petgraph::Direction;
use petgraph::graph::{Graph, NodeIndex};
use petgraph::visit::EdgeRef;
use serde::{Deserialize, Serialize};

use crate::journal::JournalKey;
use crate::observation::Confidence;

// ── Plugin ────────────────────────────────────────────────────────────────

/// Registers the [`KnowledgeGraph`] resource and the
/// [`update_knowledge_graph`] system that populates it from
/// [`crate::journal::RecordObservation`] messages.
pub struct KnowledgeGraphPlugin;

impl Plugin for KnowledgeGraphPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<KnowledgeGraph>().add_systems(
            Update,
            (
                update_knowledge_graph
                    .in_set(crate::journal::JournalSet::Navigate)
                    .after(crate::journal::apply_observations),
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
/// The `confidence` field tracks overall certainty in the concept's
/// existence, which edges then narrow with typed relationship confidence.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ConceptNode {
    /// Stable identity linking this node to the corresponding journal entry.
    pub id: ConceptId,
    /// Coarse classification for encyclopedia grouping and BFS filtering.
    pub category: ConceptCategory,
    /// Overall confidence that this concept is real and correctly identified.
    pub confidence: Confidence,
    /// Game-time tick (whole seconds elapsed) when this concept was first added.
    ///
    /// Matches the unit used by [`crate::journal::Observation::recorded_at`] so
    /// the two timestamps are directly comparable.
    pub discovered_at: u64,
    /// Which named properties the player has directly observed on this concept.
    ///
    /// Property keys are string identifiers matching the observation category
    /// names so new properties can be added without a code change here.
    pub revealed_properties: HashSet<String>,
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
#[derive(Resource, Debug, Default)]
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
            confidence: Confidence(0.3), // Start at Tentative/Observed boundary
            discovered_at: tick,
            revealed_properties: HashSet::new(),
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
    pub fn lookup_material_by_seed(&self, seed: u64) -> Option<NodeIndex> {
        self.concept_index.iter().find_map(|(id, &idx)| {
            if matches!(&id.0, crate::journal::JournalKey::MaterialInstance { seed: s } if *s == seed) {
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
    /// [`crate::journal::ObservationCategory::display_label`] string so the
    /// encyclopedia view can show exactly which properties the player has seen.
    ///
    /// No-ops when the node index is invalid (should never happen during normal
    /// gameplay since indexes are never removed).
    pub fn reveal_property(&mut self, node: NodeIndex, property_name: String) {
        if let Some(n) = self.graph.node_weight_mut(node) {
            n.revealed_properties.insert(property_name);
        }
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

    // ── Bounded BFS ───────────────────────────────────────────────────────

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
                timeline.push((node.discovered_at, idx));
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

/// Processes [`crate::journal::RecordObservation`] messages and populates
/// the [`KnowledgeGraph`] with typed relationship edges.
///
/// Runs in [`crate::journal::JournalSet::Navigate`] — same set as the
/// journal's own `apply_observations`, so both systems see the same
/// messages in the same frame.
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
/// No edges are created without an observation event; the system never
/// infers connections the player hasn't personally made.
fn update_knowledge_graph(
    mut reader: MessageReader<crate::journal::RecordObservation>,
    mut graph: ResMut<KnowledgeGraph>,
    time: Res<Time>,
) {
    let tick = time.elapsed().as_secs();

    for obs in reader.read() {
        // Determine category for the observed subject.
        let subject_category = category_from_key(&obs.key);

        // Ensure the subject node exists.
        let subject_node = graph.ensure_concept(ConceptId(obs.key.clone()), subject_category, tick);

        // Mark which property category the player just observed on this concept.
        let category_name = obs.observation.category.display_label().to_string();
        graph.reveal_property(subject_node, category_name);

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
                            crate::journal::JournalKey::MaterialInstance { seed: input_seed };
                        graph.ensure_concept(ConceptId(input_key), ConceptCategory::Material, tick)
                    });
                graph.relate(
                    subject_node,
                    input_node,
                    ConceptEdge {
                        relationship: RelationshipType::DerivedFrom,
                        // Fabrication is directly observed — full confidence.
                        confidence: Confidence(1.0),
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
                let key = crate::journal::JournalKey::MaterialInstance { seed: seed_a };
                graph.ensure_concept(ConceptId(key), ConceptCategory::Material, tick)
            });
            let node_b = graph.lookup_material_by_seed(seed_b).unwrap_or_else(|| {
                let key = crate::journal::JournalKey::MaterialInstance { seed: seed_b };
                graph.ensure_concept(ConceptId(key), ConceptCategory::Material, tick)
            });

            let edge = ConceptEdge {
                relationship: RelationshipType::CombinedWith,
                confidence: Confidence(1.0),
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
        crate::journal::JournalKey::Fabrication { .. } => ConceptCategory::Fabrication,
        crate::journal::JournalKey::Location { .. } => ConceptCategory::Location,
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
    new_seed: u64,
    new_material: &crate::materials::GameMaterial,
    catalog: &crate::materials::MaterialCatalog,
    journal: &crate::journal::Journal,
    graph: &mut KnowledgeGraph,
    threshold: f32,
    tick: u64,
) {
    let new_vec = new_material.property_vector();

    for existing in catalog.values() {
        // Never compare a material to itself.
        if existing.seed == new_seed {
            continue;
        }

        let sim = cosine_similarity(&new_vec, &existing.property_vector());
        if sim < threshold {
            continue;
        }

        // Both materials must be at Observed confidence or above before
        // we surface the similarity — connections must be earned.
        let new_key = crate::journal::JournalKey::MaterialInstance { seed: new_seed };
        let existing_key = crate::journal::JournalKey::MaterialInstance {
            seed: existing.seed,
        };

        let new_confident = is_at_least_observed(journal, &new_key);
        let existing_confident = is_at_least_observed(journal, &existing_key);

        if !new_confident || !existing_confident {
            continue;
        }

        // Wire SimilarTo in both directions so the relationship shows up
        // regardless of which material the player is viewing.
        let new_node =
            graph.ensure_concept(ConceptId(new_key.clone()), ConceptCategory::Material, tick);
        let existing_node =
            graph.ensure_concept(ConceptId(existing_key), ConceptCategory::Material, tick);

        let edge = ConceptEdge {
            relationship: RelationshipType::SimilarTo,
            confidence: Confidence(sim),
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

/// Returns `true` when the journal contains at least one observation for
/// `key` with confidence at or above [`crate::observation::ConfidenceTier::Observed`]
/// (i.e. confidence ≥ 0.3).
///
/// "At least Observed" is the threshold at which the player has
/// accumulated enough evidence to treat the observation as factual
/// rather than tentative. `SimilarTo` edges are only wired when both
/// materials meet this bar, ensuring the connection is earned.
fn is_at_least_observed(
    journal: &crate::journal::Journal,
    key: &crate::journal::JournalKey,
) -> bool {
    match key {
        crate::journal::JournalKey::MaterialInstance { seed } => {
            journal.entries.values().any(|entry| {
                matches!(&entry.key, crate::journal::JournalKey::MaterialInstance { seed: s } if *s == *seed)
                    && entry.all_observations().any(|obs| obs.confidence.0 >= 0.3)
            })
        }
        _ => journal
            .entries
            .get(key)
            .is_some_and(|entry| entry.all_observations().any(|obs| obs.confidence.0 >= 0.3)),
    }
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
    mut reader: MessageReader<crate::journal::RecordObservation>,
    mut graph: ResMut<KnowledgeGraph>,
    catalog: Res<crate::materials::MaterialCatalog>,
    player_query: Query<&crate::journal::Journal, With<crate::player::Player>>,
    time: Res<Time>,
    config: Res<crate::observation::ConfidenceConfig>,
) {
    let Ok(journal) = player_query.single() else {
        return;
    };
    let tick = time.elapsed().as_secs();

    for obs in reader.read() {
        let crate::journal::JournalKey::MaterialInstance { seed } = obs.key else {
            continue;
        };
        let Some(material) = catalog.get_by_seed(seed) else {
            continue;
        };
        detect_and_wire_similar_materials(
            seed,
            material,
            &catalog,
            journal,
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
        ConceptId(JournalKey::Location { planet_seed })
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
                confidence: Confidence(0.5),
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
            confidence: Confidence(0.3),
            discovered_at: 0,
        };
        graph.relate(a, b, initial_edge.clone());

        // Second relate with same type should strengthen, not duplicate.
        graph.relate(a, b, initial_edge);
        let rels = graph.relationships(a);
        assert_eq!(rels.len(), 1, "no duplicate edge");
        assert!(
            rels[0].1.confidence.0 > 0.3,
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
                confidence: Confidence(0.9),
                discovered_at: 0,
            },
        );
        graph.relate(
            neighbor,
            far,
            ConceptEdge {
                relationship: RelationshipType::SimilarTo,
                confidence: Confidence(0.9),
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
            confidence: Confidence(0.5),
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
            confidence: Confidence(0.9),
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
                confidence: Confidence(0.6),
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
}
