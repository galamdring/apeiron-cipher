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

    /// Return *all* classification types that match the observed properties.
    ///
    /// Unlike [`classify_observed`] (which returns only the first match),
    /// this method collects every [`ClassificationEntry`] whose constrained
    /// properties all satisfy the revealed values. Multiple entries can match
    /// a single node when their ranges overlap — this is intentional and
    /// supports cross-cutting classification schemes (e.g. a material that is
    /// simultaneously "Ferrite" and "Dense").\
    ///
    /// Returns an empty `Vec` if no entry matches.
    pub fn classify_all_observed<'a>(
        &'a self,
        revealed: &std::collections::HashMap<crate::journal::ObservationCategory, f32>,
    ) -> Vec<&'a ClassificationEntry> {
        self.entries
            .iter()
            .filter(|e| e.matches_observed(revealed))
            .collect()
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

    // ── classify_all_observed ────────────────────────────────────────────

    #[test]
    fn classify_all_returns_empty_when_no_entries() {
        let classifications = MaterialClassifications::default();
        let result = classifications.classify_all_observed(&weight_and_thermal(0.51, 0.45));
        assert!(result.is_empty());
    }

    #[test]
    fn classify_all_returns_single_match() {
        let mut classifications = MaterialClassifications::default();
        classifications.entries.push(ferrite_entry());
        let result = classifications.classify_all_observed(&weight_and_thermal(0.51, 0.45));
        assert_eq!(result.len(), 1);
        assert_eq!(result[0].name, "ferrite");
    }

    #[test]
    fn classify_all_returns_multiple_when_ranges_overlap() {
        // A single node (weight=0.51, thermal=0.45) matches two classification
        // entries whose ranges overlap — verifying the multi-match AC.
        let mut classifications = MaterialClassifications::default();
        // Entry 1: ferrite — density [0.49, 0.56], thermal [0.38, 0.51]
        classifications.entries.push(ferrite_entry());
        // Entry 2: dense_iron — overlapping density range, no thermal constraint
        // (weight=0.51 is in [0.40, 0.60] and that is the only constraint)
        classifications.entries.push(ClassificationEntry {
            name: "dense_iron".into(),
            display_name: "Dense Iron".into(),
            density: Some(PropertyRange {
                min: 0.40,
                max: 0.60,
            }),
            thermal_resistance: None,
            reactivity: None,
            conductivity: None,
            toxicity: None,
        });
        let result = classifications.classify_all_observed(&weight_and_thermal(0.51, 0.45));
        let names: Vec<&str> = result.iter().map(|e| e.name.as_str()).collect();
        assert_eq!(names.len(), 2, "expected 2 matches, got {:?}", names);
        assert!(
            names.contains(&"ferrite"),
            "expected ferrite in {:?}",
            names
        );
        assert!(
            names.contains(&"dense_iron"),
            "expected dense_iron in {:?}",
            names
        );
    }

    #[test]
    fn classify_all_returns_none_when_no_range_matches() {
        let mut classifications = MaterialClassifications::default();
        classifications.entries.push(ferrite_entry());
        // density=0.10 is well outside ferrite range [0.49, 0.56]
        let result = classifications.classify_all_observed(&weight_and_thermal(0.10, 0.45));
        assert!(result.is_empty());
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
        use crate::materials::{WellKnownMaterial, derive_material_from_seed};

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

        for (&wk, &(_mat_name, expected_class)) in
            WellKnownMaterial::all().iter().zip(expected.iter())
        {
            let mat = derive_material_from_seed(wk.seed());
            let props = mat.property_vector();
            // Simulate player having observed both Weight and ThermalBehavior.
            let revealed = weight_and_thermal(props[0], props[1]);
            let result = classifications.classify_observed(&revealed);
            assert_eq!(
                result.map(|e| e.name.as_str()),
                Some(expected_class),
                "{} (seed {}): density={:.4} thermal={:.4} — expected {} got {:?}",
                wk.display_name(),
                wk.seed(),
                props[0],
                props[1],
                expected_class,
                result.map(|e| e.name.as_str())
            );
        }
    }
}

#[cfg(test)]
mod integration_tests {
    use super::*;
    use crate::journal::{JournalKey, ObservationCategory};
    use crate::knowledge_graph::{ConceptCategory, ConceptId, KnowledgeGraph};

    #[test]
    fn revealed_properties_flow_end_to_end() {
        use crate::materials::derive_material_from_seed;

        // Simulate what update_knowledge_graph does when a Weight observation fires
        let seed = 1001u64; // Ferrite
        let mat = derive_material_from_seed(seed);
        let density = mat.density.value();
        let thermal = mat.thermal_resistance.value();

        let mut graph = KnowledgeGraph::default();
        let key = JournalKey::MaterialInstance { seed };
        let node_idx = graph.ensure_concept(ConceptId(key), ConceptCategory::Material, 0);

        // Simulate reveal_property being called for Weight
        graph.reveal_property(node_idx, ObservationCategory::Weight, density);
        graph.reveal_property(node_idx, ObservationCategory::ThermalBehavior, thermal);

        let node = graph.node(node_idx).unwrap();
        let revealed = &node.revealed_properties;

        println!("Revealed: {:?}", revealed);
        println!("density={:.4} thermal={:.4}", density, thermal);

        // Now classify
        let contents = std::fs::read_to_string(crate::classification::CLASSIFICATIONS_PATH)
            .expect("classifications.toml should exist");
        let file: crate::classification::ClassificationsFile =
            toml::from_str(&contents).expect("classifications.toml should parse");
        let classifications = MaterialClassifications {
            entries: file.classification,
        };

        let result = classifications.classify_observed(revealed);
        println!("Classification: {:?}", result.map(|e| &e.name));
        assert_eq!(
            result.map(|e| e.name.as_str()),
            Some("ferrite"),
            "Ferrite seed should classify as ferrite after Weight+Thermal revealed"
        );
    }
}
