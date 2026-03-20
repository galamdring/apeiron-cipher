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
//! The `spawn_material_objects` system creates 3D entities from the catalog and
//! distributes them across [`Surface`](crate::scene::Surface) shelves.

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::scene::Shelf;

pub(crate) struct MaterialPlugin;

impl Plugin for MaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_material_catalog)
            .add_systems(PostStartup, spawn_material_objects);
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

impl GameMaterial {
    /// Converts the stored colour triple to a Bevy [`Color`].
    pub(crate) fn bevy_color(&self) -> Color {
        Color::srgb(self.color[0], self.color[1], self.color[2])
    }

    /// Chooses a mesh shape based on material density.
    /// Light materials → sphere, heavy → cube, medium → capsule.
    fn mesh_for_density(&self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        let density = self.density.value;
        if density < 0.3 {
            meshes.add(Sphere::new(0.12).mesh().build())
        } else if density < 0.7 {
            meshes.add(Capsule3d::new(0.08, 0.18).mesh().build())
        } else {
            meshes.add(Cuboid::new(0.18, 0.18, 0.18))
        }
    }
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

// ── World-object marker ──────────────────────────────────────────────────

/// Marks an entity as a material object that exists physically in the world.
/// The material's data is on the same entity as a [`GameMaterial`] component.
#[derive(Component, Debug)]
pub(crate) struct MaterialObject;

// ── Spawning ─────────────────────────────────────────────────────────────

const OBJECT_SCALE: f32 = 1.0;

/// Places a 3D entity for each material in the catalog onto the `Surface`
/// entities created by the scene plugin. Materials are distributed across
/// surfaces round-robin and offset so they don't overlap.
fn spawn_material_objects(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    catalog: Res<MaterialCatalog>,
    shelves: Query<&Transform, With<Shelf>>,
) {
    let shelf_transforms: Vec<&Transform> = shelves.iter().collect();
    if shelf_transforms.is_empty() {
        warn!("No Shelf entities found — materials will not be spawned in the world");
        return;
    }

    let mut sorted_names: Vec<&String> = catalog.materials.keys().collect();
    sorted_names.sort();

    for (i, name) in sorted_names.iter().enumerate() {
        let mat = &catalog.materials[*name];
        let surface_tf = shelf_transforms[i % shelf_transforms.len()];

        let items_on_this_surface = sorted_names
            .iter()
            .enumerate()
            .filter(|(j, _)| j % shelf_transforms.len() == i % shelf_transforms.len())
            .position(|(j, _)| j == i)
            .unwrap_or(0);

        let x_offset = (items_on_this_surface as f32) * 0.3 - 0.3;

        let mesh = mat.mesh_for_density(&mut meshes);
        let render_mat = std_materials.add(StandardMaterial {
            base_color: mat.bevy_color(),
            perceptual_roughness: 0.5,
            metallic: if mat.conductivity.value > 0.6 {
                0.6
            } else {
                0.1
            },
            ..default()
        });

        commands.spawn((
            MaterialObject,
            mat.clone(),
            Mesh3d(mesh),
            MeshMaterial3d(render_mat),
            Transform::from_xyz(
                surface_tf.translation.x + x_offset,
                surface_tf.translation.y + 0.1,
                surface_tf.translation.z,
            )
            .with_scale(Vec3::splat(OBJECT_SCALE)),
        ));

        info!("Spawned material object '{}' on surface", mat.name);
    }
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
