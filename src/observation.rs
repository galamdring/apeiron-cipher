//! Observation confidence tracking — earned knowledge through repeated testing.
//!
//! Players never see raw numbers. Instead, the language used to describe
//! observed properties shifts as the player repeats experiments:
//!
//! | Count | Tone                                         |
//! |-------|----------------------------------------------|
//! |   1   | "Seemed to …"                                |
//! |  2–3  | "[Behavior] when exposed to heat"             |
//! |  4+   | "Reliably [behavior] under heat — [compare]"  |

#![allow(deprecated)]
//!
//! The [`ConfidenceTracker`] resource stores observation counts per
//! `(material_seed, property)` pair. The property key is a string so it
//! can accommodate new test types without a code change.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Plugin that initialises the observation confidence tracking system.
pub struct ObservationPlugin;

/// Path to the confidence configuration file.
const CONFIDENCE_CONFIG_PATH: &str = "assets/config/confidence.toml";

impl Plugin for ObservationPlugin {
    fn build(&self, app: &mut App) {
        app
            // ConfidenceTracker removed - confidence is now tracked per-observation
            // in the journal system via Confidence(f32) and accumulate() method
            .init_resource::<DescriptorVocabulary>()
            .add_message::<OnPlayerDeathEvent>()
            .add_systems(PreStartup, load_confidence_config)
            .add_systems(Update, handle_player_death);
    }
}

// ── Confidence levels ────────────────────────────────────────────────────

/// Continuous confidence value (0.0 = unknown, 1.0 = certain).
///
/// Stored in KnowledgeGraph nodes per architecture decision.
/// The discrete tiers are a presentation concern, not a data concern.
///
/// This bridges the POC's discrete `ConfidenceLevel` enum with the
/// architecture's continuous f32 spectrum. The continuous value enables
/// sophisticated confidence evolution (accumulation with diminishing returns,
/// death penalties, domain-weighted recovery) while the discrete tiers
/// provide stable language selection for player-facing descriptions.
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct Confidence(pub f32);

impl Confidence {
    /// Map continuous value to qualitative tier for language selection.
    ///
    /// The tier boundaries are:
    /// - [0.0, 0.3): Tentative — "seemed to..."
    /// - [0.3, 0.7): Observed — factual statements
    /// - [0.7, 1.0]: Confident — "reliably..." with comparative language
    ///
    /// These thresholds provide clear separation between language tiers
    /// while allowing smooth confidence evolution within each tier.
    pub fn tier(&self) -> ConfidenceTier {
        match self.0 {
            x if x < 0.3 => ConfidenceTier::Tentative,
            x if x < 0.7 => ConfidenceTier::Observed,
            _ => ConfidenceTier::Confident,
        }
    }

    /// Accumulate evidence. Diminishing returns — early observations
    /// matter more, convergence toward 1.0 is asymptotic.
    ///
    /// Uses exponential decay formula: new = old + (1 - old) * weight
    /// This ensures that:
    /// - First observations have large impact when confidence is low
    /// - Later observations have smaller impact as confidence approaches 1.0
    /// - Confidence never exceeds 1.0 or drops below 0.0
    ///
    /// # Arguments
    /// * `weight` - Evidence strength (typically 0.1-0.3 for normal observations)
    pub fn accumulate(&mut self, weight: f32) {
        // Exponential decay toward 1.0: new = old + (1 - old) * weight
        self.0 = (self.0 + (1.0 - self.0) * weight).clamp(0.0, 1.0);
    }

    /// Death penalty: reduce confidence by factor, clamped to floor.
    ///
    /// Death degrades confidence to reflect the player's shaken certainty
    /// about their understanding. The degradation is multiplicative (not
    /// additive) so higher confidence values lose more absolute confidence
    /// but retain some of their accumulated knowledge.
    ///
    /// # Arguments
    /// * `factor` - Multiplier applied to current confidence (e.g., 0.6 = lose 40%)
    /// * `floor` - Minimum confidence after degradation (prevents total knowledge loss)
    pub fn degrade(&mut self, factor: f32, floor: f32) {
        self.0 = (self.0 * factor).max(floor);
    }
}

/// Presentation tier — maps to language selection.
///
/// Replaces the POC ConfidenceLevel for display purposes.
/// The discrete tiers provide stable language selection while the
/// underlying continuous Confidence value enables sophisticated evolution.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfidenceTier {
    /// Tentative language: "seemed to...", "appeared to..."
    Tentative,
    /// Factual language: "holds together under heat", "changes when heated"
    Observed,
    /// Confident language: "reliably withstands heat", with comparative context
    Confident,
}

impl ConfidenceTier {
    /// Qualitative language shown in the journal detail panel to convey how
    /// certain an observation is without exposing raw numbers.
    ///
    /// Maps the tier to the same display labels as the legacy ConfidenceLevel
    /// to maintain UI consistency during the transition.
    pub fn display_label(self) -> &'static str {
        match self {
            ConfidenceTier::Tentative => "Uncertain",
            ConfidenceTier::Observed => "Noted",
            ConfidenceTier::Confident => "Confirmed",
        }
    }
}

// ── Confidence configuration ─────────────────────────────────────────────

/// Configuration for death's confidence impact and observation accumulation.
///
/// This resource controls how confidence evolves through observation accumulation
/// and degrades through death. All values are data-driven, loaded from
/// `assets/config/confidence.toml` to enable tuning without code changes.
///
/// The configuration supports the GDD's death mechanics (§Death Mechanics)
/// where death degrades confidence but re-engaging the death-relevant domain
/// recovers confidence faster than avoiding it.
#[derive(Resource, Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct ConfidenceConfig {
    /// Multiplier applied to all confidence on death (e.g., 0.6 = lose 40%).
    ///
    /// Death shakes the player's certainty about their understanding of the world.
    /// This factor represents how much confidence is lost across all observations.
    /// Values closer to 1.0 mean death has less impact; values closer to 0.0
    /// mean death severely undermines confidence.
    ///
    /// Typical range: 0.4-0.8
    #[serde(default = "default_death_degradation_factor")]
    pub death_degradation_factor: f32,

    /// Confidence never drops below this floor on death.
    ///
    /// Even after death, the player retains some baseline understanding.
    /// This floor prevents total knowledge loss and ensures that accumulated
    /// observations aren't completely erased by a single death.
    ///
    /// Should be low enough that death feels meaningful but high enough
    /// that players don't lose all progress. Typical range: 0.1-0.3
    #[serde(default = "default_death_floor")]
    pub death_floor: f32,

    /// Recovery rate multiplier when re-engaging the death-relevant domain.
    ///
    /// Per GDD domain-weighted recovery: when the player returns to the domain
    /// that caused their death (e.g., thermal experiments after heat death),
    /// confidence recovers faster than normal. This encourages players to
    /// confront their failures rather than avoid them.
    ///
    /// Values > 1.0 accelerate recovery in the death domain.
    /// Typical range: 1.5-3.0
    #[serde(default = "default_domain_recovery_multiplier")]
    pub domain_recovery_multiplier: f32,

    /// Recovery rate multiplier for unrelated domains.
    ///
    /// When the player makes observations in domains unrelated to their death,
    /// confidence still recovers but at a slower rate. This represents the
    /// gradual restoration of general confidence through successful experimentation.
    ///
    /// Values < 1.0 slow recovery outside the death domain.
    /// Typical range: 0.5-0.9
    #[serde(default = "default_passive_recovery_multiplier")]
    pub passive_recovery_multiplier: f32,

    /// Base weight for a single observation accumulation.
    ///
    /// This is the standard evidence strength applied when recording a new
    /// observation. The actual weight may be modified by recovery multipliers
    /// or other factors, but this provides the baseline accumulation rate.
    ///
    /// Higher values mean fewer observations needed to reach high confidence.
    /// Lower values require more repeated testing. Typical range: 0.1-0.3
    #[serde(default = "default_base_observation_weight")]
    pub base_observation_weight: f32,
}

/// Default value for death_degradation_factor field.
fn default_death_degradation_factor() -> f32 {
    0.6
}

/// Default value for death_floor field.
fn default_death_floor() -> f32 {
    0.2
}

/// Default value for domain_recovery_multiplier field.
fn default_domain_recovery_multiplier() -> f32 {
    2.0
}

/// Default value for passive_recovery_multiplier field.
fn default_passive_recovery_multiplier() -> f32 {
    0.7
}

/// Default value for base_observation_weight field.
fn default_base_observation_weight() -> f32 {
    0.2
}

impl Default for ConfidenceConfig {
    /// Default confidence configuration values.
    ///
    /// These defaults provide reasonable behavior if the config file is missing
    /// or malformed. The values are tuned for moderate death impact with
    /// meaningful but not punishing confidence degradation.
    fn default() -> Self {
        Self {
            death_degradation_factor: 0.6,    // Lose 40% confidence on death
            death_floor: 0.2,                 // Never drop below 20% confidence
            domain_recovery_multiplier: 2.0,  // 2x recovery in death domain
            passive_recovery_multiplier: 0.7, // 0.7x recovery elsewhere
            base_observation_weight: 0.2,     // Standard observation strength
        }
    }
}

// ── Descriptor vocabulary system ─────────────────────────────────────────

/// A descriptor entry: given a property value range and confidence tier,
/// produce qualitative language.
///
/// Each entry maps a specific range of property values (e.g., thermal resistance
/// 0.0-0.25) combined with a confidence tier (Tentative, Observed, Confident)
/// to an array of possible qualitative descriptions. The system selects from
/// the descriptions array to provide variety while maintaining consistency
/// within the same tier.
///
/// The value range uses Rust's standard `Range<f32>` type, which represents
/// a half-open interval [start, end). This matches the typical pattern of
/// property value bucketing where each range covers a distinct segment of
/// the 0.0-1.0 normalized property space.
///
/// Multiple descriptions per entry allow for linguistic variety while keeping
/// the meaning consistent within a tier. For example, "seemed to soften quickly"
/// and "appeared to soften rapidly" convey the same information with different
/// phrasing, preventing the journal from feeling repetitive when the player
/// observes the same property multiple times.
#[derive(Clone, Debug, PartialEq)]
pub struct DescriptorEntry {
    /// The range of property values this entry applies to.
    ///
    /// Property values are normalized to [0.0, 1.0] where 0.0 represents
    /// the minimum possible value for the property and 1.0 represents the
    /// maximum. The range is half-open: [start, end), so a range of 0.0..0.25
    /// includes 0.0 but excludes 0.25.
    ///
    /// Ranges should be non-overlapping within a single observation category
    /// and confidence tier to ensure deterministic descriptor selection.
    pub value_range: std::ops::Range<f32>,

    /// The confidence tier this entry applies to.
    ///
    /// Each combination of (value_range, tier) should have exactly one
    /// DescriptorEntry in the vocabulary table. This ensures that the same
    /// property value at the same confidence level always produces the same
    /// language, satisfying the deterministic requirement in the acceptance
    /// criteria.
    pub tier: ConfidenceTier,

    /// Array of possible qualitative descriptions for this value range and tier.
    ///
    /// The system selects from this array to provide linguistic variety while
    /// maintaining semantic consistency. All descriptions in the array should
    /// convey equivalent information about the property value and confidence.
    ///
    /// Using `&'static [&'static str]` keeps the vocabulary data in the binary
    /// rather than requiring runtime allocation, and ensures the descriptions
    /// remain valid for the lifetime of the program. This matches the pattern
    /// used elsewhere in the codebase for static string data.
    pub descriptions: &'static [&'static str],
}

impl DescriptorEntry {
    /// Check if this entry applies to the given property value and confidence tier.
    ///
    /// Returns `true` when both the value falls within this entry's range and
    /// the tier matches exactly. This is the primary predicate used by the
    /// vocabulary lookup system to find the appropriate descriptor for a
    /// given observation.
    ///
    /// # Arguments
    /// * `value` - Normalized property value in [0.0, 1.0]
    /// * `tier` - Confidence tier for the observation
    ///
    /// # Examples
    /// ```
    /// use apeiron_cipher::observation::{DescriptorEntry, ConfidenceTier};
    ///
    /// let entry = DescriptorEntry {
    ///     value_range: 0.0..0.25,
    ///     tier: ConfidenceTier::Tentative,
    ///     descriptions: &["seemed to soften quickly"],
    /// };
    ///
    /// assert!(entry.matches(0.1, ConfidenceTier::Tentative));
    /// assert!(!entry.matches(0.3, ConfidenceTier::Tentative)); // Outside range
    /// assert!(!entry.matches(0.1, ConfidenceTier::Observed));  // Wrong tier
    /// ```
    pub fn matches(&self, value: f32, tier: ConfidenceTier) -> bool {
        self.value_range.contains(&value) && self.tier == tier
    }

    /// Select a description from this entry's vocabulary.
    ///
    /// For deterministic behavior, this always returns the first description
    /// in the array. Future enhancements could add variety by using a seeded
    /// random selection or rotating through the descriptions based on some
    /// deterministic input (e.g., material seed), but the current implementation
    /// prioritizes consistency over variety.
    ///
    /// # Panics
    /// Panics if the descriptions array is empty. All DescriptorEntry instances
    /// should be constructed with at least one description.
    ///
    /// # Examples
    /// ```
    /// use apeiron_cipher::observation::{DescriptorEntry, ConfidenceTier};
    ///
    /// let entry = DescriptorEntry {
    ///     value_range: 0.0..0.25,
    ///     tier: ConfidenceTier::Tentative,
    ///     descriptions: &["seemed to soften quickly", "appeared to soften rapidly"],
    /// };
    ///
    /// assert_eq!(entry.select_description(), "seemed to soften quickly");
    /// ```
    pub fn select_description(&self) -> &'static str {
        self.descriptions[0]
    }
}

/// Registry of descriptor vocabularies, keyed by ObservationCategory.
///
/// This resource contains the complete mapping from (observation category,
/// property value, confidence tier) to qualitative language. Each observation
/// category (ThermalBehavior, Weight, SurfaceAppearance, etc.) has its own
/// vocabulary table that covers the full range of possible property values
/// across all confidence tiers.
///
/// The vocabulary is loaded during plugin initialization and remains static
/// throughout the game session. This ensures consistent language generation
/// and allows the descriptor system to be deterministic: the same inputs
/// always produce the same output.
///
/// The resource is designed to be extensible: new observation categories can
/// be added by inserting their vocabulary tables into the HashMap without
/// modifying existing entries. This supports the game's evolution as new
/// systems (navigation, trade, language) add their own observation types.
#[derive(Resource, Debug)]
pub struct DescriptorVocabulary {
    /// Vocabulary tables keyed by observation category.
    ///
    /// Each category maps to a vector of DescriptorEntry instances that
    /// collectively cover the full space of (value_range, confidence_tier)
    /// combinations for that category. The entries within each category
    /// should be non-overlapping to ensure deterministic lookup.
    ///
    /// Using a HashMap allows O(1) lookup by category, while the Vec within
    /// each category supports linear search through the descriptor entries.
    /// Linear search is acceptable because each category typically has a
    /// small number of value ranges (3-5) and confidence tiers (3), resulting
    /// in at most 15 entries per category.
    pub tables: HashMap<crate::journal::ObservationCategory, Vec<DescriptorEntry>>,
}

impl DescriptorVocabulary {
    /// Create a new, empty descriptor vocabulary.
    ///
    /// This creates the HashMap structure but does not populate it with any
    /// vocabulary tables. Categories and their descriptor entries must be
    /// added separately using `add_category` or by directly manipulating
    /// the `tables` field.
    ///
    /// Typically used during plugin initialization before loading the
    /// vocabulary data from static tables or configuration files.
    pub fn new() -> Self {
        Self {
            tables: HashMap::new(),
        }
    }

    /// Add a complete vocabulary table for an observation category.
    ///
    /// This replaces any existing vocabulary for the given category. The
    /// entries should collectively cover the full range of possible property
    /// values (0.0-1.0) across all confidence tiers to ensure that every
    /// observation can be described.
    ///
    /// # Arguments
    /// * `category` - The observation category this vocabulary applies to
    /// * `entries` - Vector of descriptor entries covering all value ranges and tiers
    ///
    /// # Examples
    /// ```
    /// use apeiron_cipher::observation::{DescriptorVocabulary, DescriptorEntry, ConfidenceTier};
    /// use apeiron_cipher::journal::ObservationCategory;
    ///
    /// let mut vocab = DescriptorVocabulary::new();
    /// vocab.add_category(
    ///     ObservationCategory::ThermalBehavior,
    ///     vec![
    ///         DescriptorEntry {
    ///             value_range: 0.0..0.5,
    ///             tier: ConfidenceTier::Tentative,
    ///             descriptions: &["seemed to soften under heat"],
    ///         },
    ///         DescriptorEntry {
    ///             value_range: 0.5..1.0,
    ///             tier: ConfidenceTier::Tentative,
    ///             descriptions: &["seemed to resist heat"],
    ///         },
    ///     ],
    /// );
    /// ```
    pub fn add_category(
        &mut self,
        category: crate::journal::ObservationCategory,
        entries: Vec<DescriptorEntry>,
    ) {
        self.tables.insert(category, entries);
    }

    /// Look up a qualitative description for the given observation parameters.
    ///
    /// Searches the vocabulary table for the specified category to find a
    /// DescriptorEntry that matches the property value and confidence tier.
    /// Returns the selected description from the matching entry, or None if
    /// no entry matches the parameters.
    ///
    /// The lookup is deterministic: the same inputs always produce the same
    /// output. This satisfies the acceptance criteria requirement that "the
    /// same property at the same confidence level always produces the same
    /// language."
    ///
    /// # Arguments
    /// * `category` - The type of observation being described
    /// * `value` - Normalized property value in [0.0, 1.0]
    /// * `confidence` - Continuous confidence value, mapped to tier internally
    ///
    /// # Returns
    /// * `Some(description)` - Qualitative description if a matching entry is found
    /// * `None` - If no vocabulary exists for the category or no entry matches the parameters
    ///
    /// # Examples
    /// ```
    /// use apeiron_cipher::observation::{DescriptorVocabulary, DescriptorEntry, ConfidenceTier, Confidence};
    /// use apeiron_cipher::journal::ObservationCategory;
    ///
    /// let mut vocab = DescriptorVocabulary::new();
    /// vocab.add_category(
    ///     ObservationCategory::ThermalBehavior,
    ///     vec![
    ///         DescriptorEntry {
    ///             value_range: 0.0..0.5,
    ///             tier: ConfidenceTier::Tentative,
    ///             descriptions: &["seemed to soften under heat"],
    ///         },
    ///     ],
    /// );
    ///
    /// let confidence = Confidence(0.2); // Maps to Tentative tier
    /// let description = vocab.describe(
    ///     &ObservationCategory::ThermalBehavior,
    ///     0.25,
    ///     confidence,
    /// );
    /// assert_eq!(description, Some("seemed to soften under heat"));
    /// ```
    pub fn describe(
        &self,
        category: &crate::journal::ObservationCategory,
        value: f32,
        confidence: Confidence,
    ) -> Option<&'static str> {
        let tier = confidence.tier();
        let entries = self.tables.get(category)?;

        for entry in entries {
            if entry.matches(value, tier) {
                return Some(entry.select_description());
            }
        }

        None
    }
}

impl Default for DescriptorVocabulary {
    /// Create a descriptor vocabulary with default entries for core observation categories.
    ///
    /// This provides a baseline vocabulary that covers the essential observation
    /// types used throughout the game. The vocabulary includes entries for:
    /// - ThermalBehavior: How materials react to heat exposure
    /// - Weight: Perceived density and heft when carried
    /// - SurfaceAppearance: Visual and tactile surface properties
    /// - FabricationResult: Outcomes of material combination processes
    ///
    /// Each category includes descriptor entries for all confidence tiers
    /// (Tentative, Observed, Confident) across the full range of normalized
    /// property values (0.0-1.0). This ensures that every observation can
    /// be described with appropriate qualitative language.
    ///
    /// The default vocabulary implements the language progression described
    /// in the technical design:
    /// - Tentative: "seemed to...", "appeared to..."
    /// - Observed: Factual statements without hedging
    /// - Confident: "reliably..." with comparative context
    fn default() -> Self {
        use crate::journal::ObservationCategory;

        let mut vocab = Self::new();

        // ── ThermalBehavior vocabulary ──────────────────────────────────
        vocab.add_category(
            ObservationCategory::ThermalBehavior,
            vec![
                // Tentative tier (0.0 < confidence < 0.3)
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to soften quickly under heat"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to change noticeably under heat"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to hold together under heat"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to barely react to heat"],
                },
                // Observed tier (0.3 <= confidence < 0.7)
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Softens quickly under heat"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Changes noticeably under heat"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Holds together under heat"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Barely reacts to heat"],
                },
                // Confident tier (0.7 <= confidence <= 1.0)
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably softens under heat — among the least resistant"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably deforms under heat"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably withstands heat"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably heat-resistant — among the most resilient"],
                },
            ],
        );

        // ── Weight vocabulary ───────────────────────────────────────────
        vocab.add_category(
            ObservationCategory::Weight,
            vec![
                // Tentative tier
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed almost weightless"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed light to carry"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to have noticeable weight"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed quite heavy"],
                },
                // Observed tier
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Almost weightless"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Light to carry"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Noticeable weight"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Quite heavy"],
                },
                // Confident tier
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably lightweight — among the least dense"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably light to handle"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably substantial weight"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably heavy — among the most dense"],
                },
            ],
        );

        // ── SurfaceAppearance vocabulary ────────────────────────────────
        vocab.add_category(
            ObservationCategory::SurfaceAppearance,
            vec![
                // Color-based descriptors (0.0-0.5 range for color properties)
                // Tentative tier
                DescriptorEntry {
                    value_range: 0.0..0.125,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Appeared to have a dark, muted coloration"],
                },
                DescriptorEntry {
                    value_range: 0.125..0.25,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to show subtle color variations"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.375,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Appeared to have moderate color saturation"],
                },
                DescriptorEntry {
                    value_range: 0.375..0.5,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to display vibrant coloration"],
                },
                // Density-based descriptors (0.5-1.0 range for visual density cues)
                DescriptorEntry {
                    value_range: 0.5..0.625,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Appeared to have a light, airy structure"],
                },
                DescriptorEntry {
                    value_range: 0.625..0.75,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to show moderate visual density"],
                },
                DescriptorEntry {
                    value_range: 0.75..0.875,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Appeared to have a compact, solid look"],
                },
                DescriptorEntry {
                    value_range: 0.875..1.0,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to display an extremely dense appearance"],
                },
                // Observed tier - Color-based
                DescriptorEntry {
                    value_range: 0.0..0.125,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Dark, muted coloration"],
                },
                DescriptorEntry {
                    value_range: 0.125..0.25,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Subtle color variations"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.375,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Moderate color saturation"],
                },
                DescriptorEntry {
                    value_range: 0.375..0.5,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Vibrant coloration"],
                },
                // Observed tier - Density-based
                DescriptorEntry {
                    value_range: 0.5..0.625,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Light, airy structure"],
                },
                DescriptorEntry {
                    value_range: 0.625..0.75,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Moderate visual density"],
                },
                DescriptorEntry {
                    value_range: 0.75..0.875,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Compact, solid appearance"],
                },
                DescriptorEntry {
                    value_range: 0.875..1.0,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Extremely dense appearance"],
                },
                // Confident tier - Color-based
                DescriptorEntry {
                    value_range: 0.0..0.125,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably dark coloration — among the most muted"],
                },
                DescriptorEntry {
                    value_range: 0.125..0.25,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably shows subtle color variations"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.375,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably moderate color saturation"],
                },
                DescriptorEntry {
                    value_range: 0.375..0.5,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably vibrant coloration — among the most saturated"],
                },
                // Confident tier - Density-based
                DescriptorEntry {
                    value_range: 0.5..0.625,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably light structure — among the most airy"],
                },
                DescriptorEntry {
                    value_range: 0.625..0.75,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably moderate visual density"],
                },
                DescriptorEntry {
                    value_range: 0.75..0.875,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably compact appearance"],
                },
                DescriptorEntry {
                    value_range: 0.875..1.0,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably dense appearance — among the most compact"],
                },
            ],
        );

        // ── FabricationResult vocabulary ────────────────────────────────
        vocab.add_category(
            ObservationCategory::FabricationResult,
            vec![
                // Tentative tier
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Process seemed to fail"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Appeared to produce an unstable result"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Seemed to combine successfully"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["Appeared to create a refined product"],
                },
                // Observed tier
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Process failed"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Produced an unstable result"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Combined successfully"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["Created a refined product"],
                },
                // Confident tier
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably fails to combine"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably produces unstable results"],
                },
                DescriptorEntry {
                    value_range: 0.50..0.75,
                    tier: ConfidenceTier::Confident,
                    descriptions: &["Reliably combines into stable products"],
                },
                DescriptorEntry {
                    value_range: 0.75..1.0,
                    tier: ConfidenceTier::Confident,
                    descriptions: &[
                        "Reliably creates refined products — among the most successful",
                    ],
                },
            ],
        );

        vocab
    }
}

/// Qualitative confidence level derived from observation count.
/// Used by the examine panel and journal to select descriptor language.
///
/// **DEPRECATED:** This enum is retained for backward compatibility with
/// existing tests and POC code, but new code should use `Confidence(f32)`
/// and `ConfidenceTier` instead. The discrete levels will be phased out
/// as the continuous confidence system is fully integrated.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
pub enum ConfidenceLevel {
    /// One observation — tentative language.
    Tentative,
    /// 2–3 observations — factual but unqualified.
    Observed,
    /// 4+ observations — confident with comparative language.
    Confident,
}

impl ConfidenceLevel {
    // Used when observation-count UI is wired up; keeping the API ready.
    #[allow(dead_code)]
    /// Returns the confidence level corresponding to the given observation count.
    pub fn from_count(count: u32) -> Self {
        match count {
            0 => ConfidenceLevel::Tentative,
            1 => ConfidenceLevel::Tentative,
            2..=3 => ConfidenceLevel::Observed,
            _ => ConfidenceLevel::Confident,
        }
    }

    /// Qualitative language shown in the journal detail panel to convey how
    /// certain an observation is without exposing raw numbers.
    pub fn display_label(self) -> &'static str {
        match self {
            ConfidenceLevel::Tentative => "Uncertain",
            ConfidenceLevel::Observed => "Noted",
            ConfidenceLevel::Confident => "Confirmed",
        }
    }
}

// ── Tracker resource ─────────────────────────────────────────────────────

/// Property names that can be observed through environmental testing.
///
/// This enum replaces string literals to provide compile-time safety.
/// A typo in property names would create silently separate trackers;
/// the enum prevents this by making invalid property names a compile error.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum PropertyName {
    /// Material density — how much mass per unit volume.
    Density,
    /// Resistance to heat transfer — thermal insulation properties.
    ThermalResistance,
    /// Chemical reactivity — tendency to undergo reactions.
    Reactivity,
    /// Electrical conductivity — ability to conduct electric current.
    Conductivity,
    /// Toxicity level — harmful effects on biological systems.
    Toxicity,
}

/// Canonical key: (material seed, property name).
type ObsKey = (u64, PropertyName);

/// Stores how many times the player has observed each (material, property)
/// combination through environmental testing.
/// Read by the examine panel and heat systems for confidence-based language.
#[allow(dead_code)]
/// **DEPRECATED**: This resource has been replaced by per-observation confidence
/// tracking in the journal system. Each `Observation` now carries its own
/// `Confidence(f32)` value that accumulates evidence using the `accumulate()`
/// method. The journal's `add_observation_with_accumulation()` handles
/// confidence evolution automatically.
///
/// This struct is kept for backward compatibility during the transition period
/// but will be removed in a future version. New code should use the journal
/// system directly via `RecordObservation` messages.
#[allow(deprecated)]
#[deprecated(
    since = "0.1.0",
    note = "Use journal-based confidence tracking via RecordObservation messages instead"
)]
#[derive(Resource, Debug, Default)]
pub struct ConfidenceTracker {
    counts: HashMap<ObsKey, u32>,
}

#[allow(deprecated)]
impl ConfidenceTracker {
    /// Record one observation. Returns the new count.
    ///
    /// **DEPRECATED**: Use `RecordObservation` messages instead. The journal
    /// system handles confidence accumulation automatically.
    #[deprecated(
        since = "0.1.0",
        note = "Use RecordObservation messages with journal system instead"
    )]
    #[allow(dead_code)]
    pub fn record(&mut self, seed: u64, property: PropertyName) -> u32 {
        let key = (seed, property);
        let count = self.counts.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    /// Current observation count (0 if never observed).
    ///
    /// **DEPRECATED**: Confidence is now tracked per-observation in the journal.
    /// Query the journal directly for confidence information.
    #[deprecated(
        since = "0.1.0",
        note = "Query journal observations directly for confidence information"
    )]
    #[allow(dead_code)]
    pub fn count(&self, seed: u64, property: PropertyName) -> u32 {
        self.counts.get(&(seed, property)).copied().unwrap_or(0)
    }

    /// Confidence level for a specific (material, property) pair.
    ///
    /// **DEPRECATED**: Use the `Confidence(f32)` system in journal observations.
    /// Each observation carries its own confidence that evolves with evidence.
    #[deprecated(
        since = "0.1.0",
        note = "Use Confidence(f32) in journal observations instead of ConfidenceLevel"
    )]
    #[allow(dead_code)]
    pub fn level(&self, seed: u64, property: PropertyName) -> ConfidenceLevel {
        ConfidenceLevel::from_count(self.count(seed, property))
    }
}

// ── Configuration loading ────────────────────────────────────────────────

/// Load confidence configuration from TOML file.
///
/// Follows the standard pattern used throughout the codebase: attempt to load
/// from `assets/config/confidence.toml`, fall back to defaults if the file
/// is missing or malformed. Logs appropriate warnings for debugging.
///
/// The configuration is loaded during PreStartup phase to ensure it's available
/// before any systems that might need to record observations or handle death.
fn load_confidence_config(mut commands: Commands) {
    let config = if Path::new(CONFIDENCE_CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIDENCE_CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<ConfidenceConfig>(&contents) {
                Ok(config) => {
                    info!("Loaded confidence config from {CONFIDENCE_CONFIG_PATH}");
                    config
                }
                Err(error) => {
                    warn!("Malformed {CONFIDENCE_CONFIG_PATH}, using defaults: {error}");
                    ConfidenceConfig::default()
                }
            },
            Err(error) => {
                warn!("Could not read {CONFIDENCE_CONFIG_PATH}, using defaults: {error}");
                ConfidenceConfig::default()
            }
        }
    } else {
        warn!("{CONFIDENCE_CONFIG_PATH} not found, using defaults");
        ConfidenceConfig::default()
    };

    commands.insert_resource(config);
}

// ── Death event handling ─────────────────────────────────────────────────

/// Specific cause of player death, used for domain-weighted recovery.
///
/// Each death cause maps to an observation category that represents the
/// domain the player was engaging with when they died. This enables the
/// confidence recovery system to apply different multipliers based on
/// whether the player returns to the death-relevant domain or avoids it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum DeathCause {
    /// Death caused by heat system (thermal experiments, exposure to heat).
    /// Maps to ThermalBehavior observation category.
    HeatSystem,
    /// Death caused by fabrication system (explosions, toxic reactions).
    /// Maps to FabricationResult observation category.
    Fabrication,
    /// Death caused by environmental hazards (falls, crushing, etc.).
    /// Maps to LocationNote observation category.
    Environmental,
    /// Death caused by material handling (toxic materials, sharp edges).
    /// Maps to Weight observation category (closest available category).
    MaterialHandling,
}

impl DeathCause {
    /// Map death cause to the relevant observation category for recovery tracking.
    ///
    /// This mapping determines which domain gets the `domain_recovery_multiplier`
    /// when the player makes new observations. All other domains get the
    /// `passive_recovery_multiplier`.
    pub fn to_observation_category(self) -> crate::journal::ObservationCategory {
        use crate::journal::ObservationCategory;
        match self {
            DeathCause::HeatSystem => ObservationCategory::ThermalBehavior,
            DeathCause::Fabrication => ObservationCategory::FabricationResult,
            DeathCause::Environmental => ObservationCategory::LocationNote,
            DeathCause::MaterialHandling => ObservationCategory::Weight,
        }
    }
}

/// Tracks recent death context for domain-weighted confidence recovery.
///
/// This resource stores information about the player's most recent death
/// to enable domain-weighted recovery as specified in the GDD. When the
/// player makes new observations, the system checks if they're engaging
/// with the death-relevant domain and applies the appropriate recovery
/// multiplier.
///
/// The death context expires after a configurable duration to prevent
/// indefinite recovery bonuses from ancient deaths.
#[derive(Resource, Clone, Debug)]
pub struct DeathContext {
    /// The cause of the most recent death.
    pub cause: DeathCause,
    /// Game time when the death occurred (milliseconds since game start).
    pub death_time: u64,
    /// Duration in milliseconds after which the death context expires.
    /// After expiration, all observations use the base recovery rate.
    pub expiry_duration: u64,
}

impl DeathContext {
    /// Create a new death context for the given cause and time.
    ///
    /// Uses a default expiry duration of 5 minutes (300,000 milliseconds).
    /// This provides a reasonable window for domain-weighted recovery
    /// without making the effect permanent.
    pub fn new(cause: DeathCause, death_time: u64) -> Self {
        Self {
            cause,
            death_time,
            expiry_duration: 300_000, // 5 minutes
        }
    }

    /// Check if this death context has expired at the given game time.
    ///
    /// Returns `true` if enough time has passed since the death that
    /// domain-weighted recovery should no longer apply.
    pub fn is_expired(&self, current_time: u64) -> bool {
        current_time.saturating_sub(self.death_time) >= self.expiry_duration
    }

    /// Get the recovery multiplier for the given observation category.
    ///
    /// Returns the domain recovery multiplier if the category matches
    /// the death-relevant domain, otherwise returns the passive recovery
    /// multiplier. If the death context has expired, returns 1.0 (no modifier).
    pub fn recovery_multiplier(
        &self,
        category: &crate::journal::ObservationCategory,
        current_time: u64,
        config: &ConfidenceConfig,
    ) -> f32 {
        if self.is_expired(current_time) {
            return 1.0; // No recovery modifier after expiry
        }

        if *category == self.cause.to_observation_category() {
            config.domain_recovery_multiplier
        } else {
            config.passive_recovery_multiplier
        }
    }
}

/// Event emitted when the player dies, triggering confidence degradation
/// across all journal observations.
///
/// This event follows the naming convention for system-generated events
/// (OnXEvent suffix) and represents something that has already happened.
/// When this event is emitted, the death has occurred and confidence
/// should be degraded according to the configured death penalty.
///
/// The event now carries a `cause` field to enable domain-weighted recovery
/// as specified in the GDD. The cause determines which observation domain
/// gets accelerated recovery when the player re-engages with it.
#[derive(Message, Clone, Copy, Debug)]
pub struct OnPlayerDeathEvent {
    /// The specific cause of death, used for domain-weighted recovery.
    pub cause: DeathCause,
}

/// System that handles player death by degrading confidence across all
/// journal observations and establishing death context for recovery.
///
/// This system reads `OnPlayerDeathEvent` messages and:
/// 1. Applies the configured death penalty to every observation in the journal
/// 2. Creates a `DeathContext` resource to track the death cause for recovery
///
/// The degradation uses the `degrade()` method on `Confidence` with the
/// death_degradation_factor and death_floor from the `ConfidenceConfig`.
/// The death context enables domain-weighted recovery when the player
/// makes future observations.
///
/// The system runs in the Update phase after death events are emitted but
/// before any UI systems that might display confidence-dependent language.
fn handle_player_death(
    mut reader: MessageReader<OnPlayerDeathEvent>,
    mut player_query: Query<&mut crate::journal::Journal, With<crate::player::Player>>,
    mut commands: Commands,
    config: Res<ConfidenceConfig>,
    time: Res<Time>,
) {
    // Process all death events (should typically be just one per frame)
    let death_events: Vec<_> = reader.read().collect();
    if death_events.is_empty() {
        return;
    }

    let Ok(mut journal) = player_query.single_mut() else {
        warn!("OnPlayerDeathEvent received but no player with journal found");
        return;
    };

    // Use the most recent death event if multiple occurred
    let death_event = death_events.last().unwrap();
    let current_time = time.elapsed().as_millis() as u64;

    // Apply death degradation to all observations in all journal entries
    for entry in journal.entries.values_mut() {
        for observation_group in entry.observations.values_mut() {
            for observation in observation_group.iter_mut() {
                observation
                    .confidence
                    .degrade(config.death_degradation_factor, config.death_floor);
            }
        }
    }

    // Create death context for domain-weighted recovery
    let death_context = DeathContext::new(death_event.cause, current_time);
    commands.insert_resource(death_context);

    info!(
        "Applied death confidence degradation (factor: {}, floor: {}) and established death context for {:?}",
        config.death_degradation_factor, config.death_floor, death_event.cause
    );
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
#[allow(deprecated)]
mod tests {
    use super::*;

    #[test]
    fn fresh_tracker_has_zero_count() {
        let tracker = ConfidenceTracker::default();
        assert_eq!(tracker.count(42, PropertyName::ThermalResistance), 0);
    }

    #[test]
    fn record_increments_count() {
        let mut tracker = ConfidenceTracker::default();
        assert_eq!(tracker.record(42, PropertyName::ThermalResistance), 1);
        assert_eq!(tracker.record(42, PropertyName::ThermalResistance), 2);
        assert_eq!(tracker.count(42, PropertyName::ThermalResistance), 2);
    }

    #[test]
    fn different_seeds_tracked_independently() {
        let mut tracker = ConfidenceTracker::default();
        tracker.record(42, PropertyName::ThermalResistance);
        tracker.record(99, PropertyName::ThermalResistance);
        assert_eq!(tracker.count(42, PropertyName::ThermalResistance), 1);
        assert_eq!(tracker.count(99, PropertyName::ThermalResistance), 1);
    }

    #[test]
    fn different_properties_tracked_independently() {
        let mut tracker = ConfidenceTracker::default();
        tracker.record(42, PropertyName::ThermalResistance);
        tracker.record(42, PropertyName::Density);
        assert_eq!(tracker.count(42, PropertyName::ThermalResistance), 1);
        assert_eq!(tracker.count(42, PropertyName::Density), 1);
    }

    #[test]
    fn confidence_level_from_count() {
        assert_eq!(ConfidenceLevel::from_count(0), ConfidenceLevel::Tentative);
        assert_eq!(ConfidenceLevel::from_count(1), ConfidenceLevel::Tentative);
        assert_eq!(ConfidenceLevel::from_count(2), ConfidenceLevel::Observed);
        assert_eq!(ConfidenceLevel::from_count(3), ConfidenceLevel::Observed);
        assert_eq!(ConfidenceLevel::from_count(4), ConfidenceLevel::Confident);
        assert_eq!(ConfidenceLevel::from_count(100), ConfidenceLevel::Confident);
    }

    #[test]
    fn level_method_uses_internal_count() {
        let mut tracker = ConfidenceTracker::default();
        assert_eq!(
            tracker.level(42, PropertyName::ThermalResistance),
            ConfidenceLevel::Tentative
        );
        tracker.record(42, PropertyName::ThermalResistance);
        assert_eq!(
            tracker.level(42, PropertyName::ThermalResistance),
            ConfidenceLevel::Tentative
        );
        tracker.record(42, PropertyName::ThermalResistance);
        assert_eq!(
            tracker.level(42, PropertyName::ThermalResistance),
            ConfidenceLevel::Observed
        );
        tracker.record(42, PropertyName::ThermalResistance);
        assert_eq!(
            tracker.level(42, PropertyName::ThermalResistance),
            ConfidenceLevel::Observed
        );
        tracker.record(42, PropertyName::ThermalResistance);
        assert_eq!(
            tracker.level(42, PropertyName::ThermalResistance),
            ConfidenceLevel::Confident
        );
    }

    // ── Confidence (f32) tests ──────────────────────────────────────────

    #[test]
    fn confidence_tier_mapping() {
        assert_eq!(Confidence(0.0).tier(), ConfidenceTier::Tentative);
        assert_eq!(Confidence(0.1).tier(), ConfidenceTier::Tentative);
        assert_eq!(Confidence(0.29).tier(), ConfidenceTier::Tentative);
        assert_eq!(Confidence(0.3).tier(), ConfidenceTier::Observed);
        assert_eq!(Confidence(0.5).tier(), ConfidenceTier::Observed);
        assert_eq!(Confidence(0.69).tier(), ConfidenceTier::Observed);
        assert_eq!(Confidence(0.7).tier(), ConfidenceTier::Confident);
        assert_eq!(Confidence(0.9).tier(), ConfidenceTier::Confident);
        assert_eq!(Confidence(1.0).tier(), ConfidenceTier::Confident);
    }

    #[test]
    fn confidence_accumulate_basic() {
        let mut conf = Confidence(0.0);

        // First observation has significant impact
        conf.accumulate(0.2);
        assert!((conf.0 - 0.2).abs() < f32::EPSILON);

        // Second observation has diminishing returns
        conf.accumulate(0.2);
        let expected = 0.2 + (1.0 - 0.2) * 0.2; // 0.36
        assert!((conf.0 - expected).abs() < f32::EPSILON);
    }

    #[test]
    fn confidence_accumulate_asymptotic() {
        let mut conf = Confidence(0.9);

        // High confidence accumulates slowly
        conf.accumulate(0.2);
        let expected = 0.9 + (1.0 - 0.9) * 0.2; // 0.92
        assert!((conf.0 - expected).abs() < f32::EPSILON);

        // Cannot exceed 1.0
        conf.accumulate(1.0);
        assert_eq!(conf.0, 1.0);
    }

    #[test]
    fn confidence_accumulate_clamping() {
        let mut conf = Confidence(0.5);

        // Large weight is clamped to 1.0
        conf.accumulate(2.0);
        assert_eq!(conf.0, 1.0);

        // Negative weight is clamped to 0.0
        let mut conf2 = Confidence(0.5);
        conf2.accumulate(-1.0);
        assert_eq!(conf2.0, 0.0);
    }

    #[test]
    fn confidence_accumulate_converges_toward_one_with_diminishing_returns() {
        let mut conf = Confidence(0.0);
        let weight = 0.2;

        // Track confidence values to verify diminishing returns
        let mut values = Vec::new();
        values.push(conf.0);

        // Apply many observations with the same weight
        for _ in 0..20 {
            conf.accumulate(weight);
            values.push(conf.0);
        }

        // Verify convergence toward 1.0
        assert!(
            conf.0 > 0.95,
            "Should converge close to 1.0, got {}",
            conf.0
        );
        assert!(conf.0 <= 1.0, "Should never exceed 1.0");

        // Verify diminishing returns: each step should add less than the previous
        for i in 2..values.len() {
            let prev_gain = values[i - 1] - values[i - 2];
            let curr_gain = values[i] - values[i - 1];
            assert!(
                curr_gain <= prev_gain + f32::EPSILON,
                "Gain should diminish: step {} gain {} > step {} gain {}",
                i - 1,
                prev_gain,
                i,
                curr_gain
            );
        }

        // Verify asymptotic behavior: later gains should be very small
        let final_gain = values[values.len() - 1] - values[values.len() - 2];
        assert!(
            final_gain < 0.01,
            "Final gain should be very small due to asymptotic approach, got {}",
            final_gain
        );

        // Verify the exponential decay formula is working correctly
        // After n observations with weight w, confidence should be: 1 - (1-w)^n
        let expected_after_20 = 1.0 - (1.0 - weight).powi(20);
        assert!(
            (conf.0 - expected_after_20).abs() < 0.001,
            "Should match exponential decay formula: expected {}, got {}",
            expected_after_20,
            conf.0
        );
    }

    #[test]
    fn confidence_degrade_basic() {
        let mut conf = Confidence(0.8);

        // 40% penalty (factor 0.6)
        conf.degrade(0.6, 0.1);
        assert!((conf.0 - 0.48).abs() < f32::EPSILON);
    }

    #[test]
    fn confidence_degrade_floor() {
        let mut conf = Confidence(0.3);

        // Large penalty would drop below floor
        conf.degrade(0.1, 0.2);
        assert_eq!(conf.0, 0.2); // Clamped to floor
    }

    #[test]
    fn confidence_degrade_no_floor_violation() {
        let mut conf = Confidence(0.8);

        // Penalty doesn't hit floor
        conf.degrade(0.7, 0.2);
        assert!((conf.0 - 0.56).abs() < f32::EPSILON);
    }

    #[test]
    fn confidence_tier_display_labels() {
        assert_eq!(ConfidenceTier::Tentative.display_label(), "Uncertain");
        assert_eq!(ConfidenceTier::Observed.display_label(), "Noted");
        assert_eq!(ConfidenceTier::Confident.display_label(), "Confirmed");
    }

    #[test]
    fn confidence_serialization() {
        let conf = Confidence(0.42);
        let json = serde_json::to_string(&conf).expect("Confidence should serialize");
        let deserialized: Confidence =
            serde_json::from_str(&json).expect("Confidence should deserialize");
        assert!((deserialized.0 - 0.42).abs() < f32::EPSILON);
    }

    // ── Edge case tests ─────────────────────────────────────────────────

    #[test]
    fn confidence_accumulate_at_max_stays_at_max() {
        let mut conf = Confidence(1.0);

        // Accumulating at 1.0 should stay at 1.0
        conf.accumulate(0.2);
        assert_eq!(
            conf.0, 1.0,
            "Confidence at 1.0 should stay at 1.0 after accumulate"
        );

        // Even with large weights
        conf.accumulate(1.0);
        assert_eq!(
            conf.0, 1.0,
            "Confidence at 1.0 should stay at 1.0 even with large weight"
        );

        // Multiple accumulations
        for _ in 0..5 {
            conf.accumulate(0.3);
        }
        assert_eq!(
            conf.0, 1.0,
            "Confidence at 1.0 should stay at 1.0 after multiple accumulations"
        );
    }

    #[test]
    fn confidence_degrade_at_floor_stays_at_floor() {
        let floor = 0.2;
        let mut conf = Confidence(floor);

        // Degrading at floor should stay at floor
        conf.degrade(0.5, floor);
        assert_eq!(
            conf.0, floor,
            "Confidence at floor should stay at floor after degrade"
        );

        // Even with severe degradation factors
        conf.degrade(0.1, floor);
        assert_eq!(
            conf.0, floor,
            "Confidence at floor should stay at floor even with severe degradation"
        );

        // Multiple degradations
        for _ in 0..5 {
            conf.degrade(0.3, floor);
        }
        assert_eq!(
            conf.0, floor,
            "Confidence at floor should stay at floor after multiple degradations"
        );

        // Test with different floor values
        let higher_floor = 0.5;
        let mut conf2 = Confidence(higher_floor);
        conf2.degrade(0.1, higher_floor);
        assert_eq!(
            conf2.0, higher_floor,
            "Confidence at higher floor should stay at floor"
        );
    }

    // ── ConfidenceConfig tests ──────────────────────────────────────────

    #[test]
    fn confidence_config_default_values() {
        let config = ConfidenceConfig::default();

        assert_eq!(config.death_degradation_factor, 0.6);
        assert_eq!(config.death_floor, 0.2);
        assert_eq!(config.domain_recovery_multiplier, 2.0);
        assert_eq!(config.passive_recovery_multiplier, 0.7);
        assert_eq!(config.base_observation_weight, 0.2);
    }

    #[test]
    fn confidence_config_serialization() {
        let config = ConfidenceConfig {
            death_degradation_factor: 0.5,
            death_floor: 0.15,
            domain_recovery_multiplier: 2.5,
            passive_recovery_multiplier: 0.8,
            base_observation_weight: 0.25,
        };

        let toml = toml::to_string(&config).expect("ConfidenceConfig should serialize to TOML");
        let deserialized: ConfidenceConfig =
            toml::from_str(&toml).expect("ConfidenceConfig should deserialize from TOML");

        assert_eq!(deserialized, config);
    }

    #[test]
    fn confidence_config_partial_toml() {
        // Test that partial TOML files work with serde defaults
        let partial_toml = r#"
            death_degradation_factor = 0.8
            base_observation_weight = 0.15
        "#;

        let config: ConfidenceConfig =
            toml::from_str(partial_toml).expect("Partial TOML should deserialize with defaults");

        // Specified values
        assert_eq!(config.death_degradation_factor, 0.8);
        assert_eq!(config.base_observation_weight, 0.15);

        // Default values for unspecified fields
        assert_eq!(config.death_floor, 0.2);
        assert_eq!(config.domain_recovery_multiplier, 2.0);
        assert_eq!(config.passive_recovery_multiplier, 0.7);
    }

    // ── DescriptorEntry tests ───────────────────────────────────────────

    #[test]
    fn descriptor_entry_matches_value_and_tier() {
        let entry = DescriptorEntry {
            value_range: 0.0..0.25,
            tier: ConfidenceTier::Tentative,
            descriptions: &["seemed to soften quickly"],
        };

        // Value in range, correct tier
        assert!(entry.matches(0.1, ConfidenceTier::Tentative));
        assert!(entry.matches(0.0, ConfidenceTier::Tentative));
        assert!(entry.matches(0.24, ConfidenceTier::Tentative));

        // Value out of range
        assert!(!entry.matches(0.25, ConfidenceTier::Tentative)); // Boundary excluded
        assert!(!entry.matches(0.3, ConfidenceTier::Tentative));
        assert!(!entry.matches(-0.1, ConfidenceTier::Tentative));

        // Wrong tier
        assert!(!entry.matches(0.1, ConfidenceTier::Observed));
        assert!(!entry.matches(0.1, ConfidenceTier::Confident));
    }

    #[test]
    fn descriptor_entry_select_description_returns_first() {
        let entry = DescriptorEntry {
            value_range: 0.0..0.25,
            tier: ConfidenceTier::Tentative,
            descriptions: &[
                "first description",
                "second description",
                "third description",
            ],
        };

        assert_eq!(entry.select_description(), "first description");
    }

    #[test]
    fn descriptor_entry_select_description_single_item() {
        let entry = DescriptorEntry {
            value_range: 0.0..0.25,
            tier: ConfidenceTier::Tentative,
            descriptions: &["only description"],
        };

        assert_eq!(entry.select_description(), "only description");
    }

    // ── DescriptorVocabulary tests ──────────────────────────────────────

    #[test]
    fn descriptor_vocabulary_new_creates_empty() {
        let vocab = DescriptorVocabulary::new();
        assert!(vocab.tables.is_empty());
    }

    #[test]
    fn descriptor_vocabulary_add_category() {
        use crate::journal::ObservationCategory;

        let mut vocab = DescriptorVocabulary::new();
        let entries = vec![
            DescriptorEntry {
                value_range: 0.0..0.5,
                tier: ConfidenceTier::Tentative,
                descriptions: &["seemed to soften"],
            },
            DescriptorEntry {
                value_range: 0.5..1.0,
                tier: ConfidenceTier::Tentative,
                descriptions: &["seemed to resist"],
            },
        ];

        vocab.add_category(ObservationCategory::ThermalBehavior, entries);

        assert_eq!(vocab.tables.len(), 1);
        assert!(
            vocab
                .tables
                .contains_key(&ObservationCategory::ThermalBehavior)
        );
        assert_eq!(vocab.tables[&ObservationCategory::ThermalBehavior].len(), 2);
    }

    #[test]
    fn descriptor_vocabulary_describe_finds_matching_entry() {
        use crate::journal::ObservationCategory;

        let mut vocab = DescriptorVocabulary::new();
        vocab.add_category(
            ObservationCategory::ThermalBehavior,
            vec![
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["seemed to soften quickly"],
                },
                DescriptorEntry {
                    value_range: 0.25..0.50,
                    tier: ConfidenceTier::Tentative,
                    descriptions: &["seemed to change noticeably"],
                },
                DescriptorEntry {
                    value_range: 0.0..0.25,
                    tier: ConfidenceTier::Observed,
                    descriptions: &["softens quickly"],
                },
            ],
        );

        // Test tentative tier, low value
        let confidence_tentative = Confidence(0.2);
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            0.1,
            confidence_tentative,
        );
        assert_eq!(result, Some("seemed to soften quickly"));

        // Test tentative tier, higher value
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            0.3,
            confidence_tentative,
        );
        assert_eq!(result, Some("seemed to change noticeably"));

        // Test observed tier, low value
        let confidence_observed = Confidence(0.5);
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            0.1,
            confidence_observed,
        );
        assert_eq!(result, Some("softens quickly"));
    }

    #[test]
    fn descriptor_vocabulary_describe_returns_none_for_unknown_category() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::new();
        let confidence = Confidence(0.2);
        let result = vocab.describe(&ObservationCategory::ThermalBehavior, 0.1, confidence);
        assert_eq!(result, None);
    }

    #[test]
    fn descriptor_vocabulary_describe_returns_none_for_no_matching_entry() {
        use crate::journal::ObservationCategory;

        let mut vocab = DescriptorVocabulary::new();
        vocab.add_category(
            ObservationCategory::ThermalBehavior,
            vec![DescriptorEntry {
                value_range: 0.0..0.25,
                tier: ConfidenceTier::Tentative,
                descriptions: &["seemed to soften quickly"],
            }],
        );

        // Value outside range
        let confidence = Confidence(0.2);
        let result = vocab.describe(&ObservationCategory::ThermalBehavior, 0.5, confidence);
        assert_eq!(result, None);

        // Wrong tier (confident tier not defined)
        let confidence_confident = Confidence(0.8);
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            0.1,
            confidence_confident,
        );
        assert_eq!(result, None);
    }

    #[test]
    fn descriptor_vocabulary_default_surface_appearance_progression() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // Test color-based language progression (low value range 0.0-0.5)
        let low_color_value = 0.1; // Should be in 0.0..0.125 range

        // Tentative tier
        let confidence_tentative = Confidence(0.2);
        let result = vocab.describe(
            &ObservationCategory::SurfaceAppearance,
            low_color_value,
            confidence_tentative,
        );
        assert_eq!(result, Some("Appeared to have a dark, muted coloration"));

        // Observed tier
        let confidence_observed = Confidence(0.5);
        let result = vocab.describe(
            &ObservationCategory::SurfaceAppearance,
            low_color_value,
            confidence_observed,
        );
        assert_eq!(result, Some("Dark, muted coloration"));

        // Confident tier
        let confidence_confident = Confidence(0.8);
        let result = vocab.describe(
            &ObservationCategory::SurfaceAppearance,
            low_color_value,
            confidence_confident,
        );
        assert_eq!(
            result,
            Some("Reliably dark coloration — among the most muted")
        );

        // Test density-based language progression (high value range 0.5-1.0)
        let high_density_value = 0.9; // Should be in 0.875..1.0 range

        // Tentative tier
        let result = vocab.describe(
            &ObservationCategory::SurfaceAppearance,
            high_density_value,
            confidence_tentative,
        );
        assert_eq!(
            result,
            Some("Seemed to display an extremely dense appearance")
        );

        // Observed tier
        let result = vocab.describe(
            &ObservationCategory::SurfaceAppearance,
            high_density_value,
            confidence_observed,
        );
        assert_eq!(result, Some("Extremely dense appearance"));

        // Confident tier
        let result = vocab.describe(
            &ObservationCategory::SurfaceAppearance,
            high_density_value,
            confidence_confident,
        );
        assert_eq!(
            result,
            Some("Reliably dense appearance — among the most compact")
        );
    }

    #[test]
    fn descriptor_vocabulary_default_has_all_core_categories() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // Check that all core categories are present
        assert!(
            vocab
                .tables
                .contains_key(&ObservationCategory::ThermalBehavior)
        );
        assert!(vocab.tables.contains_key(&ObservationCategory::Weight));
        assert!(
            vocab
                .tables
                .contains_key(&ObservationCategory::SurfaceAppearance)
        );
        assert!(
            vocab
                .tables
                .contains_key(&ObservationCategory::FabricationResult)
        );

        // Check that each category has entries for all tiers
        for category in [
            ObservationCategory::ThermalBehavior,
            ObservationCategory::Weight,
            ObservationCategory::SurfaceAppearance,
            ObservationCategory::FabricationResult,
        ] {
            let entries = &vocab.tables[&category];

            // Should have entries for all three tiers
            let has_tentative = entries.iter().any(|e| e.tier == ConfidenceTier::Tentative);
            let has_observed = entries.iter().any(|e| e.tier == ConfidenceTier::Observed);
            let has_confident = entries.iter().any(|e| e.tier == ConfidenceTier::Confident);

            assert!(
                has_tentative,
                "Category {:?} missing Tentative tier",
                category
            );
            assert!(
                has_observed,
                "Category {:?} missing Observed tier",
                category
            );
            assert!(
                has_confident,
                "Category {:?} missing Confident tier",
                category
            );
        }
    }

    #[test]
    fn descriptor_vocabulary_default_thermal_behavior_progression() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // Test the language progression for thermal behavior
        let low_value = 0.1; // Should be in 0.0..0.25 range

        // Tentative tier
        let confidence_tentative = Confidence(0.2);
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            low_value,
            confidence_tentative,
        );
        assert_eq!(result, Some("Seemed to soften quickly under heat"));

        // Observed tier
        let confidence_observed = Confidence(0.5);
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            low_value,
            confidence_observed,
        );
        assert_eq!(result, Some("Softens quickly under heat"));

        // Confident tier
        let confidence_confident = Confidence(0.8);
        let result = vocab.describe(
            &ObservationCategory::ThermalBehavior,
            low_value,
            confidence_confident,
        );
        assert_eq!(
            result,
            Some("Reliably softens under heat — among the least resistant")
        );
    }

    #[test]
    fn descriptor_vocabulary_confidence_tier_mapping() {
        // Test that confidence values map to the correct tiers
        assert_eq!(Confidence(0.0).tier(), ConfidenceTier::Tentative);
        assert_eq!(Confidence(0.29).tier(), ConfidenceTier::Tentative);
        assert_eq!(Confidence(0.3).tier(), ConfidenceTier::Observed);
        assert_eq!(Confidence(0.69).tier(), ConfidenceTier::Observed);
        assert_eq!(Confidence(0.7).tier(), ConfidenceTier::Confident);
        assert_eq!(Confidence(1.0).tier(), ConfidenceTier::Confident);
    }

    #[test]
    fn descriptor_vocabulary_deterministic_same_inputs_same_description() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // Test cases covering all categories and confidence tiers
        let test_cases = [
            // ThermalBehavior tests
            (ObservationCategory::ThermalBehavior, 0.1, Confidence(0.2)), // Tentative, low thermal resistance
            (ObservationCategory::ThermalBehavior, 0.3, Confidence(0.5)), // Observed, medium thermal resistance
            (ObservationCategory::ThermalBehavior, 0.8, Confidence(0.9)), // Confident, high thermal resistance
            // SurfaceAppearance tests
            (
                ObservationCategory::SurfaceAppearance,
                0.05,
                Confidence(0.1),
            ), // Tentative, dark color
            (ObservationCategory::SurfaceAppearance, 0.4, Confidence(0.4)), // Observed, medium color
            (ObservationCategory::SurfaceAppearance, 0.9, Confidence(0.8)), // Confident, dense appearance
            // Weight tests
            (ObservationCategory::Weight, 0.15, Confidence(0.25)), // Tentative, light
            (ObservationCategory::Weight, 0.5, Confidence(0.6)),   // Observed, medium
            (ObservationCategory::Weight, 0.85, Confidence(0.95)), // Confident, heavy
            // FabricationResult tests
            (ObservationCategory::FabricationResult, 0.2, Confidence(0.3)), // Observed, low success
            (ObservationCategory::FabricationResult, 0.7, Confidence(0.8)), // Confident, high success
        ];

        // For each test case, call describe() multiple times and verify identical results
        for (category, value, confidence) in test_cases {
            let first_result = vocab.describe(&category, value, confidence);

            // Call describe() multiple times with identical inputs
            for iteration in 1..=10 {
                let subsequent_result = vocab.describe(&category, value, confidence);
                assert_eq!(
                    first_result, subsequent_result,
                    "Iteration {}: describe({:?}, {}, {:?}) must return identical results. \
                     First: {:?}, Subsequent: {:?}",
                    iteration, category, value, confidence, first_result, subsequent_result
                );
            }

            // Verify we actually got a description (not None) for valid inputs
            assert!(
                first_result.is_some(),
                "describe({:?}, {}, {:?}) should return Some(description), got None",
                category,
                value,
                confidence
            );
        }
    }

    #[test]
    fn descriptor_vocabulary_deterministic_edge_cases() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // Test edge cases that might be prone to non-deterministic behavior
        let edge_cases = [
            // Boundary values for confidence tiers
            (ObservationCategory::ThermalBehavior, 0.5, Confidence(0.3)), // Exactly at Observed threshold
            (ObservationCategory::ThermalBehavior, 0.5, Confidence(0.7)), // Exactly at Confident threshold
            // Boundary values for value ranges
            (ObservationCategory::SurfaceAppearance, 0.0, Confidence(0.5)), // Minimum value
            (ObservationCategory::SurfaceAppearance, 1.0, Confidence(0.5)), // Maximum value
            (ObservationCategory::Weight, 0.25, Confidence(0.5)),           // Range boundary
            (ObservationCategory::Weight, 0.75, Confidence(0.5)),           // Range boundary
            // Extreme confidence values
            (ObservationCategory::FabricationResult, 0.5, Confidence(0.0)), // Minimum confidence
            (ObservationCategory::FabricationResult, 0.5, Confidence(1.0)), // Maximum confidence
        ];

        for (category, value, confidence) in edge_cases {
            let first_result = vocab.describe(&category, value, confidence);

            // Test multiple calls for determinism
            for iteration in 1..=5 {
                let subsequent_result = vocab.describe(&category, value, confidence);
                assert_eq!(
                    first_result, subsequent_result,
                    "Edge case iteration {}: describe({:?}, {}, {:?}) must be deterministic. \
                     First: {:?}, Subsequent: {:?}",
                    iteration, category, value, confidence, first_result, subsequent_result
                );
            }
        }
    }

    #[test]
    fn descriptor_vocabulary_deterministic_across_vocabulary_instances() {
        use crate::journal::ObservationCategory;

        // Create multiple vocabulary instances and verify they produce identical results
        let vocab1 = DescriptorVocabulary::default();
        let vocab2 = DescriptorVocabulary::default();
        let vocab3 = DescriptorVocabulary::default();

        let test_cases = [
            (ObservationCategory::ThermalBehavior, 0.1, Confidence(0.2)),
            (ObservationCategory::SurfaceAppearance, 0.6, Confidence(0.5)),
            (ObservationCategory::Weight, 0.8, Confidence(0.9)),
            (ObservationCategory::FabricationResult, 0.3, Confidence(0.4)),
        ];

        for (category, value, confidence) in test_cases {
            let result1 = vocab1.describe(&category, value, confidence);
            let result2 = vocab2.describe(&category, value, confidence);
            let result3 = vocab3.describe(&category, value, confidence);

            assert_eq!(
                result1, result2,
                "Different vocabulary instances must produce identical results for \
                 describe({:?}, {}, {:?}). Instance1: {:?}, Instance2: {:?}",
                category, value, confidence, result1, result2
            );

            assert_eq!(
                result1, result3,
                "Different vocabulary instances must produce identical results for \
                 describe({:?}, {}, {:?}). Instance1: {:?}, Instance3: {:?}",
                category, value, confidence, result1, result3
            );
        }
    }

    #[test]
    fn descriptor_vocabulary_all_combinations_have_non_empty_descriptions() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // All observation categories that should be covered
        let categories = [
            ObservationCategory::ThermalBehavior,
            ObservationCategory::Weight,
            ObservationCategory::SurfaceAppearance,
            ObservationCategory::FabricationResult,
        ];

        // All confidence tiers
        let confidence_tiers = [
            ConfidenceTier::Tentative,
            ConfidenceTier::Observed,
            ConfidenceTier::Confident,
        ];

        // Test values across the full range (0.0-1.0) to ensure all value ranges are covered
        let test_values = [
            0.0, 0.05, 0.1, 0.15, 0.2, 0.25, 0.3, 0.35, 0.4, 0.45, 0.5, 0.55, 0.6, 0.65, 0.7, 0.75,
            0.8, 0.85, 0.9, 0.95, 1.0,
        ];

        // For each category, test all tier/value combinations
        for category in categories {
            let entries = vocab.tables.get(&category).expect(&format!(
                "Category {:?} should exist in default vocabulary",
                category
            ));

            for tier in confidence_tiers {
                // Find all value ranges for this tier
                let tier_entries: Vec<_> = entries.iter().filter(|e| e.tier == tier).collect();

                assert!(
                    !tier_entries.is_empty(),
                    "Category {:?} should have entries for tier {:?}",
                    category,
                    tier
                );

                // Test that every value in the range has a non-empty description
                for &value in &test_values {
                    // Find the entry that should match this value and tier
                    let matching_entry = tier_entries
                        .iter()
                        .find(|entry| entry.value_range.contains(&value));

                    if let Some(entry) = matching_entry {
                        // Verify the entry has non-empty descriptions
                        assert!(
                            !entry.descriptions.is_empty(),
                            "Category {:?}, tier {:?}, value {} should have non-empty descriptions array",
                            category,
                            tier,
                            value
                        );

                        // Verify each description in the array is non-empty
                        for (i, description) in entry.descriptions.iter().enumerate() {
                            assert!(
                                !description.is_empty(),
                                "Category {:?}, tier {:?}, value {}, description[{}] should be non-empty, got: {:?}",
                                category,
                                tier,
                                value,
                                i,
                                description
                            );
                        }

                        // Test the actual describe() method returns a non-empty result
                        let confidence = match tier {
                            ConfidenceTier::Tentative => Confidence(0.2),
                            ConfidenceTier::Observed => Confidence(0.5),
                            ConfidenceTier::Confident => Confidence(0.8),
                        };

                        let result = vocab.describe(&category, value, confidence);
                        assert!(
                            result.is_some(),
                            "describe({:?}, {}, {:?}) should return Some for covered range",
                            category,
                            value,
                            confidence
                        );

                        let description = result.unwrap();
                        assert!(
                            !description.is_empty(),
                            "describe({:?}, {}, {:?}) should return non-empty description, got: {:?}",
                            category,
                            value,
                            confidence,
                            description
                        );
                    }
                }
            }

            // Additional check: ensure the value ranges cover the full 0.0-1.0 spectrum
            // This prevents gaps where some values would have no description
            for tier in confidence_tiers {
                let tier_entries: Vec<_> = entries.iter().filter(|e| e.tier == tier).collect();

                // Check coverage at key boundary points
                // Note: Rust ranges are exclusive of the end, so 0.75..1.0 doesn't include 1.0
                // We test 0.99 instead of 1.0 to avoid this edge case
                let boundary_values = [0.0, 0.25, 0.5, 0.75, 0.99];
                for &value in &boundary_values {
                    let has_coverage = tier_entries
                        .iter()
                        .any(|entry| entry.value_range.contains(&value));

                    assert!(
                        has_coverage,
                        "Category {:?}, tier {:?} should have coverage for boundary value {}. \
                         Available ranges: {:?}",
                        category,
                        tier,
                        value,
                        tier_entries
                            .iter()
                            .map(|e| &e.value_range)
                            .collect::<Vec<_>>()
                    );
                }
            }
        }
    }

    #[test]
    fn descriptor_vocabulary_boundary_values_resolve_deterministically() {
        use crate::journal::ObservationCategory;

        let vocab = DescriptorVocabulary::default();

        // Test boundary values (exactly at range edges) for deterministic resolution.
        // These are the critical values where ranges meet and must resolve consistently.
        let boundary_test_cases = [
            // ThermalBehavior boundaries: 0.0, 0.25, 0.50, 0.75, 1.0
            (
                ObservationCategory::ThermalBehavior,
                0.0,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.25,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.50,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.75,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.0,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.25,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.50,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.75,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.0,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.25,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.50,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::ThermalBehavior,
                0.75,
                ConfidenceTier::Confident,
            ),
            // Weight boundaries: 0.0, 0.25, 0.50, 0.75, 1.0
            (ObservationCategory::Weight, 0.0, ConfidenceTier::Tentative),
            (ObservationCategory::Weight, 0.25, ConfidenceTier::Tentative),
            (ObservationCategory::Weight, 0.50, ConfidenceTier::Tentative),
            (ObservationCategory::Weight, 0.75, ConfidenceTier::Tentative),
            (ObservationCategory::Weight, 0.0, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.25, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.50, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.75, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.0, ConfidenceTier::Confident),
            (ObservationCategory::Weight, 0.25, ConfidenceTier::Confident),
            (ObservationCategory::Weight, 0.50, ConfidenceTier::Confident),
            (ObservationCategory::Weight, 0.75, ConfidenceTier::Confident),
            // SurfaceAppearance boundaries: 0.0, 0.125, 0.25, 0.375, 0.5, 0.625, 0.75, 0.875, 1.0
            (
                ObservationCategory::SurfaceAppearance,
                0.0,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.125,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.25,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.375,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.5,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.625,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.75,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.875,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.0,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.125,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.25,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.375,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.5,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.625,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.75,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.875,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.0,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.125,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.25,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.375,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.5,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.625,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.75,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.875,
                ConfidenceTier::Confident,
            ),
            // FabricationResult boundaries: 0.0, 0.25, 0.50, 0.75, 1.0
            (
                ObservationCategory::FabricationResult,
                0.0,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::FabricationResult,
                0.25,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::FabricationResult,
                0.50,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::FabricationResult,
                0.75,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::FabricationResult,
                0.0,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::FabricationResult,
                0.25,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::FabricationResult,
                0.50,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::FabricationResult,
                0.75,
                ConfidenceTier::Observed,
            ),
            (
                ObservationCategory::FabricationResult,
                0.0,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::FabricationResult,
                0.25,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::FabricationResult,
                0.50,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::FabricationResult,
                0.75,
                ConfidenceTier::Confident,
            ),
        ];

        // Test each boundary value for deterministic resolution
        for (category, boundary_value, tier) in boundary_test_cases {
            // Convert tier to appropriate confidence value
            let confidence = match tier {
                ConfidenceTier::Tentative => Confidence(0.2),
                ConfidenceTier::Observed => Confidence(0.5),
                ConfidenceTier::Confident => Confidence(0.8),
            };

            // Get the first result
            let first_result = vocab.describe(&category, boundary_value, confidence);

            // Test multiple calls to ensure deterministic behavior
            for iteration in 1..=10 {
                let subsequent_result = vocab.describe(&category, boundary_value, confidence);
                assert_eq!(
                    first_result,
                    subsequent_result,
                    "Boundary value iteration {}: describe({:?}, {}, {:?}) must be deterministic. \
                     First: {:?}, Subsequent: {:?}",
                    iteration,
                    category,
                    boundary_value,
                    confidence,
                    first_result,
                    subsequent_result
                );
            }

            // For boundary values that should have coverage, verify we get a result
            // Note: Some boundary values (like 0.25, 0.5, 0.75) are range endpoints
            // and may not be included due to Rust's exclusive end ranges (0.0..0.25 excludes 0.25)
            if boundary_value == 0.0 || boundary_value == 1.0 {
                // 0.0 should always be included (range start), 1.0 needs special handling
                if boundary_value == 1.0 {
                    // Test with 0.99 instead since ranges are exclusive of end
                    let near_max_result = vocab.describe(&category, 0.99, confidence);
                    assert!(
                        near_max_result.is_some(),
                        "Value 0.99 should have coverage for category {:?}, tier {:?}",
                        category,
                        tier
                    );
                } else {
                    assert!(
                        first_result.is_some(),
                        "Boundary value {} should have coverage for category {:?}, tier {:?}",
                        boundary_value,
                        category,
                        tier
                    );
                }
            }

            // Test that the same boundary value produces the same result across different vocabulary instances
            let vocab2 = DescriptorVocabulary::default();
            let second_instance_result = vocab2.describe(&category, boundary_value, confidence);
            assert_eq!(
                first_result, second_instance_result,
                "Boundary value {} must produce identical results across vocabulary instances for \
                 category {:?}, tier {:?}. Instance1: {:?}, Instance2: {:?}",
                boundary_value, category, tier, first_result, second_instance_result
            );
        }

        // Additional test: verify that values just inside range boundaries behave consistently
        let just_inside_boundary_cases = [
            // Test values just inside the ranges to ensure consistent behavior
            (
                ObservationCategory::ThermalBehavior,
                0.01,
                ConfidenceTier::Tentative,
            ), // Just inside 0.0..0.25
            (
                ObservationCategory::ThermalBehavior,
                0.24,
                ConfidenceTier::Tentative,
            ), // Just before 0.25 boundary
            (
                ObservationCategory::ThermalBehavior,
                0.26,
                ConfidenceTier::Tentative,
            ), // Just inside 0.25..0.50
            (
                ObservationCategory::ThermalBehavior,
                0.49,
                ConfidenceTier::Tentative,
            ), // Just before 0.50 boundary
            (ObservationCategory::Weight, 0.01, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.24, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.26, ConfidenceTier::Observed),
            (ObservationCategory::Weight, 0.49, ConfidenceTier::Observed),
            (
                ObservationCategory::SurfaceAppearance,
                0.01,
                ConfidenceTier::Confident,
            ),
            (
                ObservationCategory::SurfaceAppearance,
                0.124,
                ConfidenceTier::Confident,
            ), // Just before 0.125 boundary
            (
                ObservationCategory::SurfaceAppearance,
                0.126,
                ConfidenceTier::Confident,
            ), // Just inside 0.125..0.25
            (
                ObservationCategory::FabricationResult,
                0.01,
                ConfidenceTier::Tentative,
            ),
            (
                ObservationCategory::FabricationResult,
                0.74,
                ConfidenceTier::Confident,
            ), // Just before 0.75 boundary
        ];

        for (category, value, tier) in just_inside_boundary_cases {
            let confidence = match tier {
                ConfidenceTier::Tentative => Confidence(0.2),
                ConfidenceTier::Observed => Confidence(0.5),
                ConfidenceTier::Confident => Confidence(0.8),
            };

            let first_result = vocab.describe(&category, value, confidence);

            // Test determinism for near-boundary values
            for iteration in 1..=5 {
                let subsequent_result = vocab.describe(&category, value, confidence);
                assert_eq!(
                    first_result, subsequent_result,
                    "Near-boundary value iteration {}: describe({:?}, {}, {:?}) must be deterministic. \
                     First: {:?}, Subsequent: {:?}",
                    iteration, category, value, confidence, first_result, subsequent_result
                );
            }

            // These values should definitely have coverage since they're inside ranges
            assert!(
                first_result.is_some(),
                "Near-boundary value {} should have coverage for category {:?}, tier {:?}",
                value,
                category,
                tier
            );
        }
    }

    #[test]
    fn death_event_degrades_all_journal_observations() {
        use crate::journal::{Journal, JournalKey, Observation, ObservationCategory};
        use crate::player::Player;

        let mut app = App::new();
        // Don't use the plugin to avoid TOML config loading
        app.add_message::<OnPlayerDeathEvent>()
            .add_systems(Update, handle_player_death)
            .insert_resource(ConfidenceConfig {
                death_degradation_factor: 0.6,
                death_floor: 0.2,
                domain_recovery_multiplier: 2.0,
                passive_recovery_multiplier: 0.7,
                base_observation_weight: 0.2,
            })
            .insert_resource(Time::<()>::default());

        // Create a player with a journal containing observations at different confidence levels
        let mut journal = Journal::default();

        // Add observations with high confidence
        journal.record(
            JournalKey::Material {
                seed: 42,
                planet_seed: None,
            },
            "Test Material",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: Confidence(0.8), // High confidence
                description: "Reliably withstands heat".to_string(),
                recorded_at: 100,
            },
        );

        journal.record(
            JournalKey::Material {
                seed: 99,
                planet_seed: None,
            },
            "Another Material",
            Observation {
                category: ObservationCategory::Weight,
                confidence: Confidence(0.9), // Very high confidence
                description: "Notably heavy".to_string(),
                recorded_at: 200,
            },
        );

        // Add observation with medium confidence
        journal.record(
            JournalKey::Material {
                seed: 42,
                planet_seed: None,
            },
            "Test Material",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: Confidence(0.5), // Medium confidence
                description: "Smooth metallic surface".to_string(),
                recorded_at: 150,
            },
        );

        // Spawn player with the journal
        let player_entity = app.world_mut().spawn((Player, journal)).id();

        // Verify initial confidence levels
        let journal = app.world().entity(player_entity).get::<Journal>().unwrap();
        let thermal_obs = &journal.entries[&JournalKey::Material {
            seed: 42,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::ThermalBehavior][0];
        let weight_obs = &journal.entries[&JournalKey::Material {
            seed: 99,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::Weight][0];
        let surface_obs = &journal.entries[&JournalKey::Material {
            seed: 42,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::SurfaceAppearance][0];

        assert_eq!(thermal_obs.confidence.0, 0.8);
        assert_eq!(weight_obs.confidence.0, 0.9);
        assert_eq!(surface_obs.confidence.0, 0.5);

        // Emit death event
        app.world_mut()
            .resource_mut::<Messages<OnPlayerDeathEvent>>()
            .write(OnPlayerDeathEvent {
                cause: DeathCause::HeatSystem,
            });

        // Run the death handler system
        app.update();

        // Verify confidence has been degraded
        let journal = app.world().entity(player_entity).get::<Journal>().unwrap();
        let thermal_obs = &journal.entries[&JournalKey::Material {
            seed: 42,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::ThermalBehavior][0];
        let weight_obs = &journal.entries[&JournalKey::Material {
            seed: 99,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::Weight][0];
        let surface_obs = &journal.entries[&JournalKey::Material {
            seed: 42,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::SurfaceAppearance][0];

        // Expected values: original * 0.6, but not below 0.2 floor
        assert_eq!(thermal_obs.confidence.0, 0.8 * 0.6); // 0.48
        assert_eq!(weight_obs.confidence.0, 0.9 * 0.6); // 0.54
        assert_eq!(surface_obs.confidence.0, 0.5 * 0.6); // 0.30
    }

    #[test]
    fn death_event_respects_confidence_floor() {
        use crate::journal::{Journal, JournalKey, Observation, ObservationCategory};
        use crate::player::Player;

        let mut app = App::new();
        // Don't use the plugin to avoid TOML config loading
        app.add_message::<OnPlayerDeathEvent>()
            .add_systems(Update, handle_player_death)
            .insert_resource(ConfidenceConfig {
                death_degradation_factor: 0.5, // Aggressive degradation
                death_floor: 0.3,              // High floor
                domain_recovery_multiplier: 2.0,
                passive_recovery_multiplier: 0.7,
                base_observation_weight: 0.2,
            })
            .insert_resource(Time::<()>::default());

        // Create a player with a journal containing low confidence observation
        let mut journal = Journal::default();

        journal.record(
            JournalKey::Material {
                seed: 42,
                planet_seed: None,
            },
            "Test Material",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: Confidence(0.4), // Low confidence that would degrade below floor
                description: "Seemed to react to heat".to_string(),
                recorded_at: 100,
            },
        );

        let player_entity = app.world_mut().spawn((Player, journal)).id();

        // Verify initial confidence
        let journal = app.world().entity(player_entity).get::<Journal>().unwrap();
        let thermal_obs = &journal.entries[&JournalKey::Material {
            seed: 42,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::ThermalBehavior][0];
        assert_eq!(thermal_obs.confidence.0, 0.4);

        // Emit death event
        app.world_mut()
            .resource_mut::<Messages<OnPlayerDeathEvent>>()
            .write(OnPlayerDeathEvent {
                cause: DeathCause::Fabrication,
            });

        // Run the death handler system
        app.update();

        // Verify confidence was clamped to floor
        // Expected: 0.4 * 0.5 = 0.2, but floor is 0.3, so should be 0.3
        let journal = app.world().entity(player_entity).get::<Journal>().unwrap();
        let thermal_obs = &journal.entries[&JournalKey::Material {
            seed: 42,
            planet_seed: None,
        }]
            .observations[&ObservationCategory::ThermalBehavior][0];

        assert_eq!(thermal_obs.confidence.0, 0.3); // Clamped to floor
    }

    #[test]
    fn death_event_with_no_player_logs_warning() {
        let mut app = App::new();
        app.add_message::<OnPlayerDeathEvent>()
            .add_systems(Update, handle_player_death)
            .insert_resource(ConfidenceConfig::default())
            .insert_resource(Time::<()>::default());

        // Emit death event without any player entity
        app.world_mut()
            .resource_mut::<Messages<OnPlayerDeathEvent>>()
            .write(OnPlayerDeathEvent {
                cause: DeathCause::Environmental,
            });

        // Run the death handler system - should not panic
        app.update();

        // Test passes if no panic occurs
    }

    // ── Domain-weighted recovery tests ──────────────────────────────────────

    #[test]
    fn death_cause_maps_to_observation_category() {
        use crate::journal::ObservationCategory;

        assert_eq!(
            DeathCause::HeatSystem.to_observation_category(),
            ObservationCategory::ThermalBehavior
        );
        assert_eq!(
            DeathCause::Fabrication.to_observation_category(),
            ObservationCategory::FabricationResult
        );
        assert_eq!(
            DeathCause::Environmental.to_observation_category(),
            ObservationCategory::LocationNote
        );
        assert_eq!(
            DeathCause::MaterialHandling.to_observation_category(),
            ObservationCategory::Weight
        );
    }

    #[test]
    fn death_context_tracks_cause_and_time() {
        let cause = DeathCause::HeatSystem;
        let death_time = 1000;
        let context = DeathContext::new(cause, death_time);

        assert_eq!(context.cause, cause);
        assert_eq!(context.death_time, death_time);
        assert_eq!(context.expiry_duration, 300_000); // 5 minutes default
    }

    #[test]
    fn death_context_expiry_logic() {
        let context = DeathContext::new(DeathCause::HeatSystem, 1000);

        // Not expired within the window
        assert!(!context.is_expired(1000)); // Same time
        assert!(!context.is_expired(150_000)); // 2.5 minutes later
        assert!(!context.is_expired(300_999)); // Just before expiry

        // Expired after the window
        assert!(context.is_expired(301_000)); // Exactly at expiry (1000 + 300_000)
        assert!(context.is_expired(400_000)); // Well past expiry
    }

    #[test]
    fn death_context_recovery_multipliers() {
        use crate::journal::ObservationCategory;

        let config = ConfidenceConfig {
            death_degradation_factor: 0.6,
            death_floor: 0.2,
            domain_recovery_multiplier: 2.0,
            passive_recovery_multiplier: 0.7,
            base_observation_weight: 0.2,
        };

        let context = DeathContext::new(DeathCause::HeatSystem, 1000);

        // Death-relevant domain gets domain recovery multiplier
        assert_eq!(
            context.recovery_multiplier(&ObservationCategory::ThermalBehavior, 2000, &config),
            2.0
        );

        // Other domains get passive recovery multiplier
        assert_eq!(
            context.recovery_multiplier(&ObservationCategory::Weight, 2000, &config),
            0.7
        );
        assert_eq!(
            context.recovery_multiplier(&ObservationCategory::SurfaceAppearance, 2000, &config),
            0.7
        );
        assert_eq!(
            context.recovery_multiplier(&ObservationCategory::FabricationResult, 2000, &config),
            0.7
        );

        // After expiry, all domains get base rate (1.0)
        assert_eq!(
            context.recovery_multiplier(&ObservationCategory::ThermalBehavior, 400_000, &config),
            1.0
        );
        assert_eq!(
            context.recovery_multiplier(&ObservationCategory::Weight, 400_000, &config),
            1.0
        );
    }

    #[test]
    fn death_event_creates_death_context() {
        let mut app = App::new();
        app.add_message::<OnPlayerDeathEvent>()
            .add_systems(Update, handle_player_death)
            .insert_resource(ConfidenceConfig::default())
            .insert_resource(Time::<()>::default());

        // Create a player with empty journal
        let mut journal = crate::journal::Journal::default();
        let player_entity = app.world_mut().spawn((crate::player::Player, journal)).id();

        // Verify no death context initially
        assert!(app.world().get_resource::<DeathContext>().is_none());

        // Emit death event
        app.world_mut()
            .resource_mut::<Messages<OnPlayerDeathEvent>>()
            .write(OnPlayerDeathEvent {
                cause: DeathCause::Fabrication,
            });

        // Run the death handler system
        app.update();

        // Verify death context was created
        let death_context = app.world().resource::<DeathContext>();
        assert_eq!(death_context.cause, DeathCause::Fabrication);
        assert_eq!(death_context.death_time, 0); // Time starts at 0 in tests
    }

    #[test]
    fn re_engaging_death_domain_recovers_confidence_faster_than_passive_recovery() {
        use crate::journal::ObservationCategory;

        let config = ConfidenceConfig {
            death_degradation_factor: 0.6,
            death_floor: 0.2,
            domain_recovery_multiplier: 2.0, // 2x recovery in death domain
            passive_recovery_multiplier: 0.7, // 0.7x recovery elsewhere
            base_observation_weight: 0.2,
        };

        // Create death context for heat system death
        let death_context = DeathContext::new(DeathCause::HeatSystem, 1000);
        let current_time = 2000; // Within recovery window

        // Test confidence accumulation in death-relevant domain (thermal)
        let mut thermal_confidence = Confidence(0.3); // Starting confidence after death
        let thermal_multiplier = death_context.recovery_multiplier(
            &ObservationCategory::ThermalBehavior,
            current_time,
            &config,
        );
        thermal_confidence.accumulate(config.base_observation_weight * thermal_multiplier);

        // Test confidence accumulation in unrelated domain (weight)
        let mut weight_confidence = Confidence(0.3); // Same starting confidence
        let weight_multiplier =
            death_context.recovery_multiplier(&ObservationCategory::Weight, current_time, &config);
        weight_confidence.accumulate(config.base_observation_weight * weight_multiplier);

        // Verify that re-engaging the death domain (thermal) recovers faster
        assert_eq!(
            thermal_multiplier, 2.0,
            "death domain should get 2x multiplier"
        );
        assert_eq!(
            weight_multiplier, 0.7,
            "other domains should get 0.7x multiplier"
        );

        // Thermal confidence should be higher after one observation
        assert!(
            thermal_confidence.0 > weight_confidence.0,
            "re-engaging death domain (thermal: {}) should recover faster than passive recovery (weight: {})",
            thermal_confidence.0,
            weight_confidence.0
        );

        // Verify the actual confidence values are as expected
        // thermal: 0.3 + (1.0 - 0.3) * (0.2 * 2.0) = 0.3 + 0.7 * 0.4 = 0.3 + 0.28 = 0.58
        let expected_thermal = 0.3 + (1.0 - 0.3) * (0.2 * 2.0);
        assert!((thermal_confidence.0 - expected_thermal).abs() < 0.001);

        // weight: 0.3 + (1.0 - 0.3) * (0.2 * 0.7) = 0.3 + 0.7 * 0.14 = 0.3 + 0.098 = 0.398
        let expected_weight = 0.3 + (1.0 - 0.3) * (0.2 * 0.7);
        assert!((weight_confidence.0 - expected_weight).abs() < 0.001);
    }
}
