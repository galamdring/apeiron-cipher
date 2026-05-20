//! Material classification — query-time grouping of observed instances.
//!
//! Loads `assets/materials/classifications.toml` at startup into the
//! [`MaterialClassifications`] resource. The Journal and any other query
//! layer uses [`MaterialClassifications::classify`] to map a material's
//! observed property values to a named type (e.g. "Ferrite", "Volatite").
//!
//! **Classification is never stored.** No entity, component, or graph node
//! ever carries a `classification` field — the name is always the result of
//! a range-match at query time (Core Principle 6, Data Architecture ADR).
//!
//! A material that matches no range is "Unknown" — the Journal shows it as
//! "Unknown [procedural-name]" derived from its seed so the player can still
//! refer to it consistently before enough observations exist to name it.

use std::{fs, path::Path};

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

// ── Asset schema ─────────────────────────────────────────────────────────

/// A min/max range for a single material property.
///
/// Both bounds are inclusive. A value equal to `min` or `max` is inside
/// the range.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PropertyRange {
    /// Minimum value (inclusive). Must be in \[0.0, 1.0\].
    pub min: f32,
    /// Maximum value (inclusive). Must be in \[0.0, 1.0\].
    pub max: f32,
}

impl PropertyRange {
    /// Returns `true` if `value` is within `[min, max]` (inclusive).
    pub fn contains(&self, value: f32) -> bool {
        value >= self.min && value <= self.max
    }
}

/// One classification entry loaded from `classifications.toml`.
///
/// Each entry describes the property ranges that an observed material must
/// fall within to be considered a member of this type. Omitted properties
/// are unconstrained — only the properties present in the TOML entry are
/// checked.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ClassificationEntry {
    /// Internal name used as the `classification` field in
    /// [`crate::journal::JournalKey::Material`] (e.g. "ferrite").
    pub name: String,
    /// Human-readable display name shown in the Journal (e.g. "Ferrite").
    pub display_name: String,
    /// Density range constraint, if any.
    pub density: Option<PropertyRange>,
    /// Thermal resistance range constraint, if any.
    pub thermal_resistance: Option<PropertyRange>,
    /// Reactivity range constraint, if any.
    pub reactivity: Option<PropertyRange>,
    /// Conductivity range constraint, if any.
    pub conductivity: Option<PropertyRange>,
    /// Toxicity range constraint, if any.
    pub toxicity: Option<PropertyRange>,
}

impl ClassificationEntry {
    /// Returns `true` if all property constraints whose corresponding
    /// property has been *observed* (present in `revealed`) match the
    /// recorded value, AND at least one constrained property is present.
    ///
    /// A material that has no revealed properties that overlap with this
    /// entry's constraints returns `false` — the player hasn't observed
    /// enough to classify it yet.
    ///
    /// Unconstrained properties (the `Option` is `None` on the entry) are
    /// always satisfied regardless of observation state.
    pub fn matches_observed(
        &self,
        revealed: &std::collections::HashMap<crate::journal::ObservationCategory, f32>,
    ) -> bool {
        use crate::journal::ObservationCategory;

        // Helper: if the entry has a constraint for this property AND the
        // player has observed it, the observed value must be in range.
        // If the entry has a constraint but the property is NOT yet observed,
        // we cannot classify — return false immediately.
        let check = |range: &Option<PropertyRange>, cat: ObservationCategory| -> Option<bool> {
            let r = range.as_ref()?; // None constraint → always ok (skip)
            let &value = revealed.get(&cat)?; // not yet observed → can't classify
            Some(r.contains(value))
        };

        // All constrained-and-observed properties must be in range.
        // At least one constrained property must have been observed.
        let mut any_constrained_observed = false;

        for (range, cat) in [
            (&self.density, ObservationCategory::Weight),
            (
                &self.thermal_resistance,
                ObservationCategory::ThermalBehavior,
            ),
            // Reactivity / conductivity / toxicity observation systems not wired yet;
            // their ranges are loaded but won't match until those systems exist.
        ] {
            if range.is_none() {
                continue; // unconstrained — skip
            }
            match check(range, cat) {
                None => return false,        // constrained but not observed
                Some(false) => return false, // out of range
                Some(true) => any_constrained_observed = true,
            }
        }

        any_constrained_observed
    }
}

/// Top-level shape of `classifications.toml` — an array of entries under
/// the `[[classification]]` header.
#[derive(Debug, Deserialize)]
struct ClassificationsFile {
    classification: Vec<ClassificationEntry>,
}

// ── Resource ─────────────────────────────────────────────────────────────

/// All loaded material classification ranges, available to the Journal and
/// any other query layer at runtime.
///
/// Populated once at `Startup` from `assets/materials/classifications.toml`.
/// Never written to after startup — classifications are authoritative data,
/// not runtime state.
#[derive(Resource, Debug, Default)]
pub struct MaterialClassifications {
    entries: Vec<ClassificationEntry>,
}

impl MaterialClassifications {
    /// Classify a material by its *observed* properties.
    ///
    /// Returns the first [`ClassificationEntry`] whose constrained properties
    /// all match the values the player has revealed, or `None` if no
    /// classification is confident enough yet.
    pub fn classify_observed<'a>(
        &'a self,
        revealed: &std::collections::HashMap<crate::journal::ObservationCategory, f32>,
    ) -> Option<&'a ClassificationEntry> {
        self.entries.iter().find(|e| e.matches_observed(revealed))
    }

    /// All loaded classification entries in file order.
    pub fn entries(&self) -> &[ClassificationEntry] {
        &self.entries
    }
}

// ── Plugin ────────────────────────────────────────────────────────────────

/// Registers [`MaterialClassifications`] and loads it from disk at startup.
pub struct MaterialClassificationsPlugin;

impl Plugin for MaterialClassificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<MaterialClassifications>()
            .add_systems(Startup, load_material_classifications);
    }
}

const CLASSIFICATIONS_PATH: &str = "assets/materials/classifications.toml";

/// Loads `classifications.toml` into the [`MaterialClassifications`] resource.
///
/// Runs at `Startup`. If the file is missing or malformed the resource stays
/// empty — materials will all appear as "Unknown" in the Journal rather than
/// crashing.
fn load_material_classifications(mut classifications: ResMut<MaterialClassifications>) {
    let path = Path::new(CLASSIFICATIONS_PATH);
    if !path.exists() {
        warn!(
            "classifications.toml not found at {CLASSIFICATIONS_PATH} — all materials will be unclassified"
        );
        return;
    }
    let contents = match fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            error!("Failed to read {CLASSIFICATIONS_PATH}: {e}");
            return;
        }
    };
    let file: ClassificationsFile = match toml::from_str(&contents) {
        Ok(f) => f,
        Err(e) => {
            error!("Failed to parse {CLASSIFICATIONS_PATH}: {e}");
            return;
        }
    };
    classifications.entries = file.classification;
    info!(
        "Loaded {} material classifications",
        classifications.entries.len()
    );
}

// ── Tests ─────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use std::collections::HashMap;

    use super::*;
    use crate::journal::ObservationCategory;

    /// Build a fully-revealed HashMap for ferrite-like properties.
    fn ferrite_revealed() -> HashMap<ObservationCategory, f32> {
        let mut m = HashMap::new();
        m.insert(ObservationCategory::Weight, 0.78_f32); // density
        m.insert(ObservationCategory::ThermalBehavior, 0.65_f32); // thermal_resistance
        m
    }

    fn ferrite_entry() -> ClassificationEntry {
        ClassificationEntry {
            name: "ferrite".into(),
            display_name: "Ferrite".into(),
            // Seed 1001: density=0.5086, thermal=0.4468
            density: Some(PropertyRange {
                min: 0.49,
                max: 0.56,
            }),
            thermal_resistance: Some(PropertyRange {
                min: 0.38,
                max: 0.51,
            }),
            reactivity: None,
            conductivity: None,
            toxicity: None,
        }
    }

    /// Build a HashMap containing only Weight (density) for a single-property test.
    fn weight_only(value: f32) -> HashMap<ObservationCategory, f32> {
        let mut m = HashMap::new();
        m.insert(ObservationCategory::Weight, value);
        m
    }

    /// Build a HashMap with both Weight and Thermal revealed.
    fn weight_and_thermal(weight: f32, thermal: f32) -> HashMap<ObservationCategory, f32> {
        let mut m = HashMap::new();
        m.insert(ObservationCategory::Weight, weight);
        m.insert(ObservationCategory::ThermalBehavior, thermal);
        m
    }

    #[test]
    fn classify_matches_when_all_constrained_props_observed_and_in_range() {
        // Ferrite seed-derived reference: d=0.5086, tr=0.4468 — inside ranges (0.49-0.56, 0.38-0.51)
        let entry = ferrite_entry();
        let revealed = weight_and_thermal(0.51, 0.45);
        assert!(entry.matches_observed(&revealed));
    }

    #[test]
    fn classify_boundary_values_inclusive() {
        let entry = ferrite_entry();
        // Min bounds
        assert!(entry.matches_observed(&weight_and_thermal(0.49, 0.38)));
        // Max bounds
        assert!(entry.matches_observed(&weight_and_thermal(0.56, 0.51)));
    }

    #[test]
    fn classify_outside_range_fails() {
        let entry = ferrite_entry();
        // density too low
        assert!(!entry.matches_observed(&weight_and_thermal(0.10, 0.44)));
        // thermal too high
        assert!(!entry.matches_observed(&weight_and_thermal(0.51, 0.90)));
    }

    #[test]
    fn classify_returns_false_when_constrained_prop_not_observed() {
        // density constraint present but not in revealed map
        let entry = ferrite_entry();
        let revealed = HashMap::new(); // nothing observed
        assert!(!entry.matches_observed(&revealed));
    }

    #[test]
    fn classify_with_only_weight_observed_matches_when_in_range() {
        let entry = ferrite_entry();
        // thermal_resistance is constrained — not observed → cannot classify
        assert!(!entry.matches_observed(&weight_only(0.51))); // 0.51 in d-range but thermal not observed
    }

    #[test]
    fn no_entries_returns_none() {
        let classifications = MaterialClassifications::default();
        assert!(
            classifications
                .classify_observed(&weight_and_thermal(0.51, 0.45))
                .is_none()
        );
    }

    #[test]
    fn first_matching_entry_wins() {
        let mut classifications = MaterialClassifications::default();
        classifications.entries.push(ferrite_entry());
        // A second entry with overlapping density range but no thermal constraint.
        classifications.entries.push(ClassificationEntry {
            name: "also_ferrite".into(),
            display_name: "Also Ferrite".into(),
            density: Some(PropertyRange {
                min: 0.49,
                max: 0.56,
            }),
            thermal_resistance: None,
            reactivity: None,
            conductivity: None,
            toxicity: None,
        });
        let result = classifications.classify_observed(&weight_and_thermal(0.51, 0.44));
        assert_eq!(result.map(|e| e.name.as_str()), Some("ferrite"));
    }

    #[test]
    fn classifications_toml_loads_all_entries() {
        let contents = std::fs::read_to_string(CLASSIFICATIONS_PATH)
            .expect("classifications.toml should exist");
        let file: ClassificationsFile =
            toml::from_str(&contents).expect("classifications.toml should parse");
        assert_eq!(
            file.classification.len(),
            10,
            "expected 10 classification entries"
        );
    }

    #[test]
    fn well_known_seeds_classify_correctly_when_fully_revealed() {
        // Each well-known seed should classify to the expected type when the
        // player has observed both Weight and ThermalBehavior.
        use crate::materials::{WELL_KNOWN_MATERIAL_SEEDS, derive_material_from_seed};

        let contents = std::fs::read_to_string(CLASSIFICATIONS_PATH)
            .expect("classifications.toml should exist");
        let file: ClassificationsFile =
            toml::from_str(&contents).expect("classifications.toml should parse");
        let classifications = MaterialClassifications {
            entries: file.classification,
        };

        let expected: &[(&str, &str)] = &[
            ("Ferrite", "ferrite"),
            ("Calcium", "calcium"),
            ("Sulfurite", "sulfurite"),
            ("Prismate", "prismate"),
            ("Verdant", "verdant"),
            ("Osmium", "osmium"),
            ("Volatite", "volatite"),
            ("Cobaltine", "cobaltine"),
            ("Silite", "silite"),
            ("Phosphite", "phosphite"),
        ];

        for (seed, (mat_name, expected_class)) in
            WELL_KNOWN_MATERIAL_SEEDS.iter().zip(expected.iter())
        {
            let (_label, seed_val) = seed;
            let mat = derive_material_from_seed(*seed_val);
            let props = mat.property_vector();
            // Simulate player having observed both Weight and ThermalBehavior.
            let revealed = weight_and_thermal(props[0], props[1]);
            let result = classifications.classify_observed(&revealed);
            assert_eq!(
                result.map(|e| e.name.as_str()),
                Some(*expected_class),
                "{mat_name} (seed {seed_val}): density={:.4} thermal={:.4} — expected {} got {:?}",
                props[0],
                props[1],
                expected_class,
                result.map(|e| e.name.as_str())
            );
        }
    }
}
