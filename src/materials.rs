//! Material data model plugin — defines the property system for all materials.
//!
//! Materials are the core interactive objects in Apeiron Cipher. Each material
//! has a set of typed properties (density, thermal resistance, etc.) tagged with
//! visibility states that control what the player can observe directly versus
//! what must be discovered through experimentation.
//!
//! Material definitions live in TOML files under `assets/materials/`. They are
//! loaded at startup via `std::fs` (not `AssetServer` — material definitions are
//! startup configuration, not hot-reloadable game assets). Each file defines one
//! material with its seed, color, and property values.
//!
//! The [`MaterialCatalog`] resource holds every loaded definition, keyed by name.
//! Later stories spawn material entities from this catalog.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub(crate) struct MaterialPlugin;

impl Plugin for MaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_material_catalog);
    }
}

// ── Property visibility ──────────────────────────────────────────────────

/// Controls whether the player can perceive a property directly.
///
/// `Observable` properties are visible on first inspection (color, apparent
/// weight). `Hidden` properties require environmental testing to discover.
/// `Revealed` is set at runtime once the player has uncovered a hidden property.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize, Reflect)]
pub(crate) enum PropertyVisibility {
    Observable,
    Hidden,
    Revealed,
}

// ── Material property ────────────────────────────────────────────────────

/// A single material property: a normalised f32 value and its visibility state.
///
/// Values are clamped to \[0.0, 1.0\] for uniform combination math (Story 3.2).
#[derive(Clone, Debug, Serialize, Deserialize, Reflect)]
pub(crate) struct MaterialProperty {
    pub value: f32,
    pub visibility: PropertyVisibility,
}

// ── Material definition (TOML ↔ Rust) ────────────────────────────────────

/// Complete definition of a material loaded from a TOML data file.
///
/// This struct is both the serialisation target for `assets/materials/*.toml`
/// and the ECS component attached to material entities when they are spawned
/// into the world (Story 2.2).
///
/// `seed` drives deterministic generation for fabricated materials (Story 3.2).
/// Base materials define seed explicitly; derived materials compute it from
/// input seeds.
#[derive(Component, Clone, Debug, Serialize, Deserialize, Reflect)]
pub(crate) struct GameMaterial {
    pub name: String,
    pub seed: u64,
    /// Display colour as \[R, G, B\] in sRGB 0.0–1.0.
    pub color: [f32; 3],
    pub density: MaterialProperty,
    pub thermal_resistance: MaterialProperty,
    pub reactivity: MaterialProperty,
    pub conductivity: MaterialProperty,
    pub toxicity: MaterialProperty,
}

// ── Catalog resource ─────────────────────────────────────────────────────

/// All loaded material definitions, keyed by name.
///
/// Later stories use this to spawn material entities and to look up base
/// definitions during fabrication.
#[derive(Resource, Debug, Default)]
pub(crate) struct MaterialCatalog {
    pub materials: HashMap<String, GameMaterial>,
}

// ── Loading ──────────────────────────────────────────────────────────────

const MATERIALS_DIR: &str = "assets/materials";

fn load_material_catalog(mut commands: Commands) {
    let mut catalog = MaterialCatalog::default();
    let dir = Path::new(MATERIALS_DIR);

    if !dir.exists() || !dir.is_dir() {
        warn!("{MATERIALS_DIR} directory not found — starting with an empty material catalog");
        commands.insert_resource(catalog);
        return;
    }

    let entries = match fs::read_dir(dir) {
        Ok(e) => e,
        Err(err) => {
            warn!("Could not read {MATERIALS_DIR}: {err}");
            commands.insert_resource(catalog);
            return;
        }
    };

    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().is_some_and(|ext| ext == "toml") {
            match fs::read_to_string(&path) {
                Ok(contents) => match toml::from_str::<GameMaterial>(&contents) {
                    Ok(mat) => {
                        info!("Loaded material '{}' from {}", mat.name, path.display());
                        catalog.materials.insert(mat.name.clone(), mat);
                    }
                    Err(e) => {
                        warn!("Skipping malformed material file {}: {e}", path.display());
                    }
                },
                Err(e) => {
                    warn!("Could not read {}: {e}", path.display());
                }
            }
        }
    }

    info!(
        "Material catalog loaded: {} materials",
        catalog.materials.len()
    );
    commands.insert_resource(catalog);
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(value: f32, visibility: PropertyVisibility) -> MaterialProperty {
        MaterialProperty {
            value: value.clamp(0.0, 1.0),
            visibility,
        }
    }

    fn sample_material() -> GameMaterial {
        GameMaterial {
            name: "Ferrite".into(),
            seed: 1001,
            color: [0.58, 0.55, 0.52],
            density: prop(0.78, PropertyVisibility::Observable),
            thermal_resistance: prop(0.65, PropertyVisibility::Hidden),
            reactivity: prop(0.35, PropertyVisibility::Hidden),
            conductivity: prop(0.72, PropertyVisibility::Hidden),
            toxicity: prop(0.05, PropertyVisibility::Hidden),
        }
    }

    #[test]
    fn toml_round_trip_preserves_material() {
        let original = sample_material();
        let serialized = toml::to_string(&original).expect("serialize");
        let parsed: GameMaterial = toml::from_str(&serialized).expect("deserialize");

        assert_eq!(parsed.name, original.name);
        assert_eq!(parsed.seed, original.seed);
        assert!((parsed.density.value - original.density.value).abs() < f32::EPSILON);
        assert_eq!(
            parsed.thermal_resistance.visibility,
            PropertyVisibility::Hidden
        );
    }

    #[test]
    fn property_values_clamped_to_unit_range() {
        let over = prop(1.5, PropertyVisibility::Observable);
        let under = prop(-0.3, PropertyVisibility::Hidden);
        assert!((over.value - 1.0).abs() < f32::EPSILON);
        assert!(under.value.abs() < f32::EPSILON);
    }

    #[test]
    fn observable_vs_hidden_visibility() {
        let obs = prop(0.5, PropertyVisibility::Observable);
        let hid = prop(0.5, PropertyVisibility::Hidden);
        assert_eq!(obs.visibility, PropertyVisibility::Observable);
        assert_eq!(hid.visibility, PropertyVisibility::Hidden);
    }

    #[test]
    fn same_seed_same_material_from_toml() {
        let toml_str = r#"
name = "TestMat"
seed = 42
color = [0.5, 0.5, 0.5]

[density]
value = 0.6
visibility = "Observable"

[thermal_resistance]
value = 0.4
visibility = "Hidden"

[reactivity]
value = 0.3
visibility = "Hidden"

[conductivity]
value = 0.7
visibility = "Hidden"

[toxicity]
value = 0.1
visibility = "Hidden"
"#;
        let first: GameMaterial = toml::from_str(toml_str).expect("first parse");
        let second: GameMaterial = toml::from_str(toml_str).expect("second parse");

        assert_eq!(first.seed, second.seed);
        assert!((first.density.value - second.density.value).abs() < f32::EPSILON);
        assert!((first.reactivity.value - second.reactivity.value).abs() < f32::EPSILON);
        assert!((first.conductivity.value - second.conductivity.value).abs() < f32::EPSILON);
    }

    #[test]
    fn catalog_default_is_empty() {
        let catalog = MaterialCatalog::default();
        assert!(catalog.materials.is_empty());
    }

    #[test]
    fn material_file_parsing_matches_expected_format() {
        let file_content = include_str!("../assets/materials/ferrite.toml");
        let mat: GameMaterial = toml::from_str(file_content).expect("parse ferrite.toml");
        assert_eq!(mat.name, "Ferrite");
        assert_eq!(mat.seed, 1001);
        assert_eq!(mat.density.visibility, PropertyVisibility::Observable);
        assert_eq!(
            mat.thermal_resistance.visibility,
            PropertyVisibility::Hidden
        );
    }

    #[test]
    fn all_material_files_parse_successfully() {
        let files = [
            include_str!("../assets/materials/ferrite.toml"),
            include_str!("../assets/materials/calcium.toml"),
            include_str!("../assets/materials/sulfurite.toml"),
            include_str!("../assets/materials/prismate.toml"),
            include_str!("../assets/materials/verdant.toml"),
            include_str!("../assets/materials/osmium.toml"),
            include_str!("../assets/materials/volatite.toml"),
            include_str!("../assets/materials/cobaltine.toml"),
            include_str!("../assets/materials/silite.toml"),
            include_str!("../assets/materials/phosphite.toml"),
        ];
        let mut names = std::collections::HashSet::new();
        let mut seeds = std::collections::HashSet::new();
        for (i, src) in files.iter().enumerate() {
            let mat: GameMaterial =
                toml::from_str(src).unwrap_or_else(|e| panic!("file {i} failed: {e}"));
            assert!(!mat.name.is_empty(), "material {i} has an empty name");
            assert!(
                names.insert(mat.name.clone()),
                "duplicate name: {}",
                mat.name
            );
            assert!(seeds.insert(mat.seed), "duplicate seed: {}", mat.seed);
        }
        assert_eq!(names.len(), 10, "expected 10 unique materials");
    }
}
