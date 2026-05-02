//! Discovery journal — player-owned record of observed materials and outcomes.
//!
//! The journal is a lightweight UI overlay that records what the player has
//! personally encountered: surface observations from examination, thermal test
//! results from environmental exposure, and fabrication history from the
//! fabricator. Unknown properties are omitted entirely rather than shown as
//! placeholders.

use std::collections::BTreeMap;

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};
use strum::IntoEnumIterator;

use crate::input::InputAction;
use crate::observation::ConfidenceLevel;
use crate::player::{Player, cursor_is_captured, spawn_player};

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
    fn display_label(&self) -> &'static str {
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
/// player should be ([`ConfidenceLevel`]), a human-readable description,
/// and the game-time tick when it was recorded. Observations accumulate
/// inside a [`JournalEntry`] over time — the journal never forgets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    /// What kind of knowledge this observation represents.
    pub category: ObservationCategory,
    /// How certain the player is based on repeated evidence.
    pub confidence: ConfidenceLevel,
    /// Player-facing prose description of the observation.
    pub description: String,
    /// Game-time tick when this observation was recorded.
    pub recorded_at: u64,
}

// ── Journal key ─────────────────────────────────────────────────────────

/// Unique key identifying a journal subject.
///
/// Each variant encodes both the *type* of subject (material, fabrication
/// output, etc.) and the identity that distinguishes one instance from
/// another. The enum is intentionally non-exhaustive so future systems
/// (navigation, trade, language) can add variants without breaking
/// existing match arms.
///
/// `Ord` is derived so that `JournalKey` can serve as a `BTreeMap` key,
/// giving the journal a stable, deterministic iteration order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JournalKey {
    /// A raw or discovered material, keyed by its procedural seed.
    ///
    /// The optional `planet_seed` records the planet on which this
    /// material was first observed, so context-aware filters
    /// (Story 10.3 — "entries relevant to current planet") can match
    /// entries against the player's [`WorldProfile::planet_seed`]
    /// without re-deriving provenance from observation history.
    ///
    /// `planet_seed` is `None` for entries created in contexts where
    /// no planetary world profile is in scope (early bring-up, ad-hoc
    /// integration tests, future non-planetary discovery sites).
    /// Treating it as `Option<u64>` rather than baking in a sentinel
    /// keeps the "unknown provenance" case explicit at every match
    /// site.
    ///
    /// Field ordering is `seed` then `planet_seed` so the derived
    /// `Ord` continues to sort primarily by material identity — the
    /// existing journal iteration order is preserved when
    /// `planet_seed` is `None` everywhere, which matches the
    /// pre-extension behaviour bit-for-bit.
    Material {
        /// The deterministic seed that uniquely identifies this material
        /// within the world generation system.
        seed: u64,
        /// The planet on which this material was first observed, taken
        /// from `WorldProfile::planet_seed.0` at observation time.
        /// `None` indicates the recording site had no planetary context
        /// available; such entries are excluded from
        /// [`JournalContext::CurrentPlanet`] filtering.
        planet_seed: Option<u64>,
    },
    /// The output of a fabrication process, keyed by the resulting
    /// material's seed.
    Fabrication {
        /// The deterministic seed of the fabricated output material.
        output_seed: u64,
    },
}

impl JournalKey {
    /// Planet on which the subject identified by this key was first
    /// observed, when that information is available.
    ///
    /// Returns `Some(seed)` for [`JournalKey::Material`] entries that
    /// were recorded with a planetary world profile in scope, and `None`
    /// for materials recorded without one (early bring-up, ad-hoc
    /// integration tests, future non-planetary discovery sites).
    ///
    /// [`JournalKey::Fabrication`] entries return `None` because
    /// fabrications are produced at the player's fabricator and are
    /// intentionally not tied to a discovery planet — the same recipe
    /// produces the same output regardless of where it was crafted.  The
    /// "current planet" filter therefore intentionally hides
    /// fabrications, which matches the player-mental-model of the
    /// filter ("show me things tied to where I am") better than
    /// pretending fabrications belong to whichever planet the player
    /// happened to be standing on at craft time.
    ///
    /// Used by [`matches_filter`] to evaluate
    /// [`JournalContext::CurrentPlanet`] without re-deriving provenance
    /// from observation history.
    pub fn planet_seed(&self) -> Option<u64> {
        match self {
            JournalKey::Material { planet_seed, .. } => *planet_seed,
            JournalKey::Fabrication { .. } => None,
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
    /// The key matches the biome registry's text key (e.g. `"tundra"`,
    /// `"basalt_flats"`); a `String` is used here because the biome
    /// taxonomy is data-driven and not represented as a typed enum.
    CurrentBiome {
        /// Biome registry key used as the equality value when matching
        /// entries against this context.
        biome_key: String,
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

/// Predicate evaluating whether a journal entry should be shown under the
/// active filter.
///
/// The two filter dimensions combine with **AND** logic: an entry is
/// kept when *every* `Some(_)` dimension matches it.  `None` on a
/// dimension means "no restriction on this dimension" — the
/// [`JournalFilter::default`] value (both dimensions `None`) therefore
/// returns `true` for every entry, satisfying the Story 10.3
/// acceptance criterion that the "All" filter shows everything.
///
/// Dimension semantics:
///
/// * **Category match** — the entry contains at least one observation
///   whose [`Observation::category`] equals the requested category.  An
///   entry with no observations cannot match any category restriction
///   (the `any` fold returns `false` over an empty iterator), which is
///   the correct behaviour: an entry with zero observations carries no
///   evidence of belonging to any category.
/// * **Context match** — the entry's captured location metadata
///   matches the requested context.  For
///   [`JournalContext::CurrentPlanet`] this consults
///   [`JournalKey::planet_seed`]: an entry matches iff its key recorded
///   the same planet seed.  Entries whose key has no planet seed
///   (`None`) are excluded from current-planet filtering — this is by
///   design (see [`JournalKey::Material::planet_seed`]'s docs):
///   "unknown provenance" should not silently be assumed to mean
///   "current planet".
///
///   [`JournalContext::CurrentBiome`] is not yet evaluated and
///   currently returns `true` (no restriction).  Biome provenance is
///   not captured on [`JournalKey`] today; wiring it up is tracked as a
///   follow-up so that filter UI cycling can already expose the option
///   without false-negative behaviour.  When biome capture is added,
///   this arm changes to compare against the entry's recorded biome
///   key — no other call site needs to change.
///
/// The function is intentionally pure (no `WorldContext` parameter):
/// the filter already carries the planet seed / biome key it is
/// matching against, and the entry already carries the metadata being
/// matched.  Keeping the predicate parameter-light means it can be
/// dropped into an iterator chain (`entries.filter(|e|
/// matches_filter(e, &filter))`) without threading additional
/// resources through the render pipeline.
pub fn matches_filter(entry: &JournalEntry, filter: &JournalFilter) -> bool {
    let category_match = filter
        .category
        .as_ref()
        .is_none_or(|cat| entry.observations.keys().any(|entry_cat| entry_cat == cat));

    let context_match = filter.context.as_ref().is_none_or(|ctx| match ctx {
        JournalContext::CurrentPlanet { planet_seed } => {
            entry.key.planet_seed() == Some(*planet_seed)
        }
        // Biome provenance is not yet captured on JournalKey; until it
        // is, the biome filter is a no-op (matches everything) rather
        // than excluding every entry.  Returning `true` here keeps the
        // UI affordance usable without producing a misleading "no
        // matching entries" panel for a filter that hasn't been
        // wired through to the data model yet.
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
                    journal_navigation
                        .in_set(JournalSet::Navigate)
                        .after(toggle_journal_visibility),
                    update_journal_context_on_planet_change
                        .in_set(JournalSet::Navigate)
                        .after(journal_navigation),
                    apply_observations.in_set(JournalSet::Navigate),
                    compute_journal_panels.in_set(JournalSet::Compute),
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
}

// ── Player-owned journal data ───────────────────────────────────────────

/// A journal entry that accumulates observations about a single subject over time.
///
/// Each entry is keyed by a [`JournalKey`] and holds a chronologically ordered
/// vector of [`Observation`]s. The `first_observed_at` and `last_updated_at`
/// timestamps track the game-time ticks bounding the observation window, which
/// later systems (e.g., confidence decay, staleness indicators) can use without
/// re-scanning all observations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The unique identifier for this journal subject.
    pub key: JournalKey,
    /// Player-facing display name for this subject.
    pub name: String,
    /// Observations grouped by category, each group in chronological order.
    ///
    /// Using a `BTreeMap` gives deterministic iteration order over categories
    /// (important for rendering stability and save/load reproducibility).
    /// Within each category the `Vec` preserves insertion (chronological) order.
    pub observations: BTreeMap<ObservationCategory, Vec<Observation>>,
    /// Game-time tick when the player first recorded *any* observation about
    /// this subject.
    pub first_observed_at: u64,
    /// Game-time tick of the most recent observation recorded for this subject.
    pub last_updated_at: u64,
}

impl JournalEntry {
    /// Create a new journal entry with no observations yet recorded.
    ///
    /// The `tick` parameter sets both `first_observed_at` and `last_updated_at`
    /// to the same initial value; they diverge once additional observations
    /// arrive.
    pub fn new(key: JournalKey, name: String, tick: u64) -> Self {
        Self {
            key,
            name,
            observations: BTreeMap::new(),
            first_observed_at: tick,
            last_updated_at: tick,
        }
    }

    /// Record an observation, deduplicating against existing entries in the
    /// same category group.
    ///
    /// If an observation with the same category **and** the same description
    /// already exists, the duplicate is not appended. Instead, the existing
    /// observation's confidence is upgraded to the higher of the two values
    /// and the `last_updated_at` timestamp is advanced. This prevents the
    /// journal from bloating when systems repeatedly report the same finding
    /// (e.g., picking up the same material multiple times).
    ///
    /// When the observation is genuinely new (different category or different
    /// description), it is appended to the appropriate category group.
    pub fn add_observation(&mut self, observation: Observation) {
        self.last_updated_at = observation.recorded_at;

        let group = self
            .observations
            .entry(observation.category.clone())
            .or_default();

        // Look for an existing observation with the same description within this category.
        if let Some(existing) = group
            .iter_mut()
            .find(|o| o.description == observation.description)
        {
            // Upgrade confidence if the new evidence is stronger.
            if observation.confidence > existing.confidence {
                existing.confidence = observation.confidence;
            }
            return;
        }

        group.push(observation);
    }

    /// Return all observations matching a given category, in recorded order.
    ///
    /// Returns an empty slice if no observations exist for the category.
    pub fn observations_by_category(&self, category: &ObservationCategory) -> &[Observation] {
        self.observations
            .get(category)
            .map_or(&[], |v| v.as_slice())
    }

    /// Total number of observations across all categories.
    pub fn observation_count(&self) -> usize {
        self.observations.values().map(|v| v.len()).sum()
    }

    /// Iterator over all observations across all categories, ordered by
    /// category (deterministic via `BTreeMap`) then by insertion order
    /// within each category.
    pub fn all_observations(&self) -> impl Iterator<Item = &Observation> {
        self.observations.values().flat_map(|v| v.iter())
    }
}

/// The player's journal — accumulates all observations about every subject
/// the player has investigated.
///
/// Keyed by [`JournalKey`] so lookups are O(log n) and iteration order is
/// deterministic (important for save/load reproducibility and test stability).
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct Journal {
    /// All journal entries, keyed by subject identity.
    ///
    /// Serialized as a list of entries (not a JSON object) because
    /// [`JournalKey`] is an enum and cannot serve as a JSON map key.
    #[serde(with = "journal_entries_serde")]
    pub entries: BTreeMap<JournalKey, JournalEntry>,
}

/// Serialize/deserialize a `BTreeMap<JournalKey, JournalEntry>` as a
/// `Vec<JournalEntry>`. Each entry already carries its key, so the vec
/// representation is lossless and avoids the JSON "keys must be strings"
/// limitation.
mod journal_entries_serde {
    use super::*;
    use serde::de::Deserializer;
    use serde::ser::Serializer;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<JournalKey, JournalEntry>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let entries: Vec<&JournalEntry> = map.values().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<JournalKey, JournalEntry>, D::Error> {
        let entries: Vec<JournalEntry> = Vec::deserialize(deserializer)?;
        Ok(entries.into_iter().map(|e| (e.key.clone(), e)).collect())
    }
}

impl Journal {
    /// Look up or create a journal entry for the given key.
    ///
    /// If no entry exists yet, one is created with the provided `name` and
    /// `tick` as the initial observation timestamp. If an entry already exists,
    /// the existing entry is returned unchanged (name is *not* overwritten —
    /// the first observer wins).
    pub fn ensure_entry(&mut self, key: JournalKey, name: &str, tick: u64) -> &mut JournalEntry {
        self.entries
            .entry(key.clone())
            .or_insert_with(|| JournalEntry::new(key, name.to_string(), tick))
    }

    /// Record an observation against a subject, creating the entry if needed.
    ///
    /// This is the primary write path that future systems (materials, heat,
    /// fabrication, navigation, trade, language) use to push knowledge into
    /// the journal.
    pub fn record(&mut self, key: JournalKey, name: &str, observation: Observation) {
        let entry = self.ensure_entry(key, name, observation.recorded_at);
        entry.add_observation(observation);
    }
}

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
    commands.entity(player).insert(Journal::default());
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
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut writer: MessageWriter<ToggleJournalIntent>,
) {
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
    mut state: ResMut<JournalUiState>,
) {
    for _ in reader.read() {
        state.visible = !state.visible;
    }
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
    q: Query<&Journal, With<Player>>,
    world_profile: Option<Res<crate::world_generation::WorldProfile>>,
) {
    if !state.visible {
        return;
    }

    let Ok(journal) = q.single() else {
        return;
    };

    let entry_count = journal.entries.len();
    if entry_count == 0 {
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

/// System that automatically updates the journal context filter when the
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

// ── Record ingestion ────────────────────────────────────────────────────

/// Unified ingestion system — reads [`RecordObservation`] messages and
/// writes them into the player's [`Journal`].
///
/// Callers pass `recorded_at: 0` — this system overwrites with real
/// elapsed time so caller signatures stay lean.
fn apply_observations(
    mut reader: MessageReader<RecordObservation>,
    mut player_query: Query<&mut Journal, With<Player>>,
    time: Res<Time>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    let tick = time.elapsed().as_millis() as u64;

    for event in reader.read() {
        let mut obs = event.observation.clone();
        obs.recorded_at = tick;
        journal.record(event.key.clone(), &event.name, obs);
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
    player_query: Query<&Journal, With<Player>>,
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

    let Ok(journal) = player_query.single() else {
        cache.filter_bar.clear();
        cache.list_lines.clear();
        cache.detail_spans.clear();
        cache.help.clear();
        return;
    };

    // Sort entries alphabetically for stable display order.
    let mut sorted_entries: Vec<&JournalEntry> = journal.entries.values().collect();
    sorted_entries.sort_by(|a, b| a.name.cmp(&b.name));

    // Apply active filter to the sorted entries
    let filtered_entries: Vec<&JournalEntry> = sorted_entries
        .into_iter()
        .filter(|entry| matches_filter(entry, state.filter()))
        .collect();

    let entry_count = filtered_entries.len();

    // ── Selection reconciliation ────────────────────────────────────
    //
    // Only follow the tracked key when the user did NOT navigate this
    // frame.  We detect navigation by comparing the live `selected_index`
    // to the index the tracker recorded at the end of the previous frame:
    // they match iff no navigation key fired in between.
    // When the user has not navigated this frame and the tracked entry
    // still exists, snap `selected_index` to its (possibly shifted) sort
    // position.  When it no longer exists, `selected_index` is left as-is
    // so that `clamp_to_entry_count` pulls it to the nearest valid entry
    // — the entry now occupying the deleted slot, or the new last entry
    // if the deletion was at the end of the list.
    if let Some(tracked_key) = tracker.key.clone()
        && state.selected_index == tracker.last_index
        && let Some(new_pos) = filtered_entries.iter().position(|e| e.key == tracked_key)
    {
        state.selected_index = new_pos;
    }

    // ── Scroll-offset reconciliation ────────────────────────────────
    //
    // Same idea as selection reconciliation, but for the top of the
    // visible window.  When the user did not page/jump this frame
    // (`scroll_offset` matches what the tracker recorded last frame) and
    // the entry that was at the top of the window still exists, snap
    // `scroll_offset` to that entry's new sort position.  This keeps the
    // visible rows pinned to the same subjects when a new entry is
    // recorded outside the visible window — without it, an insertion
    // before `scroll_offset` would silently scroll every visible row
    // down by one and disrupt the player's reading.
    //
    // When the previous top entry has been deleted we leave
    // `scroll_offset` alone; `clamp_to_entry_count` below ensures the
    // (possibly already re-anchored) selection stays visible.
    if let Some(top_key) = tracker.top_key.clone()
        && state.scroll_offset == tracker.last_scroll_offset
        && let Some(new_top_pos) = filtered_entries.iter().position(|e| e.key == top_key)
    {
        state.scroll_offset = new_top_pos;
    }

    state.clamp_to_entry_count(entry_count);

    // ── Update tracker for the next frame ───────────────────────────
    //
    // Anchor onto whatever entry is now selected (which, after clamping,
    // is guaranteed to exist when entry_count > 0) and onto whatever
    // entry now occupies the top of the visible window.
    if let Some(entry) = filtered_entries.get(state.selected_index) {
        tracker.key = Some(entry.key.clone());
        tracker.last_index = state.selected_index;
        tracker.top_key = filtered_entries
            .get(state.scroll_offset)
            .map(|e| e.key.clone());
        tracker.last_scroll_offset = state.scroll_offset;
    } else {
        // Empty journal: clear the tracker so a future first observation
        // does not cause us to re-anchor onto a stale key.
        tracker.key = None;
        tracker.last_index = 0;
        tracker.top_key = None;
        tracker.last_scroll_offset = 0;
    }

    cache.filter_bar = build_filter_bar_text(state.filter());
    cache.list_lines = build_entry_list_lines(&filtered_entries, &state);
    cache.detail_spans = build_detail_spans(&filtered_entries, &state, !journal.entries.is_empty());
    cache.help = build_help_text(entry_count, &state);
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
    mut panel_query: Query<&mut Visibility, With<JournalPanel>>,
    list_query: Query<(Entity, Option<&Children>), With<JournalEntryListText>>,
    detail_query: Query<(Entity, Option<&Children>), With<JournalDetailText>>,
    mut texts: ParamSet<(
        Query<&mut Text, With<JournalFilterBarText>>,
        Query<&mut Text, With<JournalEntryListText>>,
        Query<&mut Text, With<JournalDetailText>>,
        Query<&mut Text, With<JournalHelpText>>,
    )>,
) {
    let Ok(mut panel_vis) = panel_query.single_mut() else {
        return;
    };

    if !state.visible {
        *panel_vis = Visibility::Hidden;
        return;
    }

    *panel_vis = Visibility::Visible;

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
        (None, Some(JournalContext::CurrentBiome { .. })) => "Filter: Current Biome".to_string(),
        (Some(category), Some(JournalContext::CurrentPlanet { .. })) => {
            format!("Filter: {} | Current Planet", category.display_label())
        }
        (Some(category), Some(JournalContext::CurrentBiome { .. })) => {
            format!("Filter: {} | Current Biome", category.display_label())
        }
    }
}

/// Builds structured line data for the left-panel entry list.
///
/// Each line carries its display text and a flag indicating whether it is
/// the currently selected entry.  The selected entry is prefixed with `>`
/// and rendered with a distinct highlight color by the UI sync system.
fn build_entry_list_lines(entries: &[&JournalEntry], state: &JournalUiState) -> Vec<EntryListLine> {
    if entries.is_empty() {
        return Vec::new();
    }

    let page_end = (state.scroll_offset + state.entries_per_page).min(entries.len());
    let visible = &entries[state.scroll_offset..page_end];

    visible
        .iter()
        .enumerate()
        .map(|(i, entry)| {
            let abs_index = state.scroll_offset + i;
            let selected = abs_index == state.selected_index;
            let prefix = if selected { ">" } else { " " };
            let obs_count = entry.observation_count();
            EntryListLine {
                text: format!("{prefix} {} ({obs_count} obs)", entry.name),
                selected,
            }
        })
        .collect()
}

/// Builds the left-panel entry list showing names within the visible
/// scroll window.  The selected entry is prefixed with `>` and shows
/// observation count; other entries show observation count without prefix.
///
/// Format per line:
/// ```text
/// > Ferrite (3 obs)       ← selected
///   Silite (1 obs)        ← not selected
/// ```
#[cfg(test)]
fn build_entry_list_text(entries: &[&JournalEntry], state: &JournalUiState) -> String {
    let lines = build_entry_list_lines(entries, state);
    lines
        .iter()
        .map(|l| l.text.as_str())
        .collect::<Vec<_>>()
        .join("\n")
}

/// Builds styled spans for the right-panel detail view of the currently
/// selected entry.
///
/// The header (entry name) renders in a bright highlight color.  Category
/// labels ("Surface:", "Heat:", etc.) use an amber accent, while observation
/// descriptions use the normal body color.  If no entries exist, a single
/// placeholder span is returned.
///
/// The `has_any_entries` parameter distinguishes between an empty journal
/// (shows "No observations yet.") and a filter that produces no results
/// (shows "No matching entries").
fn build_detail_spans(
    entries: &[&JournalEntry],
    state: &JournalUiState,
    has_any_entries: bool,
) -> Vec<DetailSpan> {
    if entries.is_empty() {
        let message = if has_any_entries {
            "No matching entries"
        } else {
            "No observations yet."
        };
        return vec![DetailSpan {
            text: message.to_string(),
            kind: DetailSpanKind::Placeholder,
        }];
    }

    let entry = entries[state.selected_index.min(entries.len() - 1)];
    let mut spans: Vec<DetailSpan> = Vec::new();

    // Entry name header.
    spans.push(DetailSpan {
        text: entry.name.clone(),
        kind: DetailSpanKind::Header,
    });

    // Iterate categories in canonical display order, emitting a group
    // header followed by the observations for each non-empty category.
    // The order is driven by `ObservationCategory::display_order` (backed
    // by `strum::EnumIter`) so a new variant added to the enum is
    // automatically rendered here in its declared position.
    for category in ObservationCategory::display_order() {
        let observations = entry.observations_by_category(&category);
        if observations.is_empty() {
            continue;
        }

        // Category group header (e.g. "\n\nSurface").
        spans.push(DetailSpan {
            text: format!("\n\n{}", category.display_label()),
            kind: DetailSpanKind::CategoryGroupHeader,
        });

        // For categories that converge on a single reading, show only
        // the most recent observation. Otherwise show all.
        let visible: &[Observation] = if category.shows_latest_only() {
            // Safe: we checked `!is_empty()` above.
            &observations[observations.len() - 1..]
        } else {
            observations
        };

        for obs in visible {
            // Multi-line descriptions (e.g. surface observations that combine
            // color + weight) need each line indented consistently.
            let indented = obs
                .description
                .lines()
                .map(|line| format!("\n  {line}"))
                .collect::<String>();
            spans.push(DetailSpan {
                text: indented,
                kind: DetailSpanKind::Body,
            });
            // Qualitative confidence indicator — communicates certainty
            // without exposing internal counts.
            spans.push(DetailSpan {
                text: format!("  [{}]", obs.confidence.display_label()),
                kind: DetailSpanKind::ConfidenceLabel,
            });
        }
    }

    spans
}

/// Flattens detail spans into a plain string for test assertions.
///
/// Concatenates all span texts, which produces the same output as the
/// original `build_detail_text` (header, then indented category lines).
#[cfg(test)]
fn detail_spans_to_string(spans: &[DetailSpan]) -> String {
    spans.iter().map(|s| s.text.as_str()).collect()
}

/// Builds the bottom help bar showing navigation hints and a page indicator.
fn build_help_text(entry_count: usize, state: &JournalUiState) -> String {
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
        (None, Some(JournalContext::CurrentBiome { .. })) => " [Filter: Current Biome]".to_string(),
        (Some(_), Some(JournalContext::CurrentPlanet { .. })) => {
            " [Filter: Category | Current Planet]".to_string()
        }
        (Some(_), Some(JournalContext::CurrentBiome { .. })) => {
            " [Filter: Category | Current Biome]".to_string()
        }
    };

    format!(
        "\u{2191}\u{2193} Navigate  PgUp/PgDn: Page  Home/End: Jump  Shift+Tab: Context Filter  J: Close{filter_status}  [{page_start}-{page_end} of {entry_count}]"
    )
}

/// Formats all journal entries into a single display string.
///
/// Retained for backward compatibility with existing tests.  The in-game
/// UI now uses `build_entry_list_text` / `build_detail_spans` instead, but
/// this function exercises the same rendering logic in a flat format that
/// is convenient for unit-test assertions.
#[cfg(test)]
fn build_journal_text(journal: &Journal) -> String {
    if journal.entries.is_empty() {
        return "Journal\n\nNo observations yet.".to_string();
    }

    let mut out = vec!["Journal".to_string()];

    // Collect all fabrication result descriptions across all entries, in
    // insertion order (BTreeMap iteration is deterministic). This mirrors
    // the legacy "Recent Fabrication" section which was a flat log.
    let fabrication_descriptions: Vec<&str> = journal
        .entries
        .values()
        .flat_map(|entry| {
            entry
                .observations_by_category(&ObservationCategory::FabricationResult)
                .iter()
                .map(|o| o.description.as_str())
        })
        .collect();

    if !fabrication_descriptions.is_empty() {
        out.push(String::new());
        out.push("Recent Fabrication".to_string());
        for desc in &fabrication_descriptions {
            out.push(format!("  {desc}"));
        }
    }

    // Sort entries by name for stable, alphabetical display order.
    let mut entries: Vec<&JournalEntry> = journal.entries.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    for entry in entries {
        // Visual separator between entries for legibility.
        out.push(String::new());
        out.push(format!("--- {} ---", entry.name));

        for obs in entry.observations_by_category(&ObservationCategory::SurfaceAppearance) {
            out.push(format!("  Surface: {}", obs.description));
        }

        // Show only the most recent thermal observation (matches legacy
        // behavior where `thermal_observation` was a single `Option<String>`).
        if let Some(thermal) = entry
            .observations_by_category(&ObservationCategory::ThermalBehavior)
            .last()
        {
            out.push(format!("  Heat: {}", thermal.description));
        }

        // Show only the most recent weight observation (matches legacy
        // behavior where `weight_observation` was a single `Option<String>`).
        if let Some(weight) = entry
            .observations_by_category(&ObservationCategory::Weight)
            .last()
        {
            out.push(format!("  Carried: {}", weight.description));
        }

        for obs in entry.observations_by_category(&ObservationCategory::FabricationResult) {
            out.push(format!("  {}", obs.description));
        }
    }

    out.join("\n")
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_omits_unknown_properties() {
        let mut journal = Journal::default();
        journal.record(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Weight: Heavy".into(),
                recorded_at: 1,
            },
        );

        let text = build_journal_text(&journal);
        assert!(text.contains("Weight: Heavy"));
        assert!(!text.contains("Heat:"));
    }

    #[test]
    fn journal_includes_fabrication_history() {
        let mut journal = Journal::default();
        journal.record(
            JournalKey::Fabrication { output_seed: 2 },
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 1,
            },
        );

        let text = build_journal_text(&journal);
        assert!(text.contains("Combined Ferrite + Silite -> Neoite"));
        assert!(text.contains("Recent Fabrication"));
    }

    #[test]
    fn journal_shows_thermal_observation_when_present() {
        let mut journal = Journal::default();
        journal.record(
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "TestMat",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Reliably hold together under heat".into(),
                recorded_at: 1,
            },
        );

        let text = build_journal_text(&journal);
        assert!(text.contains("Heat: Reliably hold together under heat"));
    }

    #[test]
    fn journal_key_material_equality() {
        let a = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };
        let b = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };
        let c = JournalKey::Material {
            seed: 99,
            planet_seed: None,
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    /// `planet_seed` participates in `JournalKey::Material` identity: two
    /// otherwise-identical material keys captured on different planets
    /// must be distinct so the journal records them as separate entries.
    /// This is what lets the upcoming context filter (Story 10.3) treat
    /// "Ferrite seen on Planet A" and "Ferrite seen on Planet B" as
    /// independent observations.
    #[test]
    fn journal_key_material_planet_seed_participates_in_equality() {
        let unknown = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };
        let on_planet_a = JournalKey::Material {
            seed: 42,
            planet_seed: Some(1),
        };
        let on_planet_b = JournalKey::Material {
            seed: 42,
            planet_seed: Some(2),
        };
        let on_planet_a_again = JournalKey::Material {
            seed: 42,
            planet_seed: Some(1),
        };

        assert_ne!(unknown, on_planet_a);
        assert_ne!(on_planet_a, on_planet_b);
        assert_eq!(on_planet_a, on_planet_a_again);
    }

    /// Derived `Ord` sorts material keys primarily by `seed`, with
    /// `planet_seed` acting as a tiebreaker (`None` < `Some(_)` per the
    /// standard library's `Option` ordering).  Pre-existing tests assume
    /// the first axis is `seed` — this test pins both axes so a future
    /// field-reordering change in `JournalKey` cannot silently re-shuffle
    /// the `BTreeMap` iteration order the journal UI depends on.
    #[test]
    fn journal_key_material_ord_seed_then_planet_seed() {
        let mut keys = vec![
            JournalKey::Material {
                seed: 2,
                planet_seed: Some(0),
            },
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(99),
            },
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(1),
            },
        ];
        keys.sort();
        assert_eq!(
            keys,
            vec![
                JournalKey::Material {
                    seed: 1,
                    planet_seed: None
                },
                JournalKey::Material {
                    seed: 1,
                    planet_seed: Some(1)
                },
                JournalKey::Material {
                    seed: 1,
                    planet_seed: Some(99)
                },
                JournalKey::Material {
                    seed: 2,
                    planet_seed: Some(0)
                },
            ],
        );
    }

    #[test]
    fn journal_filter_default_is_unrestricted() {
        // The "All" filter required by the acceptance criteria is the
        // Default value: both dimensions are `None`, meaning no
        // restriction on either category or context.
        let filter = JournalFilter::default();
        assert!(filter.category.is_none());
        assert!(filter.context.is_none());
    }

    #[test]
    fn journal_filter_equality_distinguishes_dimensions() {
        // Two filters are equal iff both dimensions match exactly; this
        // is what allows later tasks to cache filtered results keyed by
        // the active filter and skip recomputation when nothing changed.
        let a = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        let b = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        let different_category = JournalFilter {
            category: Some(ObservationCategory::ThermalBehavior),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        let different_context = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 8 }),
        };
        assert_eq!(a, b);
        assert_ne!(a, different_category);
        assert_ne!(a, different_context);
    }

    #[test]
    fn journal_ui_state_default_filter_is_unrestricted() {
        // The UI resource starts with the "All" filter so the default
        // experience matches the Story 10.3 acceptance criterion that
        // "'All' filter shows everything (default)".
        let state = JournalUiState::default();
        assert_eq!(*state.filter(), JournalFilter::default());
        assert!(state.filter().category.is_none());
        assert!(state.filter().context.is_none());
    }

    #[test]
    fn journal_ui_state_set_filter_replaces_active_filter() {
        // The setter is the only public path for mutating the filter; it
        // must store exactly the value passed in so later UI cycling code
        // can rely on round-trip equality when comparing the previous and
        // current filter to detect changes.
        let mut state = JournalUiState::default();
        let new_filter = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
        };
        state.set_filter(new_filter.clone());
        assert_eq!(*state.filter(), new_filter);
    }

    #[test]
    fn journal_ui_state_filter_persists_across_visibility_toggle() {
        // Story 10.3 acceptance criterion: "Filter state persists when
        // journal is toggled closed/open".  Because the filter lives on
        // the long-lived `JournalUiState` resource — not derived from
        // visibility — toggling `visible` must not disturb it.
        let mut state = JournalUiState::default();
        let active = JournalFilter {
            category: Some(ObservationCategory::ThermalBehavior),
            context: None,
        };
        state.set_filter(active.clone());
        state.set_visible(true);
        assert_eq!(*state.filter(), active);
        state.set_visible(false);
        assert_eq!(*state.filter(), active);
        state.set_visible(true);
        assert_eq!(*state.filter(), active);
    }

    #[test]
    fn journal_context_biome_equality_is_string_based() {
        // CurrentBiome carries a registry key as a String; equality is
        // straightforward string equality, which is what the matching
        // logic in later tasks will rely on.
        let a = JournalContext::CurrentBiome {
            biome_key: "tundra".to_string(),
        };
        let b = JournalContext::CurrentBiome {
            biome_key: "tundra".to_string(),
        };
        let c = JournalContext::CurrentBiome {
            biome_key: "basalt_flats".to_string(),
        };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    // ── matches_filter ─────────────────────────────────────────────
    //
    // Helpers for the matches_filter tests below.  Build a small entry
    // with a single observation so the category dimension can be
    // exercised independently of the context dimension.
    fn entry_with_observation(key: JournalKey, category: ObservationCategory) -> JournalEntry {
        let mut entry = JournalEntry::new(key, "Subject".to_string(), 0);
        entry.add_observation(Observation {
            category,
            confidence: ConfidenceLevel::Tentative,
            description: "obs".to_string(),
            recorded_at: 0,
        });
        entry
    }

    #[test]
    fn matches_filter_default_accepts_every_entry() {
        // The "All" filter (Default) imposes no restriction on either
        // dimension; every entry — including one with no observations —
        // must pass.  This is the Story 10.3 default behaviour.
        let filter = JournalFilter::default();
        let entry = JournalEntry::new(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Empty".to_string(),
            0,
        );
        assert!(matches_filter(&entry, &filter));

        let populated = entry_with_observation(
            JournalKey::Material {
                seed: 2,
                planet_seed: Some(99),
            },
            ObservationCategory::SurfaceAppearance,
        );
        assert!(matches_filter(&populated, &filter));
    }

    #[test]
    fn matches_filter_category_only_keeps_matching_entries() {
        // With only a category restriction set, an entry must contain at
        // least one observation in that category to pass.
        let filter = JournalFilter {
            category: Some(ObservationCategory::ThermalBehavior),
            context: None,
        };

        let thermal = entry_with_observation(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            ObservationCategory::ThermalBehavior,
        );
        assert!(matches_filter(&thermal, &filter));

        let surface = entry_with_observation(
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            ObservationCategory::SurfaceAppearance,
        );
        assert!(!matches_filter(&surface, &filter));
    }

    #[test]
    fn matches_filter_category_rejects_entry_with_no_observations() {
        // An entry with zero observations carries no evidence of any
        // category and therefore fails any `Some(category)` restriction.
        let filter = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: None,
        };
        let empty = JournalEntry::new(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Empty".to_string(),
            0,
        );
        assert!(!matches_filter(&empty, &filter));
    }

    #[test]
    fn matches_filter_current_planet_uses_key_planet_seed() {
        // CurrentPlanet matches an entry iff its key's planet_seed
        // equals the filter's seed.  Entries without a recorded planet
        // (planet_seed == None) are excluded — "unknown provenance"
        // must not silently masquerade as "current planet".
        let filter = JournalFilter {
            category: None,
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };

        let on_planet = entry_with_observation(
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(7),
            },
            ObservationCategory::SurfaceAppearance,
        );
        assert!(matches_filter(&on_planet, &filter));

        let other_planet = entry_with_observation(
            JournalKey::Material {
                seed: 2,
                planet_seed: Some(8),
            },
            ObservationCategory::SurfaceAppearance,
        );
        assert!(!matches_filter(&other_planet, &filter));

        let no_planet = entry_with_observation(
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            ObservationCategory::SurfaceAppearance,
        );
        assert!(!matches_filter(&no_planet, &filter));
    }

    #[test]
    fn matches_filter_current_planet_excludes_fabrications() {
        // Fabrications are not tied to a discovery planet; the
        // CurrentPlanet filter therefore intentionally hides them.
        let filter = JournalFilter {
            category: None,
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        let fab = entry_with_observation(
            JournalKey::Fabrication { output_seed: 42 },
            ObservationCategory::FabricationResult,
        );
        assert!(!matches_filter(&fab, &filter));
    }

    #[test]
    fn matches_filter_combined_uses_and_logic() {
        // Both dimensions must match.  Verify the four corners of the
        // 2x2 truth table for an entry on planet 7 with a Surface
        // observation against a Surface + planet 7 filter.
        let entry = entry_with_observation(
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(7),
            },
            ObservationCategory::SurfaceAppearance,
        );

        let both_match = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        assert!(matches_filter(&entry, &both_match));

        let category_mismatch = JournalFilter {
            category: Some(ObservationCategory::ThermalBehavior),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        assert!(!matches_filter(&entry, &category_mismatch));

        let context_mismatch = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 8 }),
        };
        assert!(!matches_filter(&entry, &context_mismatch));

        let both_mismatch = JournalFilter {
            category: Some(ObservationCategory::ThermalBehavior),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 8 }),
        };
        assert!(!matches_filter(&entry, &both_mismatch));
    }

    #[test]
    fn matches_filter_current_biome_is_no_op_until_data_capture() {
        // Biome provenance is not yet captured on JournalKey; the
        // CurrentBiome arm therefore matches every entry.  This
        // documents the intentional placeholder behaviour so a future
        // change that wires biome capture through can update this test
        // alongside the implementation.
        let filter = JournalFilter {
            category: None,
            context: Some(JournalContext::CurrentBiome {
                biome_key: "tundra".to_string(),
            }),
        };
        let entry = entry_with_observation(
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(7),
            },
            ObservationCategory::SurfaceAppearance,
        );
        assert!(matches_filter(&entry, &filter));
    }

    #[test]
    fn journal_key_planet_seed_accessor() {
        // Material carries an Option<u64>; Fabrication is always None.
        assert_eq!(
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(42),
            }
            .planet_seed(),
            Some(42)
        );
        assert_eq!(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            }
            .planet_seed(),
            None
        );
        assert_eq!(
            JournalKey::Fabrication { output_seed: 7 }.planet_seed(),
            None
        );
    }

    #[test]
    fn matches_filter_handles_500_entries_quickly() {
        // Story 10.3 acceptance criterion: "Filtering is responsive
        // with 100+ entries" / "Performance: filtering 500 entries
        // < 1ms".  The threshold here is generous (10ms) to absorb
        // noise on loaded CI hardware while still catching pathological
        // regressions that would land us in the seconds.
        let entries: Vec<JournalEntry> = (0..500u64)
            .map(|i| {
                entry_with_observation(
                    JournalKey::Material {
                        seed: i,
                        planet_seed: Some(i % 4),
                    },
                    if i % 2 == 0 {
                        ObservationCategory::SurfaceAppearance
                    } else {
                        ObservationCategory::ThermalBehavior
                    },
                )
            })
            .collect();

        let filter = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 2 }),
        };

        let start = std::time::Instant::now();
        let kept = entries
            .iter()
            .filter(|e| matches_filter(e, &filter))
            .count();
        let elapsed = start.elapsed();

        // seed % 4 == 2 yields 125 entries (seeds 2, 6, 10, …, 498);
        // every such seed is even, so it also satisfies the
        // SurfaceAppearance category filter.  Hence 125 entries pass.
        assert_eq!(kept, 125);
        assert!(
            elapsed < std::time::Duration::from_millis(10),
            "matches_filter over 500 entries took {elapsed:?}, expected < 10ms"
        );
    }

    #[test]
    fn journal_key_fabrication_equality() {
        let a = JournalKey::Fabrication { output_seed: 7 };
        let b = JournalKey::Fabrication { output_seed: 7 };
        let c = JournalKey::Fabrication { output_seed: 8 };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn journal_key_variants_are_distinct() {
        let mat = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };
        let fab = JournalKey::Fabrication { output_seed: 42 };
        assert_ne!(mat, fab);
    }

    #[test]
    fn journal_key_serde_round_trip() {
        let keys = vec![
            JournalKey::Material {
                seed: 123,
                planet_seed: None,
            },
            JournalKey::Fabrication { output_seed: 456 },
        ];
        for key in &keys {
            let json = serde_json::to_string(key).expect("JournalKey should serialize to JSON");
            let deserialized: JournalKey =
                serde_json::from_str(&json).expect("JournalKey should deserialize from JSON");
            assert_eq!(*key, deserialized);
        }
    }

    #[test]
    fn journal_key_btreemap_ordering_is_stable() {
        use std::collections::BTreeMap;
        let mut map = BTreeMap::new();
        map.insert(JournalKey::Fabrication { output_seed: 1 }, "fab");
        map.insert(
            JournalKey::Material {
                seed: 99,
                planet_seed: None,
            },
            "mat99",
        );
        map.insert(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "mat1",
        );

        let keys: Vec<_> = map.keys().collect();
        // Derived Ord: enum variants ordered by declaration (Material < Fabrication),
        // then by field values within each variant.
        assert_eq!(
            *keys[0],
            JournalKey::Material {
                seed: 1,
                planet_seed: None
            }
        );
        assert_eq!(
            *keys[1],
            JournalKey::Material {
                seed: 99,
                planet_seed: None
            }
        );
        assert_eq!(*keys[2], JournalKey::Fabrication { output_seed: 1 });
    }

    #[test]
    fn journal_shows_weight_observation_only_when_present() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 4,
            planet_seed: None,
        };
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Color: Cool blue tone".into(),
                recorded_at: 1,
            },
        );

        let without_weight = build_journal_text(&journal);
        assert!(!without_weight.contains("Carried: Heavy but manageable"));

        journal.record(
            key,
            "Ferrite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Observed,
                description: "Heavy but manageable".into(),
                recorded_at: 2,
            },
        );

        let with_weight = build_journal_text(&journal);
        assert!(with_weight.contains("Carried: Heavy but manageable"));
    }

    // ── New data model tests ────────────────────────────────────────────

    #[test]
    fn journal_entry_new_sets_timestamps() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let entry = JournalEntry::new(key.clone(), "Ferrite".into(), 100);
        assert_eq!(entry.key, key);
        assert_eq!(entry.name, "Ferrite");
        assert!(entry.observations.is_empty());
        assert_eq!(entry.first_observed_at, 100);
        assert_eq!(entry.last_updated_at, 100);
    }

    #[test]
    fn journal_entry_add_observation_updates_timestamp() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 20,
        });

        assert_eq!(entry.observation_count(), 1);
        assert_eq!(entry.first_observed_at, 10);
        assert_eq!(entry.last_updated_at, 20);
    }

    #[test]
    fn journal_entry_accumulates_multiple_observations() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Observed,
            description: "Holds together under heat".into(),
            recorded_at: 50,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::Weight,
            confidence: ConfidenceLevel::Tentative,
            description: "Heavy".into(),
            recorded_at: 55,
        });

        assert_eq!(entry.observation_count(), 3);
        assert_eq!(entry.last_updated_at, 55);
    }

    #[test]
    fn journal_entry_observations_by_category() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Observed,
            description: "Holds together under heat".into(),
            recorded_at: 20,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Observed,
            description: "Slightly rough texture".into(),
            recorded_at: 30,
        });

        let surface = entry.observations_by_category(&ObservationCategory::SurfaceAppearance);
        assert_eq!(surface.len(), 2);
        assert_eq!(surface[0].description, "Warm rust tone");
        assert_eq!(surface[1].description, "Slightly rough texture");

        let thermal = entry.observations_by_category(&ObservationCategory::ThermalBehavior);
        assert_eq!(thermal.len(), 1);

        let weight = entry.observations_by_category(&ObservationCategory::Weight);
        assert!(weight.is_empty());
    }

    #[test]
    fn new_journal_ensure_entry_creates_and_retrieves() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };

        journal.ensure_entry(key.clone(), "Ferrite", 100);
        journal.ensure_entry(key.clone(), "Ignored Name", 200);

        assert_eq!(journal.entries.len(), 1);
        let entry = journal.entries.get(&key).expect("entry should exist");
        // First name wins.
        assert_eq!(entry.name, "Ferrite");
        // Timestamps unchanged by second ensure_entry call.
        assert_eq!(entry.first_observed_at, 100);
    }

    #[test]
    fn new_journal_record_accumulates_observations() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };

        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Holds together under heat".into(),
                recorded_at: 50,
            },
        );

        let entry = journal.entries.get(&key).expect("entry should exist");
        assert_eq!(entry.observation_count(), 2);
        assert_eq!(entry.first_observed_at, 10);
        assert_eq!(entry.last_updated_at, 50);
    }

    #[test]
    fn new_journal_different_keys_coexist() {
        let mut journal = Journal::default();
        let mat_key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let fab_key = JournalKey::Fabrication { output_seed: 2 };

        journal.record(
            mat_key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            fab_key.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 20,
            },
        );

        assert_eq!(journal.entries.len(), 2);
        assert!(journal.entries.contains_key(&mat_key));
        assert!(journal.entries.contains_key(&fab_key));
    }

    #[test]
    fn new_journal_serde_round_trip() {
        let mut journal = Journal::default();
        journal.record(
            JournalKey::Material {
                seed: 42,
                planet_seed: None,
            },
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            JournalKey::Fabrication { output_seed: 99 },
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 50,
            },
        );

        let json = serde_json::to_string(&journal).expect("Journal should serialize to JSON");
        let deserialized: Journal =
            serde_json::from_str(&json).expect("Journal should deserialize from JSON");

        assert_eq!(deserialized.entries.len(), 2);
        let ferrite = deserialized
            .entries
            .get(&JournalKey::Material {
                seed: 42,
                planet_seed: None,
            })
            .expect("Ferrite entry should exist");
        assert_eq!(ferrite.name, "Ferrite");
        assert_eq!(ferrite.observation_count(), 1);
        assert_eq!(ferrite.first_observed_at, 10);
    }

    #[test]
    fn new_journal_empty_default() {
        let journal = Journal::default();
        assert!(journal.entries.is_empty());
    }

    #[test]
    fn empty_journal_renders_no_observations_yet() {
        let journal = Journal::default();
        let text = build_journal_text(&journal);
        assert_eq!(text, "Journal\n\nNo observations yet.");
    }

    #[test]
    fn single_observation_recorded_correctly() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 55,
            planet_seed: None,
        };

        journal.record(
            key.clone(),
            "Quarite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Cracks under rapid heating".into(),
                recorded_at: 42,
            },
        );

        // Exactly one entry created for the key.
        assert_eq!(journal.entries.len(), 1);
        let entry = journal.entries.get(&key).expect("entry should exist");

        // Entry metadata is correct.
        assert_eq!(entry.key, key);
        assert_eq!(entry.name, "Quarite");
        assert_eq!(entry.first_observed_at, 42);
        assert_eq!(entry.last_updated_at, 42);

        // Exactly one observation stored.
        assert_eq!(entry.observation_count(), 1);
        let obs = &entry.observations_by_category(&ObservationCategory::ThermalBehavior)[0];
        assert_eq!(obs.category, ObservationCategory::ThermalBehavior);
        assert_eq!(obs.confidence, ConfidenceLevel::Observed);
        assert_eq!(obs.description, "Cracks under rapid heating");
        assert_eq!(obs.recorded_at, 42);
    }

    #[test]
    fn duplicate_observation_same_category_and_description_is_skipped() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        // Same category + same description at a later tick — should NOT add a second entry.
        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 20,
        });

        assert_eq!(entry.observation_count(), 1, "duplicate should be skipped");
        // Timestamp still advances even when the observation is deduplicated.
        assert_eq!(entry.last_updated_at, 20);
    }

    #[test]
    fn duplicate_observation_upgrades_confidence() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Tentative,
            description: "Holds together under heat".into(),
            recorded_at: 10,
        });
        // Same category + description but higher confidence — should upgrade.
        entry.add_observation(Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Confident,
            description: "Holds together under heat".into(),
            recorded_at: 30,
        });

        assert_eq!(entry.observation_count(), 1, "duplicate should be skipped");
        assert_eq!(
            entry.observations_by_category(&ObservationCategory::ThermalBehavior)[0].confidence,
            ConfidenceLevel::Confident,
            "confidence should be upgraded"
        );
        assert_eq!(entry.last_updated_at, 30);
    }

    #[test]
    fn duplicate_does_not_downgrade_confidence() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::Weight,
            confidence: ConfidenceLevel::Confident,
            description: "Heavy".into(),
            recorded_at: 10,
        });
        // Same category + description but lower confidence — confidence should stay.
        entry.add_observation(Observation {
            category: ObservationCategory::Weight,
            confidence: ConfidenceLevel::Tentative,
            description: "Heavy".into(),
            recorded_at: 20,
        });

        assert_eq!(entry.observation_count(), 1);
        assert_eq!(
            entry.observations_by_category(&ObservationCategory::Weight)[0].confidence,
            ConfidenceLevel::Confident,
            "confidence should not downgrade"
        );
    }

    /// Examining the same material twice with identical observations must not
    /// duplicate the journal entry or its observations. The journal should
    /// contain exactly one entry with one observation, and confidence should
    /// be preserved (or upgraded if the second look is stronger).
    #[test]
    fn examine_same_material_twice_does_not_duplicate() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };

        let observation = Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        };

        // First examination.
        journal.record(key.clone(), "Ferrite", observation.clone());

        // Second examination — identical observation at a later tick.
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 50,
            },
        );

        assert_eq!(journal.entries.len(), 1, "only one entry for the material");
        let entry = journal.entries.get(&key).expect("entry must exist");
        assert_eq!(
            entry.observation_count(),
            1,
            "duplicate observation must not be appended"
        );
        assert_eq!(entry.last_updated_at, 50, "timestamp should advance");
    }

    /// Examining the same material twice where the second look carries higher
    /// confidence upgrades the stored observation without duplicating it.
    #[test]
    fn examine_same_material_twice_upgrades_confidence() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };

        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );

        // Second examination — same description, higher confidence.
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Warm rust tone".into(),
                recorded_at: 50,
            },
        );

        assert_eq!(journal.entries.len(), 1);
        let entry = journal.entries.get(&key).expect("entry must exist");
        assert_eq!(entry.observation_count(), 1);
        assert_eq!(
            entry.observations_by_category(&ObservationCategory::SurfaceAppearance)[0].confidence,
            ConfidenceLevel::Observed,
            "confidence should upgrade on re-examination"
        );
    }

    #[test]
    fn same_category_different_description_is_not_duplicate() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Slightly rough texture".into(),
            recorded_at: 20,
        });

        assert_eq!(
            entry
                .observations_by_category(&ObservationCategory::SurfaceAppearance)
                .len(),
            2,
            "different descriptions are distinct observations"
        );
    }

    #[test]
    fn same_description_different_category_is_not_duplicate() {
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Notable".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::LocationNote,
            confidence: ConfidenceLevel::Tentative,
            description: "Notable".into(),
            recorded_at: 20,
        });

        assert_eq!(
            entry.observation_count(),
            2,
            "different categories are distinct observations"
        );
    }

    /// Multiple observations recorded against the same `JournalKey` via
    /// `Journal::record` accumulate in chronological order. The entry is
    /// created once and subsequent observations append without replacing
    /// earlier ones, timestamps track the full observation window, and each
    /// observation preserves its own category, confidence, and description.
    #[test]
    fn multiple_observations_for_same_key_accumulate() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 77,
            planet_seed: None,
        };

        // First observation — creates the entry.
        journal.record(
            key.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Dark mineral grey".into(),
                recorded_at: 10,
            },
        );

        // Second observation — same key, different category.
        journal.record(
            key.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Glows faintly when heated".into(),
                recorded_at: 25,
            },
        );

        // Third observation — same key, same category as first but different description.
        journal.record(
            key.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Slightly crystalline texture".into(),
                recorded_at: 40,
            },
        );

        // Fourth observation — same key, yet another category.
        journal.record(
            key.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Very heavy".into(),
                recorded_at: 60,
            },
        );

        // Fifth observation — fabrication result recorded against the same material key.
        journal.record(
            key.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Volite + Silite -> Crystite".into(),
                recorded_at: 80,
            },
        );

        // Only one entry exists for the key.
        assert_eq!(journal.entries.len(), 1, "all observations share one entry");
        let entry = journal.entries.get(&key).expect("entry should exist");

        // Name set by the first record call is retained.
        assert_eq!(entry.name, "Volite");

        // Timestamps span the full observation window.
        assert_eq!(entry.first_observed_at, 10);
        assert_eq!(entry.last_updated_at, 80);

        // All five distinct observations accumulated across four categories.
        assert_eq!(entry.observation_count(), 5);

        // Verify observations grouped by category.
        let surface = entry.observations_by_category(&ObservationCategory::SurfaceAppearance);
        assert_eq!(surface.len(), 2);
        assert_eq!(surface[0].description, "Dark mineral grey");
        assert_eq!(surface[0].confidence, ConfidenceLevel::Tentative);
        assert_eq!(surface[0].recorded_at, 10);
        assert_eq!(surface[1].description, "Slightly crystalline texture");
        assert_eq!(surface[1].confidence, ConfidenceLevel::Observed);
        assert_eq!(surface[1].recorded_at, 40);

        let thermal = entry.observations_by_category(&ObservationCategory::ThermalBehavior);
        assert_eq!(thermal.len(), 1);
        assert_eq!(thermal[0].description, "Glows faintly when heated");
        assert_eq!(thermal[0].confidence, ConfidenceLevel::Observed);
        assert_eq!(thermal[0].recorded_at, 25);

        let weight = entry.observations_by_category(&ObservationCategory::Weight);
        assert_eq!(weight.len(), 1);
        assert_eq!(weight[0].description, "Very heavy");
        assert_eq!(weight[0].confidence, ConfidenceLevel::Confident);
        assert_eq!(weight[0].recorded_at, 60);

        let fab = entry.observations_by_category(&ObservationCategory::FabricationResult);
        assert_eq!(fab.len(), 1);
        assert_eq!(fab[0].description, "Combined Volite + Silite -> Crystite");
        assert_eq!(fab[0].confidence, ConfidenceLevel::Confident);
        assert_eq!(fab[0].recorded_at, 80);

        let loc = entry.observations_by_category(&ObservationCategory::LocationNote);
        assert_eq!(loc.len(), 0);
    }

    /// Every type in the journal data model serializes to JSON and deserializes
    /// back to an identical value. Covers all `JournalKey` variants, all
    /// `ObservationCategory` variants, all `ConfidenceLevel` variants, the
    /// `Observation` struct, `JournalEntry`, and a `Journal` containing
    /// entries of every key type with observations of every category.
    #[test]
    fn all_types_serde_round_trip() {
        // ── JournalKey variants ─────────────────────────────────────
        let keys = vec![
            JournalKey::Material {
                seed: 0,
                planet_seed: None,
            },
            JournalKey::Material {
                seed: u64::MAX,
                planet_seed: None,
            },
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(0),
            },
            JournalKey::Material {
                seed: 7,
                planet_seed: Some(u64::MAX),
            },
            JournalKey::Fabrication { output_seed: 42 },
        ];
        for key in &keys {
            let json = serde_json::to_string(key).expect("JournalKey should serialize");
            let rt: JournalKey =
                serde_json::from_str(&json).expect("JournalKey should deserialize");
            assert_eq!(*key, rt);
        }

        // ── ObservationCategory variants ────────────────────────────
        let categories = vec![
            ObservationCategory::SurfaceAppearance,
            ObservationCategory::ThermalBehavior,
            ObservationCategory::Weight,
            ObservationCategory::FabricationResult,
            ObservationCategory::LocationNote,
        ];
        for cat in &categories {
            let json = serde_json::to_string(cat).expect("ObservationCategory should serialize");
            let rt: ObservationCategory =
                serde_json::from_str(&json).expect("ObservationCategory should deserialize");
            assert_eq!(*cat, rt);
        }

        // ── ConfidenceLevel variants ────────────────────────────────
        let levels = vec![
            ConfidenceLevel::Tentative,
            ConfidenceLevel::Observed,
            ConfidenceLevel::Confident,
        ];
        for level in &levels {
            let json = serde_json::to_string(level).expect("ConfidenceLevel should serialize");
            let rt: ConfidenceLevel =
                serde_json::from_str(&json).expect("ConfidenceLevel should deserialize");
            assert_eq!(*level, rt);
        }

        // ── Observation struct ──────────────────────────────────────
        let observation = Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Observed,
            description: "Holds together under heat".into(),
            recorded_at: 999,
        };
        let json = serde_json::to_string(&observation).expect("Observation should serialize");
        let rt: Observation = serde_json::from_str(&json).expect("Observation should deserialize");
        assert_eq!(rt.category, observation.category);
        assert_eq!(rt.confidence, observation.confidence);
        assert_eq!(rt.description, observation.description);
        assert_eq!(rt.recorded_at, observation.recorded_at);

        // ── JournalEntry struct ─────────────────────────────────────
        let mut entry = JournalEntry::new(
            JournalKey::Material {
                seed: 7,
                planet_seed: None,
            },
            "Ferrite".into(),
            10,
        );
        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::Weight,
            confidence: ConfidenceLevel::Confident,
            description: "Very heavy".into(),
            recorded_at: 20,
        });

        let json = serde_json::to_string(&entry).expect("JournalEntry should serialize");
        let rt: JournalEntry =
            serde_json::from_str(&json).expect("JournalEntry should deserialize");
        assert_eq!(rt.key, entry.key);
        assert_eq!(rt.name, entry.name);
        assert_eq!(rt.observation_count(), 2);
        assert_eq!(rt.first_observed_at, entry.first_observed_at);
        assert_eq!(rt.last_updated_at, entry.last_updated_at);
        assert_eq!(
            rt.observations_by_category(&ObservationCategory::SurfaceAppearance)
                .len(),
            1
        );
        assert_eq!(
            rt.observations_by_category(&ObservationCategory::Weight)
                .len(),
            1
        );

        // ── Journal with all key types and all categories ────────
        let mut journal = Journal::default();

        // Material entry with surface, thermal, and weight observations.
        let mat_key = JournalKey::Material {
            seed: 100,
            planet_seed: None,
        };
        journal.record(
            mat_key.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Cool blue tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            mat_key.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Softens quickly under heat".into(),
                recorded_at: 5,
            },
        );
        journal.record(
            mat_key.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Light".into(),
                recorded_at: 8,
            },
        );

        // Fabrication entry with fabrication result and location note.
        let fab_key = JournalKey::Fabrication { output_seed: 200 };
        journal.record(
            fab_key.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Silite + Ferrite -> Neoite".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            fab_key.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::LocationNote,
                confidence: ConfidenceLevel::Tentative,
                description: "Found near volcanic ridge".into(),
                recorded_at: 15,
            },
        );

        let json = serde_json::to_string(&journal).expect("Journal should serialize");
        let rt: Journal = serde_json::from_str(&json).expect("Journal should deserialize");

        // Verify structure preserved.
        assert_eq!(rt.entries.len(), 2);

        let silite = rt
            .entries
            .get(&mat_key)
            .expect("Material entry should exist");
        assert_eq!(silite.name, "Silite");
        assert_eq!(silite.observation_count(), 3);
        assert_eq!(silite.first_observed_at, 1);
        assert_eq!(silite.last_updated_at, 8);
        assert_eq!(
            silite
                .observations_by_category(&ObservationCategory::SurfaceAppearance)
                .len(),
            1
        );
        assert_eq!(
            silite.observations_by_category(&ObservationCategory::SurfaceAppearance)[0].confidence,
            ConfidenceLevel::Tentative
        );
        assert_eq!(
            silite
                .observations_by_category(&ObservationCategory::ThermalBehavior)
                .len(),
            1
        );
        assert_eq!(
            silite
                .observations_by_category(&ObservationCategory::Weight)
                .len(),
            1
        );

        let neoite = rt
            .entries
            .get(&fab_key)
            .expect("Fabrication entry should exist");
        assert_eq!(neoite.name, "Neoite");
        assert_eq!(neoite.observation_count(), 2);
        assert_eq!(neoite.first_observed_at, 10);
        assert_eq!(neoite.last_updated_at, 15);
        assert_eq!(
            neoite
                .observations_by_category(&ObservationCategory::FabricationResult)
                .len(),
            1
        );
        assert_eq!(
            neoite.observations_by_category(&ObservationCategory::FabricationResult)[0].confidence,
            ConfidenceLevel::Confident
        );
        assert_eq!(
            neoite
                .observations_by_category(&ObservationCategory::LocationNote)
                .len(),
            1
        );
        assert_eq!(
            neoite.observations_by_category(&ObservationCategory::LocationNote)[0].description,
            "Found near volcanic ridge"
        );
    }

    /// Different `JournalKey`s are stored independently: observations recorded
    /// against one key never appear in, modify, or interfere with entries keyed
    /// by a different key — even when seeds overlap across variant types or when
    /// the same observation category is used for multiple subjects.
    #[test]
    fn different_keys_stored_independently() {
        let mut journal = Journal::default();

        // Three keys: two Material keys with different seeds and one
        // Fabrication key whose output_seed numerically equals the first
        // Material seed (verifies variant-level isolation).
        let mat_a = JournalKey::Material {
            seed: 10,
            planet_seed: None,
        };
        let mat_b = JournalKey::Material {
            seed: 20,
            planet_seed: None,
        };
        let fab_a = JournalKey::Fabrication { output_seed: 10 };

        // Record a surface observation on material A.
        journal.record(
            mat_a.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );

        // Record a surface observation on material B (same category, different key).
        journal.record(
            mat_b.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Cool blue tone".into(),
                recorded_at: 2,
            },
        );

        // Record a fabrication result on fab_a (same numeric id as mat_a).
        journal.record(
            fab_a.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 3,
            },
        );

        // Add a second observation to material A to verify accumulation is
        // scoped to that key alone.
        journal.record(
            mat_a.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Holds together under heat".into(),
                recorded_at: 4,
            },
        );

        // ── Verify entry count ──────────────────────────────────────
        assert_eq!(
            journal.entries.len(),
            3,
            "three distinct keys = three entries"
        );

        // ── Verify material A ───────────────────────────────────────
        let entry_a = journal
            .entries
            .get(&mat_a)
            .expect("mat_a entry should exist");
        assert_eq!(entry_a.name, "Ferrite");
        assert_eq!(entry_a.observation_count(), 2);
        assert_eq!(entry_a.first_observed_at, 1);
        assert_eq!(entry_a.last_updated_at, 4);
        assert_eq!(
            entry_a.observations_by_category(&ObservationCategory::SurfaceAppearance)[0]
                .description,
            "Warm rust tone"
        );
        assert_eq!(
            entry_a
                .observations_by_category(&ObservationCategory::ThermalBehavior)
                .len(),
            1
        );

        // ── Verify material B ───────────────────────────────────────
        let entry_b = journal
            .entries
            .get(&mat_b)
            .expect("mat_b entry should exist");
        assert_eq!(entry_b.name, "Silite");
        assert_eq!(entry_b.observation_count(), 1);
        assert_eq!(entry_b.first_observed_at, 2);
        assert_eq!(entry_b.last_updated_at, 2);
        assert_eq!(
            entry_b.observations_by_category(&ObservationCategory::SurfaceAppearance)[0]
                .description,
            "Cool blue tone"
        );

        // ── Verify fabrication A (same numeric id as mat_a) ─────────
        let entry_fab = journal
            .entries
            .get(&fab_a)
            .expect("fab_a entry should exist");
        assert_eq!(entry_fab.name, "Neoite");
        assert_eq!(entry_fab.observation_count(), 1);
        assert_eq!(entry_fab.first_observed_at, 3);
        assert_eq!(entry_fab.last_updated_at, 3);
        assert_eq!(
            entry_fab
                .observations_by_category(&ObservationCategory::FabricationResult)
                .len(),
            1
        );

        // ── Cross-contamination checks ──────────────────────────────
        // Material A must not contain material B's or fab_a's observations.
        assert!(
            entry_a
                .all_observations()
                .all(|o| o.description != "Cool blue tone"
                    && o.description != "Combined Ferrite + Silite -> Neoite"),
            "mat_a must not contain observations from other keys"
        );

        // Material B must not contain material A's or fab_a's observations.
        assert!(
            entry_b
                .all_observations()
                .all(|o| o.description != "Warm rust tone"
                    && o.description != "Holds together under heat"
                    && o.description != "Combined Ferrite + Silite -> Neoite"),
            "mat_b must not contain observations from other keys"
        );

        // Fabrication A must not contain either material's observations.
        assert!(
            entry_fab
                .all_observations()
                .all(|o| o.description != "Warm rust tone"
                    && o.description != "Cool blue tone"
                    && o.description != "Holds together under heat"),
            "fab_a must not contain observations from other keys"
        );
    }

    /// The rendered text from `build_journal_text` preserves all information
    /// that the legacy POC journal displayed: material names, surface
    /// observations, thermal observations, weight observations, fabrication
    /// history, and the "Recent Fabrication" header. This test populates a
    /// `Journal` with the same variety of data and asserts every piece of
    /// information appears in the output.
    #[test]
    fn rendered_text_contains_same_information_as_legacy_journal() {
        let mut journal = Journal::default();

        // ── Material entry with surface, thermal, and weight observations ──
        let mat_key = JournalKey::Material {
            seed: 42,
            planet_seed: None,
        };

        journal.record(
            mat_key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            mat_key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Slightly rough texture".into(),
                recorded_at: 2,
            },
        );
        journal.record(
            mat_key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Holds together under heat".into(),
                recorded_at: 3,
            },
        );
        journal.record(
            mat_key,
            "Ferrite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Heavy but manageable".into(),
                recorded_at: 4,
            },
        );

        // ── Second material with only surface observation ───────────────
        // Legacy equivalent: entry with surface_observations only (no thermal
        // or weight).
        let mat_key_b = JournalKey::Material {
            seed: 99,
            planet_seed: None,
        };
        journal.record(
            mat_key_b,
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Cool blue tone".into(),
                recorded_at: 5,
            },
        );

        // ── Fabrication entry ───────────────────────────────────────────
        let fab_key = JournalKey::Fabrication { output_seed: 200 };
        journal.record(
            fab_key,
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 6,
            },
        );

        let text = build_journal_text(&journal);

        // ── Header ──────────────────────────────────────────────────────
        assert!(
            text.starts_with("Journal"),
            "rendered text must start with Journal header"
        );

        // ── Material names displayed ────────────────────────────────────
        assert!(
            text.contains("Ferrite"),
            "material name Ferrite must appear"
        );
        assert!(text.contains("Silite"), "material name Silite must appear");

        // ── Surface observations prefixed with "Surface:" ───────────────
        assert!(
            text.contains("Surface: Warm rust tone"),
            "surface observation must appear with Surface: prefix"
        );
        assert!(
            text.contains("Surface: Slightly rough texture"),
            "multiple surface observations must all appear"
        );
        assert!(
            text.contains("Surface: Cool blue tone"),
            "second material surface observation must appear"
        );

        // ── Thermal observation prefixed with "Heat:" ───────────────────
        assert!(
            text.contains("Heat: Holds together under heat"),
            "thermal observation must appear with Heat: prefix"
        );

        // ── Weight observation prefixed with "Carried:" ─────────────────
        assert!(
            text.contains("Carried: Heavy but manageable"),
            "weight observation must appear with Carried: prefix"
        );

        // ── Fabrication result in "Recent Fabrication" section ───────────
        assert!(
            text.contains("Recent Fabrication"),
            "fabrication header must appear"
        );
        assert!(
            text.contains("Combined Ferrite + Silite -> Neoite"),
            "fabrication description must appear"
        );

        // ── Material without thermal/weight must NOT show those prefixes ─
        // Split rendered text by material name to isolate Silite's section.
        // Silite appears after Ferrite alphabetically — but we verify the
        // overall text does not associate Heat:/Carried: with Silite by
        // checking that Heat: and Carried: only appear once each (belonging
        // to Ferrite).
        let heat_count = text.matches("Heat:").count();
        assert_eq!(
            heat_count, 1,
            "Heat: should appear exactly once (only for Ferrite)"
        );
        let carried_count = text.matches("Carried:").count();
        assert_eq!(
            carried_count, 1,
            "Carried: should appear exactly once (only for Ferrite)"
        );

        // ── Fabrication name listed as an entry ─────────────────────────
        assert!(
            text.contains("Neoite"),
            "fabrication output name must appear as an entry"
        );
    }

    /// An empty journal renders without panic and shows the expected
    /// placeholder text — matching the legacy behavior where an empty
    /// journal simply displayed a header with no entries.
    #[test]
    fn empty_journal_renders_placeholder_text() {
        let journal = Journal::default();
        let text = build_journal_text(&journal);
        assert!(
            text.contains("Journal"),
            "empty journal must still show header"
        );
        assert!(
            text.contains("No observations yet."),
            "empty journal must show placeholder message"
        );
    }

    /// A journal with 100+ entries of mixed key types and observation
    /// categories must not panic during recording, lookup, rendering, or
    /// serialization round-trip.
    #[test]
    fn journal_with_100_plus_mixed_entries_does_not_panic() {
        let categories = [
            ObservationCategory::SurfaceAppearance,
            ObservationCategory::ThermalBehavior,
            ObservationCategory::Weight,
            ObservationCategory::FabricationResult,
            ObservationCategory::LocationNote,
        ];

        let confidences = [
            ConfidenceLevel::Tentative,
            ConfidenceLevel::Observed,
            ConfidenceLevel::Confident,
        ];

        let mut journal = Journal::default();

        // Record 120 entries: 80 Material keys and 40 Fabrication keys,
        // each with between 1 and 3 observations across different categories.
        for i in 0u64..120 {
            let key = if i % 3 == 0 {
                JournalKey::Fabrication { output_seed: i }
            } else {
                JournalKey::Material {
                    seed: i,
                    planet_seed: None,
                }
            };

            let name = format!("Subject-{i}");
            let tick_base = i * 10;

            // Primary observation — category and confidence rotate through variants.
            let primary_cat = &categories[i as usize % categories.len()];
            let primary_conf = confidences[i as usize % confidences.len()];
            journal.record(
                key.clone(),
                &name,
                Observation {
                    category: primary_cat.clone(),
                    confidence: primary_conf,
                    description: format!("Primary observation for {name}"),
                    recorded_at: tick_base,
                },
            );

            // Every other entry gets a second observation in a different category.
            if i % 2 == 0 {
                let secondary_cat = &categories[(i as usize + 1) % categories.len()];
                journal.record(
                    key.clone(),
                    &name,
                    Observation {
                        category: secondary_cat.clone(),
                        confidence: ConfidenceLevel::Tentative,
                        description: format!("Secondary observation for {name}"),
                        recorded_at: tick_base + 1,
                    },
                );
            }

            // Every third entry gets a third observation (same category as
            // primary but different description — should not deduplicate).
            if i % 3 == 0 {
                journal.record(
                    key.clone(),
                    &name,
                    Observation {
                        category: primary_cat.clone(),
                        confidence: ConfidenceLevel::Confident,
                        description: format!("Follow-up observation for {name}"),
                        recorded_at: tick_base + 2,
                    },
                );
            }
        }

        // Verify entry count.
        assert!(
            journal.entries.len() >= 100,
            "expected at least 100 entries, got {}",
            journal.entries.len()
        );

        // Verify both key types are present.
        let material_count = journal
            .entries
            .keys()
            .filter(|k| matches!(k, JournalKey::Material { .. }))
            .count();
        let fabrication_count = journal
            .entries
            .keys()
            .filter(|k| matches!(k, JournalKey::Fabrication { .. }))
            .count();
        assert!(material_count > 0, "must contain Material entries");
        assert!(fabrication_count > 0, "must contain Fabrication entries");

        // Verify all five observation categories are represented.
        let mut seen_categories = std::collections::HashSet::new();
        for entry in journal.entries.values() {
            for cat in entry.observations.keys() {
                seen_categories.insert(cat.clone());
            }
        }
        for cat in &categories {
            assert!(
                seen_categories.contains(cat),
                "category {cat:?} must be present in the journal"
            );
        }

        // Rendering must not panic.
        let text = build_journal_text(&journal);
        assert!(!text.is_empty(), "rendered text must not be empty");

        // Serde round-trip must not panic or lose entries.
        let serialized = serde_json::to_string(&journal).expect("journal must serialize");
        let deserialized: Journal =
            serde_json::from_str(&serialized).expect("journal must deserialize");
        assert_eq!(
            journal.entries.len(),
            deserialized.entries.len(),
            "round-trip must preserve entry count"
        );
    }

    /// Examining 3+ different materials produces separate journal entries with
    /// no cross-contamination. Each material's observations are isolated, and
    /// rendering displays each material as a distinct section with only its
    /// own observations.
    #[test]
    fn multiple_materials_have_separate_entries_and_rendering() {
        let mut journal = Journal::default();

        // ── Material 1: Ferrite ─────────────────────────────────────
        let key_ferrite = JournalKey::Material {
            seed: 10,
            planet_seed: None,
        };
        journal.record(
            key_ferrite.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            key_ferrite.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Holds together under heat".into(),
                recorded_at: 2,
            },
        );

        // ── Material 2: Silite ──────────────────────────────────────
        let key_silite = JournalKey::Material {
            seed: 20,
            planet_seed: None,
        };
        journal.record(
            key_silite.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Cool blue tone".into(),
                recorded_at: 3,
            },
        );
        journal.record(
            key_silite.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Feather-light".into(),
                recorded_at: 4,
            },
        );

        // ── Material 3: Volite ──────────────────────────────────────
        let key_volite = JournalKey::Material {
            seed: 30,
            planet_seed: None,
        };
        journal.record(
            key_volite.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Dark mineral grey".into(),
                recorded_at: 5,
            },
        );
        journal.record(
            key_volite.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Confident,
                description: "Glows faintly when heated".into(),
                recorded_at: 6,
            },
        );
        journal.record(
            key_volite.clone(),
            "Volite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Observed,
                description: "Very heavy".into(),
                recorded_at: 7,
            },
        );

        // ── Material 4: Crystite (ensures "3+" is exceeded) ─────────
        let key_crystite = JournalKey::Material {
            seed: 40,
            planet_seed: None,
        };
        journal.record(
            key_crystite.clone(),
            "Crystite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Translucent with prismatic flecks".into(),
                recorded_at: 8,
            },
        );

        // ── Verify entry separation ─────────────────────────────────
        assert_eq!(journal.entries.len(), 4, "four distinct material entries");
        assert!(journal.entries.contains_key(&key_ferrite));
        assert!(journal.entries.contains_key(&key_silite));
        assert!(journal.entries.contains_key(&key_volite));
        assert!(journal.entries.contains_key(&key_crystite));

        // ── Verify observation counts per entry ─────────────────────
        let ferrite = journal.entries.get(&key_ferrite).unwrap();
        assert_eq!(ferrite.observation_count(), 2);
        assert_eq!(ferrite.name, "Ferrite");

        let silite = journal.entries.get(&key_silite).unwrap();
        assert_eq!(silite.observation_count(), 2);
        assert_eq!(silite.name, "Silite");

        let volite = journal.entries.get(&key_volite).unwrap();
        assert_eq!(volite.observation_count(), 3);
        assert_eq!(volite.name, "Volite");

        let crystite = journal.entries.get(&key_crystite).unwrap();
        assert_eq!(crystite.observation_count(), 1);
        assert_eq!(crystite.name, "Crystite");

        // ── Cross-contamination: each entry contains only its own data ──
        assert!(
            ferrite
                .all_observations()
                .all(|o| o.description == "Warm rust tone"
                    || o.description == "Holds together under heat"),
            "Ferrite must only contain its own observations"
        );
        assert!(
            silite
                .all_observations()
                .all(|o| o.description == "Cool blue tone" || o.description == "Feather-light"),
            "Silite must only contain its own observations"
        );
        assert!(
            volite
                .all_observations()
                .all(|o| o.description == "Dark mineral grey"
                    || o.description == "Glows faintly when heated"
                    || o.description == "Very heavy"),
            "Volite must only contain its own observations"
        );
        assert!(
            crystite
                .all_observations()
                .all(|o| o.description == "Translucent with prismatic flecks"),
            "Crystite must only contain its own observations"
        );

        // ── Verify rendering shows all four materials separated ─────
        let text = build_journal_text(&journal);

        // All material names appear.
        assert!(text.contains("Ferrite"));
        assert!(text.contains("Silite"));
        assert!(text.contains("Volite"));
        assert!(text.contains("Crystite"));

        // Each material's observations appear in the rendered text.
        assert!(text.contains("Surface: Warm rust tone"));
        assert!(text.contains("Heat: Holds together under heat"));
        assert!(text.contains("Surface: Cool blue tone"));
        assert!(text.contains("Carried: Feather-light"));
        assert!(text.contains("Surface: Dark mineral grey"));
        assert!(text.contains("Heat: Glows faintly when heated"));
        assert!(text.contains("Carried: Very heavy"));
        assert!(text.contains("Surface: Translucent with prismatic flecks"));

        // Entries are rendered alphabetically (Crystite, Ferrite, Silite, Volite).
        let pos_crystite = text.find("Crystite").unwrap();
        let pos_ferrite = text.find("Ferrite").unwrap();
        let pos_silite = text.find("Silite").unwrap();
        let pos_volite = text.find("Volite").unwrap();
        assert!(
            pos_crystite < pos_ferrite && pos_ferrite < pos_silite && pos_silite < pos_volite,
            "materials must be rendered in alphabetical order"
        );

        // Thermal observations appear exactly where expected (Ferrite and
        // Volite have thermal data; Silite and Crystite do not).
        assert_eq!(
            text.matches("Heat:").count(),
            2,
            "exactly two materials have thermal observations"
        );
        // Weight observations: Silite and Volite.
        assert_eq!(
            text.matches("Carried:").count(),
            2,
            "exactly two materials have weight observations"
        );
    }

    // ── Two-panel rendering tests ───────────────────────────────────

    /// Helper: create a journal with N material entries named alphabetically.
    fn make_journal_with_n_entries(n: usize) -> Journal {
        let mut journal = Journal::default();
        for i in 0..n {
            let key = JournalKey::Material {
                seed: i as u64,
                planet_seed: None,
            };
            let name = format!("Material-{i:03}");
            journal.record(
                key,
                &name,
                Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: ConfidenceLevel::Tentative,
                    description: format!("Appearance of {name}"),
                    recorded_at: i as u64,
                },
            );
        }
        journal
    }

    #[test]
    fn entry_list_shows_selected_entry_with_prefix() {
        let journal = make_journal_with_n_entries(3);
        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };

        let state = JournalUiState {
            visible: true,
            selected_index: 1,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };

        let list = build_entry_list_text(&entries, &state);
        let lines: Vec<&str> = list.lines().collect();
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0].starts_with(' '),
            "non-selected should start with space"
        );
        assert!(lines[1].starts_with('>'), "selected should start with >");
        assert!(
            lines[2].starts_with(' '),
            "non-selected should start with space"
        );
    }

    #[test]
    fn entry_list_shows_observation_count() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            key,
            "Ferrite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Heat resistant".into(),
                recorded_at: 2,
            },
        );

        let entries: Vec<&JournalEntry> = journal.entries.values().collect();
        let state = JournalUiState::default();
        let list = build_entry_list_text(&entries, &state);
        assert!(
            list.contains("(2 obs)"),
            "entry list should show observation count"
        );
    }

    #[test]
    fn detail_shows_selected_entry_observations() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        journal.record(
            key,
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );

        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };

        let state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };

        let detail = detail_spans_to_string(&build_detail_spans(&entries, &state, true));
        assert!(detail.contains("Ferrite"), "detail should show entry name");
        assert!(
            detail.contains("Surface"),
            "detail should show category group header"
        );
        assert!(
            detail.contains("Warm rust tone"),
            "detail should show observations"
        );
    }

    #[test]
    fn detail_empty_journal_shows_placeholder() {
        let state = JournalUiState::default();
        let entries: Vec<&JournalEntry> = vec![];
        let detail = detail_spans_to_string(&build_detail_spans(&entries, &state, false));
        assert_eq!(detail, "No observations yet.");
    }

    #[test]
    fn detail_filtered_empty_shows_no_matching_entries() {
        let state = JournalUiState::default();
        let entries: Vec<&JournalEntry> = vec![];
        // has_any_entries = true simulates the case where the journal has entries
        // but the current filter produces no results
        let detail = detail_spans_to_string(&build_detail_spans(&entries, &state, true));
        assert_eq!(detail, "No matching entries");
    }

    #[test]
    fn detail_spans_have_correct_kinds() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            key,
            "Ferrite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Observed,
                description: "Heavy but manageable".into(),
                recorded_at: 2,
            },
        );

        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };

        let state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };

        let spans = build_detail_spans(&entries, &state, true);
        // First span: header with entry name.
        assert_eq!(spans[0].kind, DetailSpanKind::Header);
        assert_eq!(spans[0].text, "Ferrite");
        // Surface category group header + observation body + confidence.
        assert_eq!(spans[1].kind, DetailSpanKind::CategoryGroupHeader);
        assert!(spans[1].text.contains("Surface"));
        assert_eq!(spans[2].kind, DetailSpanKind::Body);
        assert!(spans[2].text.contains("Warm rust tone"));
        assert_eq!(spans[3].kind, DetailSpanKind::ConfidenceLabel);
        assert!(spans[3].text.contains("Uncertain"));
        // Weight category group header + observation body + confidence.
        assert_eq!(spans[4].kind, DetailSpanKind::CategoryGroupHeader);
        assert!(spans[4].text.contains("Weight"));
        assert_eq!(spans[5].kind, DetailSpanKind::Body);
        assert!(spans[5].text.contains("Heavy but manageable"));
        assert_eq!(spans[6].kind, DetailSpanKind::ConfidenceLabel);
        assert!(spans[6].text.contains("Noted"));
    }

    #[test]
    fn detail_placeholder_span_kind() {
        let state = JournalUiState::default();
        let entries: Vec<&JournalEntry> = vec![];
        let spans = build_detail_spans(&entries, &state, false);
        assert_eq!(spans.len(), 1);
        assert_eq!(spans[0].kind, DetailSpanKind::Placeholder);
    }

    #[test]
    fn detail_panel_shows_correct_observations_for_selected_entry() {
        let mut journal = Journal::default();

        // Create three entries with distinct observations.
        journal.record(
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Observed,
                description: "Glassy smooth surface".into(),
                recorded_at: 2,
            },
        );
        journal.record(
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Neoite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Surprisingly light".into(),
                recorded_at: 3,
            },
        );

        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };
        // Sorted alphabetically: Ferrite, Neoite, Silite
        assert_eq!(entries[0].name, "Ferrite");
        assert_eq!(entries[1].name, "Neoite");
        assert_eq!(entries[2].name, "Silite");

        // Select first entry (Ferrite) — detail should show Ferrite's observations.
        let state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let detail = detail_spans_to_string(&build_detail_spans(&entries, &state, true));
        assert!(detail.contains("Ferrite"), "header should be Ferrite");
        assert!(
            detail.contains("Warm rust tone"),
            "should show Ferrite's observation"
        );
        assert!(
            !detail.contains("Glassy smooth surface"),
            "should not show Silite's observation"
        );
        assert!(
            !detail.contains("Surprisingly light"),
            "should not show Neoite's observation"
        );

        // Select second entry (Neoite) — detail should show Neoite's observations.
        let state = JournalUiState {
            visible: true,
            selected_index: 1,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let detail = detail_spans_to_string(&build_detail_spans(&entries, &state, true));
        assert!(detail.contains("Neoite"), "header should be Neoite");
        assert!(
            detail.contains("Surprisingly light"),
            "should show Neoite's observation"
        );
        assert!(
            !detail.contains("Warm rust tone"),
            "should not show Ferrite's observation"
        );
        assert!(
            !detail.contains("Glassy smooth surface"),
            "should not show Silite's observation"
        );

        // Select third entry (Silite) — detail should show Silite's observations.
        let state = JournalUiState {
            visible: true,
            selected_index: 2,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let detail = detail_spans_to_string(&build_detail_spans(&entries, &state, true));
        assert!(detail.contains("Silite"), "header should be Silite");
        assert!(
            detail.contains("Glassy smooth surface"),
            "should show Silite's observation"
        );
        assert!(
            !detail.contains("Warm rust tone"),
            "should not show Ferrite's observation"
        );
        assert!(
            !detail.contains("Surprisingly light"),
            "should not show Neoite's observation"
        );
    }

    #[test]
    fn detail_panel_shows_all_observations_for_multi_category_entry() {
        let mut journal = Journal::default();
        let key = JournalKey::Material {
            seed: 1,
            planet_seed: None,
        };

        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Observed,
                description: "Heavy but manageable".into(),
                recorded_at: 2,
            },
        );
        journal.record(
            key,
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Confident,
                description: "Rough, pitted texture".into(),
                recorded_at: 3,
            },
        );

        // Add a second entry to confirm isolation.
        journal.record(
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Glassy smooth surface".into(),
                recorded_at: 4,
            },
        );

        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };

        // Select Ferrite (index 0).
        let state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let spans = build_detail_spans(&entries, &state, true);
        let detail = detail_spans_to_string(&spans);

        // Should contain the header.
        assert_eq!(spans[0].text, "Ferrite");
        assert_eq!(spans[0].kind, DetailSpanKind::Header);

        // Should contain both categories' observations.
        assert!(detail.contains("Surface"), "should have Surface category");
        assert!(detail.contains("Weight"), "should have Weight category");
        assert!(
            detail.contains("Warm rust tone"),
            "should show first surface observation"
        );
        assert!(
            detail.contains("Rough, pitted texture"),
            "should show second surface observation"
        );
        assert!(
            detail.contains("Heavy but manageable"),
            "should show weight observation"
        );

        // Should not contain Silite's observations.
        assert!(
            !detail.contains("Glassy smooth surface"),
            "should not leak other entry's observations"
        );
    }

    #[test]
    fn navigation_clamp_up_from_first_stays_at_first() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        // Simulate pressing up — selection would go to saturating_sub(1) = 0.
        state.selected_index = state.selected_index.saturating_sub(1);
        state.clamp_to_entry_count(5);
        assert_eq!(state.selected_index, 0);
    }

    #[test]
    fn navigation_clamp_down_from_last_stays_at_last() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 4,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        state.selected_index = (state.selected_index + 1).min(4);
        state.clamp_to_entry_count(5);
        assert_eq!(state.selected_index, 4);
    }

    #[test]
    fn scroll_offset_adjusts_when_selection_moves_past_visible_range() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 3,
            filter: JournalFilter::default(),
        };
        // Move selection to index 4 (past the 3-entry window).
        state.selected_index = 4;
        state.clamp_to_entry_count(10);
        // scroll_offset should adjust so index 4 is visible.
        assert!(
            state.scroll_offset + state.entries_per_page > 4,
            "selected entry must be within visible window"
        );
        assert_eq!(state.scroll_offset, 2, "scroll should be 4+1-3=2");
    }

    #[test]
    fn scroll_offset_adjusts_when_selection_moves_above_visible_range() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 1,
            scroll_offset: 3,
            entries_per_page: 3,
            filter: JournalFilter::default(),
        };
        state.clamp_to_entry_count(10);
        assert_eq!(
            state.scroll_offset, 1,
            "scroll should snap to selected index when it is above the window"
        );
    }

    #[test]
    fn page_down_moves_selection_by_entries_per_page() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 2,
            scroll_offset: 0,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        };
        // Simulate PageDown: advance by entries_per_page.
        state.selected_index = (state.selected_index + state.entries_per_page).min(20 - 1);
        state.clamp_to_entry_count(20);
        assert_eq!(state.selected_index, 7);
        // Scroll offset should adjust so index 7 is visible.
        assert!(state.scroll_offset + state.entries_per_page > 7);
    }

    #[test]
    fn page_down_clamps_to_last_entry() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 8,
            scroll_offset: 5,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        };
        let entry_count = 10;
        // PageDown from index 8 with page size 5 would overshoot — should clamp to 9.
        state.selected_index = (state.selected_index + state.entries_per_page).min(entry_count - 1);
        state.clamp_to_entry_count(entry_count);
        assert_eq!(state.selected_index, 9);
    }

    #[test]
    fn page_up_moves_selection_by_entries_per_page() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 12,
            scroll_offset: 10,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        };
        // Simulate PageUp: go back by entries_per_page.
        state.selected_index = state.selected_index.saturating_sub(state.entries_per_page);
        state.clamp_to_entry_count(20);
        assert_eq!(state.selected_index, 7);
        // Scroll offset should snap so index 7 is visible.
        assert!(state.scroll_offset <= 7);
        assert!(state.scroll_offset + state.entries_per_page > 7);
    }

    #[test]
    fn page_up_clamps_to_first_entry() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 2,
            scroll_offset: 0,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        };
        // PageUp from index 2 with page size 5 would underflow — saturating_sub clamps to 0.
        state.selected_index = state.selected_index.saturating_sub(state.entries_per_page);
        state.clamp_to_entry_count(10);
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn home_jumps_to_first_entry() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 42,
            scroll_offset: 30,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        // Simulate Home key — sets selected_index to 0.
        state.selected_index = 0;
        state.clamp_to_entry_count(50);
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn end_jumps_to_last_entry() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 3,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let entry_count = 50;
        // Simulate End key — sets selected_index to last entry.
        state.selected_index = entry_count - 1;
        state.clamp_to_entry_count(entry_count);
        assert_eq!(state.selected_index, 49);
        // scroll_offset should adjust so the last entry is visible.
        assert_eq!(state.scroll_offset, 35);
    }

    #[test]
    fn page_down_adjusts_scroll_offset_past_visible_range() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 3,
            filter: JournalFilter::default(),
        };
        // PageDown jumps selection to index 3, which is outside window [0..3).
        state.selected_index = (state.selected_index + state.entries_per_page).min(10 - 1);
        state.clamp_to_entry_count(10);
        assert_eq!(state.selected_index, 3);
        assert_eq!(
            state.scroll_offset, 1,
            "scroll_offset should be 3+1-3=1 so index 3 is the last visible"
        );
    }

    #[test]
    fn clamp_to_entry_count_zero_entries() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 5,
            scroll_offset: 3,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        state.clamp_to_entry_count(0);
        assert_eq!(state.selected_index, 0);
        assert_eq!(state.scroll_offset, 0);
    }

    #[test]
    fn entry_list_respects_scroll_offset_and_page_size() {
        let journal = make_journal_with_n_entries(20);
        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };

        let state = JournalUiState {
            visible: true,
            selected_index: 5,
            scroll_offset: 3,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        };

        let list = build_entry_list_text(&entries, &state);
        let lines: Vec<&str> = list.lines().collect();
        assert_eq!(lines.len(), 5, "should show exactly entries_per_page lines");
        // The selected entry (index 5) is at position 5-3=2 in the visible window.
        assert!(
            lines[2].starts_with('>'),
            "selected entry should be highlighted"
        );
    }

    #[test]
    fn help_text_shows_page_indicator() {
        let state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let help = build_help_text(42, &state);
        assert!(
            help.contains("[1-15 of 42]"),
            "help should show page indicator, got: {help}"
        );
    }

    #[test]
    fn help_text_empty_journal() {
        let state = JournalUiState::default();
        let help = build_help_text(0, &state);
        assert!(help.contains("J: Close"), "help should show close hint");
        assert!(
            !help.contains("Navigate"),
            "no navigation hints for empty journal"
        );
    }

    #[test]
    fn two_panel_rendering_100_plus_entries_does_not_panic() {
        let journal = make_journal_with_n_entries(120);
        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };

        let mut state = JournalUiState {
            visible: true,
            selected_index: 50,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        state.clamp_to_entry_count(entries.len());

        let list = build_entry_list_text(&entries, &state);
        assert!(!list.is_empty());
        let detail = build_detail_spans(&entries, &state, true);
        assert!(!detail.is_empty());
        let help = build_help_text(entries.len(), &state);
        assert!(help.contains("of 120"));
    }

    /// The two-panel journal UI spawns without panic and the render pipeline
    /// (`compute_journal_panels` → `sync_journal_ui`) executes successfully
    /// when the journal is visible, both with an empty journal and one
    /// populated with entries.  This exercises the full ECS wiring: resource
    /// initialisation, UI node spawning, text computation, and text sync.
    #[test]
    fn panels_render_without_panic() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        // Initialise the resources and spawn the UI nodes that the journal
        // plugin registers at Startup.
        app.init_resource::<JournalUiState>();
        app.init_resource::<JournalRenderCache>();
        app.init_resource::<JournalSelectionTracker>();
        app.add_systems(Startup, spawn_journal_ui);
        app.add_systems(
            Update,
            (
                compute_journal_panels,
                sync_journal_ui.after(compute_journal_panels),
            ),
        );

        // Spawn a player entity with an empty journal.
        app.world_mut()
            .spawn((Player, Journal::default(), Transform::default()));

        // Frame 0: run Startup (spawns UI nodes).
        app.update();

        // Make journal visible so the render path is exercised.
        app.world_mut().resource_mut::<JournalUiState>().visible = true;

        // Frame 1: compute + sync with empty journal — should not panic.
        app.update();

        // Populate the journal with a few entries and re-render.
        {
            let mut query = app
                .world_mut()
                .query_filtered::<&mut Journal, With<Player>>();
            let mut journal = query
                .single_mut(app.world_mut())
                .expect("player must exist");
            for i in 0..5u64 {
                journal.record(
                    JournalKey::Material {
                        seed: i,
                        planet_seed: None,
                    },
                    &format!("Mat-{i}"),
                    Observation {
                        category: ObservationCategory::SurfaceAppearance,
                        confidence: ConfidenceLevel::Tentative,
                        description: format!("Appearance of Mat-{i}"),
                        recorded_at: i,
                    },
                );
            }
        }

        // Frame 2: compute + sync with populated journal — should not panic.
        app.update();

        // Verify the render cache was populated (non-empty list text).
        let cache = app.world().resource::<JournalRenderCache>();
        assert!(
            !cache.list_lines.is_empty(),
            "entry list lines should be populated after rendering with entries"
        );
        assert!(
            !cache.detail_spans.is_empty(),
            "detail spans should be populated after rendering with entries"
        );
        assert!(
            !cache.help.is_empty(),
            "help text should be populated after rendering with entries"
        );
    }

    #[test]
    fn correct_entries_shown_for_given_scroll_offset() {
        let journal = make_journal_with_n_entries(10);
        let entries: Vec<&JournalEntry> = {
            let mut v: Vec<_> = journal.entries.values().collect();
            v.sort_by(|a, b| a.name.cmp(&b.name));
            v
        };
        // Sorted names: Material-000 .. Material-009

        // Page starting at offset 0, page size 3: should show entries 0, 1, 2.
        let state = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 3,
            filter: JournalFilter::default(),
        };
        let lines = build_entry_list_lines(&entries, &state);
        assert_eq!(lines.len(), 3);
        assert!(lines[0].text.contains("Material-000"));
        assert!(lines[1].text.contains("Material-001"));
        assert!(lines[2].text.contains("Material-002"));

        // Page starting at offset 4, page size 3: should show entries 4, 5, 6.
        let state = JournalUiState {
            visible: true,
            selected_index: 5,
            scroll_offset: 4,
            entries_per_page: 3,
            filter: JournalFilter::default(),
        };
        let lines = build_entry_list_lines(&entries, &state);
        assert_eq!(lines.len(), 3);
        assert!(
            lines[0].text.contains("Material-004"),
            "first visible entry should be Material-004, got: {}",
            lines[0].text
        );
        assert!(
            lines[1].text.contains("Material-005"),
            "second visible entry should be Material-005, got: {}",
            lines[1].text
        );
        assert!(
            lines[1].selected,
            "Material-005 at abs index 5 should be selected"
        );
        assert!(
            lines[2].text.contains("Material-006"),
            "third visible entry should be Material-006, got: {}",
            lines[2].text
        );

        // Page at the tail: offset 8, page size 3 but only 2 remain.
        let state = JournalUiState {
            visible: true,
            selected_index: 9,
            scroll_offset: 8,
            entries_per_page: 3,
            filter: JournalFilter::default(),
        };
        let lines = build_entry_list_lines(&entries, &state);
        assert_eq!(
            lines.len(),
            2,
            "should clamp to remaining entries when page extends past end"
        );
        assert!(lines[0].text.contains("Material-008"));
        assert!(lines[1].text.contains("Material-009"));
        assert!(lines[1].selected, "last entry should be selected");
    }

    #[test]
    fn toggle_close_reopen_preserves_selection_and_scroll() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 7,
            scroll_offset: 3,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };

        // Toggle closed.
        state.visible = false;
        // Toggle reopened.
        state.visible = true;

        assert_eq!(state.selected_index, 7, "selection preserved after toggle");
        assert_eq!(state.scroll_offset, 3, "scroll preserved after toggle");
    }

    /// Drives `toggle_journal_visibility` through real `ToggleJournalIntent`
    /// messages and verifies that closing then reopening the journal leaves
    /// the navigation state (`selected_index`, `scroll_offset`, and
    /// `entries_per_page`) untouched.  This exercises the actual system path
    /// that runs in-game, not just direct field manipulation.
    #[test]
    fn toggle_visibility_system_preserves_navigation_state() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_message::<ToggleJournalIntent>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 7,
            scroll_offset: 3,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, toggle_journal_visibility);

        // ── Close: send one toggle intent. ──────────────────────────
        app.world_mut().write_message(ToggleJournalIntent);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert!(!state.visible, "first toggle should hide the journal");
        assert_eq!(
            state.selected_index, 7,
            "closing must not reset selected_index"
        );
        assert_eq!(
            state.scroll_offset, 3,
            "closing must not reset scroll_offset"
        );
        assert_eq!(
            state.entries_per_page, 15,
            "closing must not reset entries_per_page"
        );

        // ── Reopen: send a second toggle intent. ────────────────────
        app.world_mut().write_message(ToggleJournalIntent);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert!(state.visible, "second toggle should re-show the journal");
        assert_eq!(
            state.selected_index, 7,
            "reopening must restore the previous selected_index"
        );
        assert_eq!(
            state.scroll_offset, 3,
            "reopening must restore the previous scroll_offset"
        );
        assert_eq!(
            state.entries_per_page, 15,
            "reopening must preserve entries_per_page"
        );
    }

    /// Verifies that `journal_navigation` ignores all key presses when the
    /// journal is hidden.  We build a minimal `App` with the system, insert
    /// a player with a journal, press ArrowDown, and confirm that
    /// `selected_index` stays at its initial value.
    #[test]
    fn navigation_ignored_when_journal_is_hidden() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: false,
            selected_index: 3,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        // Spawn a player with a journal containing entries so navigation
        // would normally have something to move through.
        let mut journal = Journal::default();
        for i in 0..10 {
            journal.record(
                JournalKey::Material {
                    seed: i,
                    planet_seed: None,
                },
                &format!("Mat-{i:03}"),
                Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: ConfidenceLevel::Tentative,
                    description: format!("Obs {i}"),
                    recorded_at: 0,
                },
            );
        }
        app.world_mut().spawn((Player, journal));

        // Simulate pressing ArrowDown.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);

        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 3,
            "navigation must not change selection when journal is hidden"
        );
        assert_eq!(
            state.scroll_offset, 0,
            "navigation must not change scroll when journal is hidden"
        );
    }

    /// Mirror test: confirms navigation *does* work when the journal is
    /// visible, so the hidden-guard test above isn't vacuously passing.
    #[test]
    fn navigation_active_when_journal_is_visible() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 3,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        let mut journal = Journal::default();
        for i in 0..10 {
            journal.record(
                JournalKey::Material {
                    seed: i,
                    planet_seed: None,
                },
                &format!("Mat-{i:03}"),
                Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: ConfidenceLevel::Tentative,
                    description: format!("Obs {i}"),
                    recorded_at: 0,
                },
            );
        }
        app.world_mut().spawn((Player, journal));

        // Simulate pressing ArrowDown.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);

        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 4,
            "navigation must advance selection when journal is visible"
        );
    }

    /// Verifies full first-to-last navigation: Home key resets to the first
    /// entry, End key jumps to the last entry, ArrowUp from the first entry
    /// stays at first (no wrap), and ArrowDown from the last entry stays at
    /// last (no wrap).
    #[test]
    fn navigation_first_to_last_entry() {
        let entry_count: usize = 20;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        let mut journal = Journal::default();
        for i in 0..entry_count {
            journal.record(
                JournalKey::Material {
                    seed: i.try_into().expect("entry index fits in u64"),
                    planet_seed: None,
                },
                &format!("Mat-{i:03}"),
                Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: ConfidenceLevel::Tentative,
                    description: format!("Obs {i}"),
                    recorded_at: 0,
                },
            );
        }
        app.world_mut().spawn((Player, journal));

        // ── End key: jump from first to last ────────────────────────────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::End);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index,
            entry_count - 1,
            "End key must jump to the last entry"
        );

        // Clear previous input so the next press registers as `just_pressed`.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();

        // ── ArrowDown from last entry: must stay at last (no wrap) ──────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowDown);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index,
            entry_count - 1,
            "ArrowDown at last entry must not wrap or exceed bounds"
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();

        // ── Home key: jump back to first ────────────────────────────────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::Home);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 0,
            "Home key must jump to the first entry"
        );

        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();

        // ── ArrowUp from first entry: must stay at first (no wrap) ──────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::ArrowUp);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 0,
            "ArrowUp at first entry must not wrap below zero"
        );

        // ── Scroll offset tracks selection after End ────────────────────
        // Jump to the end again and verify scroll_offset adjusted so the
        // selected entry is within the visible page.
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::End);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert!(
            state.selected_index >= state.scroll_offset
                && state.selected_index < state.scroll_offset + state.entries_per_page,
            "scroll_offset must keep the selected entry within the visible page \
             (selected={}, scroll_offset={}, entries_per_page={})",
            state.selected_index,
            state.scroll_offset,
            state.entries_per_page,
        );
    }

    /// With a single-entry journal, every navigation key must leave the
    /// selection at index 0 — there is nowhere else to go.
    #[test]
    fn navigation_bounds_single_entry_journal() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        let mut journal = Journal::default();
        journal.record(
            JournalKey::Material {
                seed: 0,
                planet_seed: None,
            },
            "Sole-Material",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Only entry".to_string(),
                recorded_at: 0,
            },
        );
        app.world_mut().spawn((Player, journal));

        let keys_to_test = [
            KeyCode::ArrowDown,
            KeyCode::ArrowUp,
            KeyCode::PageDown,
            KeyCode::PageUp,
            KeyCode::Home,
            KeyCode::End,
        ];

        for key in keys_to_test {
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .clear();
            app.world_mut()
                .resource_mut::<ButtonInput<KeyCode>>()
                .press(key);
            app.update();

            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.selected_index, 0,
                "{key:?} must not move selection past the only entry"
            );
            assert_eq!(
                state.scroll_offset, 0,
                "{key:?} must not shift scroll offset with a single entry"
            );
        }
    }

    /// PageDown at the last entry and PageUp at the first entry must not
    /// exceed bounds when exercised through the full `journal_navigation`
    /// system.
    #[test]
    fn navigation_bounds_page_keys_at_extremes() {
        let entry_count: usize = 20;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        let mut journal = Journal::default();
        for i in 0..entry_count {
            journal.record(
                JournalKey::Material {
                    seed: i.try_into().expect("entry index fits in u64"),
                    planet_seed: None,
                },
                &format!("Mat-{i:03}"),
                Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: ConfidenceLevel::Tentative,
                    description: format!("Obs {i}"),
                    recorded_at: 0,
                },
            );
        }
        app.world_mut().spawn((Player, journal));

        // ── PageUp from index 0: must stay at 0 ────────────────────────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::PageUp);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 0,
            "PageUp at first entry must not go below zero"
        );
        assert_eq!(state.scroll_offset, 0);

        // ── Jump to last entry via End ──────────────────────────────────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::End);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(state.selected_index, entry_count - 1);

        // ── PageDown from last entry: must stay at last ─────────────────
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .clear();
        app.world_mut()
            .resource_mut::<ButtonInput<KeyCode>>()
            .press(KeyCode::PageDown);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index,
            entry_count - 1,
            "PageDown at last entry must not exceed bounds"
        );
        assert!(
            state.selected_index >= state.scroll_offset
                && state.selected_index < state.scroll_offset + state.entries_per_page,
            "scroll_offset must keep selection visible after PageDown at end"
        );
    }

    /// When `selected_index` starts beyond the actual entry count (e.g.,
    /// entries were removed), `clamp_to_entry_count` must pull it back
    /// within valid bounds.
    #[test]
    fn clamp_corrects_out_of_range_selected_index() {
        let mut state = JournalUiState {
            visible: true,
            selected_index: 25,
            scroll_offset: 20,
            entries_per_page: 5,
            filter: JournalFilter::default(),
        };
        state.clamp_to_entry_count(10);
        assert_eq!(
            state.selected_index, 9,
            "selected_index must clamp to last valid index"
        );
        assert!(
            state.selected_index >= state.scroll_offset
                && state.selected_index < state.scroll_offset + state.entries_per_page,
            "scroll_offset must adjust to keep clamped selection visible \
             (selected={}, scroll_offset={}, entries_per_page={})",
            state.selected_index,
            state.scroll_offset,
            state.entries_per_page,
        );
    }

    /// Hammers the full `journal_navigation` system with a long, deterministic
    /// sequence of every navigation key from a variety of starting positions
    /// and asserts that the bounds invariants hold after every single press:
    ///
    /// * `selected_index < entry_count`
    /// * `scroll_offset + entries_per_page` strictly greater than
    ///   `selected_index` (i.e. selection always within the visible window)
    /// * `scroll_offset <= selected_index` (selection not above the window)
    ///
    /// This is the integration-level "navigation does not exceed bounds"
    /// guarantee — point tests cover individual extremes; this test covers
    /// arbitrary sequences against the live system to catch regressions
    /// where any single key handler could silently overshoot.
    #[test]
    fn navigation_never_exceeds_bounds_under_key_sequence() {
        let entry_count: usize = 25;
        let entries_per_page: usize = 7;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        let mut journal = Journal::default();
        for i in 0..entry_count {
            journal.record(
                JournalKey::Material {
                    seed: i.try_into().expect("entry index fits in u64"),
                    planet_seed: None,
                },
                &format!("Mat-{i:03}"),
                Observation {
                    category: ObservationCategory::SurfaceAppearance,
                    confidence: ConfidenceLevel::Tentative,
                    description: format!("Obs {i}"),
                    recorded_at: 0,
                },
            );
        }
        app.world_mut().spawn((Player, journal));

        // A deterministic sequence covering every navigation key, repeated
        // and interleaved so the cumulative position lands at the extremes,
        // mid-page, and across page boundaries.  Repeating the full sequence
        // four times exercises overshoot from both ends multiple times.
        let key_sequence = [
            KeyCode::ArrowDown,
            KeyCode::ArrowDown,
            KeyCode::ArrowDown,
            KeyCode::PageDown,
            KeyCode::PageDown,
            KeyCode::ArrowDown,
            KeyCode::End,
            KeyCode::ArrowDown,
            KeyCode::PageDown,
            KeyCode::ArrowUp,
            KeyCode::PageUp,
            KeyCode::Home,
            KeyCode::ArrowUp,
            KeyCode::PageUp,
            KeyCode::ArrowUp,
        ];

        for repeat in 0..4 {
            for (step, key) in key_sequence.iter().enumerate() {
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .clear();
                app.world_mut()
                    .resource_mut::<ButtonInput<KeyCode>>()
                    .press(*key);
                app.update();

                let state = app.world().resource::<JournalUiState>();
                assert!(
                    state.selected_index < entry_count,
                    "selected_index out of bounds after repeat {repeat} step {step} key {key:?} \
                     (selected={}, entry_count={})",
                    state.selected_index,
                    entry_count,
                );
                assert!(
                    state.scroll_offset <= state.selected_index,
                    "scroll_offset above selection after repeat {repeat} step {step} key {key:?} \
                     (selected={}, scroll_offset={})",
                    state.selected_index,
                    state.scroll_offset,
                );
                assert!(
                    state.selected_index < state.scroll_offset + state.entries_per_page,
                    "selection scrolled out of visible window after repeat {repeat} step {step} \
                     key {key:?} (selected={}, scroll_offset={}, entries_per_page={})",
                    state.selected_index,
                    state.scroll_offset,
                    state.entries_per_page,
                );
                // scroll_offset itself must never exceed the last possible
                // first-visible-row (entry_count - entries_per_page when the
                // list is longer than a page; 0 otherwise).
                let max_scroll = entry_count.saturating_sub(entries_per_page);
                assert!(
                    state.scroll_offset <= max_scroll,
                    "scroll_offset past end-of-list after repeat {repeat} step {step} key {key:?} \
                     (scroll_offset={}, max_scroll={})",
                    state.scroll_offset,
                    max_scroll,
                );
            }
        }
    }

    // ── Graceful entry-deletion tests ───────────────────────────────────
    //
    // These tests exercise `compute_journal_panels`'s reconciliation of
    // `selected_index` against `JournalSelectionTracker`.  They cover the
    // four behaviours promised by the "select nearest on deletion" rule:
    //
    // 1. The selection follows its anchored subject when other entries
    //    are inserted *before* it (sort-position shift).
    // 2. When the selected subject is removed, the highlight moves to the
    //    entry now occupying that sort slot — the nearest in alphabetical
    //    order.
    // 3. When the last entry is removed while selected, the highlight
    //    falls back to the new last entry.
    // 4. When the journal becomes empty, the tracker resets so a future
    //    first observation does not re-anchor onto a stale key.

    /// Helper: build a minimal `App` running just `compute_journal_panels`
    /// against a player-owned journal.  Returns the `App` ready to mutate
    /// the journal and re-run frames.  Visibility is set to true so the
    /// reconciliation path runs every frame.
    fn make_panel_app(initial_entries_per_page: usize) -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<JournalRenderCache>();
        app.init_resource::<JournalSelectionTracker>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: initial_entries_per_page,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, compute_journal_panels);
        app.world_mut()
            .spawn((Player, Journal::default(), Transform::default()));
        app
    }

    /// Helper: append an observation to the player's journal.
    fn record(app: &mut App, key: JournalKey, name: &str, recorded_at: u64) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Journal, With<Player>>();
        let mut journal = query
            .single_mut(app.world_mut())
            .expect("player must exist");
        journal.record(
            key,
            name,
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: format!("Appearance of {name}"),
                recorded_at,
            },
        );
    }

    /// Helper: remove an entry from the player's journal by key.
    fn delete(app: &mut App, key: &JournalKey) {
        let mut query = app
            .world_mut()
            .query_filtered::<&mut Journal, With<Player>>();
        let mut journal = query
            .single_mut(app.world_mut())
            .expect("player must exist");
        journal.entries.remove(key);
    }

    /// Inserting an entry that sorts *before* the selected entry must
    /// shift `selected_index` so the highlight stays on the same subject.
    /// Without the tracker, `selected_index` would still point at index N
    /// — but index N now refers to a different (newly inserted) entry.
    #[test]
    fn selection_follows_subject_when_entry_inserted_before_it() {
        let mut app = make_panel_app(15);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Bravo",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Charlie",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Delta",
            3,
        );
        // Frame 1: panel reconciles initial state.
        app.update();

        // User navigates onto "Charlie" (sort index 1).
        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 1;
        app.update();

        // Insert "Alpha" — sorts before "Bravo", so "Charlie" shifts from
        // index 1 to index 2.
        record(
            &mut app,
            JournalKey::Material {
                seed: 4,
                planet_seed: None,
            },
            "Alpha",
            4,
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 2,
            "selection must follow Charlie to its new sort position"
        );
    }

    /// Deleting the currently selected entry must move the highlight to
    /// the entry now occupying that sort slot — the nearest remaining
    /// entry in alphabetical order.
    #[test]
    fn deleting_selected_entry_selects_nearest_by_sort_position() {
        let mut app = make_panel_app(15);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Charlie",
            3,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 4,
                planet_seed: None,
            },
            "Delta",
            4,
        );
        app.update();

        // Select "Bravo" at index 1.
        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 1;
        app.update();

        // Delete "Bravo".  Sorted list becomes [Alpha, Charlie, Delta].
        // "Charlie" now occupies the old slot (index 1) — that is the
        // nearest entry by sort position.
        delete(
            &mut app,
            &JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 1,
            "highlight must land on the entry now at the deleted slot"
        );

        // And the tracker must have re-anchored onto Charlie so further
        // inserts/deletions follow the right subject.
        let tracker = app.world().resource::<JournalSelectionTracker>();
        assert_eq!(
            tracker.key,
            Some(JournalKey::Material {
                seed: 3,
                planet_seed: None
            }),
            "tracker must re-anchor onto the nearest entry"
        );
    }

    /// Deleting the *last* entry while it is selected must fall back to
    /// the new last entry rather than panic or leave `selected_index`
    /// out of bounds.
    #[test]
    fn deleting_last_entry_while_selected_falls_back_to_new_last() {
        let mut app = make_panel_app(15);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Charlie",
            3,
        );
        app.update();

        // Select "Charlie" — the last entry, index 2.
        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 2;
        app.update();

        // Delete "Charlie".  Sorted list becomes [Alpha, Bravo].  There
        // is no entry at the old slot (index 2), so the nearest valid
        // entry is the new last one (index 1, "Bravo").
        delete(
            &mut app,
            &JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 1,
            "highlight must clamp to the new last entry"
        );
        let tracker = app.world().resource::<JournalSelectionTracker>();
        assert_eq!(
            tracker.key,
            Some(JournalKey::Material {
                seed: 2,
                planet_seed: None
            }),
            "tracker must re-anchor onto Bravo"
        );
    }

    /// Deleting every entry while the panel is open must reset the
    /// tracker so a later first observation does not snap selection
    /// onto a key that no longer matches the current contents.
    #[test]
    fn emptying_journal_resets_tracker_then_re_anchors_on_repopulation() {
        let mut app = make_panel_app(15);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        app.update();

        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 1;
        app.update();

        // Delete both entries.
        delete(
            &mut app,
            &JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
        );
        delete(
            &mut app,
            &JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
        );
        app.update();

        let tracker = app.world().resource::<JournalSelectionTracker>();
        assert!(
            tracker.key.is_none(),
            "tracker must reset to None on empty journal"
        );

        // Repopulate with a different key.  Selection must anchor onto
        // the new entry rather than wait for a (deleted) prior key.
        record(
            &mut app,
            JournalKey::Material {
                seed: 99,
                planet_seed: None,
            },
            "Charlie",
            10,
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        let tracker = app.world().resource::<JournalSelectionTracker>();
        assert_eq!(state.selected_index, 0);
        assert_eq!(
            tracker.key,
            Some(JournalKey::Material {
                seed: 99,
                planet_seed: None
            }),
            "tracker must anchor onto the new sole entry"
        );
    }

    /// Deleting an entry that sorts *before* the selection must shift
    /// `selected_index` down so the highlight stays pinned on the same
    /// subject — the symmetric counterpart to the "insert before"
    /// behaviour.
    #[test]
    fn selection_follows_subject_when_entry_deleted_before_it() {
        let mut app = make_panel_app(15);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Charlie",
            3,
        );
        app.update();

        // Select "Charlie" at index 2.
        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 2;
        app.update();

        // Delete "Alpha".  Sorted list becomes [Bravo, Charlie].
        // "Charlie" now sits at index 1.
        delete(
            &mut app,
            &JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.selected_index, 1,
            "selection must follow Charlie to its new sort position after deletion before it"
        );
    }

    // ── Phase 5: new entries arriving while journal is open ─────────────
    //
    // The journal must not disrupt the player's current view when a new
    // observation is recorded while the panel is visible.  "Not disrupt"
    // means three things:
    //
    // 1. The highlighted subject stays highlighted, even if its sort
    //    position shifts (covered by the existing selection-tracker tests
    //    above and re-confirmed here in the scroll-window context).
    // 2. The visible window of entries continues to show the same
    //    subjects — a new entry inserted *before* the visible window
    //    must not silently scroll every visible row down by one.
    // 3. A new entry inserted *outside* the visible window must not
    //    cause the window to jump; the page indicator updates but the
    //    visible entries stay put.

    /// A new entry inserted before the visible window must shift
    /// `scroll_offset` so the same set of entries remains visible.
    /// Without scroll-anchoring, the visible rows would all shift down
    /// by one — disruptive when the player is reading the panel.
    #[test]
    fn new_entry_before_visible_window_keeps_visible_entries_stable() {
        let mut app = make_panel_app(3);

        // Build a 6-entry journal: Bravo, Charlie, Delta, Echo, Foxtrot, Golf.
        // Sorted order will match insertion order since names are alphabetical.
        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Bravo",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Charlie",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Delta",
            3,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 4,
                planet_seed: None,
            },
            "Echo",
            4,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 5,
                planet_seed: None,
            },
            "Foxtrot",
            5,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 6,
                planet_seed: None,
            },
            "Golf",
            6,
        );
        app.update();

        // Scroll down so the window shows entries 3-5: Echo, Foxtrot, Golf.
        // Selection sits on Foxtrot (index 4).
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.scroll_offset = 3;
            state.selected_index = 4;
        }
        app.update();

        // Sanity: the tracker is now anchored on Foxtrot (selection) and
        // Echo (top of window) at scroll_offset 3.
        let state = app.world().resource::<JournalUiState>();
        assert_eq!(state.scroll_offset, 3, "precondition: window starts at 3");
        assert_eq!(state.selected_index, 4, "precondition: Foxtrot selected");

        // Insert "Alpha" — sorts before everything, shifting every existing
        // entry down by one slot.  The visible entries (Echo, Foxtrot, Golf)
        // are now at indices 4, 5, 6 instead of 3, 4, 5.  To keep them
        // visible, scroll_offset must shift from 3 to 4 and selected_index
        // from 4 to 5.
        record(
            &mut app,
            JournalKey::Material {
                seed: 99,
                planet_seed: None,
            },
            "Alpha",
            7,
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.scroll_offset, 4,
            "scroll_offset must follow the top entry (Echo) so the visible \
             window keeps showing the same subjects"
        );
        assert_eq!(
            state.selected_index, 5,
            "selection must follow Foxtrot to its new sort position"
        );
    }

    /// A new entry inserted *after* the visible window must not move the
    /// window at all.  Selection and scroll stay put; only the page
    /// indicator (rendered separately) reflects the new total.
    #[test]
    fn new_entry_after_visible_window_does_not_move_view() {
        let mut app = make_panel_app(3);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Charlie",
            3,
        );
        app.update();

        // Window shows Alpha, Bravo, Charlie (indices 0..3).  Select Bravo.
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.selected_index = 1;
        }
        app.update();

        // Insert "Zulu" — sorts after everything, lands at index 3 (just
        // past the visible window).  Nothing in view should change.
        record(
            &mut app,
            JournalKey::Material {
                seed: 99,
                planet_seed: None,
            },
            "Zulu",
            4,
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.scroll_offset, 0,
            "scroll_offset must not move when entry is inserted past the \
             visible window"
        );
        assert_eq!(
            state.selected_index, 1,
            "selection must stay on Bravo when an unrelated entry is added"
        );
    }

    /// A new entry inserted *between* the top of the visible window and
    /// the selection must shift `selected_index` (the selected subject
    /// has moved down) but must leave `scroll_offset` alone — the top
    /// entry has not moved, so the window's anchor is still valid.  The
    /// new entry naturally appears in the middle of the visible window;
    /// that is the correct outcome of recording an observation about a
    /// subject the player can already see.
    #[test]
    fn new_entry_between_top_and_selection_shifts_only_selection() {
        let mut app = make_panel_app(5);

        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Delta",
            3,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 4,
                planet_seed: None,
            },
            "Echo",
            4,
        );
        app.update();

        // Window shows all four (indices 0..4).  Select Echo at index 3.
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.selected_index = 3;
        }
        app.update();

        // Insert "Charlie" — sorts between Bravo and Delta at index 2.
        // Echo shifts from index 3 to index 4.  Top entry (Alpha) is
        // unchanged at index 0.
        record(
            &mut app,
            JournalKey::Material {
                seed: 99,
                planet_seed: None,
            },
            "Charlie",
            5,
        );
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert_eq!(
            state.scroll_offset, 0,
            "scroll_offset must not move when the top entry's position is \
             unchanged"
        );
        assert_eq!(
            state.selected_index, 4,
            "selection must follow Echo to its new sort position"
        );
    }

    /// Phase 5 capstone: while the journal is open and an entry is
    /// highlighted, recording new observations that create brand-new
    /// entries must (a) update the journal data and the rendered list
    /// to reflect every new subject — either as a visible row or by an
    /// updated page indicator and reachable entry — and (b) leave the
    /// highlight pinned to the originally selected subject.  The earlier
    /// Phase 5 tests assert the bookkeeping invariants (`selected_index`
    /// / `scroll_offset` shift correctly); this test asserts the
    /// player-facing outcome by inspecting the actual `JournalRenderCache`
    /// contents and the page-indicator help text.
    ///
    /// Three insertion positions are exercised in a single fixture so
    /// the assertion holds across all relative positions of the new
    /// entry vs. the selection:
    ///
    /// * inserted *before* the visible window's top entry (visible
    ///   window stays pinned to the same subjects per Phase 5
    ///   scroll-anchoring; the new entry is reachable but offscreen and
    ///   the page indicator reflects the larger total);
    /// * inserted *between* the top of the window and the selection
    ///   (the new subject appears mid-window, selection follows); and
    /// * inserted *after* the visible window (page indicator reflects
    ///   the new total; visible window untouched).
    ///
    /// In every case the highlighted line must still belong to the
    /// originally selected subject ("Echo") and that subject's
    /// observation count must be unchanged — selection stability means
    /// both that the highlight stays put and that the underlying data
    /// for the selected subject is undisturbed by additions of other
    /// subjects.
    #[test]
    fn adding_entry_while_open_updates_list_and_keeps_selection_stable() {
        let mut app = make_panel_app(5);

        // Initial fixture: five entries spanning the visible window
        // (entries_per_page = 5).  Sorted alphabetically:
        // Bravo, Delta, Echo, Foxtrot, Hotel.
        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Bravo",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Delta",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Echo",
            3,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 4,
                planet_seed: None,
            },
            "Foxtrot",
            4,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 5,
                planet_seed: None,
            },
            "Hotel",
            5,
        );
        app.update();

        // Select "Echo" (sort index 2).  All five entries fit on a single
        // page so the visible window is [0..5).
        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 2;
        app.update();

        // Helper: assert the rendered entry list contains a line for `name`.
        fn list_contains(app: &App, name: &str) -> bool {
            let cache = app.world().resource::<JournalRenderCache>();
            cache.list_lines.iter().any(|l| l.text.contains(name))
        }

        // Helper: return the single highlighted line text, asserting that
        // exactly one line is selected.
        fn highlighted_line(app: &App) -> String {
            let cache = app.world().resource::<JournalRenderCache>();
            let hits: Vec<&str> = cache
                .list_lines
                .iter()
                .filter(|l| l.selected)
                .map(|l| l.text.as_str())
                .collect();
            assert_eq!(
                hits.len(),
                1,
                "exactly one entry must be highlighted (got {hits:?})"
            );
            hits[0].to_string()
        }

        // Helper: read the cached page-indicator help text.
        fn help_text(app: &App) -> String {
            app.world().resource::<JournalRenderCache>().help.clone()
        }

        // Helper: read the journal entry count via a query.
        fn entry_count(app: &mut App) -> usize {
            let mut q = app.world_mut().query_filtered::<&Journal, With<Player>>();
            q.single(app.world())
                .expect("player must exist")
                .entries
                .len()
        }

        // Sanity: initial rendered state has Echo selected and all five
        // subjects present in the list.
        for name in ["Bravo", "Delta", "Echo", "Foxtrot", "Hotel"] {
            assert!(list_contains(&app, name), "precondition: {name} visible");
        }
        assert!(
            highlighted_line(&app).contains("Echo"),
            "precondition: Echo highlighted"
        );
        assert!(
            help_text(&app).contains("of 5"),
            "precondition: help indicator shows 5 total entries, got: {:?}",
            help_text(&app)
        );

        // ── Insertion 1: before the window's top entry ──────────────────
        //
        // "Alpha" sorts before everything.  By Phase 5 scroll-anchoring,
        // the visible window stays pinned to the same five subjects
        // (Bravo, Delta, Echo, Foxtrot, Hotel) — Alpha is reachable but
        // offscreen.  Echo's highlight follows.  The page indicator must
        // update to reflect the new total of six.
        record(
            &mut app,
            JournalKey::Material {
                seed: 10,
                planet_seed: None,
            },
            "Alpha",
            10,
        );
        app.update();

        assert_eq!(
            entry_count(&mut app),
            6,
            "Alpha must be present in the journal after recording"
        );
        assert!(
            help_text(&app).contains("of 6"),
            "help indicator must reflect the new total of 6 entries, got: {:?}",
            help_text(&app)
        );
        assert!(
            highlighted_line(&app).contains("Echo"),
            "highlight must stay on Echo after an insert before the window"
        );
        // Visible window still shows the original five subjects.
        for name in ["Bravo", "Delta", "Echo", "Foxtrot", "Hotel"] {
            assert!(
                list_contains(&app, name),
                "{name} must still be visible after insert before the window"
            );
        }

        // ── Insertion 2: between window top and the selection ───────────
        //
        // "Charlie" sorts between Bravo and Delta — i.e. above Echo.  The
        // visible window is anchored on Bravo (its top entry); inserting
        // Charlie between Bravo and Delta makes Charlie naturally appear
        // in the visible window (no scroll change needed).  Echo's sort
        // index advances by one; the highlight must follow.
        record(
            &mut app,
            JournalKey::Material {
                seed: 11,
                planet_seed: None,
            },
            "Charlie",
            11,
        );
        app.update();

        assert_eq!(entry_count(&mut app), 7);
        assert!(
            list_contains(&app, "Charlie"),
            "Charlie must appear in the visible window when inserted between top and selection"
        );
        assert!(
            highlighted_line(&app).contains("Echo"),
            "highlight must stay on Echo after an insert between top and selection"
        );
        assert!(
            help_text(&app).contains("of 7"),
            "help indicator must reflect 7 entries, got: {:?}",
            help_text(&app)
        );

        // ── Insertion 3: after the visible window ───────────────────────
        //
        // "Zulu" sorts past everything else.  By Phase 5 anchoring the
        // visible window does not move, so Zulu may be offscreen — the
        // contract is that the journal contains it and the page
        // indicator reflects the new total.  Echo's highlight remains.
        record(
            &mut app,
            JournalKey::Material {
                seed: 12,
                planet_seed: None,
            },
            "Zulu",
            12,
        );
        app.update();

        assert_eq!(entry_count(&mut app), 8);
        {
            let mut q = app.world_mut().query_filtered::<&Journal, With<Player>>();
            let journal = q.single(app.world()).expect("player must exist");
            assert!(
                journal.entries.contains_key(&JournalKey::Material {
                    seed: 12,
                    planet_seed: None
                }),
                "Zulu entry must be present in the journal after recording"
            );
        }
        assert!(
            help_text(&app).contains("of 8"),
            "help indicator must reflect 8 entries, got: {:?}",
            help_text(&app)
        );
        assert!(
            highlighted_line(&app).contains("Echo"),
            "highlight must stay on Echo after an insert past the window"
        );

        // ── Echo's own observation count must be untouched ──────────────
        //
        // Selection-stability also means the *contents* of the selected
        // subject are unaffected by additions of unrelated subjects.
        {
            let mut q = app.world_mut().query_filtered::<&Journal, With<Player>>();
            let journal = q.single(app.world()).expect("player must exist");
            let echo = journal
                .entries
                .get(&JournalKey::Material {
                    seed: 3,
                    planet_seed: None,
                })
                .expect("Echo entry must still exist");
            assert_eq!(
                echo.observation_count(),
                1,
                "Echo's observations must be unchanged by additions of other subjects"
            );
        }

        // ── Tracker still anchored on Echo ──────────────────────────────
        let tracker = app.world().resource::<JournalSelectionTracker>();
        assert_eq!(
            tracker.key,
            Some(JournalKey::Material {
                seed: 3,
                planet_seed: None
            }),
            "tracker must remain anchored on Echo across all three insertions"
        );
    }

    /// End-to-end: with a populated journal, navigate to a specific subject,
    /// close the journal via a `ToggleJournalIntent`, reopen it via another
    /// intent, and confirm the same subject is still highlighted.
    ///
    /// This complements `toggle_close_reopen_preserves_selection_and_scroll`
    /// (which manipulates `JournalUiState` fields directly) and
    /// `toggle_visibility_system_preserves_navigation_state` (which drives
    /// the toggle system but with no journal data) by exercising the full
    /// pipeline: real entries, real navigation, real toggle messages, and
    /// the panel-reconciliation pass that runs every frame.  The asserted
    /// invariant is the player-facing one called out by the Story 10.2
    /// acceptance criterion: "Journal remembers selection when toggled
    /// closed and reopened."
    #[test]
    fn reopen_journal_preserves_same_selected_entry() {
        let mut app = make_panel_app(15);
        app.add_message::<ToggleJournalIntent>();
        // Run the visibility toggle before panel reconciliation so any
        // visibility flip this frame is reflected in the same update().
        app.add_systems(
            Update,
            toggle_journal_visibility.before(compute_journal_panels),
        );

        // Populate four entries.  Sorted alphabetically the order is
        // Alpha (0), Bravo (1), Charlie (2), Delta (3).
        record(
            &mut app,
            JournalKey::Material {
                seed: 1,
                planet_seed: None,
            },
            "Alpha",
            1,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 2,
                planet_seed: None,
            },
            "Bravo",
            2,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Charlie",
            3,
        );
        record(
            &mut app,
            JournalKey::Material {
                seed: 4,
                planet_seed: None,
            },
            "Delta",
            4,
        );
        app.update();

        // User navigates to "Charlie" (sort index 2).
        app.world_mut()
            .resource_mut::<JournalUiState>()
            .selected_index = 2;
        app.update();

        // Sanity: the tracker has anchored onto Charlie.
        {
            let state = app.world().resource::<JournalUiState>();
            assert!(state.visible, "journal must be visible before close");
            assert_eq!(state.selected_index, 2);
        }

        // Close: send one toggle intent.
        app.world_mut().write_message(ToggleJournalIntent);
        app.update();
        assert!(
            !app.world().resource::<JournalUiState>().visible,
            "first toggle must hide the journal"
        );

        // Reopen: send a second toggle intent.
        app.world_mut().write_message(ToggleJournalIntent);
        app.update();

        let state = app.world().resource::<JournalUiState>();
        assert!(state.visible, "second toggle must reopen the journal");
        assert_eq!(
            state.selected_index, 2,
            "reopening must keep the same entry selected"
        );

        // Verify the *subject* (not just the index) — the highlighted line
        // in the entry list must still be Charlie's.
        let cache = app.world().resource::<JournalRenderCache>();
        let highlighted: Vec<&str> = cache
            .list_lines
            .iter()
            .filter(|l| l.selected)
            .map(|l| l.text.as_str())
            .collect();
        assert_eq!(
            highlighted.len(),
            1,
            "exactly one entry must be highlighted after reopening"
        );
        assert!(
            highlighted[0].contains("Charlie"),
            "highlighted entry after reopening must still be Charlie, got {:?}",
            highlighted[0]
        );
    }

    /// Shift+Tab cycles through context filter options: All → Current Planet → All.
    /// The filter state persists and affects which entries are shown.
    /// When the filter changes, selection resets to the top of the filtered list.
    #[test]
    fn shift_tab_cycles_context_filter() {
        use crate::world_generation::{WorldGenerationConfig, WorldProfile};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        // Set up WorldProfile with planet seed 0 to match test expectations
        let config = WorldGenerationConfig {
            planet_seed: Some(0u64.into()),
            ..Default::default()
        };
        let profile = WorldProfile::from_config(&config).unwrap();
        app.world_mut().insert_resource(profile);

        // Create a journal with entries from different planets
        let mut journal = Journal::default();

        // Material from planet 0
        journal.record(
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(0),
            },
            "Planet0-Material",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "From planet 0".to_string(),
                recorded_at: 1,
            },
        );

        // Material from planet 1
        journal.record(
            JournalKey::Material {
                seed: 2,
                planet_seed: Some(1),
            },
            "Planet1-Material",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "From planet 1".to_string(),
                recorded_at: 2,
            },
        );

        // Material with no planet context
        journal.record(
            JournalKey::Material {
                seed: 3,
                planet_seed: None,
            },
            "Unknown-Material",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Unknown origin".to_string(),
                recorded_at: 3,
            },
        );

        app.world_mut().spawn((Player, journal));

        // Initial state: All filter (default)
        {
            let state = app.world().resource::<JournalUiState>();
            assert!(state.filter().category.is_none());
            assert!(state.filter().context.is_none());
        }

        // First Shift+Tab: All → Current Planet (planet 0)
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
            keys.release(KeyCode::ShiftLeft);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert!(state.filter().category.is_none());
            assert!(matches!(
                state.filter().context,
                Some(JournalContext::CurrentPlanet { planet_seed: 0 })
            ));
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }

        // Second Shift+Tab: Current Planet → All
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::ShiftLeft);
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
            keys.release(KeyCode::ShiftLeft);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert!(state.filter().category.is_none());
            assert!(state.filter().context.is_none());
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }
    }

    /// Tab cycles through category filter options: All → SurfaceAppearance → ThermalBehavior → Weight → FabricationResult → All.
    /// The filter state persists and affects which entries are shown.
    /// When the filter changes, selection resets to the top of the filtered list.
    #[test]
    fn tab_cycles_category_filter() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        // Create a journal with entries to test filtering
        let mut journal = Journal::default();

        // Add a material entry with SurfaceAppearance observation
        journal.record(
            JournalKey::Material {
                seed: 1,
                planet_seed: Some(0),
            },
            "Surface-Material",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Smooth metallic surface".to_string(),
                recorded_at: 1,
            },
        );

        // Add a material entry with ThermalBehavior observation
        journal.record(
            JournalKey::Material {
                seed: 2,
                planet_seed: Some(0),
            },
            "Thermal-Material",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Warm to the touch".to_string(),
                recorded_at: 2,
            },
        );

        // Add a material entry with Weight observation
        journal.record(
            JournalKey::Material {
                seed: 3,
                planet_seed: Some(0),
            },
            "Heavy-Material",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Heavy material".to_string(),
                recorded_at: 3,
            },
        );

        // Add a fabrication entry with FabricationResult observation
        journal.record(
            JournalKey::Fabrication { output_seed: 4 },
            "Alloy-Fabrication",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Successfully fabricated alloy".to_string(),
                recorded_at: 4,
            },
        );

        app.world_mut().spawn((Player, journal));

        // Initial state: All filter (no restrictions)
        {
            let state = app.world().resource::<JournalUiState>();
            assert!(state.filter().category.is_none());
            assert!(state.filter().context.is_none());
        }

        // First Tab: All → SurfaceAppearance
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter().category,
                Some(ObservationCategory::SurfaceAppearance)
            );
            assert!(state.filter().context.is_none());
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }

        // Second Tab: SurfaceAppearance → ThermalBehavior
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter().category,
                Some(ObservationCategory::ThermalBehavior)
            );
            assert!(state.filter().context.is_none());
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }

        // Third Tab: ThermalBehavior → Weight
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(state.filter().category, Some(ObservationCategory::Weight));
            assert!(state.filter().context.is_none());
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }

        // Fourth Tab: Weight → FabricationResult
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter().category,
                Some(ObservationCategory::FabricationResult)
            );
            assert!(state.filter().context.is_none());
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }

        // Fifth Tab: FabricationResult → All
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.press(KeyCode::Tab);
        }
        app.update();
        {
            let mut keys = app.world_mut().resource_mut::<ButtonInput<KeyCode>>();
            keys.release(KeyCode::Tab);
        }

        {
            let state = app.world().resource::<JournalUiState>();
            assert!(state.filter().category.is_none());
            assert!(state.filter().context.is_none());
            // Selection should reset to top when filter changes
            assert_eq!(state.selected_index, 0);
            assert_eq!(state.scroll_offset, 0);
        }
    }

    /// Help text shows Shift+Tab context filter hint and displays active filter status.
    #[test]
    fn help_text_shows_context_filter_hint_and_status() {
        let state_all = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        };
        let help_all = build_help_text(10, &state_all);
        assert!(
            help_all.contains("Shift+Tab: Context Filter"),
            "help should show Shift+Tab hint, got: {help_all}"
        );
        assert!(
            !help_all.contains("[Filter:"),
            "help should not show filter status when no filter active, got: {help_all}"
        );

        let state_planet = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter {
                category: None,
                context: Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
            },
        };
        let help_planet = build_help_text(10, &state_planet);
        assert!(
            help_planet.contains("Shift+Tab: Context Filter"),
            "help should show Shift+Tab hint with filter active, got: {help_planet}"
        );
        assert!(
            help_planet.contains("[Filter: Current Planet]"),
            "help should show current planet filter status, got: {help_planet}"
        );

        let state_combined = JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter {
                category: Some(ObservationCategory::SurfaceAppearance),
                context: Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
            },
        };
        let help_combined = build_help_text(10, &state_combined);
        assert!(
            help_combined.contains("[Filter: Category | Current Planet]"),
            "help should show combined filter status, got: {help_combined}"
        );
    }

    /// Filter bar renders correctly with different filter states.
    #[test]
    fn filter_bar_renders_correctly() {
        // Test empty filter (All) - should render empty string
        let filter_all = JournalFilter::default();
        let filter_bar_all = build_filter_bar_text(&filter_all);
        assert_eq!(filter_bar_all, "", "All filter should render empty string");

        // Test category-only filter
        let filter_category = JournalFilter {
            category: Some(ObservationCategory::SurfaceAppearance),
            context: None,
        };
        let filter_bar_category = build_filter_bar_text(&filter_category);
        assert_eq!(
            filter_bar_category, "Filter: Surface",
            "Category filter should show category label"
        );

        // Test context-only filter (Current Planet)
        let filter_planet = JournalFilter {
            category: None,
            context: Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
        };
        let filter_bar_planet = build_filter_bar_text(&filter_planet);
        assert_eq!(
            filter_bar_planet, "Filter: Current Planet",
            "Planet filter should show planet context"
        );

        // Test context-only filter (Current Biome)
        let filter_biome = JournalFilter {
            category: None,
            context: Some(JournalContext::CurrentBiome {
                biome_key: "tundra".to_string(),
            }),
        };
        let filter_bar_biome = build_filter_bar_text(&filter_biome);
        assert_eq!(
            filter_bar_biome, "Filter: Current Biome",
            "Biome filter should show biome context"
        );

        // Test combined filter (Category + Planet)
        let filter_combined_planet = JournalFilter {
            category: Some(ObservationCategory::ThermalBehavior),
            context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
        };
        let filter_bar_combined_planet = build_filter_bar_text(&filter_combined_planet);
        assert_eq!(
            filter_bar_combined_planet, "Filter: Thermal | Current Planet",
            "Combined category+planet filter should show both"
        );

        // Test combined filter (Category + Biome)
        let filter_combined_biome = JournalFilter {
            category: Some(ObservationCategory::Weight),
            context: Some(JournalContext::CurrentBiome {
                biome_key: "basalt_flats".to_string(),
            }),
        };
        let filter_bar_combined_biome = build_filter_bar_text(&filter_combined_biome);
        assert_eq!(
            filter_bar_combined_biome, "Filter: Weight | Current Biome",
            "Combined category+biome filter should show both"
        );
    }

    /// Empty journal with filter applied shows "No observations yet." not "No matching entries".
    /// This test verifies that when the journal has zero entries, applying any filter
    /// still shows the empty journal message rather than the empty filter message.
    #[test]
    fn empty_journal_with_filter_shows_no_observations_yet() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.init_resource::<ButtonInput<KeyCode>>();
        app.insert_resource(JournalUiState {
            visible: true,
            selected_index: 0,
            scroll_offset: 0,
            entries_per_page: 15,
            filter: JournalFilter::default(),
        });
        app.add_systems(Update, journal_navigation);

        // Create a Player entity with an empty Journal component
        app.world_mut().spawn((Player, Journal::default()));

        // Apply a category filter to the empty journal
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: Some(ObservationCategory::SurfaceAppearance),
                context: None,
            });
        }

        // Update the app to process the filter
        app.update();

        // Verify that empty journal with filter shows "No observations yet."
        let mut q = app.world_mut().query_filtered::<&Journal, With<Player>>();
        let journal = q.single(app.world()).expect("player must exist");
        let state = app.world().resource::<JournalUiState>();

        // Build the detail spans using the same logic as the UI
        let filtered_entries: Vec<&JournalEntry> = journal
            .entries
            .values()
            .filter(|entry| matches_filter(entry, state.filter()))
            .collect();

        let detail_spans =
            build_detail_spans(&filtered_entries, state, !journal.entries.is_empty());
        let detail_text = detail_spans_to_string(&detail_spans);

        assert_eq!(
            detail_text, "No observations yet.",
            "Empty journal with filter should show 'No observations yet.' not 'No matching entries'"
        );

        // Also test with context filter
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: None,
                context: Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
            });
        }

        app.update();

        // Verify context filter on empty journal also shows "No observations yet."
        let mut q = app.world_mut().query_filtered::<&Journal, With<Player>>();
        let journal = q.single(app.world()).expect("player must exist");
        let state = app.world().resource::<JournalUiState>();

        let filtered_entries: Vec<&JournalEntry> = journal
            .entries
            .values()
            .filter(|entry| matches_filter(entry, state.filter()))
            .collect();

        let detail_spans =
            build_detail_spans(&filtered_entries, state, !journal.entries.is_empty());
        let detail_text = detail_spans_to_string(&detail_spans);

        assert_eq!(
            detail_text, "No observations yet.",
            "Empty journal with context filter should show 'No observations yet.' not 'No matching entries'"
        );

        // Test combined filter on empty journal
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: Some(ObservationCategory::ThermalBehavior),
                context: Some(JournalContext::CurrentPlanet { planet_seed: 7 }),
            });
        }

        app.update();

        // Verify combined filter on empty journal also shows "No observations yet."
        let mut q = app.world_mut().query_filtered::<&Journal, With<Player>>();
        let journal = q.single(app.world()).expect("player must exist");
        let state = app.world().resource::<JournalUiState>();

        let filtered_entries: Vec<&JournalEntry> = journal
            .entries
            .values()
            .filter(|entry| matches_filter(entry, state.filter()))
            .collect();

        let detail_spans =
            build_detail_spans(&filtered_entries, state, !journal.entries.is_empty());
        let detail_text = detail_spans_to_string(&detail_spans);

        assert_eq!(
            detail_text, "No observations yet.",
            "Empty journal with combined filter should show 'No observations yet.' not 'No matching entries'"
        );
    }

    #[test]
    fn test_planet_switch_updates_context_filter() {
        use crate::world_generation::{WorldGenerationConfig, WorldProfile};

        let mut app = App::new();
        app.add_plugins(MinimalPlugins)
            .add_plugins(JournalPlugin)
            .init_resource::<ButtonInput<KeyCode>>();

        // Create a Player entity with an empty Journal component
        app.world_mut().spawn((Player, Journal::default()));

        // Set up initial WorldProfile with planet seed 42
        let initial_config = WorldGenerationConfig {
            planet_seed: Some(42u64.into()),
            ..Default::default()
        };
        let initial_profile = WorldProfile::from_config(&initial_config).unwrap();
        app.world_mut().insert_resource(initial_profile);

        // Set journal filter to CurrentPlanet with the initial planet seed
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: None,
                context: Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
            });
        }

        // Update the app to process the initial state
        app.update();

        // Verify initial filter state
        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter().context,
                Some(JournalContext::CurrentPlanet { planet_seed: 42 }),
                "Initial filter should be set to planet seed 42"
            );
        }

        // Switch to a new planet by updating the WorldProfile resource
        let new_config = WorldGenerationConfig {
            planet_seed: Some(123u64.into()),
            ..Default::default()
        };
        let new_profile = WorldProfile::from_config(&new_config).unwrap();
        app.world_mut().insert_resource(new_profile);

        // Update the app to process the planet change
        app.update();

        // Verify that the context filter was automatically updated to the new planet
        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter().context,
                Some(JournalContext::CurrentPlanet { planet_seed: 123 }),
                "Context filter should be automatically updated to new planet seed 123"
            );

            // Verify that scroll position was reset
            assert_eq!(
                state.selected_index, 0,
                "Selected index should be reset to 0"
            );
            assert_eq!(state.scroll_offset, 0, "Scroll offset should be reset to 0");
        }

        // Test that category filter is preserved during planet switch
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: Some(ObservationCategory::ThermalBehavior),
                context: Some(JournalContext::CurrentPlanet { planet_seed: 123 }),
            });
        }

        // Switch to another planet
        let another_config = WorldGenerationConfig {
            planet_seed: Some(456u64.into()),
            ..Default::default()
        };
        let another_profile = WorldProfile::from_config(&another_config).unwrap();
        app.world_mut().insert_resource(another_profile);

        app.update();

        // Verify that category filter is preserved while context is updated
        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter(),
                &JournalFilter {
                    category: Some(ObservationCategory::ThermalBehavior),
                    context: Some(JournalContext::CurrentPlanet { planet_seed: 456 }),
                },
                "Category filter should be preserved while context is updated to new planet"
            );
        }

        // Test that non-CurrentPlanet context filters are not affected
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: None,
                context: Some(JournalContext::CurrentBiome {
                    biome_key: "tundra".to_string(),
                }),
            });
        }

        // Switch to yet another planet
        let final_config = WorldGenerationConfig {
            planet_seed: Some(789u64.into()),
            ..Default::default()
        };
        let final_profile = WorldProfile::from_config(&final_config).unwrap();
        app.world_mut().insert_resource(final_profile);

        app.update();

        // Verify that CurrentBiome filter is not affected by planet changes
        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter().context,
                Some(JournalContext::CurrentBiome {
                    biome_key: "tundra".to_string()
                }),
                "CurrentBiome filter should not be affected by planet changes"
            );
        }

        // Test that filter with no context is not affected
        {
            let mut state = app.world_mut().resource_mut::<JournalUiState>();
            state.set_filter(JournalFilter {
                category: Some(ObservationCategory::SurfaceAppearance),
                context: None,
            });
        }

        // Switch planet one more time
        let last_config = WorldGenerationConfig {
            planet_seed: Some(999u64.into()),
            ..Default::default()
        };
        let last_profile = WorldProfile::from_config(&last_config).unwrap();
        app.world_mut().insert_resource(last_profile);

        app.update();

        // Verify that filter with no context remains unchanged
        {
            let state = app.world().resource::<JournalUiState>();
            assert_eq!(
                state.filter(),
                &JournalFilter {
                    category: Some(ObservationCategory::SurfaceAppearance),
                    context: None,
                },
                "Filter with no context should not be affected by planet changes"
            );
        }
    }
}
