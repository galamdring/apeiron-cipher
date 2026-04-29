//! Combination rules — data-driven system for material combination outcomes.
//!
//! Rules are loaded from `assets/config/combinations.toml` at startup. Each
//! rule entry maps a pair of material names to per-property combination rules.
//! Pairs without an explicit entry use the default rule (equal-weight blend).
//!
//! Rule types:
//! - `Blend { weight_a, weight_b }`: weighted average (predictable)
//! - `Max`: takes the higher of the two input values (predictable)
//! - `Min`: takes the lower of the two input values (predictable)
//! - `Catalyze { multiplier }`: max of inputs × multiplier, clamped to 1.0 (emergent)
//! - `Inert`: all outputs become 0.1 — a failed experiment (emergent)

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

/// Loads combination rules from TOML config for material pair interactions.
pub struct CombinationPlugin;

impl Plugin for CombinationPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_combination_rules);
    }
}

// ── Rule types ──────────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
#[serde(tag = "type")]
/// How a single material property is combined when two materials are mixed.
pub enum PropertyRule {
    /// Weighted average of the two input values.
    Blend {
        /// Weight applied to the first input.
        weight_a: f32,
        /// Weight applied to the second input.
        weight_b: f32,
    },
    /// Takes the higher of the two input values.
    Max,
    /// Takes the lower of the two input values.
    Min,
    /// Multiplies the higher input value by a factor, clamped to 1.0.
    Catalyze {
        /// Scale factor applied to the dominant input.
        multiplier: f32,
    },
    /// Failed experiment — always produces 0.1.
    Inert,
}

impl PropertyRule {
    /// Computes the output value from two input property values using this rule.
    pub fn apply(&self, a: f32, b: f32) -> f32 {
        match self {
            PropertyRule::Blend { weight_a, weight_b } => {
                let total = weight_a + weight_b;
                if total < f32::EPSILON {
                    return (a + b) * 0.5;
                }
                ((a * weight_a + b * weight_b) / total).clamp(0.0, 1.0)
            }
            PropertyRule::Max => a.max(b),
            PropertyRule::Min => a.min(b),
            PropertyRule::Catalyze { multiplier } => (a.max(b) * multiplier).clamp(0.0, 1.0),
            PropertyRule::Inert => 0.1,
        }
    }
}

impl Default for PropertyRule {
    fn default() -> Self {
        PropertyRule::Blend {
            weight_a: 0.5,
            weight_b: 0.5,
        }
    }
}

// ── Rule set for a material pair ────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
/// Per-property combination rules for a specific material pair.
pub struct PairRuleSet {
    /// Rule applied to density when combining this pair.
    pub density: PropertyRule,
    /// Rule applied to thermal resistance when combining this pair.
    pub thermal_resistance: PropertyRule,
    /// Rule applied to reactivity when combining this pair.
    pub reactivity: PropertyRule,
    /// Rule applied to conductivity when combining this pair.
    pub conductivity: PropertyRule,
    /// Rule applied to toxicity when combining this pair.
    pub toxicity: PropertyRule,
}

impl PairRuleSet {
    /// Used in tests and by future waste-detection visuals (e.g. grey-out output).
    #[allow(dead_code)]
    pub fn all_inert() -> Self {
        Self {
            density: PropertyRule::Inert,
            thermal_resistance: PropertyRule::Inert,
            reactivity: PropertyRule::Inert,
            conductivity: PropertyRule::Inert,
            toxicity: PropertyRule::Inert,
        }
    }

    /// Used in tests and by future waste-detection visuals.
    #[allow(dead_code)]
    pub fn is_inert(&self) -> bool {
        self.density == PropertyRule::Inert
            && self.thermal_resistance == PropertyRule::Inert
            && self.reactivity == PropertyRule::Inert
            && self.conductivity == PropertyRule::Inert
            && self.toxicity == PropertyRule::Inert
    }
}

// ── TOML file format ────────────────────────────────────────────────────

#[derive(Debug, Deserialize)]
struct CombinationFile {
    #[serde(default)]
    default_rule: PropertyRule,
    #[serde(default)]
    rules: Vec<PairRuleEntry>,
}

#[derive(Debug, Deserialize)]
struct PairRuleEntry {
    material_a: String,
    material_b: String,
    #[serde(default)]
    density: Option<PropertyRule>,
    #[serde(default)]
    thermal_resistance: Option<PropertyRule>,
    #[serde(default)]
    reactivity: Option<PropertyRule>,
    #[serde(default)]
    conductivity: Option<PropertyRule>,
    #[serde(default)]
    toxicity: Option<PropertyRule>,
}

// ── Resource ────────────────────────────────────────────────────────────

/// Canonical key for a material pair — alphabetically sorted so (A,B) == (B,A).
fn pair_key(a: &str, b: &str) -> (String, String) {
    if a <= b {
        (a.to_string(), b.to_string())
    } else {
        (b.to_string(), a.to_string())
    }
}

/// Loaded combination rules, keyed by sorted material name pairs.
#[derive(Resource, Debug, Default)]
pub struct CombinationRules {
    /// Fallback rule used when no pair-specific entry exists.
    pub default_rule: PropertyRule,
    /// Per-pair overrides keyed by alphabetically sorted material name tuples.
    pub pair_rules: HashMap<(String, String), PairRuleSet>,
}

impl CombinationRules {
    /// Look up the rule set for a pair, falling back to the default for each property.
    pub fn rules_for(&self, name_a: &str, name_b: &str) -> PairRuleSet {
        let key = pair_key(name_a, name_b);
        if let Some(rules) = self.pair_rules.get(&key) {
            rules.clone()
        } else {
            let d = &self.default_rule;
            PairRuleSet {
                density: d.clone(),
                thermal_resistance: d.clone(),
                reactivity: d.clone(),
                conductivity: d.clone(),
                toxicity: d.clone(),
            }
        }
    }
}

// ── Loading ─────────────────────────────────────────────────────────────

const CONFIG_PATH: &str = "assets/config/combinations.toml";

fn load_combination_rules(mut commands: Commands) {
    let rules = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<CombinationFile>(&contents) {
                Ok(file) => {
                    let default = file.default_rule;
                    let mut pair_rules = HashMap::new();

                    for entry in file.rules {
                        let key = pair_key(&entry.material_a, &entry.material_b);
                        let rule_set = PairRuleSet {
                            density: entry.density.unwrap_or_else(|| default.clone()),
                            thermal_resistance: entry
                                .thermal_resistance
                                .unwrap_or_else(|| default.clone()),
                            reactivity: entry.reactivity.unwrap_or_else(|| default.clone()),
                            conductivity: entry.conductivity.unwrap_or_else(|| default.clone()),
                            toxicity: entry.toxicity.unwrap_or_else(|| default.clone()),
                        };
                        pair_rules.insert(key, rule_set);
                    }

                    info!(
                        "Loaded combination rules from {CONFIG_PATH}: {} pair rules",
                        pair_rules.len()
                    );
                    CombinationRules {
                        default_rule: default,
                        pair_rules,
                    }
                }
                Err(e) => {
                    warn!("Malformed {CONFIG_PATH}, using defaults: {e}");
                    CombinationRules::default()
                }
            },
            Err(e) => {
                warn!("Could not read {CONFIG_PATH}, using defaults: {e}");
                CombinationRules::default()
            }
        }
    } else {
        warn!("{CONFIG_PATH} not found, using default blend rules");
        CombinationRules::default()
    };

    commands.insert_resource(rules);
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn blend_equal_weights() {
        let rule = PropertyRule::Blend {
            weight_a: 0.5,
            weight_b: 0.5,
        };
        assert!((rule.apply(0.4, 0.6) - 0.5).abs() < f32::EPSILON);
    }

    #[test]
    fn blend_weighted() {
        let rule = PropertyRule::Blend {
            weight_a: 0.7,
            weight_b: 0.3,
        };
        let result = rule.apply(1.0, 0.0);
        assert!((result - 0.7).abs() < f32::EPSILON);
    }

    #[test]
    fn max_rule() {
        assert!((PropertyRule::Max.apply(0.3, 0.8) - 0.8).abs() < f32::EPSILON);
    }

    #[test]
    fn min_rule() {
        assert!((PropertyRule::Min.apply(0.3, 0.8) - 0.3).abs() < f32::EPSILON);
    }

    #[test]
    fn catalyze_multiplies_max() {
        let rule = PropertyRule::Catalyze { multiplier: 1.5 };
        let result = rule.apply(0.4, 0.6);
        assert!((result - 0.9).abs() < f32::EPSILON);
    }

    #[test]
    fn catalyze_clamps_to_one() {
        let rule = PropertyRule::Catalyze { multiplier: 2.0 };
        let result = rule.apply(0.8, 0.9);
        assert!((result - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn inert_always_returns_point_one() {
        assert!((PropertyRule::Inert.apply(0.9, 0.8) - 0.1).abs() < f32::EPSILON);
    }

    #[test]
    fn pair_key_is_order_independent() {
        assert_eq!(pair_key("Ferrite", "Silite"), pair_key("Silite", "Ferrite"));
    }

    #[test]
    fn rules_for_returns_default_for_unknown_pair() {
        let rules = CombinationRules::default();
        let pair = rules.rules_for("Unknown", "Other");
        assert_eq!(pair.density, PropertyRule::default());
    }

    #[test]
    fn pair_rule_set_all_inert() {
        let prs = PairRuleSet::all_inert();
        assert!(prs.is_inert());
    }

    #[test]
    fn toml_round_trip_rule_types() {
        let toml_str = r#"
[default_rule]
type = "Blend"
weight_a = 0.5
weight_b = 0.5

[[rules]]
material_a = "A"
material_b = "B"
density = { type = "Max" }
thermal_resistance = { type = "Catalyze", multiplier = 1.3 }
reactivity = { type = "Min" }
conductivity = { type = "Inert" }
toxicity = { type = "Blend", weight_a = 0.6, weight_b = 0.4 }
"#;
        let file: CombinationFile = toml::from_str(toml_str).expect("parse");
        assert_eq!(file.rules.len(), 1);
        assert_eq!(file.rules[0].density, Some(PropertyRule::Max));
        assert_eq!(
            file.rules[0].thermal_resistance,
            Some(PropertyRule::Catalyze { multiplier: 1.3 })
        );
    }

    #[test]
    fn combinations_toml_parses() {
        let contents = include_str!("../assets/config/combinations.toml");
        let file: CombinationFile = toml::from_str(contents).expect("parse combinations.toml");
        assert!(
            file.rules.len() >= 5,
            "expected at least 5 pair rules in combinations.toml"
        );
    }
}
