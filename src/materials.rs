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
pub struct MaterialPlugin;

pub const MATERIAL_SURFACE_GAP: f32 = 0.01;

// ── Seed-derived material property channels ──────────────────────────────
//
// Each channel constant is mixed with a material seed via `mix_seed` to
// deterministically derive a single property value. The 0xA7E1_0001 prefix
// groups all material-property channels; the low word distinguishes each
// property. These must never change once shipped — doing so would alter
// every seed-derived material in every saved world.

/// Channel for deriving material density from a seed.
#[allow(dead_code)] // Used by derive_material_from_seed; callers arrive in Story 5a.4 Phase 2+.
pub const MAT_DENSITY_CHANNEL: u64 = 0xA7E1_0001_0000_0001;
/// Channel for deriving material thermal resistance from a seed.
#[allow(dead_code)]
pub const MAT_THERMAL_RESISTANCE_CHANNEL: u64 = 0xA7E1_0001_0000_0002;
/// Channel for deriving material reactivity from a seed.
#[allow(dead_code)]
pub const MAT_REACTIVITY_CHANNEL: u64 = 0xA7E1_0001_0000_0003;
/// Channel for deriving material conductivity from a seed.
#[allow(dead_code)]
pub const MAT_CONDUCTIVITY_CHANNEL: u64 = 0xA7E1_0001_0000_0004;
/// Channel for deriving material toxicity from a seed.
#[allow(dead_code)]
pub const MAT_TOXICITY_CHANNEL: u64 = 0xA7E1_0001_0000_0005;
/// Channel for deriving the red component of material color from a seed.
#[allow(dead_code)]
pub const MAT_COLOR_R_CHANNEL: u64 = 0xA7E1_0001_0000_0006;
/// Channel for deriving the green component of material color from a seed.
#[allow(dead_code)]
pub const MAT_COLOR_G_CHANNEL: u64 = 0xA7E1_0001_0000_0007;
/// Channel for deriving the blue component of material color from a seed.
#[allow(dead_code)]
pub const MAT_COLOR_B_CHANNEL: u64 = 0xA7E1_0001_0000_0008;

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
pub enum PropertyVisibility {
    Observable,
    Hidden,
    Revealed,
}

// ── Material property ────────────────────────────────────────────────────

/// A single material property: a normalised f32 value and its visibility state.
///
/// Values are clamped to \[0.0, 1.0\] for uniform combination math (Story 3.2).
#[derive(Clone, Debug, Serialize, Deserialize, Reflect)]
pub struct MaterialProperty {
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
pub struct GameMaterial {
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
    pub fn bevy_color(&self) -> Color {
        Color::srgb(self.color[0], self.color[1], self.color[2])
    }

    /// Chooses a mesh shape based on material density.
    /// Light materials → sphere, heavy → cube, medium → capsule.
    pub fn mesh_for_density(&self, meshes: &mut Assets<Mesh>) -> Handle<Mesh> {
        let density = self.density.value;
        if density < 0.3 {
            meshes.add(Sphere::new(0.12).mesh().build())
        } else if density < 0.7 {
            meshes.add(Capsule3d::new(0.08, 0.18).mesh().build())
        } else {
            meshes.add(Cuboid::new(0.18, 0.18, 0.18))
        }
    }

    /// Height from the support surface to the entity origin for the selected mesh.
    pub fn support_height(&self) -> f32 {
        let density = self.density.value;
        if density < 0.3 {
            0.12
        } else if density < 0.7 {
            0.17
        } else {
            0.09
        }
    }

    pub fn resting_center_y(&self, surface_y: f32) -> f32 {
        surface_y + self.support_height() + MATERIAL_SURFACE_GAP
    }

    pub fn footprint_radius(&self) -> f32 {
        let density = self.density.value;
        if density < 0.3 {
            0.12
        } else if density < 0.7 {
            0.10
        } else {
            0.13
        }
    }
}

// ── Seed-derived helpers ─────────────────────────────────────────────────

/// Deterministically mix a base seed and a channel into a new 64-bit value.
///
/// SplitMix64-style bit mixer — cheap, deterministic, no external crate.
/// Identical to the mixer in `world_generation`; duplicated here so the
/// material module has no coupling to world-gen internals.
#[allow(dead_code)] // Called by derive_material_from_seed; callers arrive in later phases.
fn mix_seed(base: u64, channel: u64) -> u64 {
    let mut z = base.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Map a `u64` into the closed unit interval \[0.0, 1.0\].
#[allow(dead_code)]
fn unit_interval_01(value: u64) -> f32 {
    (value as f64 / u64::MAX as f64) as f32
}

/// Derive a complete [`GameMaterial`] deterministically from a seed.
///
/// Every property is produced by mixing the seed with a fixed channel constant
/// and mapping the result to \[0.0, 1.0\]. Color channels (R, G, B) use three
/// additional channels. The name is generated procedurally via
/// [`crate::naming::procedural_name`].
///
/// All property visibilities start as [`PropertyVisibility::Hidden`] — the
/// observation/journal system reveals them through gameplay.
///
/// **Determinism guarantee:** same seed always produces the same material.
#[allow(dead_code)] // Public API for Story 5a.4 Phase 2+ (biome palette integration).
pub fn derive_material_from_seed(seed: u64) -> GameMaterial {
    let name = crate::naming::procedural_name(seed);

    let color = [
        unit_interval_01(mix_seed(seed, MAT_COLOR_R_CHANNEL)),
        unit_interval_01(mix_seed(seed, MAT_COLOR_G_CHANNEL)),
        unit_interval_01(mix_seed(seed, MAT_COLOR_B_CHANNEL)),
    ];

    GameMaterial {
        name,
        seed,
        color,
        density: MaterialProperty {
            value: unit_interval_01(mix_seed(seed, MAT_DENSITY_CHANNEL)),
            visibility: PropertyVisibility::Hidden,
        },
        thermal_resistance: MaterialProperty {
            value: unit_interval_01(mix_seed(seed, MAT_THERMAL_RESISTANCE_CHANNEL)),
            visibility: PropertyVisibility::Hidden,
        },
        reactivity: MaterialProperty {
            value: unit_interval_01(mix_seed(seed, MAT_REACTIVITY_CHANNEL)),
            visibility: PropertyVisibility::Hidden,
        },
        conductivity: MaterialProperty {
            value: unit_interval_01(mix_seed(seed, MAT_CONDUCTIVITY_CHANNEL)),
            visibility: PropertyVisibility::Hidden,
        },
        toxicity: MaterialProperty {
            value: unit_interval_01(mix_seed(seed, MAT_TOXICITY_CHANNEL)),
            visibility: PropertyVisibility::Hidden,
        },
    }
}

// ── Catalog resource ─────────────────────────────────────────────────────

/// All loaded material definitions, keyed by name.
///
/// Later stories use this to spawn material entities and to look up base
/// definitions during fabrication.
#[derive(Resource, Debug, Default)]
pub struct MaterialCatalog {
    pub materials: HashMap<String, GameMaterial>,
}

impl MaterialCatalog {
    /// Derive a material from a seed and register it in the catalog, returning a
    /// reference to the (possibly already-present) entry.
    ///
    /// If a material with the same **seed** already exists, returns the existing
    /// entry unchanged.  If the procedurally generated name collides with a
    /// *different* seed's material, a deterministic disambiguator derived from
    /// the seed is appended (e.g. `"Vexorite-a3f1"`) until the name is unique.
    #[allow(dead_code)] // Public API for Story 5a.4 Phase 3+ (biome palette integration).
    pub fn derive_and_register(&mut self, seed: u64) -> &GameMaterial {
        // Fast path: already registered (lookup by name of this seed's base material).
        // We check all entries for a matching seed to avoid re-deriving.
        if let Some(name) = self.materials.values().find_map(|m| {
            if m.seed == seed {
                Some(m.name.clone())
            } else {
                None
            }
        }) {
            return &self.materials[&name];
        }

        let mut mat = derive_material_from_seed(seed);
        mat.name = Self::disambiguated_name(&mat.name, seed, &self.materials);
        let key = mat.name.clone();
        self.materials.insert(key.clone(), mat);
        &self.materials[&key]
    }

    /// Return `base_name` if it is not already taken in `existing`, otherwise
    /// append a short hex suffix derived deterministically from `seed`.
    ///
    /// The suffix is produced by taking successive 16-bit windows of the seed
    /// (formatted as lowercase hex).  In the astronomically unlikely case that
    /// *all* eight 16-bit windows also collide, we fall back to the full 16-hex
    /// seed representation which is unique by definition (different seeds).
    #[allow(dead_code)] // Used by `derive_and_register`; called indirectly in tests.
    fn disambiguated_name(
        base_name: &str,
        seed: u64,
        existing: &HashMap<String, GameMaterial>,
    ) -> String {
        if !existing.contains_key(base_name) {
            return base_name.to_owned();
        }

        // Try successive 16-bit windows of the seed as a 4-hex-char suffix.
        for shift in (0..64).step_by(16) {
            let fragment = (seed >> shift) as u16;
            let candidate = format!("{base_name}-{fragment:04x}");
            if !existing.contains_key(&candidate) {
                return candidate;
            }
        }

        // Ultimate fallback: full seed hex (guaranteed unique for distinct seeds).
        format!("{base_name}-{seed:016x}")
    }
}

// ── World-object marker ──────────────────────────────────────────────────

/// Marks an entity as a material object that exists physically in the world.
/// The material's data is on the same entity as a [`GameMaterial`] component.
#[derive(Component, Debug)]
pub struct MaterialObject;

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
                mat.resting_center_y(surface_tf.translation.y),
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
    fn support_height_matches_density_mesh_shape() {
        let mut light = sample_material();
        light.density.value = 0.2;
        assert!((light.support_height() - 0.12).abs() < f32::EPSILON);

        let mut medium = sample_material();
        medium.density.value = 0.5;
        assert!((medium.support_height() - 0.17).abs() < f32::EPSILON);

        let heavy = sample_material();
        assert!((heavy.support_height() - 0.09).abs() < f32::EPSILON);
    }

    #[test]
    fn footprint_radius_matches_density_mesh_shape() {
        let mut light = sample_material();
        light.density.value = 0.2;
        assert!((light.footprint_radius() - 0.12).abs() < f32::EPSILON);

        let mut medium = sample_material();
        medium.density.value = 0.5;
        assert!((medium.footprint_radius() - 0.10).abs() < f32::EPSILON);

        let heavy = sample_material();
        assert!((heavy.footprint_radius() - 0.13).abs() < f32::EPSILON);
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

    // ── derive_material_from_seed tests ──────────────────────────────────

    #[test]
    fn derive_material_deterministic() {
        let a = derive_material_from_seed(0xDEAD_BEEF);
        let b = derive_material_from_seed(0xDEAD_BEEF);
        assert_eq!(a.name, b.name);
        assert_eq!(a.seed, b.seed);
        assert!((a.density.value - b.density.value).abs() < f32::EPSILON);
        assert!((a.thermal_resistance.value - b.thermal_resistance.value).abs() < f32::EPSILON);
        assert!((a.reactivity.value - b.reactivity.value).abs() < f32::EPSILON);
        assert!((a.conductivity.value - b.conductivity.value).abs() < f32::EPSILON);
        assert!((a.toxicity.value - b.toxicity.value).abs() < f32::EPSILON);
        assert_eq!(a.color, b.color);
    }

    #[test]
    fn derive_material_different_seeds_differ() {
        let a = derive_material_from_seed(1);
        let b = derive_material_from_seed(2);
        // With good mixing, at least one property should differ.
        let same_density = (a.density.value - b.density.value).abs() < f32::EPSILON;
        let same_reactivity = (a.reactivity.value - b.reactivity.value).abs() < f32::EPSILON;
        let same_conductivity = (a.conductivity.value - b.conductivity.value).abs() < f32::EPSILON;
        assert!(
            !(same_density && same_reactivity && same_conductivity),
            "different seeds should produce different materials"
        );
    }

    #[test]
    fn derive_material_all_hidden() {
        let mat = derive_material_from_seed(42);
        assert_eq!(mat.density.visibility, PropertyVisibility::Hidden);
        assert_eq!(
            mat.thermal_resistance.visibility,
            PropertyVisibility::Hidden
        );
        assert_eq!(mat.reactivity.visibility, PropertyVisibility::Hidden);
        assert_eq!(mat.conductivity.visibility, PropertyVisibility::Hidden);
        assert_eq!(mat.toxicity.visibility, PropertyVisibility::Hidden);
    }

    #[test]
    fn derive_material_values_in_unit_range() {
        // Test across a spread of seeds to ensure all properties stay in [0, 1].
        for seed in [0, 1, u64::MAX, 0xCAFE_BABE, 0x1234_5678_9ABC_DEF0] {
            let mat = derive_material_from_seed(seed);
            for (label, val) in [
                ("density", mat.density.value),
                ("thermal_resistance", mat.thermal_resistance.value),
                ("reactivity", mat.reactivity.value),
                ("conductivity", mat.conductivity.value),
                ("toxicity", mat.toxicity.value),
                ("color_r", mat.color[0]),
                ("color_g", mat.color[1]),
                ("color_b", mat.color[2]),
            ] {
                assert!(
                    (0.0..=1.0).contains(&val),
                    "seed {seed:#X}: {label} = {val} out of [0,1]"
                );
            }
        }
    }

    #[test]
    fn derive_material_name_not_empty() {
        let mat = derive_material_from_seed(999);
        assert!(!mat.name.is_empty());
    }

    #[test]
    fn derive_material_preserves_seed() {
        let seed = 0xFE00_0000_0000_0001;
        let mat = derive_material_from_seed(seed);
        assert_eq!(mat.seed, seed);
    }

    #[test]
    fn derive_material_non_degenerate_across_100_seeds() {
        use std::collections::HashSet;

        let count: usize = 128;
        // Use seeds spread across the u64 range so every bit window in the
        // mixer and naming function gets exercised.  Sequential small integers
        // share low-order bits which would under-test higher bit windows.
        let materials: Vec<GameMaterial> = (0..count as u64)
            .map(|i| {
                let seed = i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1);
                derive_material_from_seed(seed)
            })
            .collect();

        // Collect unique values per property to verify the mixer spreads well.
        let mut unique_density = HashSet::new();
        let mut unique_thermal = HashSet::new();
        let mut unique_reactivity = HashSet::new();
        let mut unique_conductivity = HashSet::new();
        let mut unique_toxicity = HashSet::new();
        let mut unique_names = HashSet::new();
        let mut unique_colors = HashSet::new();

        for mat in &materials {
            unique_density.insert(mat.density.value.to_bits());
            unique_thermal.insert(mat.thermal_resistance.value.to_bits());
            unique_reactivity.insert(mat.reactivity.value.to_bits());
            unique_conductivity.insert(mat.conductivity.value.to_bits());
            unique_toxicity.insert(mat.toxicity.value.to_bits());
            unique_names.insert(mat.name.clone());
            unique_colors.insert((
                mat.color[0].to_bits(),
                mat.color[1].to_bits(),
                mat.color[2].to_bits(),
            ));
        }

        // With 128 seeds and a good mixer, every property should have many
        // distinct values — at least 10 unique values out of 128.  A degenerate
        // mixer that collapses to a handful of buckets will fail this.
        let threshold = 10;
        assert!(
            unique_density.len() >= threshold,
            "density collapsed: only {} unique values out of {count}",
            unique_density.len()
        );
        assert!(
            unique_thermal.len() >= threshold,
            "thermal_resistance collapsed: only {} unique values out of {count}",
            unique_thermal.len()
        );
        assert!(
            unique_reactivity.len() >= threshold,
            "reactivity collapsed: only {} unique values out of {count}",
            unique_reactivity.len()
        );
        assert!(
            unique_conductivity.len() >= threshold,
            "conductivity collapsed: only {} unique values out of {count}",
            unique_conductivity.len()
        );
        assert!(
            unique_toxicity.len() >= threshold,
            "toxicity collapsed: only {} unique values out of {count}",
            unique_toxicity.len()
        );
        assert!(
            unique_names.len() >= threshold,
            "names collapsed: only {} unique values out of {count}",
            unique_names.len()
        );
        assert!(
            unique_colors.len() >= threshold,
            "colors collapsed: only {} unique values out of {count}",
            unique_colors.len()
        );

        // Additionally verify no two materials are fully identical (all properties match).
        for i in 0..materials.len() {
            for j in (i + 1)..materials.len() {
                let a = &materials[i];
                let b = &materials[j];
                let all_same = a.density.value.to_bits() == b.density.value.to_bits()
                    && a.thermal_resistance.value.to_bits() == b.thermal_resistance.value.to_bits()
                    && a.reactivity.value.to_bits() == b.reactivity.value.to_bits()
                    && a.conductivity.value.to_bits() == b.conductivity.value.to_bits()
                    && a.toxicity.value.to_bits() == b.toxicity.value.to_bits()
                    && a.color[0].to_bits() == b.color[0].to_bits()
                    && a.color[1].to_bits() == b.color[1].to_bits()
                    && a.color[2].to_bits() == b.color[2].to_bits();
                assert!(
                    !all_same,
                    "seeds {} and {} produced identical materials",
                    a.seed, b.seed
                );
            }
        }
    }

    #[test]
    fn mix_seed_deterministic() {
        let a = mix_seed(100, 200);
        let b = mix_seed(100, 200);
        assert_eq!(a, b);
    }

    #[test]
    fn mix_seed_different_channels_differ() {
        let a = mix_seed(100, 1);
        let b = mix_seed(100, 2);
        assert_ne!(a, b);
    }

    #[test]
    fn unit_interval_01_bounds() {
        assert!((unit_interval_01(0) - 0.0).abs() < f32::EPSILON);
        assert!((unit_interval_01(u64::MAX) - 1.0).abs() < f32::EPSILON);
        let mid = unit_interval_01(u64::MAX / 2);
        assert!((0.0..=1.0).contains(&mid));
    }

    // ── Collision-avoidance tests ────────────────────────────────────────

    #[test]
    fn derive_and_register_returns_same_entry_for_same_seed() {
        let mut catalog = MaterialCatalog::default();
        let name1 = catalog.derive_and_register(42).name.clone();
        let name2 = catalog.derive_and_register(42).name.clone();
        assert_eq!(name1, name2);
        assert_eq!(catalog.materials.len(), 1);
    }

    #[test]
    fn derive_and_register_disambiguates_name_collision() {
        // Force a collision by pre-inserting a material whose name matches
        // what seed 999 would generate, but with a different seed.
        let mut catalog = MaterialCatalog::default();
        let base_name = crate::naming::procedural_name(999);

        let mut imposter = derive_material_from_seed(0xBEEF);
        imposter.name = base_name.clone();
        imposter.seed = 0xBEEF; // different seed, same name
        catalog.materials.insert(base_name.clone(), imposter);

        let registered = catalog.derive_and_register(999);
        // Name must differ from the pre-existing entry.
        assert_ne!(registered.name, base_name);
        // Must contain the base name as a prefix with a hex suffix.
        assert!(
            registered.name.starts_with(&base_name),
            "disambiguated name '{}' should start with base '{}'",
            registered.name,
            base_name
        );
        assert!(
            registered.name.contains('-'),
            "disambiguated name should contain a '-' separator"
        );
        // Catalog now has both entries.
        assert_eq!(catalog.materials.len(), 2);
    }

    #[test]
    fn disambiguated_name_no_collision_returns_base() {
        let existing = HashMap::new();
        let result = MaterialCatalog::disambiguated_name("Vexorite", 42, &existing);
        assert_eq!(result, "Vexorite");
    }

    #[test]
    fn disambiguated_name_with_collision_appends_suffix() {
        let mut existing = HashMap::new();
        existing.insert("Vexorite".to_string(), derive_material_from_seed(0xAAAA));
        let result =
            MaterialCatalog::disambiguated_name("Vexorite", 0x1234_5678_9ABC_DEF0, &existing);
        assert_eq!(result, "Vexorite-def0");
    }

    #[test]
    fn disambiguated_name_deterministic() {
        let mut existing = HashMap::new();
        existing.insert("Coranite".to_string(), derive_material_from_seed(0xBBBB));
        let a = MaterialCatalog::disambiguated_name("Coranite", 777, &existing);
        let b = MaterialCatalog::disambiguated_name("Coranite", 777, &existing);
        assert_eq!(a, b);
    }
}
