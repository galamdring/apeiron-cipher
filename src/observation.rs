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

pub struct ObservationPlugin;

impl Plugin for ObservationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<ConfidenceTracker>();
    }
}

// ── Confidence levels ────────────────────────────────────────────────────

/// Qualitative confidence level derived from observation count.
/// Used by the examine panel in the next PR to select descriptor language.
#[allow(dead_code)]
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ConfidenceLevel {
    /// One observation — tentative language.
    Tentative,
    /// 2–3 observations — factual but unqualified.
    Observed,
    /// 4+ observations — confident with comparative language.
    Confident,
}

impl ConfidenceLevel {
    #[allow(dead_code)]
    pub fn from_count(count: u32) -> Self {
        match count {
            0 => ConfidenceLevel::Tentative,
            1 => ConfidenceLevel::Tentative,
            2..=3 => ConfidenceLevel::Observed,
            _ => ConfidenceLevel::Confident,
        }
    }
}

fn describe_thermal_behavior(value: f32) -> &'static str {
    if value < 0.25 {
        "soften quickly under heat"
    } else if value < 0.5 {
        "change noticeably under heat"
    } else if value < 0.75 {
        "hold together under heat"
    } else {
        "barely react to heat"
    }
}

pub fn describe_thermal_observation(value: f32, confidence: ConfidenceLevel) -> String {
    let behavior = describe_thermal_behavior(value);
    match confidence {
        ConfidenceLevel::Tentative => format!("Seemed to {behavior}"),
        ConfidenceLevel::Observed => {
            let mut chars = behavior.chars();
            let Some(first) = chars.next() else {
                return String::new();
            };
            format!("{}{}", first.to_uppercase(), chars.as_str())
        }
        ConfidenceLevel::Confident => format!("Reliably {behavior}"),
    }
}

// ── Tracker resource ─────────────────────────────────────────────────────

/// Canonical key: (material seed, property name).
type ObsKey = (u64, String);

/// Stores how many times the player has observed each (material, property)
/// combination through environmental testing.
/// Fields read by the examine panel and heat systems in the next PRs.
#[allow(dead_code)]
#[derive(Resource, Debug, Default)]
pub struct ConfidenceTracker {
    counts: HashMap<ObsKey, u32>,
}

impl ConfidenceTracker {
    /// Record one observation. Returns the new count.
    /// Called by the heat revelation system in the next PR.
    #[allow(dead_code)]
    pub fn record(&mut self, seed: u64, property: &str) -> u32 {
        let key = (seed, property.to_string());
        let count = self.counts.entry(key).or_insert(0);
        *count += 1;
        *count
    }

    /// Current observation count (0 if never observed).
    /// Used by the examine panel in the next PR.
    #[allow(dead_code)]
    pub fn count(&self, seed: u64, property: &str) -> u32 {
        self.counts
            .get(&(seed, property.to_string()))
            .copied()
            .unwrap_or(0)
    }

    /// Confidence level for a specific (material, property) pair.
    /// Used by the examine panel in the next PR.
    #[allow(dead_code)]
    pub fn level(&self, seed: u64, property: &str) -> ConfidenceLevel {
        ConfidenceLevel::from_count(self.count(seed, property))
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fresh_tracker_has_zero_count() {
        let tracker = ConfidenceTracker::default();
        assert_eq!(tracker.count(42, "thermal_resistance"), 0);
    }

    #[test]
    fn record_increments_count() {
        let mut tracker = ConfidenceTracker::default();
        assert_eq!(tracker.record(42, "thermal_resistance"), 1);
        assert_eq!(tracker.record(42, "thermal_resistance"), 2);
        assert_eq!(tracker.count(42, "thermal_resistance"), 2);
    }

    #[test]
    fn different_seeds_tracked_independently() {
        let mut tracker = ConfidenceTracker::default();
        tracker.record(42, "thermal_resistance");
        tracker.record(99, "thermal_resistance");
        assert_eq!(tracker.count(42, "thermal_resistance"), 1);
        assert_eq!(tracker.count(99, "thermal_resistance"), 1);
    }

    #[test]
    fn different_properties_tracked_independently() {
        let mut tracker = ConfidenceTracker::default();
        tracker.record(42, "thermal_resistance");
        tracker.record(42, "density");
        assert_eq!(tracker.count(42, "thermal_resistance"), 1);
        assert_eq!(tracker.count(42, "density"), 1);
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
            tracker.level(42, "thermal_resistance"),
            ConfidenceLevel::Tentative
        );
        tracker.record(42, "thermal_resistance");
        assert_eq!(
            tracker.level(42, "thermal_resistance"),
            ConfidenceLevel::Tentative
        );
        tracker.record(42, "thermal_resistance");
        assert_eq!(
            tracker.level(42, "thermal_resistance"),
            ConfidenceLevel::Observed
        );
        tracker.record(42, "thermal_resistance");
        assert_eq!(
            tracker.level(42, "thermal_resistance"),
            ConfidenceLevel::Observed
        );
        tracker.record(42, "thermal_resistance");
        assert_eq!(
            tracker.level(42, "thermal_resistance"),
            ConfidenceLevel::Confident
        );
    }
}
