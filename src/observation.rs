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
//!
//! The [`ConfidenceTracker`] resource stores observation counts per
//! `(material_seed, property)` pair. The property key is a string so it
//! can accommodate new test types without a code change.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::carry::WeightDescriptionBand;

/// Plugin that initialises the observation confidence tracking system.
pub struct ObservationPlugin;

impl Plugin for ObservationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConfidenceTracker>();
    }
}

// ── Confidence levels ────────────────────────────────────────────────────

/// Qualitative confidence level derived from observation count.
/// Used by the examine panel and journal to select descriptor language.
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
#[derive(Resource, Debug, Default)]
pub struct ConfidenceTracker {
    counts: HashMap<ObsKey, u32>,
}

impl ConfidenceTracker {
    /// Record one observation. Returns the new count.
    #[allow(dead_code)]
    pub fn record(&mut self, seed: u64, property: PropertyName) -> u32 {
        let key = (seed, property);
        let count = self.counts.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    /// Current observation count (0 if never observed).
    #[allow(dead_code)]
    pub fn count(&self, seed: u64, property: PropertyName) -> u32 {
        self.counts.get(&(seed, property)).copied().unwrap_or(0)
    }

    /// Confidence level for a specific (material, property) pair.
    #[allow(dead_code)]
    pub fn level(&self, seed: u64, property: PropertyName) -> ConfidenceLevel {
        ConfidenceLevel::from_count(self.count(seed, property))
    }
}

// ── Weight observation descriptors ──────────────────────────────────────────

/// Describes a weight observation based on density-to-strength ratio and confidence level.
///
/// This function was moved from carry.rs to align with the thermal observation pattern
/// where descriptors are centralized in the observation module rather than computed
/// inline in the system that records them.
///
/// The weight description depends on the ratio of material density to the player's
/// current carry strength, mapped through configurable description bands. This allows
/// the same material to feel different as the player's strength grows through practice.
///
/// **Design Note:** This function intentionally produces different text than surface
/// density observations (which use `describe_density` from descriptions.rs). Surface
/// observations describe the material's absolute density ("Heavy", "Very light"), while
/// carry-weight observations describe the subjective experience of lifting it relative
/// to the player's current strength ("Straining under the weight", "Almost weightless").
/// This distinction reflects that surface examination reveals objective properties while
/// carrying reveals the player's personal relationship with the material's weight.
pub fn describe_weight_observation(
    density: f32,
    carry_strength: f32,
    confidence: ConfidenceLevel,
    bands: &[WeightDescriptionBand],
) -> String {
    let ratio = if carry_strength <= f32::EPSILON {
        f32::INFINITY
    } else {
        density / carry_strength
    };
    let base = bands
        .iter()
        .find(|band| ratio <= band.max_ratio)
        .map(|band| band.text.as_str())
        .unwrap_or("Barely able to lift");

    match confidence {
        ConfidenceLevel::Tentative => format!("Seemed {}", base.to_lowercase()),
        ConfidenceLevel::Observed | ConfidenceLevel::Confident => base.to_string(),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
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
}
