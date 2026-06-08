//! Material data model plugin — defines the property system for all materials.
//!
//! Materials are the core interactive objects in Apeiron Cipher. Each material
//! has a set of typed properties (density, thermal resistance, etc.) tagged with
//! visibility states that control what the player can observe directly versus
//! what must be discovered through experimentation.
//!
//! Materials are seed-derived: each material is deterministically generated from
//! a `u64` seed via [`derive_material_from_seed`]. The [`MaterialCatalog`]
//! starts empty at startup and grows as the player explores — biome palettes
//! define which seeds appear in each region, and materials are registered on
//! first encounter.
//!
//! Legacy TOML files under `assets/materials/` are retained as reference
//! documentation but are no longer loaded at startup.
//!
//! Materials are spawned in the world exclusively through exterior chunk
//! generation — deposits place entities with `origin_planet_seed` stamped at
//! spawn time. There is no longer a room-based starter set.

use std::collections::HashMap;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::seed_util::{
    MAT_COLOR_B_CHANNEL, MAT_COLOR_G_CHANNEL, MAT_COLOR_R_CHANNEL, MAT_CONDUCTIVITY_CHANNEL,
    MAT_DENSITY_CHANNEL, MAT_REACTIVITY_CHANNEL, MAT_THERMAL_RESISTANCE_CHANNEL,
    MAT_TOXICITY_CHANNEL, mix_seed,
};
use crate::world_generation::PlanetSeed;

/// Typed seed for a material instance.
///
/// Wraps the raw `u64` seed so that material seeds cannot be silently confused
/// with planet seeds or other seed domains at the type level.  Bare `u64` is
/// only permitted at serialisation / asset-loading edges; everywhere else pass
/// `MaterialSeed`.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Reflect)]
pub struct MaterialSeed(pub u64);

impl std::fmt::Display for MaterialSeed {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        write!(f, "{:#018x}", self.0)
    }
}

/// Registers the material data model, catalog, and world-object spawning systems.
pub struct MaterialPlugin;

/// Small vertical gap between a material object and the surface it rests on.
pub const MATERIAL_SURFACE_GAP: f32 = 0.01;

/// Number of properties in a [`GameMaterial`] property vector.
///
/// Used as the compile-time array size for [`GameMaterial::property_vector`]
/// so callers never need a magic number and adding a new property automatically
/// updates the type.
pub const PROPERTY_DIM: usize = 5;

// ── Well-known material seeds ────────────────────────────────────────────
//
// Migration table: maps the 10 original hand-authored material names to their
// canonical seed values (from the `seed` field in each `assets/materials/*.toml`
// file). These seeds are referenced by biome palettes so the legacy materials
// appear naturally through exploration. The seed values must never change —
// doing so would break saved worlds and biome palette references.

/// The 10 base materials whose seeds, display names, and classification
/// identities are part of the game's authoritative data model.
///
/// Seeds are stable forever — changing a seed renames every deposit of that
/// material across every generated world. Display names are cosmetic and may
/// be updated freely. Classification ranges in `classifications.toml` must
/// stay in sync with the seed values here.
///
/// Every variant's [`Self::seed`] value must be unique. A const assertion below
/// validates the full variant list at compile time so a duplicate seed fails
/// the build before it can silently collapse two deterministic material
/// identities into the same generated material.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash)]
pub enum WellKnownMaterial {
    /// Iron-rich metal. Dense, thermally resilient, conductive.
    Ferrite,
    /// Bone-like mineral. Light, calcium-rich, high thermal emission.
    Calcium,
    /// Volcanic compound. Reactive, sulfurous, medium density.
    Sulfurite,
    /// Crystalline lattice material. Low density, moderate thermal.
    Prismate,
    /// Organic mineral. Moderate density, thrives in temperate biomes.
    Verdant,
    /// Ultra-dense metal. Very high density, robust thermal resistance.
    Osmium,
    /// Volatile reactive compound. Medium density, very low thermal.
    Volatite,
    /// Metallic ore. High density, mid-range all-round properties.
    Cobaltine,
    /// Silicate mineral. Light, very low thermal resistance.
    Silite,
    /// Phosphorescent mineral. Very low density, very high thermal.
    Phosphite,
}

const ALL_WELL_KNOWN_MATERIALS: [WellKnownMaterial; 10] = [
    WellKnownMaterial::Ferrite,
    WellKnownMaterial::Calcium,
    WellKnownMaterial::Sulfurite,
    WellKnownMaterial::Prismate,
    WellKnownMaterial::Verdant,
    WellKnownMaterial::Osmium,
    WellKnownMaterial::Volatite,
    WellKnownMaterial::Cobaltine,
    WellKnownMaterial::Silite,
    WellKnownMaterial::Phosphite,
];

impl WellKnownMaterial {
    /// The generation seed that deterministically defines this material's
    /// property vector, color, and procedural name. Stable across all worlds.
    pub const fn seed(self) -> u64 {
        match self {
            Self::Ferrite => 1001,
            Self::Calcium => 1002,
            Self::Sulfurite => 1003,
            Self::Prismate => 1004,
            Self::Verdant => 1005,
            Self::Osmium => 1006,
            Self::Volatite => 1007,
            Self::Cobaltine => 1008,
            Self::Silite => 1009,
            Self::Phosphite => 1010,
        }
    }

    /// Human-readable classification label used in journal and examine panels.
    pub const fn display_name(self) -> &'static str {
        match self {
            Self::Ferrite => "Ferrite",
            Self::Calcium => "Calcium",
            Self::Sulfurite => "Sulfurite",
            Self::Prismate => "Prismate",
            Self::Verdant => "Verdant",
            Self::Osmium => "Osmium",
            Self::Volatite => "Volatite",
            Self::Cobaltine => "Cobaltine",
            Self::Silite => "Silite",
            Self::Phosphite => "Phosphite",
        }
    }

    /// All well-known materials in seed order.
    ///
    /// Use this wherever the old `WELL_KNOWN_MATERIAL_SEEDS` array was iterated.
    pub fn all() -> &'static [WellKnownMaterial] {
        &ALL_WELL_KNOWN_MATERIALS
    }
}

const WELL_KNOWN_MATERIAL_SEEDS_FOR_VALIDATION: [u64; ALL_WELL_KNOWN_MATERIALS.len()] =
    well_known_material_seed_values();

const fn well_known_material_seed_values() -> [u64; ALL_WELL_KNOWN_MATERIALS.len()] {
    let mut seeds = [0_u64; ALL_WELL_KNOWN_MATERIALS.len()];
    let mut i = 0;

    while i < ALL_WELL_KNOWN_MATERIALS.len() {
        seeds[i] = ALL_WELL_KNOWN_MATERIALS[i].seed();
        i += 1;
    }

    seeds
}

const fn validate_well_known_material_seed_uniqueness(seeds: &[u64]) {
    let mut i = 0;

    while i < seeds.len() {
        let mut j = i + 1;

        while j < seeds.len() {
            if seeds[i] == seeds[j] {
                panic!(
                    "duplicate WellKnownMaterial seed detected; every \
                     WellKnownMaterial::seed() value must be unique",
                );
            }
            j += 1;
        }

        i += 1;
    }
}

const _: () =
    validate_well_known_material_seed_uniqueness(&WELL_KNOWN_MATERIAL_SEEDS_FOR_VALIDATION);

/// Backward-compatible flat array for code that still iterates `(label, seed)` pairs.
/// New code should use [`WellKnownMaterial::all()`] instead.
#[deprecated(note = "use WellKnownMaterial::all() instead")]
pub const WELL_KNOWN_MATERIAL_SEEDS: &[(&str, u64)] = &[
    ("Ferrite", 1001),
    ("Calcium", 1002),
    ("Sulfurite", 1003),
    ("Prismate", 1004),
    ("Verdant", 1005),
    ("Osmium", 1006),
    ("Volatite", 1007),
    ("Cobaltine", 1008),
    ("Silite", 1009),
    ("Phosphite", 1010),
];

impl Plugin for MaterialPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_material_catalog);

        // In debug builds, run a per-frame assertion that no GameMaterial entity
        // has more than one of the three mutually-exclusive location markers:
        // MaterialObject (in world), HeldItem (held by player), InCarry (stashed).
        #[cfg(debug_assertions)]
        app.add_systems(PostUpdate, assert_material_state_exclusivity);
    }
}

/// Debug-only system that panics if any [`GameMaterial`] entity has more than
/// one of [`MaterialObject`], [`HeldItem`](crate::interaction::HeldItem), or
/// [`InCarry`](crate::carry::InCarry). These three markers represent mutually
/// exclusive physical locations for a material entity.
#[cfg(debug_assertions)]
#[allow(clippy::type_complexity)] // Debug validation query — clarity over brevity.
fn assert_material_state_exclusivity(
    query: Query<(
        Entity,
        &GameMaterial,
        Has<MaterialObject>,
        Has<crate::interaction::HeldItem>,
        Has<crate::carry::InCarry>,
    )>,
) {
    for (entity, material, in_world, held, stashed) in &query {
        let count = in_world as u8 + held as u8 + stashed as u8;
        assert!(
            count <= 1,
            "Entity {entity:?} ({}) has {count} location markers (MaterialObject={in_world}, \
             HeldItem={held}, InCarry={stashed}). These must be mutually exclusive.",
            material.name,
        );
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
    /// The player can perceive this property on first inspection.
    Observable,
    /// This property is not yet visible to the player.
    Hidden,
    /// This property was hidden but has been uncovered through experimentation.
    Revealed,
}

// ── Material property ────────────────────────────────────────────────────

/// A single material property: a normalised f32 value and its visibility state.
///
/// Values are clamped to \[0.0, 1.0\] for uniform combination math (Story 3.2).
#[derive(Clone, Debug, Serialize, Deserialize, Reflect)]
pub struct MaterialProperty {
    /// Normalised property value in \[0.0, 1.0\].
    value: f32,
    /// Whether the player can currently see this property.
    pub visibility: PropertyVisibility,
}

impl MaterialProperty {
    /// Creates a new material property with the given value and visibility.
    ///
    /// The value is automatically clamped to the valid range \[0.0, 1.0\].
    pub fn new(value: f32, visibility: PropertyVisibility) -> Self {
        Self {
            value: value.clamp(0.0, 1.0),
            visibility,
        }
    }

    /// Returns the normalised property value in \[0.0, 1.0\].
    pub fn value(&self) -> f32 {
        self.value
    }

    /// Sets the property value, automatically clamping to \[0.0, 1.0\].
    pub fn set_value(&mut self, value: f32) {
        self.value = value.clamp(0.0, 1.0);
    }
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
    /// Human-readable display name (procedurally generated or disambiguated).
    pub name: String,
    /// Deterministic seed used for generation and catalog identity.
    pub seed: u64,
    /// Display colour as \[R, G, B\] in sRGB 0.0–1.0.
    pub color: [f32; 3],
    /// The planet seed this material instance was generated on.
    ///
    /// Set at spawn time from `WorldProfile::planet_seed` and immutable
    /// thereafter — this piece came from this planet, forever. Used by
    /// observation systems to wire `FoundOn` edges in the KnowledgeGraph
    /// and by the `CurrentPlanet` journal filter.
    ///
    /// `None` in contexts where no planetary world profile exists (early
    /// bring-up, fabricated materials, integration tests).
    pub origin_planet_seed: Option<PlanetSeed>,
    /// How heavy the material feels — affects mesh shape selection.
    pub density: MaterialProperty,
    /// Resistance to heat transfer.
    pub thermal_resistance: MaterialProperty,
    /// Tendency to react when combined with other materials.
    pub reactivity: MaterialProperty,
    /// Ability to conduct energy (electrical/thermal).
    pub conductivity: MaterialProperty,
    /// Degree of toxicity when handled or combined.
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
        let density = self.density.value();
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
        let density = self.density.value();
        if density < 0.3 {
            0.12
        } else if density < 0.7 {
            0.17
        } else {
            0.09
        }
    }

    /// Returns the Y coordinate for the entity center when resting on a surface.
    pub fn resting_center_y(&self, surface_y: f32) -> f32 {
        surface_y + self.support_height() + MATERIAL_SURFACE_GAP
    }

    /// Returns the horizontal collision radius of the mesh for this material's density.
    pub fn footprint_radius(&self) -> f32 {
        let density = self.density.value();
        if density < 0.3 {
            0.12
        } else if density < 0.7 {
            0.10
        } else {
            0.13
        }
    }

    /// Returns the material's measured properties as a normalised 5-dimensional
    /// vector for cosine-similarity comparison (Story 10.5 — `SimilarTo` edges).
    ///
    /// Component order: `[density, thermal_resistance, reactivity, conductivity, toxicity]`.
    ///
    /// All values are in \[0.0, 1.0\] by construction (clamped at creation time
    /// in [`MaterialProperty::new`]), so the cosine similarity between any two
    /// vectors is always non-negative — no centring or normalisation required.
    ///
    /// The vector includes ALL five properties regardless of their visibility
    /// state. This is intentional: the knowledge graph is the simulation layer
    /// and operates on ground-truth data; the player only sees the similarity
    /// when they have sufficient observation confidence (checked at call site).
    pub fn property_vector(&self) -> [f32; PROPERTY_DIM] {
        [
            self.density.value(),
            self.thermal_resistance.value(),
            self.reactivity.value(),
            self.conductivity.value(),
            self.toxicity.value(),
        ]
    }
}

// ── Seed-derived helpers ─────────────────────────────────────────────────

/// Map a `u64` into the closed unit interval \[0.0, 1.0\].
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
        origin_planet_seed: None, // set at spawn time by world generation
        density: MaterialProperty::new(
            unit_interval_01(mix_seed(seed, MAT_DENSITY_CHANNEL)),
            PropertyVisibility::Hidden,
        ),
        thermal_resistance: MaterialProperty::new(
            unit_interval_01(mix_seed(seed, MAT_THERMAL_RESISTANCE_CHANNEL)),
            PropertyVisibility::Hidden,
        ),
        reactivity: MaterialProperty::new(
            unit_interval_01(mix_seed(seed, MAT_REACTIVITY_CHANNEL)),
            PropertyVisibility::Hidden,
        ),
        conductivity: MaterialProperty::new(
            unit_interval_01(mix_seed(seed, MAT_CONDUCTIVITY_CHANNEL)),
            PropertyVisibility::Hidden,
        ),
        toxicity: MaterialProperty::new(
            unit_interval_01(mix_seed(seed, MAT_TOXICITY_CHANNEL)),
            PropertyVisibility::Hidden,
        ),
    }
}

// ── Catalog resource ─────────────────────────────────────────────────────

/// All loaded material definitions, keyed by name.
///
/// Later stories use this to spawn material entities and to look up base
/// definitions during fabrication.
#[derive(Resource, Debug, Default)]
pub struct MaterialCatalog {
    /// Primary index: seed → material.
    by_seed: HashMap<MaterialSeed, GameMaterial>,
    /// Secondary index: name → seed (for name-based lookups).
    by_name: HashMap<String, MaterialSeed>,
}

impl MaterialCatalog {
    /// Derive a material from a seed and register it in the catalog, returning a
    /// reference to the (possibly already-present) entry.
    ///
    /// If a material with the same **seed** already exists, returns the existing
    /// entry unchanged.  If the procedurally generated name collides with a
    /// *different* seed's material, a deterministic disambiguator derived from
    /// the seed is appended (e.g. `"Vexorite-a3f1"`) until the name is unique.
    pub fn derive_and_register(&mut self, seed: MaterialSeed) -> &GameMaterial {
        // Fast path: already registered by seed — O(1) lookup.
        if self.by_seed.contains_key(&seed) {
            return &self.by_seed[&seed];
        }

        let mut mat = derive_material_from_seed(seed.0);
        mat.name = Self::disambiguated_name(&mat.name, seed, &self.by_name);
        self.by_name.insert(mat.name.clone(), seed);
        self.by_seed.insert(seed, mat);
        &self.by_seed[&seed]
    }

    /// Register a pre-built material (e.g. from the fabricator) in the catalog.
    ///
    /// If a material with the same **seed** already exists, the existing entry is
    /// kept unchanged and a reference to it is returned.  Otherwise the supplied
    /// material is inserted after applying name disambiguation, and a reference
    /// to the newly-inserted entry is returned.
    pub fn register_fabricated(&mut self, mut mat: GameMaterial) -> &GameMaterial {
        let key = MaterialSeed(mat.seed);
        if self.by_seed.contains_key(&key) {
            return &self.by_seed[&key];
        }

        mat.name = Self::disambiguated_name(&mat.name, key, &self.by_name);
        self.by_name.insert(mat.name.clone(), key);
        self.by_seed.insert(key, mat);
        &self.by_seed[&key]
    }

    /// Look up a material by its seed, returning `None` if not yet registered.
    #[allow(dead_code)]
    pub fn get_by_seed(&self, seed: MaterialSeed) -> Option<&GameMaterial> {
        self.by_seed.get(&seed)
    }

    /// Look up a material by its display name, returning `None` if not found.
    pub fn get_by_name(&self, name: &str) -> Option<&GameMaterial> {
        self.by_name
            .get(name)
            .and_then(|seed| self.by_seed.get(seed))
    }

    /// Returns the number of materials in the catalog.
    #[allow(dead_code)]
    pub fn len(&self) -> usize {
        self.by_seed.len()
    }

    /// Returns `true` if the catalog contains no materials.
    #[allow(dead_code)]
    pub fn is_empty(&self) -> bool {
        self.by_seed.is_empty()
    }

    /// Iterate over all materials in the catalog.
    #[allow(dead_code)]
    pub fn values(&self) -> impl Iterator<Item = &GameMaterial> {
        self.by_seed.values()
    }

    /// Iterate over all material names in the catalog.
    pub fn names(&self) -> impl Iterator<Item = &String> {
        self.by_name.keys()
    }

    /// Iterate over all seeds in the catalog.
    #[allow(dead_code)]
    pub fn seeds(&self) -> impl Iterator<Item = &MaterialSeed> {
        self.by_seed.keys()
    }

    /// Return `base_name` if it is not already taken, otherwise append a short
    /// hex suffix derived deterministically from `seed`.
    ///
    /// The suffix is produced by taking successive 16-bit windows of the seed
    /// (formatted as lowercase hex).  In the astronomically unlikely case that
    /// *all* eight 16-bit windows also collide, we fall back to the full 16-hex
    /// seed representation which is unique by definition (different seeds).
    fn disambiguated_name(
        base_name: &str,
        seed: MaterialSeed,
        existing_names: &HashMap<String, MaterialSeed>,
    ) -> String {
        if !existing_names.contains_key(base_name) {
            return base_name.to_owned();
        }

        // Try successive 16-bit windows of the seed as a 4-hex-char suffix.
        for shift in (0..64).step_by(16) {
            let fragment = (seed.0 >> shift) as u16;
            let candidate = format!("{base_name}-{fragment:04x}");
            if !existing_names.contains_key(&candidate) {
                return candidate;
            }
        }

        // Ultimate fallback: full seed hex (guaranteed unique for distinct seeds).
        format!("{base_name}-{:016x}", seed.0)
    }
}

// ── World-object marker ──────────────────────────────────────────────────

/// Marks an entity as a material object that exists physically in the world.
/// The material's data is on the same entity as a [`GameMaterial`] component.
#[derive(Component, Debug)]
pub struct MaterialObject;

// ── Loading ──────────────────────────────────────────────────────────────

/// Initializes an empty [`MaterialCatalog`].
///
/// Materials are no longer loaded from TOML files at startup. Instead, the
/// catalog starts empty and grows as the player explores — biome palettes
/// define which material seeds appear in each region, and
/// [`MaterialCatalog::derive_and_register`] inserts them on first encounter.
fn load_material_catalog(mut commands: Commands) {
    let mut catalog = MaterialCatalog::default();

    // Pre-seed the catalog with the 10 well-known materials so that indoor
    // scenes (which spawn material objects at PostStartup) have something to
    // display before exterior chunk generation populates the catalog further.
    for mat in WellKnownMaterial::all() {
        catalog.derive_and_register(MaterialSeed(mat.seed()));
    }

    info!(
        "Material catalog initialized with {} well-known starter materials",
        catalog.len()
    );
    commands.insert_resource(catalog);
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn prop(value: f32, visibility: PropertyVisibility) -> MaterialProperty {
        MaterialProperty::new(value, visibility)
    }

    fn sample_material() -> GameMaterial {
        GameMaterial {
            name: "Ferrite".into(),
            seed: 1001,
            color: [0.58, 0.55, 0.52],
            origin_planet_seed: None,
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
        light.density.set_value(0.2);
        assert!((light.support_height() - 0.12).abs() < f32::EPSILON);

        let mut medium = sample_material();
        medium.density.set_value(0.5);
        assert!((medium.support_height() - 0.17).abs() < f32::EPSILON);

        let heavy = sample_material();
        assert!((heavy.support_height() - 0.09).abs() < f32::EPSILON);
    }

    #[test]
    fn footprint_radius_matches_density_mesh_shape() {
        let mut light = sample_material();
        light.density.set_value(0.2);
        assert!((light.footprint_radius() - 0.12).abs() < f32::EPSILON);

        let mut medium = sample_material();
        medium.density.set_value(0.5);
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
        assert!((parsed.density.value() - original.density.value()).abs() < f32::EPSILON);
        assert_eq!(
            parsed.thermal_resistance.visibility,
            PropertyVisibility::Hidden
        );
    }

    #[test]
    fn property_values_clamped_to_unit_range() {
        let over = prop(1.5, PropertyVisibility::Observable);
        let under = prop(-0.3, PropertyVisibility::Hidden);
        assert!((over.value() - 1.0).abs() < f32::EPSILON);
        assert!(under.value().abs() < f32::EPSILON);
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
        assert!((first.density.value() - second.density.value()).abs() < f32::EPSILON);
        assert!((first.reactivity.value() - second.reactivity.value()).abs() < f32::EPSILON);
        assert!((first.conductivity.value() - second.conductivity.value()).abs() < f32::EPSILON);
    }

    #[test]
    fn catalog_default_is_empty() {
        let catalog = MaterialCatalog::default();
        assert!(catalog.is_empty());
    }

    // ── derive_material_from_seed tests ──────────────────────────────────

    #[test]
    fn derive_material_deterministic() {
        let a = derive_material_from_seed(0xDEAD_BEEF);
        let b = derive_material_from_seed(0xDEAD_BEEF);
        assert_eq!(a.name, b.name);
        assert_eq!(a.seed, b.seed);
        assert!((a.density.value() - b.density.value()).abs() < f32::EPSILON);
        assert!((a.thermal_resistance.value() - b.thermal_resistance.value()).abs() < f32::EPSILON);
        assert!((a.reactivity.value() - b.reactivity.value()).abs() < f32::EPSILON);
        assert!((a.conductivity.value() - b.conductivity.value()).abs() < f32::EPSILON);
        assert!((a.toxicity.value() - b.toxicity.value()).abs() < f32::EPSILON);
        assert_eq!(a.color, b.color);
    }

    #[test]
    fn derive_material_different_seeds_differ() {
        let a = derive_material_from_seed(1);
        let b = derive_material_from_seed(2);
        // With good mixing, at least one property should differ.
        let same_density = (a.density.value() - b.density.value()).abs() < f32::EPSILON;
        let same_reactivity = (a.reactivity.value() - b.reactivity.value()).abs() < f32::EPSILON;
        let same_conductivity =
            (a.conductivity.value() - b.conductivity.value()).abs() < f32::EPSILON;
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
                ("density", mat.density.value()),
                ("thermal_resistance", mat.thermal_resistance.value()),
                ("reactivity", mat.reactivity.value()),
                ("conductivity", mat.conductivity.value()),
                ("toxicity", mat.toxicity.value()),
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
            unique_density.insert(mat.density.value().to_bits());
            unique_thermal.insert(mat.thermal_resistance.value().to_bits());
            unique_reactivity.insert(mat.reactivity.value().to_bits());
            unique_conductivity.insert(mat.conductivity.value().to_bits());
            unique_toxicity.insert(mat.toxicity.value().to_bits());
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
                let all_same = a.density.value().to_bits() == b.density.value().to_bits()
                    && a.thermal_resistance.value().to_bits()
                        == b.thermal_resistance.value().to_bits()
                    && a.reactivity.value().to_bits() == b.reactivity.value().to_bits()
                    && a.conductivity.value().to_bits() == b.conductivity.value().to_bits()
                    && a.toxicity.value().to_bits() == b.toxicity.value().to_bits()
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
        let name1 = catalog.derive_and_register(MaterialSeed(42)).name.clone();
        let name2 = catalog.derive_and_register(MaterialSeed(42)).name.clone();
        assert_eq!(name1, name2);
        assert_eq!(catalog.len(), 1);
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
        catalog
            .by_name
            .insert(base_name.clone(), MaterialSeed(0xBEEF));
        catalog.by_seed.insert(MaterialSeed(0xBEEF), imposter);

        let registered = catalog.derive_and_register(MaterialSeed(999));
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
        assert_eq!(catalog.len(), 2);
    }

    #[test]
    fn disambiguated_name_no_collision_returns_base() {
        let existing = HashMap::new();
        let result = MaterialCatalog::disambiguated_name("Vexorite", MaterialSeed(42), &existing);
        assert_eq!(result, "Vexorite");
    }

    #[test]
    fn disambiguated_name_with_collision_appends_suffix() {
        let mut existing: HashMap<String, MaterialSeed> = HashMap::new();
        existing.insert("Vexorite".to_string(), MaterialSeed(0xAAAA));
        let result = MaterialCatalog::disambiguated_name(
            "Vexorite",
            MaterialSeed(0x1234_5678_9ABC_DEF0),
            &existing,
        );
        assert_eq!(result, "Vexorite-def0");
    }

    #[test]
    fn derive_and_register_1000_seeds_no_duplicate_names() {
        // With only 4 096 possible base names (16³), 1 000 seeds are expected
        // to produce raw collisions.  `derive_and_register` must disambiguate
        // every collision so the catalog never contains duplicate names.
        let mut catalog = MaterialCatalog::default();

        // Use a deterministic spread across the u64 range.
        let seeds: Vec<u64> = (0u64..1000)
            .map(|i| i.wrapping_mul(0x9E37_79B9_7F4A_7C15).wrapping_add(1))
            .collect();

        for &seed in &seeds {
            catalog.derive_and_register(MaterialSeed(seed));
        }

        // Every entry in the catalog must have a unique name (dual-index
        // guarantees this structurally, but verify the count matches).
        assert_eq!(
            catalog.len(),
            1000,
            "catalog should contain exactly 1000 materials after 1000 unique seeds"
        );

        // Double-check: collect all names into a HashSet and confirm no loss.
        let unique_names: std::collections::HashSet<&String> = catalog.names().collect();
        assert_eq!(
            unique_names.len(),
            1000,
            "all 1000 registered material names must be unique"
        );
    }

    #[test]
    fn disambiguated_name_deterministic() {
        let mut existing: HashMap<String, MaterialSeed> = HashMap::new();
        existing.insert("Coranite".to_string(), MaterialSeed(0xBBBB));
        let a = MaterialCatalog::disambiguated_name("Coranite", MaterialSeed(777), &existing);
        let b = MaterialCatalog::disambiguated_name("Coranite", MaterialSeed(777), &existing);
        assert_eq!(a, b);
    }

    /// Defense-in-depth check for the compile-time uniqueness assertion above.
    ///
    /// The const assertion fails the build before tests can run when two variants
    /// share a seed. This test keeps a human-readable regression check in the test
    /// suite and verifies that the canonical 10 starter seeds are still represented
    /// exactly once.
    #[test]
    fn well_known_seeds_are_unique() {
        let mut seen: std::collections::HashMap<u64, &'static str> =
            std::collections::HashMap::new();

        for &wk in WellKnownMaterial::all() {
            let seed = wk.seed();
            if let Some(previous_label) = seen.insert(seed, wk.display_name()) {
                panic!(
                    "duplicate WellKnownMaterial seed {seed} produced by both \
                     {previous_label} and {}",
                    wk.display_name(),
                );
            }
        }

        assert_eq!(
            WellKnownMaterial::all().len(),
            10,
            "WellKnownMaterial must continue to expose exactly 10 starter variants",
        );
        assert_eq!(
            seen.len(),
            10,
            "all 10 WellKnownMaterial seeds must appear exactly once",
        );

        for expected_seed in 1001_u64..=1010_u64 {
            assert!(
                seen.contains_key(&expected_seed),
                "canonical WellKnownMaterial seed {expected_seed} is missing",
            );
        }
    }

    /// Verifies that the 10 well-known material seeds each produce a material
    /// with reasonable, non-degenerate properties.  The derived values will NOT
    /// match the old hand-authored TOML values — that's expected.  What matters
    /// is that every property falls in `[0.0, 1.0]`, that no two well-known
    /// seeds collide on all properties, and that names are non-empty.
    #[test]
    fn well_known_seeds_produce_reasonable_materials() {
        let materials: Vec<(WellKnownMaterial, GameMaterial)> = WellKnownMaterial::all()
            .iter()
            .map(|&wk| (wk, derive_material_from_seed(wk.seed())))
            .collect();

        for (wk, mat) in &materials {
            let label = wk.display_name();
            let seed = wk.seed();

            // Seed round-trips.
            assert_eq!(
                mat.seed, seed,
                "{label}: seed not preserved (expected {seed}, got {})",
                mat.seed
            );

            // Name is non-empty.
            assert!(
                !mat.name.is_empty(),
                "{label} (seed {seed}): derived name is empty"
            );

            // Every scalar property is in the valid unit interval [0, 1].
            let props = [
                ("density", mat.density.value),
                ("thermal_resistance", mat.thermal_resistance.value),
                ("reactivity", mat.reactivity.value),
                ("conductivity", mat.conductivity.value),
                ("toxicity", mat.toxicity.value),
            ];
            for (prop_name, val) in &props {
                assert!(
                    (0.0..=1.0).contains(val),
                    "{label} (seed {seed}): {prop_name} out of range: {val}"
                );
            }

            // Color channels in [0, 1].
            for (ch, &val) in ["R", "G", "B"].iter().zip(mat.color.iter()) {
                assert!(
                    (0.0..=1.0).contains(&val),
                    "{label} (seed {seed}): color {ch} out of range: {val}"
                );
            }

            // All properties start hidden.
            assert_eq!(mat.density.visibility, PropertyVisibility::Hidden);
            assert_eq!(
                mat.thermal_resistance.visibility,
                PropertyVisibility::Hidden
            );
            assert_eq!(mat.reactivity.visibility, PropertyVisibility::Hidden);
            assert_eq!(mat.conductivity.visibility, PropertyVisibility::Hidden);
            assert_eq!(mat.toxicity.visibility, PropertyVisibility::Hidden);
        }

        // No two well-known materials share every property (uniqueness).
        for i in 0..materials.len() {
            for j in (i + 1)..materials.len() {
                let (wk_a, a) = &materials[i];
                let (wk_b, b) = &materials[j];
                let all_same = a.density.value().to_bits() == b.density.value().to_bits()
                    && a.thermal_resistance.value().to_bits()
                        == b.thermal_resistance.value().to_bits()
                    && a.reactivity.value().to_bits() == b.reactivity.value().to_bits()
                    && a.conductivity.value().to_bits() == b.conductivity.value().to_bits()
                    && a.toxicity.value().to_bits() == b.toxicity.value().to_bits();
                assert!(
                    !all_same,
                    "well-known seeds {} ({}) and {} ({}) produced identical properties",
                    a.seed,
                    wk_a.display_name(),
                    b.seed,
                    wk_b.display_name(),
                );
            }
        }

        // Spot-check: across 10 materials we expect meaningful spread.
        let unique_densities: std::collections::HashSet<u32> = materials
            .iter()
            .map(|(_, m)| m.density.value().to_bits())
            .collect();
        assert!(
            unique_densities.len() >= 5,
            "density spread too narrow: only {} distinct values among 10 well-known seeds",
            unique_densities.len()
        );
    }

    /// Verifies that every well-known seed material derives a distinct name.
    /// Duplicate names would confuse the player and break the journal/catalog UX.
    #[test]
    fn well_known_seeds_have_distinct_names() {
        let mut seen: std::collections::HashMap<String, (&'static str, u64)> =
            std::collections::HashMap::new();
        for &wk in WellKnownMaterial::all() {
            let mat = derive_material_from_seed(wk.seed());
            if let Some(&(prev_label, prev_seed)) = seen.get(&mat.name) {
                panic!(
                    "name collision: \"{}\") produced by both {} (seed {:#X}) and {} (seed {:#X})",
                    mat.name,
                    prev_label,
                    prev_seed,
                    wk.display_name(),
                    wk.seed(),
                );
            }
            seen.insert(mat.name.clone(), (wk.display_name(), wk.seed()));
        }
    }

    /// Verifies that `load_material_catalog` pre-seeds the catalog with the
    /// well-known materials so that indoor scene spawning (which runs at
    /// `PostStartup`) has materials to display before exterior chunk generation.
    #[test]
    fn catalog_pre_seeded_with_well_known_materials() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(PreStartup, load_material_catalog);
        app.update();

        let catalog = app
            .world()
            .get_resource::<MaterialCatalog>()
            .expect("MaterialCatalog resource must exist after startup");
        assert_eq!(
            catalog.len(),
            WellKnownMaterial::all().len(),
            "catalog must contain exactly the well-known starter materials",
        );
        for &wk in WellKnownMaterial::all() {
            let seed = wk.seed();
            assert!(
                catalog.get_by_seed(MaterialSeed(seed)).is_some(),
                "well-known seed {seed} must be present in the catalog after startup",
            );
        }
    }
}
