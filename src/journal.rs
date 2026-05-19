//! Discovery journal — player-owned record of observed materials and outcomes.
//!
//! The journal is a lightweight UI overlay that records what the player has
//! personally encountered: surface observations from examination, thermal test
//! results from environmental exposure, and fabrication history from the
//! fabricator. Unknown properties are omitted entirely rather than shown as
//! placeholders.

use std::collections::VecDeque;

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use petgraph::graph::NodeIndex;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::diegetic_ui::{DiegeticFocusState, DiegeticSurface, DiegeticSurfaceKind};
use crate::input::InputAction;
use crate::knowledge_graph::{ConceptId, ConceptNode, KnowledgeGraph};
use crate::observation::Confidence;
use crate::player::{Player, cursor_is_captured, spawn_player};
use crate::world_generation::BiomeType;

// ── Biome key type safety ──────────────────────────────────────────────

/// Type-safe wrapper for biome identifiers used in journal filtering.
///
/// Prevents silent filter failures from string typos by ensuring biome keys
/// are always constructed from valid [`BiomeType`] enum values.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct BiomeKey(BiomeType);

impl BiomeKey {
    /// Create a new biome key from a biome type.
    ///
    /// This is the only way to construct a `BiomeKey`, ensuring all instances
    /// correspond to valid biome registry entries.
    pub fn new(biome_type: BiomeType) -> Self {
        Self(biome_type)
    }

    /// Get the underlying biome type.
    pub fn biome_type(&self) -> BiomeType {
        self.0
    }

    /// Get the string representation used for serialization and display.
    ///
    /// This returns the snake_case string that matches the biome registry's
    /// text key format (e.g., "scorched_flats", "mineral_steppe", "frost_shelf").
    ///
    /// Note: These strings must match BiomeType's serde serialization format
    /// (snake_case). The match is exhaustive, so adding a new BiomeType variant
    /// will cause a compile error here, prompting the developer to add the
    /// corresponding snake_case string.
    pub fn as_str(&self) -> &'static str {
        match self.0 {
            BiomeType::ScorchedFlats => "scorched_flats",
            BiomeType::MineralSteppe => "mineral_steppe",
            BiomeType::FrostShelf => "frost_shelf",
        }
    }
}

impl From<BiomeType> for BiomeKey {
    fn from(biome_type: BiomeType) -> Self {
        Self::new(biome_type)
    }
}

impl std::fmt::Display for BiomeKey {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{}", self.as_str())
    }
}

// ── Observation data model ──────────────────────────────────────────────

/// Categories of observation — extensible by adding variants.
///
/// Each variant represents a distinct *kind* of knowledge the player can
/// accumulate about a journal subject. New game systems (navigation,
/// trade, language) add variants here without touching existing match
/// arms or storage structures.
///
/// **Display ordering:** the *declaration order* of the variants below is
/// the order in which their groups appear in the journal detail panel.
/// Iteration is driven by [`strum::EnumIter`] (see [`Self::display_order`])
/// so adding a new variant automatically makes it visible in the UI in its
/// declared position — there is no separate hand-maintained list that can
/// drift out of sync with the enum.
#[derive(
    Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize, strum::EnumIter,
)]
pub enum ObservationCategory {
    /// Visual or tactile surface properties noticed on first examination.
    SurfaceAppearance,
    /// How the subject reacts to heat or cold exposure.
    ThermalBehavior,
    /// Perceived heft or density when the player picks up the subject.
    Weight,
    /// Outcome of combining materials in the fabricator.
    FabricationResult,
    /// A note about a specific location (landmark, hazard, resource).
    LocationNote,
    // Future: LanguageFragment, CulturalBehavior, TradePrice, etc.
}

impl ObservationCategory {
    /// Canonical display order for category groups in the detail panel.
    ///
    /// Returns variants in the order they are declared on the enum, driven
    /// by [`strum::EnumIter`].  This is the single source of truth used by
    /// [`build_detail_spans`]; adding a new variant to the enum makes it
    /// appear in the detail panel automatically — there is no separate
    /// list to update and therefore no way to silently hide a new
    /// category from the UI.
    fn display_order() -> impl Iterator<Item = Self> {
        Self::iter()
    }

    /// Player-facing label used as a group header in the detail panel.
    pub fn display_label(&self) -> &'static str {
        match self {
            ObservationCategory::SurfaceAppearance => "Surface",
            ObservationCategory::ThermalBehavior => "Thermal",
            ObservationCategory::Weight => "Weight",
            ObservationCategory::FabricationResult => "Fabrication",
            ObservationCategory::LocationNote => "Location",
        }
    }

    /// Whether the detail panel shows only the most recent observation
    /// for this category rather than the full history.
    ///
    /// Returning `true` means "this category converges on a single best
    /// reading" — repeated observations supersede earlier ones, so only
    /// the latest is worth showing (e.g. Thermal and Weight, where each
    /// new measurement refines the player's understanding of the same
    /// underlying property).
    ///
    /// Returning `false` means "this category accumulates distinct
    /// observations worth preserving" — each new entry is a separate
    /// finding, not a refinement of an earlier one (e.g. surface
    /// appearance facets, fabrication outputs, and location notes are all
    /// independently meaningful and the journal should remember each
    /// one).
    ///
    /// **New variants default to accumulating history** because the safer
    /// failure mode is "the journal remembers too much" rather than
    /// "the journal silently drops observations the player worked for."
    /// Override only when a category genuinely has a single converging
    /// reading.
    fn shows_latest_only(&self) -> bool {
        matches!(
            self,
            ObservationCategory::ThermalBehavior | ObservationCategory::Weight
        )
    }
}

/// A single observation about a journal subject, timestamped.
///
/// Observations are the atomic unit of player knowledge. Each one records
/// *what* was observed ([`ObservationCategory`]), *how confident* the
/// player should be ([`Confidence`]), a human-readable description,
/// and the game-time tick when it was recorded. Observations accumulate
/// inside a [`JournalEntry`] over time — the journal never forgets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    /// What kind of knowledge this observation represents.
    pub category: ObservationCategory,
    /// How certain the player is based on repeated evidence.
    pub confidence: Confidence,
    /// Player-facing prose description of the observation.
    pub description: String,
    /// Game-time tick when this observation was recorded.
    pub recorded_at: u64,
}

// ── Journal key ─────────────────────────────────────────────────────────

/// Unique key identifying a journal subject.
///
/// Each variant encodes both the *type* of subject (material instance,
/// fabrication output, etc.) and the identity that distinguishes one from
/// another. The enum is intentionally non-exhaustive so future systems
/// (navigation, trade, language, material classification) can add variants
/// without breaking existing match arms.
///
/// # Material identity
///
/// `MaterialInstance` identifies a specific observed material entity by its
/// generation seed. Planet of origin is carried on [`RecordObservation`] as
/// context for the `FoundOn` KnowledgeGraph edge — not baked into the key.
///
/// `Ord` is derived so that `JournalKey` can serve as a `BTreeMap` key,
/// giving the journal a stable, deterministic iteration order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JournalKey {
    /// A specific observed material instance, keyed by its generation seed.
    ///
    /// All knowledge accumulated about this material — density, reactivity,
    /// thermal response, sightings — is stored on the KnowledgeGraph node
    /// identified by this key.  Planet of origin is recorded as a `FoundOn`
    /// edge on that node, not as part of this key.
    MaterialInstance {
        /// The generation seed that uniquely identifies this material instance.
        seed: u64,
    },
    /// A material *type* (classification), grouping instances that share a
    /// property profile into a named family — e.g. "cesium", "ferrite".
    ///
    /// Classification names are defined by asset-side ranges (Story N.3).
    /// Until N.3 lands this variant is introduced here so the KnowledgeGraph
    /// and journal query layer can represent type-level nodes; no instances
    /// will carry this key until the classification system assigns them.
    ///
    /// A `MaterialInstance` node with seed `cesium-386` would be linked to
    /// its `Material { classification: "cesium" }` node via a `ClassifiedAs`
    /// edge once N.3 wires the classification pass.
    Material {
        /// The human-readable classification name assigned by the asset
        /// pipeline (e.g. "cesium", "ferrite", "volatite").
        classification: String,
    },
    /// The output of a fabrication process, keyed by the resulting
    /// material's seed.
    Fabrication {
        /// The deterministic seed of the fabricated output material.
        output_seed: u64,
    },
    /// A planetary or regional location, keyed by its planet seed.
    ///
    /// Created automatically by the knowledge graph system whenever a
    /// material observation carries planet provenance — giving the graph
    /// a concrete node to attach `FoundOn` and `ObservedAt` edges to.
    /// Location entries are not displayed in the main journal entry list
    /// because the player encounters planets through the world, not
    /// through a catalog — but they are reachable as cross-reference
    /// targets from material entries.
    Location {
        /// The planet seed that uniquely identifies this location within
        /// the world generation system.  Matches `WorldProfile::planet_seed.0`.
        planet_seed: u64,
    },
}

impl JournalKey {
    /// Planet on which the subject identified by this key was observed,
    /// when that information is available.
    ///
    /// `Location` keys return their planet seed directly. All other variants
    /// return `None` — planet provenance for material instances is stored on
    /// the KnowledgeGraph node as a `FoundOn` edge, not on the key.
    pub fn planet_seed(&self) -> Option<u64> {
        match self {
            JournalKey::MaterialInstance { .. } => None,
            JournalKey::Material { .. } => None,
            JournalKey::Fabrication { .. } => None,
            JournalKey::Location { planet_seed } => Some(*planet_seed),
        }
    }

    /// Map this key to the coarse [`crate::knowledge_graph::ConceptCategory`]
    /// used for graph storage.
    pub fn concept_category(&self) -> crate::knowledge_graph::ConceptCategory {
        use crate::knowledge_graph::ConceptCategory;
        match self {
            JournalKey::MaterialInstance { .. } => ConceptCategory::Material,
            JournalKey::Material { .. } => ConceptCategory::Material,
            JournalKey::Fabrication { .. } => ConceptCategory::Fabrication,
            JournalKey::Location { .. } => ConceptCategory::Location,
        }
    }
}

// ── Filtering ───────────────────────────────────────────────────────────

/// Contextual scope used to narrow journal entries to those relevant to
/// the player's current situation (e.g. only the current planet, only
/// the current biome).
///
/// Each variant carries the identity of *what* the player is currently
/// engaged with so the filter can be evaluated against the metadata
/// captured on each [`JournalEntry`] / [`JournalKey`].  The enum is
/// intentionally small at this stage — only the contexts already implied
/// by Story 10.3's acceptance criteria (current planet, current biome)
/// are included.  Additional contexts (current solar system, time
/// period, etc.) are explicitly anticipated by the design but are out of
/// scope for this task and will be added when their underlying world
/// metadata is available.
///
/// The variant payloads use the same identifier types the rest of the
/// codebase uses for these concepts: a raw planet seed (`u64`, matching
/// `WorldGenerationConfig::planet_seed` and the public `PlanetSeed.0`
/// representation already exposed in serialized world data) and a
/// biome key as a `String` (matching the biome registry's text key
/// format).  Keeping the payloads as plain owned values rather than
/// borrowing keeps `JournalFilter` cheap to clone and store inside a
/// long-lived UI state resource.
#[derive(Clone, Debug, PartialEq, Eq, Hash)]
pub enum JournalContext {
    /// Restrict to entries that were observed on the planet identified
    /// by this seed.  The seed matches `WorldProfile::planet_seed.0`,
    /// which is the value future tasks will copy into [`JournalKey`]
    /// metadata at observation time.
    CurrentPlanet {
        /// Raw planet seed (unwrapped from `PlanetSeed`) used as the
        /// equality key when matching entries against this context.
        planet_seed: u64,
    },
    /// Restrict to entries that were observed within the named biome.
    /// The key is a type-safe wrapper around [`BiomeType`] that ensures
    /// consistency with the biome registry and prevents silent filter
    /// failures from typos or mismatched strings.
    CurrentBiome {
        /// Type-safe biome identifier that corresponds to a valid biome
        /// registry entry. Constructed only from [`BiomeType`] enum values.
        biome_key: BiomeKey,
    },
    // Future variants (CurrentSystem, TimePeriod, …) will be added when
    // the underlying world metadata is captured on JournalKey.  They are
    // intentionally omitted now to avoid defining identifiers whose
    // semantics have not yet been pinned down by their respective
    // systems.
}

/// Combined filter applied to the journal entry list before rendering.
///
/// Both fields are independent and combine with **AND** logic: an entry
/// is kept when *every* `Some(_)` filter matches it.  A `None` field is
/// treated as "no restriction on this dimension", so the [`Default`]
/// value (both fields `None`) corresponds to the "All" filter required
/// by the Story 10.3 acceptance criteria — every entry is shown.
///
/// `JournalFilter` is a plain data type with no behavior: the matching
/// logic, UI cycling, and persistence across journal toggles are added
/// in subsequent Phase 1 tasks.  Defining the data shape first lets
/// those tasks build on a stable type without coupling concerns.
///
/// `Hash` is derived alongside `PartialEq`/`Eq` so the filter can be
/// used as part of cache keys in later tasks (e.g. memoising the
/// filtered entry list while the filter is unchanged).
#[derive(Clone, Debug, Default, PartialEq, Eq, Hash)]
pub struct JournalFilter {
    /// Optional restriction to a single observation category.  When
    /// `Some(category)`, only entries that contain at least one
    /// observation in `category` are kept.  `None` means "no category
    /// restriction" (the "All" category filter).
    pub category: Option<ObservationCategory>,
    /// Optional restriction to entries tied to a particular world
    /// context (current planet, current biome, …).  When `Some(ctx)`,
    /// only entries whose captured location metadata matches `ctx` are
    /// kept.  `None` means "no contextual restriction" (the "All"
    /// context filter).
    pub context: Option<JournalContext>,
}

/// Predicate evaluating whether a knowledge-graph concept node should be shown
/// under the active journal filter.
///
/// Both filter dimensions combine with AND logic — a node is kept when every
/// `Some(_)` dimension matches. `None` on any dimension means "no restriction",
/// so the default filter (both `None`) keeps every node.
pub fn matches_filter_node(node: &ConceptNode, filter: &JournalFilter) -> bool {
    let category_match = filter
        .category
        .as_ref()
        .is_none_or(|cat| node.observations.keys().any(|node_cat| node_cat == cat));

    let context_match = filter.context.as_ref().is_none_or(|ctx| match ctx {
        JournalContext::CurrentPlanet { planet_seed } => {
            node.origin_planet_seed == Some(*planet_seed)
        }
        JournalContext::CurrentBiome { .. } => true,
    });

    category_match && context_match
}

/// Plugin that manages the player journal, recording observations and discoveries.
pub struct JournalPlugin;

/// Public system set ordering for the journal pipeline.
///
/// `JournalSelectionTracker` reconciliation depends on detecting whether
/// `JournalUiState::selected_index` changed *between frames* due to user
/// navigation versus *within a frame* due to entries shifting.  That
/// detection requires a strict order:
///
/// 1. [`JournalSet::Navigate`] — exclusive owner of in-frame
///    `selected_index` mutation in response to player input.
/// 2. [`JournalSet::Compute`] — reads the post-navigation index, runs
///    selection-survival reconciliation against the tracker, clamps to
///    bounds, and writes the render cache + tracker snapshot.
/// 3. [`JournalSet::Sync`] — pushes the cached output to the UI.
///
/// Any future system that touches `JournalUiState` from outside this
/// module **must** be ordered inside `JournalSet::Navigate` (or strictly
/// before `Compute`), otherwise the tracker will misinterpret a same-
/// frame index change as user navigation and re-anchor incorrectly.
/// Field privacy on [`JournalUiState`] makes the violation hard to
/// commit by accident; this set makes the ordering contract explicit
/// for the cases that legitimately need write access.
#[derive(SystemSet, Clone, Debug, PartialEq, Eq, Hash)]
pub enum JournalSet {
    /// User-input navigation that mutates `JournalUiState::selected_index`
    /// or `scroll_offset`.  Must finish before `Compute`.
    Navigate,
    /// Selection-survival reconciliation, bounds clamping, and render
    /// cache population.  Must run after `Navigate` and before `Sync`.
    Compute,
    /// Pushes the render cache into the Bevy UI text nodes.
    Sync,
}

impl Plugin for JournalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RecordObservation>()
            .add_message::<ToggleJournalIntent>()
            .init_resource::<JournalUiState>()
            .init_resource::<JournalSelectionTracker>()
            .init_resource::<JournalRenderCache>()
            .init_resource::<JournalNavigationStack>()
            .configure_sets(
                Update,
                (JournalSet::Navigate, JournalSet::Compute, JournalSet::Sync).chain(),
            )
            .add_systems(
                Startup,
                (
                    attach_journal_to_player.after(spawn_player),
                    spawn_journal_ui,
                ),
            )
            .add_systems(
                Update,
                (
                    emit_toggle_journal_intent.in_set(JournalSet::Navigate),
                    toggle_journal_visibility
                        .in_set(JournalSet::Navigate)
                        .after(emit_toggle_journal_intent),
                    sync_journal_ui_state_from_focus
                        .in_set(JournalSet::Navigate)
                        .after(toggle_journal_visibility),
                    journal_navigation
                        .in_set(JournalSet::Navigate)
                        .after(sync_journal_ui_state_from_focus),
                    update_journal_context_on_planet_change
                        .in_set(JournalSet::Navigate)
                        .after(journal_navigation),
                    journal_cross_ref_navigation
                        .in_set(JournalSet::Navigate)
                        .after(journal_navigation),
                    compute_journal_panels.in_set(JournalSet::Compute),
                    populate_cross_ref_links
                        .in_set(JournalSet::Compute)
                        .after(compute_journal_panels),
                    sync_journal_ui.in_set(JournalSet::Sync),
                ),
            );
    }
}

// ── Messages ────────────────────────────────────────────────────────────

/// Unified message for recording any observation in the player's journal.
///
/// Any game system (materials, heat, carry, fabrication, navigation, trade)
/// sends this single message type instead of a per-category variant. The
/// journal ingestion system routes based on the [`Observation::category`]
/// field — callers only need to fill in the key, name, and observation.
///
/// **Planet seed handling:** For [`JournalKey::Material`] observations,
/// the `planet_seed` field is automatically resolved by the ingestion
/// system from the current [`WorldProfile`] resource. Observation producers
/// Message sent by any game system to record a player observation.
///
/// The knowledge graph system wires `FoundOn` and `ObservedAt` edges from
/// the `planet_seed` and `context_location` fields respectively.
/// Journal ingestion ignores both — they are purely for graph wiring.
///
/// **Cross-reference metadata:** The `input_seeds` field is optional
/// metadata consumed by the knowledge graph system to wire `DerivedFrom`
/// edges for fabrication observations.
#[derive(Message, Clone)]
pub struct RecordObservation {
    /// Which journal subject this observation belongs to.
    pub key: JournalKey,
    /// Player-facing display name for the subject (used to initialise the
    /// entry on first encounter; ignored for subsequent observations of the
    /// same key).
    pub name: String,
    /// The observation payload including category, confidence, description,
    /// and game-time tick.
    pub observation: Observation,
    /// Planet on which this observation was made.
    ///
    /// When `Some`, the knowledge graph system wires a `FoundOn` edge from
    /// the observed subject to the corresponding location concept.  Callers
    /// that don't have planetary context (integration tests, fabrication)
    /// leave this `None`.
    pub planet_seed: Option<u64>,
    /// For fabrication observations: seeds of the input materials that were
    /// combined to produce the output. The knowledge graph system uses these
    /// to wire `DerivedFrom` edges from the output concept to each input.
    ///
    /// Empty for non-fabrication observations.
    pub input_seeds: Vec<u64>,
    /// Optional location context where this observation was made.
    ///
    /// When `Some`, the knowledge graph system wires an `ObservedAt` edge
    /// from the observed subject to this location concept. Callers that
    /// don't have explicit location context leave this `None`.
    pub context_location: Option<JournalKey>,
}

// ── Player-owned journal component ──────────────────────────────────────

/// Marker component attached to the player entity.
///
/// As of Story 387 the `Journal` no longer stores observations — that data
/// lives in [`KnowledgeGraph`] `ConceptNode`s. The component is kept so
/// existing system queries (`With<Player>`, `With<Journal>`) continue to
/// compile without change while the query layer is being migrated in Phase 5.
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct Journal;

// ── UI state ────────────────────────────────────────────────────────────

/// Tracks the journal panel's visibility and navigation state.
///
/// Fields are deliberately **private**.  External systems must go through
/// the accessor and mutator methods so the navigation invariants
/// (`selected_index < entry_count`, selection inside the visible window)
/// stay enforced — making the fields public would let any system stomp
/// the state and produce a one-frame window where indices are out of
/// bounds before [`Self::clamp_to_entry_count`] gets a chance to run.
///
/// Within this module the navigation systems still touch the fields
/// directly: they are the *owners* of the navigation invariant and run
/// in a fixed order (`journal_navigation` → `compute_journal_panels`)
/// that re-establishes the invariant before any reader sees the state.
///
/// Scroll position and selection survive close/reopen — toggling
/// visibility does **not** reset navigation fields.
#[derive(Resource)]
pub struct JournalUiState {
    /// Whether the journal overlay is currently shown.
    visible: bool,
    /// Index of the currently highlighted entry in the sorted entry list.
    selected_index: usize,
    /// Index of the first entry visible in the left-hand list panel.
    scroll_offset: usize,
    /// Maximum number of entry rows displayed per page.  Loaded from
    /// configuration; falls back to `Self::DEFAULT_ENTRIES_PER_PAGE`.
    entries_per_page: usize,
    /// Active contextual filter applied to the entry list before
    /// rendering (Story 10.3).
    ///
    /// Stored on the long-lived UI state resource — rather than recomputed
    /// per-frame from input — so the filter persists across journal
    /// visibility toggles, satisfying the acceptance criterion that
    /// "filter state persists when journal is toggled closed/open".
    /// The [`Default`] value is the empty filter ([`JournalFilter::default`]),
    /// which corresponds to the "All" filter showing every entry — also a
    /// Story 10.3 acceptance criterion.
    ///
    /// Field privacy mirrors the other navigation fields on this resource:
    /// the matching logic, UI cycling, and rendering systems added by
    /// later Phase 1 tasks read and mutate the filter only through the
    /// accessor and setter on this type, keeping the [`JournalSet`]
    /// ordering contract enforceable.
    filter: JournalFilter,
}

impl JournalUiState {
    /// Sensible default when no configuration override is provided.
    const DEFAULT_ENTRIES_PER_PAGE: usize = 15;

    /// Whether the journal overlay is currently visible.
    ///
    /// Read-only accessor for systems outside this module.
    #[allow(dead_code)]
    pub fn is_visible(&self) -> bool {
        self.visible
    }

    /// Currently highlighted entry index in the sorted entry list.
    ///
    /// Always within `[0, entry_count)` after `compute_journal_panels`
    /// runs (see `clamp_to_entry_count`).
    #[allow(dead_code)]
    pub fn selected_index(&self) -> usize {
        self.selected_index
    }

    /// Index of the first entry visible in the left-hand list panel.
    #[allow(dead_code)]
    pub fn scroll_offset(&self) -> usize {
        self.scroll_offset
    }

    /// Maximum number of entry rows displayed per page.
    #[allow(dead_code)]
    pub fn entries_per_page(&self) -> usize {
        self.entries_per_page
    }

    /// Show or hide the journal overlay.
    ///
    /// Navigation fields (`selected_index`, `scroll_offset`) are
    /// intentionally preserved across visibility toggles so the player
    /// returns to the same view they left.
    #[allow(dead_code)]
    pub fn set_visible(&mut self, visible: bool) {
        self.visible = visible;
    }

    /// Currently active contextual filter (Story 10.3).
    ///
    /// Returns a borrow rather than a clone because the filter is read on
    /// every render frame by the upcoming filtering logic; cloning would
    /// be wasteful when the field is otherwise cheap to inspect in place.
    #[allow(dead_code)]
    pub fn filter(&self) -> &JournalFilter {
        &self.filter
    }

    /// Replace the currently active contextual filter (Story 10.3).
    ///
    /// Mutation goes through this setter rather than a public field so
    /// future tasks can hook reset-on-change behavior here (e.g., resetting
    /// `scroll_offset` to the top when the filter changes, as called for
    /// in the Story 10.3 technical design) without having to find every
    /// call site.  The current implementation is a plain assignment;
    /// behavioral hooks are deferred to the task that wires the filter
    /// into navigation and rendering.
    #[allow(dead_code)]
    pub fn set_filter(&mut self, filter: JournalFilter) {
        self.filter = filter;
    }

    /// Clamp `selected_index` and `scroll_offset` so they stay within valid
    /// bounds for the given total entry count.  Called after any navigation
    /// input and before rendering so the UI never references out-of-range
    /// indices.
    fn clamp_to_entry_count(&mut self, entry_count: usize) {
        if entry_count == 0 {
            self.selected_index = 0;
            self.scroll_offset = 0;
            return;
        }
        // Clamp selection to last valid index.
        self.selected_index = self.selected_index.min(entry_count - 1);
        // Ensure selected entry is within the visible scroll window.
        if self.selected_index < self.scroll_offset {
            self.scroll_offset = self.selected_index;
        }
        if self.selected_index >= self.scroll_offset + self.entries_per_page {
            self.scroll_offset = self.selected_index + 1 - self.entries_per_page;
        }
    }
}

impl Default for JournalUiState {
    fn default() -> Self {
        Self {
            visible: false,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: Self::DEFAULT_ENTRIES_PER_PAGE,
            filter: JournalFilter::default(),
        }
    }
}

#[derive(Message)]
struct ToggleJournalIntent;

/// Tracks the [`JournalKey`] of the entry the player has highlighted, so the
/// selection survives entry deletions and additions.
///
/// The selection in [`JournalUiState`] is index-based, but indices shift
/// whenever entries appear or disappear in the (alphabetically sorted)
/// entry list.  This tracker remembers *which subject* was selected, so
/// `compute_journal_panels` can:
///
/// * Re-anchor `selected_index` onto the tracked key when its sort position
///   moves (e.g. another entry was added before it).
/// * Re-anchor `scroll_offset` onto the entry that occupied the top of the
///   visible window in the previous frame (`top_key`), so the visible
///   contents of the entry list do not shift underneath the player when a
///   new entry is recorded while the journal is open.  Without this, a
///   single insertion before the visible window would scroll every visible
///   row down by one — disruptive when the player is reading the panel.
/// * Fall back to the nearest remaining entry — by sort position — when the
///   tracked key has been removed altogether.  "Nearest" is the entry now
///   occupying the deleted entry's sort slot, falling back to the last
///   entry when the deletion was at the end of the list.  This is what
///   `JournalUiState::clamp_to_entry_count` produces naturally once the
///   tracker has decided not to override `selected_index`.
///
/// `last_index` records the sort position the tracker key occupied at the
/// end of the previous frame.  It lets `compute_journal_panels` distinguish
/// "the user navigated this frame" (selected_index changed away from
/// last_index) from "entries shifted underneath the user" (selected_index
/// equals last_index but the tracked key's new position differs).
///
/// `top_key` and `last_scroll_offset` mirror the same idea for the visible
/// window's top entry: when `scroll_offset` matches `last_scroll_offset`
/// the user did not page/jump this frame, so we follow `top_key` to its
/// new sort position to keep the visible window pinned to the same entries.
///
/// The tracker is internal bookkeeping; gameplay systems do not interact
/// with it directly. It is `None` until the panel reconciles against a
/// non-empty journal, and is reset to `None` whenever the journal becomes
/// empty.
#[derive(Resource, Default)]
struct JournalSelectionTracker {
    /// The key of the entry currently considered "selected", or `None`
    /// when no selection has yet been established (empty journal).
    key: Option<JournalKey>,
    /// The sort position the tracked key occupied at the end of the
    /// previous frame — used to detect whether `selected_index` was
    /// changed by user navigation or by entries shifting.
    last_index: usize,
    /// The key of the entry that occupied the top of the visible window
    /// at the end of the previous frame (i.e. the entry at sort position
    /// `last_scroll_offset`).  `None` when no anchor has been established
    /// yet (empty journal) or when the previous top entry was deleted.
    top_key: Option<JournalKey>,
    /// The `scroll_offset` value at the end of the previous frame, used
    /// to detect whether the user changed the scroll position this frame
    /// (Page Up/Down, Home/End, or selection-driven scroll adjustment)
    /// versus entries shifting underneath a stationary view.
    last_scroll_offset: usize,
}

// ── Cross-reference navigation stack ────────────────────────────────────

/// Tracks the player's breadcrumb trail through journal cross-reference links.
///
/// When the player presses Enter on a cross-reference link, the current
/// entry's [`JournalKey`] is pushed here before the view jumps to the
/// linked entry. Pressing Backspace pops the stack and returns to the
/// previous entry. This gives the player a browser-history-like
/// back-navigation experience.
///
/// The stack is bounded at [`Self::MAX_DEPTH`] entries to prevent
/// unbounded growth from very long browsing sessions. When the limit is
/// reached, the oldest entry is silently dropped (the stack slides).
///
/// Persists across journal close/reopen so the player can close the
/// journal mid-browse and return to their trail.
#[derive(Resource, Default)]
pub struct JournalNavigationStack {
    /// Breadcrumb entries from oldest to newest.
    ///
    /// Back of the deque is the most recently visited entry — the one that
    /// Backspace will return the player to. Front of the deque is the oldest
    /// entry (evicted first when the depth limit is reached).
    history: VecDeque<(JournalKey, JournalFilter)>,
}

impl JournalNavigationStack {
    /// Maximum number of entries held in the navigation history.
    ///
    /// Capped to prevent unbounded memory growth and to keep the
    /// back-navigation depth reasonable. 32 steps should cover any
    /// real browsing session.
    pub const MAX_DEPTH: usize = 32;

    /// Push the current entry and active filter onto the back-navigation stack
    /// before jumping to a cross-reference target. Backspace pops and restores
    /// both.
    pub fn push(&mut self, key: JournalKey, filter: JournalFilter) {
        if self.history.len() >= Self::MAX_DEPTH {
            // Evict the oldest entry in O(1) — VecDeque makes this free.
            self.history.pop_front();
        }
        self.history.push_back((key, filter));
    }

    /// Pop and return the most recent entry on the back-navigation stack.
    ///
    /// Returns `None` when the history is empty (the player is already at
    /// their starting point and there is nothing to go back to).
    pub fn pop(&mut self) -> Option<(JournalKey, JournalFilter)> {
        self.history.pop_back()
    }

    /// Returns `true` when there are entries on the back-navigation stack.
    ///
    /// Used by the navigation system to decide whether to show the Backspace
    /// hint in the help bar.
    pub fn can_go_back(&self) -> bool {
        !self.history.is_empty()
    }
}

/// Root container for the entire journal overlay (two-panel layout).
#[derive(Component)]
struct JournalPanel;

/// Marker for the filter bar text node above the entry list.
#[derive(Component)]
struct JournalFilterBarText;

/// Marker for the left-hand entry list text node.
#[derive(Component)]
struct JournalEntryListText;

/// Marker for the right-hand detail panel text node.
#[derive(Component)]
struct JournalDetailText;

/// Marker for the bottom help bar text node.
#[derive(Component)]
struct JournalHelpText;

fn attach_journal_to_player(mut commands: Commands, player_query: Query<Entity, With<Player>>) {
    let Ok(player) = player_query.single() else {
        return;
    };
    commands.entity(player).insert(Journal);
}

/// Spawns the two-panel journal overlay: a vertical flex container holding
/// a title row, a horizontal body (entry list | detail), and a help bar.
///
/// Layout hierarchy:
/// ```text
/// JournalPanel (absolute, column)
///   ├─ Title text ("Journal")
///   ├─ Body row (row)
///   │   ├─ Entry list column (30% width)
///   │   │   ├─ JournalFilterBarText (filter bar above entry list)
///   │   │   └─ JournalEntryListText
///   │   └─ Detail column (70% width)
///   │       └─ JournalDetailText
///   └─ Help bar
///       └─ JournalHelpText
/// ```
fn spawn_journal_ui(mut commands: Commands) {
    let text_color = TextColor(Color::srgba(0.92, 0.92, 0.88, 1.0));
    let font = TextFont {
        font_size: 16.0,
        ..default()
    };
    let dim_text_color = TextColor(Color::srgba(0.6, 0.6, 0.56, 1.0));

    commands
        .spawn((
            JournalPanel,
            // ── Diegetic surface registration (Story 10.6) ───────────────
            //
            // The journal is an in-world datapad the player holds up to read,
            // not a HUD overlay.  Attaching these three components registers it
            // with the DiegeticUiPlugin:
            //   • DiegeticSurface  — marks it as a diegetic information surface
            //     so the CI compliance test can verify no rogue screen-space text exists.
            //   • DiegeticSurfaceKind::Readable — declares interaction model.
            //     The player "holds up" the datapad (Active state); physical distance
            //     does not drive focus here because the journal is always on the player.
            //     The ranges are set to 0.0 so proximity logic collapses to Focused
            //     the moment the entity exists — actual open/close is managed by
            //     toggle_journal_datapad_focus below.
            //   • DiegeticFocusState::OutOfRange — journal starts closed.
            DiegeticSurface,
            DiegeticSurfaceKind::Readable {
                perceivable_range: 0.0,
                legible_range: 0.0,
            },
            DiegeticFocusState::OutOfRange,
            // ─────────────────────────────────────────────────────────────
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(24.0),
                left: Val::Px(24.0),
                width: Val::Px(640.0),
                height: Val::Percent(80.0),
                padding: UiRect::all(Val::Px(16.0)),
                flex_direction: FlexDirection::Column,
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.09, 0.92)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            // ── Title ──────────────────────────────────────────
            parent.spawn((
                Text::new("Journal"),
                font.clone(),
                text_color,
                Node {
                    margin: UiRect::bottom(Val::Px(8.0)),
                    padding: UiRect::bottom(Val::Px(6.0)),
                    border: UiRect::bottom(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.3, 0.3, 0.28, 0.4)),
            ));

            // ── Body row (entry list | detail) ─────────────────
            parent
                .spawn(Node {
                    flex_direction: FlexDirection::Row,
                    flex_grow: 1.0,
                    overflow: Overflow::clip(),
                    ..default()
                })
                .with_children(|body| {
                    // Left: entry list (30% width) with subtle background
                    // to distinguish from the detail panel.
                    body.spawn((
                        Node {
                            width: Val::Percent(30.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(8.0)),
                            margin: UiRect::right(Val::Px(4.0)),
                            overflow: Overflow::clip(),
                            border: UiRect::right(Val::Px(1.0)),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.05, 0.05, 0.07, 0.6)),
                        BorderColor::all(Color::srgba(0.3, 0.3, 0.28, 0.4)),
                    ))
                    .with_children(|left| {
                        // Filter bar above entry list
                        left.spawn((
                            JournalFilterBarText,
                            Text::new(""),
                            TextFont {
                                font_size: 14.0,
                                ..default()
                            },
                            TextColor(Color::srgba(0.75, 0.68, 0.45, 1.0)), // Amber accent for filter status
                            Node {
                                margin: UiRect::bottom(Val::Px(4.0)),
                                padding: UiRect::all(Val::Px(4.0)),
                                ..default()
                            },
                        ));

                        // Entry list
                        left.spawn((
                            JournalEntryListText,
                            Text::new(""),
                            font.clone(),
                            text_color,
                        ));
                    });

                    // Right: detail panel (70% width) with slightly lighter
                    // background to visually separate from the entry list.
                    body.spawn((
                        Node {
                            width: Val::Percent(70.0),
                            flex_direction: FlexDirection::Column,
                            padding: UiRect::all(Val::Px(8.0)),
                            margin: UiRect::left(Val::Px(4.0)),
                            overflow: Overflow::clip(),
                            ..default()
                        },
                        BackgroundColor(Color::srgba(0.06, 0.06, 0.08, 0.5)),
                    ))
                    .with_children(|right| {
                        right.spawn((JournalDetailText, Text::new(""), font.clone(), text_color));
                    });
                });

            // ── Help bar ───────────────────────────────────────
            parent.spawn((
                JournalHelpText,
                Text::new(""),
                TextFont {
                    font_size: 13.0,
                    ..default()
                },
                dim_text_color,
                Node {
                    margin: UiRect::top(Val::Px(4.0)),
                    padding: UiRect::new(Val::Px(8.0), Val::Px(8.0), Val::Px(6.0), Val::Px(6.0)),
                    border: UiRect::top(Val::Px(1.0)),
                    ..default()
                },
                BorderColor::all(Color::srgba(0.3, 0.3, 0.28, 0.4)),
            ));
        });
}

// ── Input ───────────────────────────────────────────────────────────────

fn emit_toggle_journal_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Option<Single<&bevy::window::CursorOptions>>,
    mut writer: MessageWriter<ToggleJournalIntent>,
) {
    // If no window entity exists (e.g. headless integration tests), the journal
    // cannot receive input — skip gracefully.
    let Some(cursor_options) = cursor_options else {
        return;
    };
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }

    let Ok(action) = player_query.single() else {
        return;
    };
    if action.just_pressed(&InputAction::ToggleJournal) {
        writer.write(ToggleJournalIntent);
    }
}

fn toggle_journal_visibility(
    mut reader: MessageReader<ToggleJournalIntent>,
    mut panel_query: Query<&mut DiegeticFocusState, With<JournalPanel>>,
) {
    for _ in reader.read() {
        let Ok(mut focus) = panel_query.single_mut() else {
            continue;
        };
        // Toggle the journal datapad between Active (open) and OutOfRange (closed).
        // DiegeticUiPlugin's VisibilitySync will flip the Visibility component;
        // sync_journal_ui_state_from_focus keeps JournalUiState.visible in lockstep.
        *focus = match *focus {
            DiegeticFocusState::Active => DiegeticFocusState::OutOfRange,
            _ => DiegeticFocusState::Active,
        };
    }
}

/// Keeps [`JournalUiState::visible`] in lockstep with the journal panel's
/// [`DiegeticFocusState`].
///
/// All existing journal systems (navigation, compute, sync) gate on
/// `JournalUiState::visible` — this bridge system means none of them need
/// to know about the diegetic framework.  It runs in [`JournalSet::Navigate`]
/// before navigation so that `compute_journal_panels` always sees a
/// consistent `visible` flag.
fn sync_journal_ui_state_from_focus(
    panel_query: Query<&DiegeticFocusState, With<JournalPanel>>,
    mut state: ResMut<JournalUiState>,
) {
    let Ok(focus) = panel_query.single() else {
        return;
    };
    state.visible = matches!(focus, DiegeticFocusState::Active);
}

// ── Navigation ──────────────────────────────────────────────────────────

/// Handles keyboard navigation within the journal overlay.
///
/// Runs in Update after `toggle_journal_visibility`.  Only processes input
/// when the journal is visible.  Reads raw `ButtonInput<KeyCode>` because
/// journal navigation keys (arrows, Page Up/Down, Home/End) are internal
/// to the journal UI and intentionally not routed through the game-wide
/// `InputAction` enum.
///
/// Navigation rules:
/// - Up/Down: move selection by one entry, clamped to [0, entry_count-1].
/// - PageUp/PageDown: move selection by `entries_per_page`.
/// - Home/End: jump to first/last entry.
/// - Scroll offset auto-adjusts via `clamp_to_entry_count` so the
///   selected entry is always within the visible window.
fn journal_navigation(
    mut state: ResMut<JournalUiState>,
    keys: Res<ButtonInput<KeyCode>>,
    graph: Option<Res<KnowledgeGraph>>,
    world_profile: Option<Res<crate::world_generation::WorldProfile>>,
) {
    if !state.visible {
        return;
    }

    // ── Context filter cycling (Shift+Tab) ─────────────────────────────
    //
    // Cycles through context filter options: All → Current Planet.
    // Uses Shift+Tab to distinguish from category filter cycling (Tab).
    // When no world context is available, Current Planet is skipped.
    if keys.just_pressed(KeyCode::Tab)
        && (keys.pressed(KeyCode::ShiftLeft) || keys.pressed(KeyCode::ShiftRight))
    {
        let current_filter = state.filter().clone();
        let new_context = match current_filter.context {
            None => {
                // All → Current Planet (if world context available)
                world_profile
                    .as_ref()
                    .map(|profile| JournalContext::CurrentPlanet {
                        planet_seed: profile.planet_seed.0,
                    })
            }
            Some(JournalContext::CurrentPlanet { .. }) => {
                // Current Planet → All
                None
            }
            Some(JournalContext::CurrentBiome { .. }) => {
                // CurrentBiome → All (future expansion)
                None
            }
        };

        let new_filter = JournalFilter {
            category: current_filter.category,
            context: new_context,
        };
        state.set_filter(new_filter);

        // Reset scroll to top when filter changes, as specified in the technical design
        state.selected_index = 0;
        state.scroll_offset = 0;
    }

    // ── Category filter cycling (Tab) ──────────────────────────────────────
    //
    // Cycles through category filter options: All → SurfaceAppearance → ThermalBehavior → Weight → FabricationResult.
    // Uses Tab without Shift to distinguish from context filter cycling (Shift+Tab).
    if keys.just_pressed(KeyCode::Tab)
        && !keys.pressed(KeyCode::ShiftLeft)
        && !keys.pressed(KeyCode::ShiftRight)
    {
        let current_filter = state.filter().clone();
        let new_category = match current_filter.category {
            None => {
                // All → SurfaceAppearance
                Some(ObservationCategory::SurfaceAppearance)
            }
            Some(ObservationCategory::SurfaceAppearance) => {
                // SurfaceAppearance → ThermalBehavior
                Some(ObservationCategory::ThermalBehavior)
            }
            Some(ObservationCategory::ThermalBehavior) => {
                // ThermalBehavior → Weight
                Some(ObservationCategory::Weight)
            }
            Some(ObservationCategory::Weight) => {
                // Weight → FabricationResult
                Some(ObservationCategory::FabricationResult)
            }
            Some(ObservationCategory::FabricationResult) => {
                // FabricationResult → All
                None
            }
            Some(ObservationCategory::LocationNote) => {
                // LocationNote → All (for future expansion)
                None
            }
        };

        let new_filter = JournalFilter {
            category: new_category,
            context: current_filter.context,
        };
        state.set_filter(new_filter);

        // Reset scroll to top when filter changes, as specified in the technical design
        state.selected_index = 0;
        state.scroll_offset = 0;
    }

    // ── Navigation keys — only when there are entries ─────────────────
    let entry_count = graph
        .as_ref()
        .map(|g| g.named_node_count_filtered(state.filter()))
        .unwrap_or(0);
    if entry_count == 0 {
        return;
    }

    if keys.just_pressed(KeyCode::ArrowUp) {
        state.selected_index = state.selected_index.saturating_sub(1);
    }
    if keys.just_pressed(KeyCode::ArrowDown) {
        state.selected_index = (state.selected_index + 1).min(entry_count - 1);
    }
    if keys.just_pressed(KeyCode::PageUp) {
        state.selected_index = state.selected_index.saturating_sub(state.entries_per_page);
    }
    if keys.just_pressed(KeyCode::PageDown) {
        state.selected_index = (state.selected_index + state.entries_per_page).min(entry_count - 1);
    }
    if keys.just_pressed(KeyCode::Home) {
        state.selected_index = 0;
    }
    if keys.just_pressed(KeyCode::End) {
        state.selected_index = entry_count - 1;
    }

    state.clamp_to_entry_count(entry_count);
}

/// Handles cross-reference link navigation within the journal detail panel.
///
/// Runs in Update in [`JournalSet::Navigate`], after `journal_navigation`.
/// Only active when the journal is visible and the current entry has cross-
/// reference links (i.e. `cache.cross_ref_links` is non-empty).
///
/// Key bindings:
/// - `Alt+Down` / `Alt+Up` — cycle the highlighted cross-reference link.
/// - `Enter` — follow the highlighted link: push current key onto
///   [`JournalNavigationStack`], then find the linked entry's position
///   in the sorted filtered list and set `selected_index` to it.
/// - `Backspace` — pop the navigation stack and return to the previous
///   entry.
fn journal_cross_ref_navigation(
    mut state: ResMut<JournalUiState>,
    mut cache: ResMut<JournalRenderCache>,
    mut nav_stack: ResMut<JournalNavigationStack>,
    keys: Res<ButtonInput<KeyCode>>,
    graph: Option<Res<KnowledgeGraph>>,
) {
    if !state.visible {
        return;
    }

    // ── Back-navigation (Backspace) ─────────────────────────────────
    if keys.just_pressed(KeyCode::Backspace) {
        if let Some((prev_key, prev_filter)) = nav_stack.pop() {
            state.set_filter(prev_filter);
            if let Some(graph) = graph.as_ref() {
                let sorted = graph.nodes_sorted_by_name();
                let filtered: Vec<NodeIndex> = sorted
                    .into_iter()
                    .filter(|&idx| {
                        graph
                            .node(idx)
                            .is_some_and(|n| matches_filter_node(n, state.filter()))
                    })
                    .collect();
                let target_id = ConceptId(prev_key);
                if let Some(pos) = filtered
                    .iter()
                    .position(|&idx| graph.lookup(&target_id).is_some_and(|ti| ti == idx))
                {
                    state.selected_index = pos;
                    state.scroll_offset = pos.saturating_sub(state.entries_per_page / 2);
                    state.clamp_to_entry_count(filtered.len());
                }
            }
        }
        return;
    }

    let link_count = cache.cross_ref_links.len();
    if link_count == 0 {
        return;
    }

    // ── Cross-ref link cursor movement (Alt+Up / Alt+Down) ───────────
    let alt = keys.pressed(KeyCode::AltLeft) || keys.pressed(KeyCode::AltRight);
    if alt && keys.just_pressed(KeyCode::ArrowDown) {
        cache.selected_cross_ref = (cache.selected_cross_ref + 1).min(link_count - 1);
    }
    if alt && keys.just_pressed(KeyCode::ArrowUp) {
        cache.selected_cross_ref = cache.selected_cross_ref.saturating_sub(1);
    }

    // ── Follow cross-reference (Enter) ────────────────────────────────
    if keys.just_pressed(KeyCode::Enter) {
        let selected_idx = cache.selected_cross_ref.min(link_count - 1);
        let (_, target_key, _) = cache.cross_ref_links[selected_idx].clone();

        let Some(graph) = graph.as_ref() else {
            return;
        };

        let sorted = graph.nodes_sorted_by_name();

        // Push the current selection AND active filter onto the back-nav stack.
        if let Some(&current_idx) = sorted.get(state.selected_index)
            && let Some(node) = graph.node(current_idx)
        {
            nav_stack.push(node.id.0.clone(), state.filter().clone());
        }

        // Clear filter so the target is always in view, then jump.
        state.set_filter(JournalFilter::default());
        let sorted_all = graph.nodes_sorted_by_name();
        let target_id = ConceptId(target_key);
        if let Some(pos) = sorted_all
            .iter()
            .position(|&idx| graph.lookup(&target_id).is_some_and(|ti| ti == idx))
        {
            state.selected_index = pos;
            state.scroll_offset = pos.saturating_sub(state.entries_per_page / 2);
            state.clamp_to_entry_count(sorted_all.len());
            cache.selected_cross_ref = 0;
        }
    }
}
/// planet changes (WorldProfile resource changes).
///
/// When the player switches planets, this system detects the change in
/// WorldProfile and updates any active CurrentPlanet context filter to
/// use the new planet's seed. This ensures that the journal filter stays
/// relevant to the current planet without requiring manual re-filtering.
///
/// The system only acts when:
/// 1. WorldProfile resource has changed (detected via `Changed<WorldProfile>`)
/// 2. The current journal filter is set to CurrentPlanet context
/// 3. The new planet seed differs from the filter's current planet seed
///
/// When these conditions are met, the filter is updated to use the new
/// planet seed, maintaining the same category filter but updating the
/// context to match the new planet.
fn update_journal_context_on_planet_change(
    mut state: ResMut<JournalUiState>,
    world_profile: Option<Res<crate::world_generation::WorldProfile>>,
) {
    // Only proceed if WorldProfile exists and has changed
    let Some(profile) = world_profile.as_ref() else {
        return;
    };

    if !profile.is_changed() {
        return;
    }

    // Check if the current filter is using CurrentPlanet context
    let current_filter = state.filter().clone();
    if let Some(JournalContext::CurrentPlanet {
        planet_seed: current_seed,
    }) = current_filter.context
    {
        let new_seed = profile.planet_seed.0;

        // Only update if the planet seed has actually changed
        if current_seed != new_seed {
            let new_filter = JournalFilter {
                category: current_filter.category,
                context: Some(JournalContext::CurrentPlanet {
                    planet_seed: new_seed,
                }),
            };
            state.set_filter(new_filter);

            // Reset scroll to top when filter changes, as specified in the technical design
            state.selected_index = 0;
            state.scroll_offset = 0;
        }
    }
}

/// Cached text output computed by `compute_journal_panels` and consumed
/// by `sync_journal_ui` to update the Bevy `Text` nodes.  Keeping the
/// computation and UI sync in separate systems allows each system to stay
/// within the 4-parameter limit.
#[derive(Resource, Default)]
struct JournalRenderCache {
    /// Text for the filter bar above the entry list, showing active filter status.
    filter_bar: String,
    /// Structured lines for the left-hand entry list panel, each carrying
    /// its display text and whether it represents the selected entry.
    list_lines: Vec<EntryListLine>,
    /// Styled spans for the right-hand detail panel, rendered as `TextSpan`
    /// children with per-span coloring (header, category label, body).
    detail_spans: Vec<DetailSpan>,
    /// Text for the bottom help bar.
    help: String,
    /// Cross-reference links available on the currently selected entry.
    ///
    /// Each entry is `(relationship_label, target_key, target_name)`. The
    /// navigation system uses this list to jump to a related entry when Enter
    /// is pressed. Populated each frame by `compute_journal_panels`.
    cross_ref_links: Vec<(String, JournalKey, String)>,
    /// Index of the currently highlighted cross-reference link.
    ///
    /// `0` when no cross-references exist (no link is highlighted). Clamped
    /// to `[0, cross_ref_links.len() - 1]` each frame.
    selected_cross_ref: usize,
    /// The journal key of the entry whose cross-references are currently cached.
    ///
    /// When `populate_cross_ref_links` finds a different selected entry than this
    /// key, it resets `selected_cross_ref` to 0 so the link cursor doesn't carry
    /// over from a different entry.
    cross_ref_entry_key: Option<JournalKey>,
}

/// A single line in the entry list panel, carrying its display text and
/// whether it is the currently selected entry (for highlight rendering).
#[derive(Clone, Debug, PartialEq, Eq)]
struct EntryListLine {
    /// The formatted display text for this line (e.g. `"> Ferrite (3 obs)"`).
    text: String,
    /// `true` when this line is the currently selected entry.
    selected: bool,
}

/// Visual role of a span in the detail panel, used to pick a text color.
#[derive(Clone, Debug, PartialEq, Eq)]
enum DetailSpanKind {
    /// Entry name header line (bright highlight).
    Header,
    /// Category group header (e.g. "Surface", "Thermal") — amber accent,
    /// separates observation groups in the detail panel.
    CategoryGroupHeader,
    /// Observation description text — normal body color.
    Body,
    /// Qualitative confidence label (e.g. "Uncertain", "Noted", "Confirmed")
    /// rendered after each observation description in a subdued style.
    ConfidenceLabel,
    /// Placeholder text when the journal is empty.
    Placeholder,
    /// A cross-reference link in the "Related" section.
    ///
    /// The selected link is highlighted with a brighter cyan accent so the
    /// player can see which one Enter will follow.  Unselected links use a
    /// muted teal.
    CrossRef {
        /// Whether this link is the one currently highlighted by the
        /// cross-reference cursor (i.e. Enter will follow this link).
        selected: bool,
    },
}

/// A styled segment in the detail panel.  The panel is rebuilt each frame
/// as a sequence of `TextSpan` children, each carrying one of these to
/// determine its color.
#[derive(Clone, Debug, PartialEq, Eq)]
struct DetailSpan {
    text: String,
    kind: DetailSpanKind,
}

// ── Rendering ───────────────────────────────────────────────────────────

/// Computes the text content for both journal panels and caches it in
/// [`JournalRenderCache`].
///
/// Runs in Update after `apply_observations` and `journal_navigation`.
/// Reads the player's `Journal` and `JournalUiState`, reconciles the
/// selection against [`JournalSelectionTracker`] so it survives entry
/// additions and deletions, clamps indices, and writes the computed
/// strings into the render cache resource.
///
/// Selection-survival rules:
///
/// * If the tracker's `selected_index` from the previous frame still
///   matches the current `selected_index`, the user did not navigate this
///   frame.  In that case we follow the tracked [`JournalKey`] to its new
///   sort position — this keeps the highlight pinned to the player's
///   subject when other entries are inserted before or after it.
/// * If the tracked key no longer exists (the entry was deleted), we
///   leave `selected_index` alone and let `clamp_to_entry_count` pull it
///   to the nearest valid entry — which, in an alphabetically sorted
///   list, is the entry now occupying the deleted slot, or the last
///   entry when the deletion was at the end.
/// * If the user navigated (`selected_index` differs from the tracker's
///   recorded position), we trust the new index and re-anchor the tracker
///   onto whatever entry is now selected.
fn compute_journal_panels(
    mut state: ResMut<JournalUiState>,
    graph: Option<Res<KnowledgeGraph>>,
    mut cache: ResMut<JournalRenderCache>,
    mut tracker: ResMut<JournalSelectionTracker>,
) {
    if !state.visible {
        cache.filter_bar.clear();
        cache.list_lines.clear();
        cache.detail_spans.clear();
        cache.help.clear();
        return;
    }

    let Some(graph) = graph else {
        cache.filter_bar.clear();
        cache.list_lines.clear();
        cache.detail_spans.clear();
        cache.help.clear();
        return;
    };

    // Alphabetically sorted, filtered node list.
    let filtered_nodes: Vec<NodeIndex> = graph
        .nodes_sorted_by_name()
        .into_iter()
        .filter(|&idx| {
            graph
                .node(idx)
                .is_some_and(|n| matches_filter_node(n, state.filter()))
        })
        .collect();

    let entry_count = filtered_nodes.len();
    let has_any = graph.named_node_count() > 0;

    // ── Selection reconciliation ────────────────────────────────────
    if let Some(tracked_key) = tracker.key.clone()
        && state.selected_index == tracker.last_index
    {
        let tracked_id = ConceptId(tracked_key);
        if let Some(pos) = filtered_nodes
            .iter()
            .position(|&idx| graph.lookup(&tracked_id).is_some_and(|ti| ti == idx))
        {
            state.selected_index = pos;
        }
    }

    // ── Scroll-offset reconciliation ────────────────────────────────
    if let Some(top_key) = tracker.top_key.clone()
        && state.scroll_offset == tracker.last_scroll_offset
    {
        let top_id = ConceptId(top_key);
        if let Some(new_top_pos) = filtered_nodes
            .iter()
            .position(|&idx| graph.lookup(&top_id).is_some_and(|ti| ti == idx))
        {
            state.scroll_offset = new_top_pos;
        }
    }

    state.clamp_to_entry_count(entry_count);

    // ── Update tracker for the next frame ───────────────────────────
    if let Some(&sel_idx) = filtered_nodes.get(state.selected_index) {
        if let Some(node) = graph.node(sel_idx) {
            tracker.key = Some(node.id.0.clone());
        }
        tracker.last_index = state.selected_index;
        tracker.top_key = filtered_nodes
            .get(state.scroll_offset)
            .and_then(|&idx| graph.node(idx).map(|n| n.id.0.clone()));
        tracker.last_scroll_offset = state.scroll_offset;
    } else {
        tracker.key = None;
        tracker.last_index = 0;
        tracker.top_key = None;
        tracker.last_scroll_offset = 0;
    }

    cache.filter_bar = build_filter_bar_text(state.filter());
    cache.list_lines = build_entry_list_lines(&filtered_nodes, &graph, &state);
    cache.cross_ref_links.clear();

    // Build detail spans from the selected ConceptNode.
    let selected_node = filtered_nodes
        .get(state.selected_index)
        .and_then(|&idx| graph.node(idx));
    cache.detail_spans = build_detail_spans(selected_node, has_any);
    cache.help = build_help_text(entry_count, &state, 0);
}

/// Populates [`JournalRenderCache::cross_ref_links`] from the knowledge
/// graph for the currently selected journal entry.
///
/// Runs in the [`JournalSet::Compute`] set, immediately after
/// [`compute_journal_panels`], which clears `cross_ref_links` first.
///
/// If the KnowledgeGraph resource is absent (lightweight tests), cross-
/// references are silently omitted.
fn populate_cross_ref_links(
    state: Res<JournalUiState>,
    mut cache: ResMut<JournalRenderCache>,
    graph: Option<Res<KnowledgeGraph>>,
) {
    if !state.visible {
        return;
    }
    let Some(graph) = graph else {
        return;
    };

    // Rebuild the filtered node list to find the selected node.
    let filtered_nodes: Vec<NodeIndex> = graph
        .nodes_sorted_by_name()
        .into_iter()
        .filter(|&idx| {
            graph
                .node(idx)
                .is_some_and(|n| matches_filter_node(n, state.filter()))
        })
        .collect();

    let Some(&sel_idx) = filtered_nodes.get(state.selected_index) else {
        return;
    };
    let Some(selected_node) = graph.node(sel_idx) else {
        return;
    };

    // Reset link cursor when selected entry changes.
    if cache.cross_ref_entry_key.as_ref() != Some(&selected_node.id.0) {
        cache.selected_cross_ref = 0;
        cache.cross_ref_entry_key = Some(selected_node.id.0.clone());
    }

    // Collect relationships from the graph.
    for (neighbor_idx, edge) in graph.relationships(sel_idx) {
        let Some(neighbor_node) = graph.node(neighbor_idx) else {
            continue;
        };
        // Use the node's own name; fall back to category label for unnamed concepts.
        let target_name = if neighbor_node.name.is_empty() {
            format!("{:?}", neighbor_node.category)
        } else {
            neighbor_node.name.clone()
        };
        let rel_label = edge.relationship.display_label().to_string();
        cache
            .cross_ref_links
            .push((rel_label, neighbor_node.id.0.clone(), target_name));
    }

    let link_count = cache.cross_ref_links.len();
    if link_count > 0 {
        cache.selected_cross_ref = cache.selected_cross_ref.min(link_count - 1);
    } else {
        cache.selected_cross_ref = 0;
    }

    let links_snapshot = cache.cross_ref_links.clone();
    let selected_cross_ref = cache.selected_cross_ref;

    cache.detail_spans = build_detail_spans_with_cross_refs(
        selected_node,
        true,
        &links_snapshot,
        selected_cross_ref,
    );
    cache.help = build_help_text(filtered_nodes.len(), &state, links_snapshot.len());
}

/// Syncs the cached text into the Bevy UI `Text` nodes and toggles
/// panel visibility.
///
/// Runs in Update after `compute_journal_panels`.  Reads `JournalUiState`
/// for visibility, and `JournalRenderCache` for the precomputed text.
/// The `ParamSet` groups three text queries that would otherwise conflict
/// on the `Text` component; clippy's type-complexity lint is suppressed
/// because the alternative (a custom `SystemParam`) adds indirection
/// without improving clarity for a single call-site.
#[allow(clippy::type_complexity)]
fn sync_journal_ui(
    state: Res<JournalUiState>,
    cache: Res<JournalRenderCache>,
    mut commands: Commands,
    list_query: Query<(Entity, Option<&Children>), With<JournalEntryListText>>,
    detail_query: Query<(Entity, Option<&Children>), With<JournalDetailText>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<JournalFilterBarText>>,
        Query<&mut Text, With<JournalEntryListText>>,
        Query<&mut Text, With<JournalDetailText>>,
        Query<&mut Text, With<JournalHelpText>>,
    )>,
) {
    // Visibility is now managed by DiegeticUiPlugin::sync_readable_surface_visibility.
    // This system only updates text content; it skips work when the journal is not
    // visible to avoid rebuilding TextSpan trees every frame while closed.
    if !state.visible {
        return;
    }

    // ── Entry list: rebuild TextSpan children for per-line coloring ──
    //
    // The root Text node stays empty; each visible line becomes a child
    // TextSpan with a highlight color for the selected entry and the
    // default color for the rest.
    let selected_color = TextColor(Color::srgba(1.0, 0.85, 0.35, 1.0));
    let normal_color = TextColor(Color::srgba(0.92, 0.92, 0.88, 1.0));
    let span_font = TextFont {
        font_size: 16.0,
        ..default()
    };

    if let Ok((list_entity, children)) = list_query.single() {
        // Despawn old spans.
        if let Some(children) = children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

        // Clear root text so only spans render.
        if let Ok(mut root_text) = texts.p1().single_mut() {
            root_text.0.clear();
        }

        // Spawn new spans — one per visible entry line.
        let line_count = cache.list_lines.len();
        commands.entity(list_entity).with_children(|parent| {
            for (i, line) in cache.list_lines.iter().enumerate() {
                let color = if line.selected {
                    selected_color
                } else {
                    normal_color
                };
                // Append newline between lines but not after the last one.
                let suffix = if i + 1 < line_count { "\n" } else { "" };
                parent.spawn((
                    TextSpan::new(format!("{}{suffix}", line.text)),
                    span_font.clone(),
                    color,
                ));
            }
        });
    }

    // ── Detail panel: rebuild TextSpan children for styled rendering ──
    //
    // Each DetailSpan becomes a child TextSpan with a color determined by
    // its kind: header (bright highlight), category label (amber accent),
    // body (normal text), or placeholder (dimmed).
    let header_color = TextColor(Color::srgba(1.0, 0.85, 0.35, 1.0));
    let category_group_color = TextColor(Color::srgba(0.75, 0.68, 0.45, 1.0));
    let body_color = TextColor(Color::srgba(0.92, 0.92, 0.88, 1.0));
    let confidence_color = TextColor(Color::srgba(0.6, 0.65, 0.7, 1.0));
    let placeholder_color = TextColor(Color::srgba(0.55, 0.55, 0.50, 1.0));

    if let Ok((detail_entity, detail_children)) = detail_query.single() {
        if let Some(children) = detail_children {
            for child in children.iter() {
                commands.entity(child).despawn();
            }
        }

        if let Ok(mut root_text) = texts.p2().single_mut() {
            root_text.0.clear();
        }

        commands.entity(detail_entity).with_children(|parent| {
            for span in cache.detail_spans.iter() {
                let color = match span.kind {
                    DetailSpanKind::Header => header_color,
                    DetailSpanKind::CategoryGroupHeader => category_group_color,
                    DetailSpanKind::Body => body_color,
                    DetailSpanKind::ConfidenceLabel => confidence_color,
                    DetailSpanKind::Placeholder => placeholder_color,
                    // Selected cross-reference link: bright cyan to draw the eye.
                    DetailSpanKind::CrossRef { selected: true } => {
                        TextColor(Color::srgba(0.3, 0.9, 0.85, 1.0))
                    }
                    // Unselected cross-reference link: muted teal — visible but not
                    // competing with the observation text.
                    DetailSpanKind::CrossRef { selected: false } => {
                        TextColor(Color::srgba(0.3, 0.65, 0.62, 1.0))
                    }
                };
                parent.spawn((TextSpan::new(span.text.clone()), span_font.clone(), color));
            }
        });
    }

    // ── Filter bar text ──────────────────────────────────────────────
    if let Ok(mut filter_text) = texts.p0().single_mut() {
        filter_text.0.clone_from(&cache.filter_bar);
    }

    if let Ok(mut help_text) = texts.p3().single_mut() {
        help_text.0.clone_from(&cache.help);
    }
}

/// Builds the filter bar text showing the currently active filter.
/// Returns an empty string when no filter is active (All filter).
fn build_filter_bar_text(filter: &JournalFilter) -> String {
    match (&filter.category, &filter.context) {
        (None, None) => String::new(), // All filter - no text needed
        (Some(category), None) => format!("Filter: {}", category.display_label()),
        (None, Some(JournalContext::CurrentPlanet { .. })) => "Filter: Current Planet".to_string(),
        (None, Some(JournalContext::CurrentBiome { biome_key })) => {
            format!("Filter: Current Biome ({})", biome_key)
        }
        (Some(category), Some(JournalContext::CurrentPlanet { .. })) => {
            format!("Filter: {} | Current Planet", category.display_label())
        }
        (Some(category), Some(JournalContext::CurrentBiome { biome_key })) => {
            format!(
                "Filter: {} | Current Biome ({})",
                category.display_label(),
                biome_key
            )
        }
    }
}

/// Builds structured line data for the left-panel entry list.
///
/// Each line carries its display text and a flag indicating whether it is
/// the currently selected entry.  The selected entry is prefixed with `>`
/// and rendered with a distinct highlight color by the UI sync system.
/// Builds structured line data for the left-panel entry list from KG nodes.
fn build_entry_list_lines(
    nodes: &[NodeIndex],
    graph: &KnowledgeGraph,
    state: &JournalUiState,
) -> Vec<EntryListLine> {
    if nodes.is_empty() {
        return Vec::new();
    }

    let page_end = (state.scroll_offset + state.entries_per_page).min(nodes.len());
    let visible = &nodes[state.scroll_offset..page_end];

    visible
        .iter()
        .enumerate()
        .map(|(i, &idx)| {
            let abs_index = state.scroll_offset + i;
            let selected = abs_index == state.selected_index;
            let prefix = if selected { ">" } else { " " };
            let (name, obs_count) = graph
                .node(idx)
                .map(|n| (n.name.as_str(), n.observation_count()))
                .unwrap_or(("<unknown>", 0));
            EntryListLine {
                text: format!("{prefix} {} ({obs_count} obs)", name),
                selected,
            }
        })
        .collect()
}

/// Builds styled spans for the right-panel detail view of the currently
/// selected `ConceptNode`.
///
/// `has_any` — whether the KG has any named nodes at all (distinguishes
/// "no observations yet" from "filter produced no results").
fn build_detail_spans(node: Option<&ConceptNode>, has_any: bool) -> Vec<DetailSpan> {
    build_detail_spans_with_cross_refs_opt(node, has_any, &[], 0)
}

/// Internal implementation of detail-span building that accepts cross-reference
/// data computed externally.
fn build_detail_spans_with_cross_refs(
    node: &ConceptNode,
    has_any: bool,
    cross_ref_links: &[(String, JournalKey, String)],
    selected_cross_ref: usize,
) -> Vec<DetailSpan> {
    build_detail_spans_with_cross_refs_opt(Some(node), has_any, cross_ref_links, selected_cross_ref)
}

fn build_detail_spans_with_cross_refs_opt(
    node: Option<&ConceptNode>,
    has_any: bool,
    cross_ref_links: &[(String, JournalKey, String)],
    selected_cross_ref: usize,
) -> Vec<DetailSpan> {
    let Some(node) = node else {
        let message = if has_any {
            "No matching entries"
        } else {
            "No observations yet."
        };
        return vec![DetailSpan {
            text: message.to_string(),
            kind: DetailSpanKind::Placeholder,
        }];
    };

    let mut spans: Vec<DetailSpan> = Vec::new();

    // Entry name header.
    spans.push(DetailSpan {
        text: node.name.clone(),
        kind: DetailSpanKind::Header,
    });

    // Iterate categories in canonical display order.
    for category in ObservationCategory::display_order() {
        let observations = node.observations_by_category(&category);
        if observations.is_empty() {
            continue;
        }

        spans.push(DetailSpan {
            text: format!("\n\n{}", category.display_label()),
            kind: DetailSpanKind::CategoryGroupHeader,
        });

        let visible: &[Observation] = if category.shows_latest_only() {
            &observations[observations.len() - 1..]
        } else {
            observations
        };

        for obs in visible {
            let indented = obs
                .description
                .lines()
                .map(|line| format!("\n  {line}"))
                .collect::<String>();
            spans.push(DetailSpan {
                text: indented,
                kind: DetailSpanKind::Body,
            });
            spans.push(DetailSpan {
                text: format!("  [{}]", obs.confidence.tier().display_label()),
                kind: DetailSpanKind::ConfidenceLabel,
            });
        }
    }

    // ── Related section (cross-references) ──────────────────────────
    if !cross_ref_links.is_empty() {
        spans.push(DetailSpan {
            text: "\n\nRelated".to_string(),
            kind: DetailSpanKind::CategoryGroupHeader,
        });
        for (i, (rel_label, _, target_name)) in cross_ref_links.iter().enumerate() {
            let selected = i == selected_cross_ref;
            let prefix = if selected { "→ " } else { "  " };
            spans.push(DetailSpan {
                text: format!("\n{prefix}{}: {}", rel_label, target_name),
                kind: DetailSpanKind::CrossRef { selected },
            });
        }
    }

    spans
}

/// Builds the bottom help bar showing navigation hints and a page indicator.
fn build_help_text(entry_count: usize, state: &JournalUiState, cross_ref_count: usize) -> String {
    if entry_count == 0 {
        return "J: Close".to_string();
    }

    let page_start = state.scroll_offset + 1;
    let page_end = (state.scroll_offset + state.entries_per_page).min(entry_count);

    // Show active filter status if any filter is applied
    let filter_status = match (&state.filter().category, &state.filter().context) {
        (None, None) => String::new(),
        (Some(_), None) => " [Filter: Category]".to_string(),
        (None, Some(JournalContext::CurrentPlanet { .. })) => {
            " [Filter: Current Planet]".to_string()
        }
        (None, Some(JournalContext::CurrentBiome { biome_key })) => {
            format!(" [Filter: Current Biome ({})]", biome_key)
        }
        (Some(_), Some(JournalContext::CurrentPlanet { .. })) => {
            " [Filter: Category | Current Planet]".to_string()
        }
        (Some(_), Some(JournalContext::CurrentBiome { biome_key })) => {
            format!(" [Filter: Category | Current Biome ({})]", biome_key)
        }
    };

    // Cross-reference hint when links exist.
    let cross_ref_hint = if cross_ref_count > 0 {
        "  Alt+↑↓: Link  Enter: Follow  Backspace: Back"
    } else {
        ""
    };

    format!(
        "\u{2191}\u{2193} Navigate  PgUp/PgDn: Page  Home/End: Jump  Shift+Tab: Context Filter  J: Close{filter_status}{cross_ref_hint}  [{page_start}-{page_end} of {entry_count}]"
    )
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
