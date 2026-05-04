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

use crate::carry::WeightDescriptionBand;
use crate::input::InputAction;
use crate::observation::{ConfidenceLevel, describe_weight_observation};
use crate::player::{Player, cursor_is_captured, spawn_player};
use crate::world_generation::{BiomeType, WorldProfile};

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
        //
        // When biome capture is added, this arm will compare the entry's
        // recorded biome key against `biome_key.biome_type()` — no other
        // call site needs to change.
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
            .add_message::<RecordWeightObservation>()
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
                    apply_weight_records.in_set(JournalSet::Navigate),
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
///
/// **Planet seed handling:** For [`JournalKey::Material`] observations,
/// the `planet_seed` field is automatically resolved by the ingestion
/// system from the current [`WorldProfile`] resource. Observation producers
/// should pass `planet_seed: None` and let the system fill it in centrally.
/// This eliminates the need for every observation site to manually extract
/// `world_profile.as_deref().map(|p| p.planet_seed.0)` and prevents silent
/// failures when new observation sites forget this pattern.
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

/// Message for recording weight observations with raw data instead of pre-rendered descriptions.
///
/// This message carries the raw density, carry strength, and confidence data, allowing the
/// journal system to call the descriptor function rather than having the carry system
/// pre-render the description. This aligns the weight observation pattern with the thermal
/// observation pattern for consistency.
#[derive(Message, Clone)]
pub struct RecordWeightObservation {
    /// Which journal subject this weight observation belongs to.
    pub key: JournalKey,
    /// Player-facing display name for the subject.
    pub name: String,
    /// Raw material density value.
    pub density: f32,
    /// Player's current carry strength.
    pub carry_strength: f32,
    /// Confidence level based on observation count.
    pub confidence: ConfidenceLevel,
    /// Weight description bands from carry configuration.
    pub weight_bands: Vec<WeightDescriptionBand>,
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
///
/// **Planet seed resolution:** For [`JournalKey::Material`] observations,
/// this system automatically fills in the `planet_seed` field from the
/// current [`WorldProfile`] resource if available. Observation producers
/// no longer need to extract `planet_seed` manually — they can pass
/// `planet_seed: None` and this system will resolve it centrally.
/// This eliminates the implicit contract fragility where every observation
/// site had to remember the exact `world_profile.as_deref().map(|p| p.planet_seed.0)`
/// extraction pattern.
pub fn apply_observations(
    mut reader: MessageReader<RecordObservation>,
    mut player_query: Query<&mut Journal, With<Player>>,
    time: Res<Time>,
    world_profile: Option<Res<WorldProfile>>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    let tick = time.elapsed().as_millis() as u64;
    let current_planet_seed = world_profile.as_deref().map(|p| p.planet_seed.0);

    for event in reader.read() {
        let mut obs = event.observation.clone();
        obs.recorded_at = tick;

        // Automatically resolve planet_seed for Material observations if not already set
        let key = match &event.key {
            JournalKey::Material {
                seed,
                planet_seed: None,
            } => JournalKey::Material {
                seed: *seed,
                planet_seed: current_planet_seed,
            },
            // For Material observations that already have planet_seed set, or non-Material observations, use as-is
            key => key.clone(),
        };

        journal.record(key, &event.name, obs);
    }
}

/// Processes weight observation messages and converts them to journal entries.
///
/// This system reads [`RecordWeightObservation`] messages containing raw weight data
/// and calls the descriptor function to generate the final observation text. This
/// aligns the weight observation pattern with the thermal observation pattern where
/// the journal system handles description generation rather than the originating system.
fn apply_weight_records(
    mut reader: MessageReader<RecordWeightObservation>,
    mut player_query: Query<&mut Journal, With<Player>>,
    time: Res<Time>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    let tick = time.elapsed().as_millis() as u64;

    for event in reader.read() {
        let description = describe_weight_observation(
            event.density,
            event.carry_strength,
            event.confidence,
            &event.weight_bands,
        );

        let observation = Observation {
            category: ObservationCategory::Weight,
            confidence: event.confidence,
            description,
            recorded_at: tick,
        };

        journal.record(event.key.clone(), &event.name, observation);
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

    format!(
        "\u{2191}\u{2193} Navigate  PgUp/PgDn: Page  Home/End: Jump  Shift+Tab: Context Filter  J: Close{filter_status}  [{page_start}-{page_end} of {entry_count}]"
    )
}

#[cfg(test)]
#[path = "journal_tests.rs"]
mod tests;
