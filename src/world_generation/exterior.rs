//! Deterministic exterior baseline generation for Epic 5.
//!
//! Story 5.1 established "which world / chunk are we talking about?"
//! Story 5.2 answers the next question:
//! "what untouched baseline content appears in that chunk?"
//! Story 5.3 makes placement surface-aware: instead of assuming the exterior is
//! flat, every placement candidate queries a [`SurfaceProvider`] for the actual
//! surface height and normal at that point, and rejects locations that are too
//! steep or invalid.
//!
//! ## Why placement does not assume y = 0
//!
//! The generation functions never read a hardcoded height. They call
//! `surface.query_surface(x, z)` and use the returned `position_y`. The current
//! live implementation is [`FlatSurface`] which returns a constant y, but the
//! generation code does not know or care about that. When non-flat terrain
//! arrives, a different [`SurfaceProvider`] slots in and placement keeps working.
//!
//! ## Which functions operate in sampling space vs world space
//!
//! - `generate_surface_mineral_deposit_sites`: iterates a grid in **world space**
//!   (chunk origin + cell offsets in world units). Each candidate position is a
//!   world-space X/Z coordinate passed directly to the surface provider.
//! - `expand_deposit_site_into_cluster`: computes child positions in **world
//!   space** relative to the site center. Each child queries the surface
//!   independently because the surface may vary across the deposit radius.
//! - `continuous_value_field_01`: operates in a **sampling space** scaled by
//!   `site_density_field_scale_world_units`. The input is a world-space X/Z
//!   divided by the scale factor; the output is a [0, 1] density value with no
//!   spatial unit.
//!
//! The first generated exterior object type is intentionally narrow:
//! a surface mineral deposit. That choice is not arbitrary. It matches the
//! current gameplay loop the player actually has today:
//! look at something on the ground, pick it up, and carry it somewhere useful.
//!
//! This file is heavily commented because the next reader is likely to ask:
//! - why do adjacent chunks feel related but not identical?
//! - what part is deterministic baseline generation versus later persistence?
//! - why does a generated deposit have this identity?
//! - why is there a separate deposit definition file instead of inline constants?

use std::collections::{HashMap, HashSet};
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use super::{
    ActiveChunkNeighborhood, BiomeRegistry, ChunkBiome, ChunkCoord,
    DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS, GeneratedObjectId, PaletteMaterial, PlanetSurface,
    SurfaceProvider, WorldGenerationConfig, WorldProfile, chunk_origin_xz, derive_chunk_biome,
    derive_chunk_generation_key, derive_generated_object_id, generate_chunk_heightmap_mesh,
    is_placement_valid, surface_alignment_rotation, world_position_to_chunk_coord,
};
use crate::carry::InCarry;
use crate::interaction::HeldItem;
use crate::materials::{GameMaterial, MaterialCatalog, MaterialObject};
use crate::scene::{ExteriorGroundPatch, PositionXZ, RectXZ};
use crate::seed_util::lerp;

const DEPOSIT_CONFIG_PATH: &str = "assets/exterior/surface_mineral_deposits.toml";
const SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION: u32 = 1;

/// Plugin that registers exterior terrain generation, mineral deposits, and chunk spawn/despawn systems.
pub struct ExteriorGenerationPlugin;

impl Plugin for ExteriorGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurfaceMineralDepositCatalog>()
            .init_resource::<ActiveExteriorChunkSpawns>()
            .init_resource::<ChunkRemovalDeltas>()
            .init_resource::<ChunkPlayerAdditions>()
            .init_resource::<PlayerAddedIdCounter>()
            .add_systems(PreStartup, load_surface_mineral_deposit_catalog)
            .add_systems(
                Update,
                (
                    sync_active_exterior_chunks,
                    release_collected_generated_objects.after(sync_active_exterior_chunks),
                    release_collected_player_added_objects.after(sync_active_exterior_chunks),
                    claim_exterior_drops
                        .after(release_collected_generated_objects)
                        .after(release_collected_player_added_objects),
                ),
            );
    }
}

/// One concrete generated exterior object definition.
///
/// The object type is always "surface mineral deposit". The definition tells us
/// *which* deposit flavor this is:
/// - how likely it is relative to sibling deposit definitions
/// - how large it can appear
///
/// Deposit definitions are material-agnostic: they define *how* a deposit
/// looks (shape, clustering), not *what* material it contains. Material
/// selection is driven by the biome's `material_palette`.
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SurfaceMineralDepositDefinition {
    pub key: String,
    pub selection_weight: f32,
    pub scale_min: f32,
    pub scale_max: f32,
    pub deposit_radius_min: f32,
    pub deposit_radius_max: f32,
    pub child_count_min: u32,
    pub child_count_max: u32,
    pub cluster_compactness: f32,
}

/// Dedicated data source for Story 5.2 baseline exterior generation.
///
/// The story explicitly calls for a separate exterior-object data file instead
/// of hiding this in generator constants. We also keep the candidate spacing and
/// threshold here because they are part of "what this exterior object family
/// looks like in the world" rather than generic world foundation config.
#[derive(Clone, Debug, PartialEq, Resource, Serialize, Deserialize)]
struct SurfaceMineralDepositCatalog {
    #[serde(default = "default_site_spacing_world_units")]
    pub site_spacing_world_units: f32,
    #[serde(default = "default_site_density_field_scale_world_units")]
    pub site_density_field_scale_world_units: f32,
    #[serde(default = "default_site_spawn_threshold")]
    pub site_spawn_threshold: f32,
    #[serde(default = "default_site_jitter_fraction")]
    pub site_jitter_fraction: f32,
    #[serde(default = "default_site_min_gap_world_units")]
    pub site_min_gap_world_units: f32,
    #[serde(default = "default_surface_mineral_deposits")]
    pub deposits: Vec<SurfaceMineralDepositDefinition>,
}

impl Default for SurfaceMineralDepositCatalog {
    fn default() -> Self {
        Self {
            site_spacing_world_units: default_site_spacing_world_units(),
            site_density_field_scale_world_units: default_site_density_field_scale_world_units(),
            site_spawn_threshold: default_site_spawn_threshold(),
            site_jitter_fraction: default_site_jitter_fraction(),
            site_min_gap_world_units: default_site_min_gap_world_units(),
            deposits: default_surface_mineral_deposits(),
        }
    }
}

fn default_site_spacing_world_units() -> f32 {
    11.0
}

fn default_site_density_field_scale_world_units() -> f32 {
    24.0
}

fn default_site_spawn_threshold() -> f32 {
    0.55
}

fn default_site_jitter_fraction() -> f32 {
    0.28
}

fn default_site_min_gap_world_units() -> f32 {
    2.5
}

fn default_surface_mineral_deposits() -> Vec<SurfaceMineralDepositDefinition> {
    vec![
        SurfaceMineralDepositDefinition {
            key: "dense_cluster_deposit".into(),
            selection_weight: 1.0,
            scale_min: 0.9,
            scale_max: 1.2,
            deposit_radius_min: 2.2,
            deposit_radius_max: 3.4,
            child_count_min: 5,
            child_count_max: 9,
            cluster_compactness: 0.75,
        },
        SurfaceMineralDepositDefinition {
            key: "scattered_cluster_deposit".into(),
            selection_weight: 0.8,
            scale_min: 0.85,
            scale_max: 1.15,
            deposit_radius_min: 1.8,
            deposit_radius_max: 3.0,
            child_count_min: 4,
            child_count_max: 7,
            cluster_compactness: 0.68,
        },
        SurfaceMineralDepositDefinition {
            key: "compact_cluster_deposit".into(),
            selection_weight: 0.45,
            scale_min: 0.8,
            scale_max: 1.05,
            deposit_radius_min: 1.4,
            deposit_radius_max: 2.2,
            child_count_min: 3,
            child_count_max: 5,
            cluster_compactness: 0.82,
        },
    ]
}

/// Runtime marker for a generated baseline object still owned by chunk generation.
///
/// This ownership marker matters because Story 5.2 deliberately stops short of
/// persistence. Once the player picks up a generated deposit, it stops being an
/// untouched chunk-baseline object and should no longer be managed by chunk
/// loading/unloading rules.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct GeneratedExteriorObject {
    pub home_chunk: ChunkCoord,
}

/// The first generated exterior object type.
///
/// We keep this component explicit, even though the same entity also carries a
/// `GameMaterial`, because the deposit is an exterior-world thing in its own
/// right. Future stories can persist, remove, or restyle deposits by querying
/// this exterior-specific role instead of pretending they are just anonymous
/// loose material objects.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct SurfaceMineralDeposit {
    pub definition_key: String,
}

/// Stable identity for one deterministic deposit site.
///
/// Story 5.2 scattered independent child objects. Story 5.2b adds a site layer
/// above them so the world can say "this whole Ferrite patch is one deposit"
/// instead of pretending the individual loose pieces are unrelated.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Component)]
struct GeneratedDepositSiteId {
    pub planet_seed: u64,
    pub chunk_coord: ChunkCoord,
    pub definition_key: String,
    pub local_site_index: u32,
    pub generator_version: u32,
}

/// Marker connecting a generated child mineral back to its parent deposit site.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct DepositSiteMember {
    pub site_id: GeneratedDepositSiteId,
    pub local_child_index: u32,
}

/// Baseline generated placement before it becomes a live Bevy entity.
///
/// This separation is important for testing. The deterministic generation logic
/// should be testable as pure Rust data without rendering, ECS world setup, or
/// scene spawning.
///
/// Story 5.3 added `surface_normal` so placed objects can align to the surface
/// they rest on. For a flat surface this is always `[0, 1, 0]`; for slopes the
/// object tilts to match.
#[derive(Clone, Debug, PartialEq)]
struct GeneratedSurfaceMineralPlacement {
    generated_id: GeneratedObjectId,
    deposit_site_id: GeneratedDepositSiteId,
    definition_key: String,
    material_seed: u64,
    position_xz: PositionXZ,
    surface_y: f32,
    /// Surface normal at the placement point, used for object alignment.
    surface_normal: [f32; 3],
    visual_scale: f32,
    local_child_index: u32,
}

/// Deterministic deposit site before child minerals are expanded around it.
///
/// The `surface_y` and `surface_normal` here are for the site center. Individual
/// child minerals query the surface independently because the surface may vary
/// across the deposit radius (especially on non-flat terrain).
#[derive(Clone, Debug, PartialEq)]
struct GeneratedSurfaceMineralDepositSite {
    site_id: GeneratedDepositSiteId,
    definition_key: String,
    material_seed: u64,
    center_xz: PositionXZ,
    radius_world_units: f32,
    child_count: u32,
    surface_y: f32,
    surface_normal: [f32; 3],
    scale_min: f32,
    scale_max: f32,
    cluster_compactness: f32,
}

/// Active chunk baseline entities currently spawned into the world.
#[derive(Resource, Default)]
struct ActiveExteriorChunkSpawns {
    pub spawned_entities_by_chunk: HashMap<ChunkCoord, Vec<Entity>>,
}

/// Tracks which generated baseline objects have been removed from each chunk.
///
/// ## Baseline vs Delta: the core persistence model
///
/// Every chunk has a *baseline*: the deterministic set of objects produced by
/// `generate_surface_mineral_chunk_baseline()` for a given world seed. The
/// baseline is always reproducible from the seed alone — it is never mutated.
///
/// A *delta* is the set of player-caused modifications layered on top of the
/// baseline. For Story 5.4, the only delta type is *removal*: the player picked
/// up a generated object, so it should not reappear when the chunk reloads.
///
/// The pipeline is: **generate baseline → apply removal deltas → spawn survivors**.
///
/// ## Why filter after generation instead of changing the seed?
///
/// The baseline generation is deterministic and depends only on the world seed
/// and chunk coordinate. If we tried to bake removals into the seed, every
/// removal would cascade into a completely different baseline for the entire
/// chunk. Instead, we generate the full baseline and then subtract removals.
/// This keeps the baseline stable and makes deltas composable.
///
/// ## Why not use runtime Entity IDs?
///
/// Entity IDs are ephemeral — they change every time the chunk is spawned.
/// [`GeneratedObjectId`] is deterministic: it encodes the generator version,
/// chunk coordinate, and candidate index, so the same logical object gets the
/// same ID across chunk load/unload cycles.
///
/// ## Serialization readiness
///
/// This resource derives `Serialize` and `Deserialize` so it can be written to
/// a save file in a future story. Story 5.4 only keeps the data in memory;
/// save/load is deferred to later persistence work.
#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
struct ChunkRemovalDeltas {
    /// For each chunk, the set of `GeneratedObjectId`s that the player has
    /// removed (e.g. by picking up). These IDs are filtered out of the baseline
    /// during chunk spawning so the objects stay gone.
    pub removed_by_chunk: HashMap<ChunkCoord, HashSet<GeneratedObjectId>>,
}

// ── Story 5.5: Player-added exterior objects ─────────────────────────────
//
// ## Generated world state vs player-authored world state
//
// Generated objects come from the deterministic baseline: given a seed and a
// chunk coordinate, the same objects always appear. Their identity
// (`GeneratedObjectId`) encodes that determinism — planet seed, chunk, index.
//
// Player-added objects are the opposite: they exist because a human chose to
// drop something at a specific location. They cannot be reconstructed from the
// world seed. They must be explicitly recorded and replayed.
//
// The two identity models are deliberately separate types. Conflating them
// would create a category error: a player-added object has no candidate index
// or generator version because it was never generated by a generator.
//
// ## Final chunk state composition pipeline
//
// The runtime assembly order is:
//   1. Generate deterministic baseline for the chunk
//   2. Subtract removal deltas (Story 5.4) — objects the player picked up
//   3. Append player additions (Story 5.5) — objects the player dropped there
//
// This three-layer model (baseline - removals + additions) is the canonical
// runtime chunk state. Future save/load stories serialize these layers; the
// generation code itself never changes.

/// Monotonic counter for assigning unique IDs to player-added exterior objects.
///
/// Unlike [`GeneratedObjectId`], which is derived deterministically from the
/// world seed, player-added IDs are simply sequential. They only need to be
/// unique within a single session (and eventually within a save file).
#[derive(Resource, Default)]
struct PlayerAddedIdCounter {
    next_id: u64,
}

impl PlayerAddedIdCounter {
    fn next(&mut self) -> u64 {
        let id = self.next_id;
        self.next_id += 1;
        id
    }
}

/// A snapshot of a player-added object sufficient to respawn it on chunk reload.
///
/// This record stores everything needed to recreate the object's visual and
/// logical presence: the full `GameMaterial` (not just a catalog key, because
/// the object might be a fabricated material that does not exist in the catalog),
/// the world-space position, and the visual scale.
///
/// ## Why store the full GameMaterial?
///
/// A generated baseline object can be reconstructed from a catalog key because
/// the catalog is data-driven and deterministic. But a player-dropped object
/// might be a fabricated material with procedurally generated properties that
/// exist nowhere else. Storing the full material ensures the object can always
/// be faithfully recreated.
///
/// ## Serialization readiness
///
/// This struct derives `Serialize` and `Deserialize` so a future save-file
/// story can persist it without any semantic rewrite. Story 5.5 keeps the data
/// in memory only.
#[derive(Clone, Debug, Serialize, Deserialize)]
struct PlayerAddedObjectRecord {
    /// Unique ID for this player-added object (monotonic, session-scoped).
    pub id: u64,
    /// The full material data — may be a base or fabricated material.
    pub material: GameMaterial,
    /// World-space position where the object was placed.
    pub position: [f32; 3],
    /// Visual scale factor (uniform).
    pub visual_scale: f32,
}

/// Tracks player-added objects per chunk — the additive layer of chunk state.
///
/// ## Relationship to ChunkRemovalDeltas
///
/// `ChunkRemovalDeltas` is subtractive: it records what was taken away from the
/// generated baseline. `ChunkPlayerAdditions` is additive: it records what the
/// player brought to the chunk. Together with the baseline, they form the
/// complete runtime chunk state:
///
/// ```text
/// final_chunk_state = baseline(seed, chunk)
///                   - removal_deltas[chunk]
///                   + player_additions[chunk]
/// ```
///
/// ## Why this is limited to dropped objects for now
///
/// Story 5.5 only records objects that the player drops (or places) in the
/// exterior. Future stories might add built structures, planted items, or other
/// categories. The data model is deliberately simple — a flat `Vec` of records
/// per chunk — to avoid over-engineering for unknown future categories.
#[derive(Resource, Default, Debug, Clone, Serialize, Deserialize)]
struct ChunkPlayerAdditions {
    /// For each chunk, the list of player-added objects. Order is insertion
    /// order (chronological), which is also the spawn order on reload.
    pub added_by_chunk: HashMap<ChunkCoord, Vec<PlayerAddedObjectRecord>>,
}

/// Runtime marker for a player-added exterior object managed by chunk state.
///
/// This is the additive counterpart to [`GeneratedExteriorObject`]. Where
/// `GeneratedExteriorObject` means "this entity came from deterministic
/// baseline generation," `PlayerAddedExteriorObject` means "this entity exists
/// because the player dropped something here."
///
/// The two markers are mutually exclusive on any given entity. An object starts
/// as generated (with `GeneratedExteriorObject`) or player-added (with this
/// component), never both.
///
/// When the player picks up a player-added object, this component is removed
/// and the object's record is deleted from `ChunkPlayerAdditions`. If the
/// player drops it again later (possibly in a different chunk), a new record
/// and a new `PlayerAddedExteriorObject` are created.
#[derive(Component, Clone, Debug, PartialEq, Eq)]
struct PlayerAddedExteriorObject {
    pub home_chunk: ChunkCoord,
    pub player_added_id: u64,
}

fn load_surface_mineral_deposit_catalog(mut commands: Commands) {
    let catalog = if Path::new(DEPOSIT_CONFIG_PATH).exists() {
        match fs::read_to_string(DEPOSIT_CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<SurfaceMineralDepositCatalog>(&contents) {
                Ok(catalog) => {
                    info!("Loaded surface mineral deposit catalog from {DEPOSIT_CONFIG_PATH}");
                    catalog
                }
                Err(error) => {
                    warn!("Malformed {DEPOSIT_CONFIG_PATH}, using defaults: {error}");
                    SurfaceMineralDepositCatalog::default()
                }
            },
            Err(error) => {
                warn!("Could not read {DEPOSIT_CONFIG_PATH}, using defaults: {error}");
                SurfaceMineralDepositCatalog::default()
            }
        }
    } else {
        warn!("{DEPOSIT_CONFIG_PATH} not found, using defaults");
        SurfaceMineralDepositCatalog::default()
    };

    commands.insert_resource(catalog);
}

// Bevy system — parameter count is driven by ECS query requirements, not design smell.
#[allow(clippy::too_many_arguments)]
fn sync_active_exterior_chunks(
    mut commands: Commands,
    active_chunks: Res<ActiveChunkNeighborhood>,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    deposit_catalog: Res<SurfaceMineralDepositCatalog>,
    mut material_catalog: ResMut<MaterialCatalog>,
    _exterior_patch: Res<ExteriorGroundPatch>,
    biome_registry: Res<BiomeRegistry>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut render_materials: ResMut<Assets<StandardMaterial>>,
    mut spawned_chunks: ResMut<ActiveExteriorChunkSpawns>,
    removal_deltas: Res<ChunkRemovalDeltas>,
    player_additions: Res<ChunkPlayerAdditions>,
    surface_registry: Res<crate::surface::SurfaceOverrideRegistry>,
    planet_env: Option<Res<crate::solar_system::PlanetEnvironment>>,
) {
    let Some(world_profile) = world_profile else {
        return;
    };
    let active_chunk_set: HashSet<ChunkCoord> = active_chunks.chunks.iter().copied().collect();
    let inactive_chunks: Vec<ChunkCoord> = spawned_chunks
        .spawned_entities_by_chunk
        .keys()
        .copied()
        .filter(|chunk| !active_chunk_set.contains(chunk))
        .collect();

    for chunk in inactive_chunks {
        if let Some(entities) = spawned_chunks.spawned_entities_by_chunk.remove(&chunk) {
            for entity in entities {
                commands.entity(entity).despawn();
            }
        }
    }

    // Build the surface provider for chunk generation.
    //
    // Story 5.3: the generation functions receive a &dyn SurfaceProvider so
    // they can be tested against synthetic surfaces without any Bevy dependency.
    //
    // Story 5a.3: at runtime we now use PlanetSurface, which samples
    // multi-octave elevation noise for realistic terrain variation. The
    // FlatSurface, SteppedSurface, and TiltedSurface providers remain
    // available for deterministic tests.
    let surface = PlanetSurface::new_from_profile(&world_profile, &world_gen_config);

    for &chunk in &active_chunks.chunks {
        if spawned_chunks
            .spawned_entities_by_chunk
            .contains_key(&chunk)
        {
            continue;
        }

        // Story 5a.2: derive the biome for this chunk. The biome determines
        // the ground tile color, deposit density modifier, and per-deposit
        // weight multipliers. All three feed into the generation pipeline
        // below so that different biomes produce visibly different exteriors.
        let chunk_biome = derive_chunk_biome(
            &world_profile,
            &biome_registry,
            chunk,
            planet_env.as_deref(),
        );
        trace!(
            chunk = ?chunk,
            biome = ?chunk_biome.biome_type,
            "derived biome for chunk"
        );

        let baseline_placements = generate_surface_mineral_chunk_baseline(
            &world_profile,
            &deposit_catalog,
            &surface,
            chunk,
            &chunk_biome,
        );

        // Story 5.4: apply removal deltas so picked-up objects stay gone.
        //
        // The baseline is the full deterministic set. We subtract any IDs the
        // player has removed. This is the core of the persistence-delta model:
        // the baseline never changes, and the delta is a subtractive overlay.
        let placements = apply_removal_deltas(
            baseline_placements,
            removal_deltas.removed_by_chunk.get(&chunk),
        );

        // UAT2: suppress deposits that fall inside a surface override (e.g.,
        // the room floor). Without this filter, mineral deposits spawn through
        // structure floors.
        let placements: Vec<_> = placements
            .into_iter()
            .filter(|p| !surface_registry.any_contains_xz(p.position_xz.x, p.position_xz.z))
            .collect();

        let mut spawned_entities = Vec::new();

        // Story 5a.3: spawn a per-chunk heightmap ground tile colored by the
        // biome.
        //
        // Each active chunk gets a subdivided heightmap mesh whose vertices
        // sample the planet elevation noise. The mesh vertices are in
        // world-space so the entity Transform is identity. The ground tile
        // color comes from the biome definition.
        //
        // These tile entities are tracked in `spawned_entities` alongside
        // material objects, so they are automatically despawned when the chunk
        // deactivates.
        {
            let [r, g, b] = chunk_biome.ground_color;
            let ground_material = render_materials.add(StandardMaterial {
                base_color: Color::srgb(r, g, b),
                perceptual_roughness: 0.98,
                ..default()
            });
            let ground_mesh = meshes.add(generate_chunk_heightmap_mesh(
                &surface,
                chunk,
                world_gen_config.elevation_subdivisions,
            ));
            let tile_entity = commands
                .spawn((
                    Mesh3d(ground_mesh),
                    MeshMaterial3d(ground_material),
                    Transform::default(),
                ))
                .id();
            spawned_entities.push(tile_entity);
        }

        for placement in placements {
            // Skip deposits with no material (biome had an empty palette).
            if placement.material_seed == 0 {
                continue;
            }

            let deposit_material = material_catalog
                .derive_and_register(placement.material_seed)
                .clone();
            let mesh = deposit_material.mesh_for_density(&mut meshes);
            let render_material = render_materials.add(StandardMaterial {
                base_color: deposit_material.bevy_color(),
                perceptual_roughness: 0.82,
                metallic: if deposit_material.conductivity.value > 0.6 {
                    0.35
                } else {
                    0.05
                },
                ..default()
            });

            // Story 5.3: compute the surface-aligned rotation so placed objects
            // lean naturally on slopes. On flat surfaces this is the identity
            // quaternion (no rotation).
            let [qx, qy, qz, qw] = surface_alignment_rotation(placement.surface_normal);
            let rotation = Quat::from_xyzw(qx, qy, qz, qw);

            let entity = commands
                .spawn((
                    MaterialObject,
                    deposit_material.clone(),
                    SurfaceMineralDeposit {
                        definition_key: placement.definition_key.clone(),
                    },
                    GeneratedExteriorObject { home_chunk: chunk },
                    DepositSiteMember {
                        site_id: placement.deposit_site_id.clone(),
                        local_child_index: placement.local_child_index,
                    },
                    placement.generated_id.clone(),
                    Mesh3d(mesh),
                    MeshMaterial3d(render_material),
                    Transform::from_xyz(
                        placement.position_xz.x,
                        deposit_material.resting_center_y(placement.surface_y),
                        placement.position_xz.z,
                    )
                    .with_rotation(rotation)
                    .with_scale(Vec3::splat(placement.visual_scale)),
                ))
                .id();

            spawned_entities.push(entity);
        }

        // Story 5.5: append player-added objects — the additive layer.
        //
        // After the generated baseline (minus removals), we spawn any objects
        // the player previously dropped in this chunk. These are reconstructed
        // from `PlayerAddedObjectRecord` snapshots, not from the world seed.
        // This is the third and final layer of the chunk state composition:
        //   baseline - removals + player_additions
        if let Some(records) = player_additions.added_by_chunk.get(&chunk) {
            for record in records {
                let mesh = record.material.mesh_for_density(&mut meshes);
                let render_material = render_materials.add(StandardMaterial {
                    base_color: record.material.bevy_color(),
                    perceptual_roughness: 0.82,
                    metallic: if record.material.conductivity.value > 0.6 {
                        0.35
                    } else {
                        0.05
                    },
                    ..default()
                });

                let entity = commands
                    .spawn((
                        MaterialObject,
                        record.material.clone(),
                        PlayerAddedExteriorObject {
                            home_chunk: chunk,
                            player_added_id: record.id,
                        },
                        Mesh3d(mesh),
                        MeshMaterial3d(render_material),
                        Transform::from_xyz(
                            record.position[0],
                            record.position[1],
                            record.position[2],
                        )
                        .with_scale(Vec3::splat(record.visual_scale)),
                    ))
                    .id();

                spawned_entities.push(entity);
            }
        }

        spawned_chunks
            .spawned_entities_by_chunk
            .insert(chunk, spawned_entities);
    }
}

// Bevy system — the Query tuple is wide because we need all components for re-parenting.
#[allow(clippy::type_complexity)]
fn release_collected_generated_objects(
    mut commands: Commands,
    mut spawned_chunks: ResMut<ActiveExteriorChunkSpawns>,
    mut removal_deltas: ResMut<ChunkRemovalDeltas>,
    collected_query: Query<
        (Entity, &GeneratedExteriorObject, &GeneratedObjectId),
        Or<(With<HeldItem>, With<InCarry>)>,
    >,
) {
    for (entity, generated, object_id) in collected_query.iter() {
        // Story 5.4: record the removal *before* stripping the identity
        // components. This is the moment where a baseline object transitions
        // from "generated chunk content" to "player-owned item". We persist the
        // GeneratedObjectId in the chunk's removal delta so the object will not
        // reappear when the chunk is regenerated.
        //
        // The removal delta is keyed by the object's home_chunk — the chunk
        // that originally spawned it. Even if the player walks away and the
        // chunk unloads, the delta stays in memory (and eventually on disk in
        // a future save-file story).
        removal_deltas
            .removed_by_chunk
            .entry(generated.home_chunk)
            .or_default()
            .insert(object_id.clone());

        if let Some(chunk_entities) = spawned_chunks
            .spawned_entities_by_chunk
            .get_mut(&generated.home_chunk)
        {
            chunk_entities.retain(|tracked| *tracked != entity);
        }

        commands
            .entity(entity)
            .remove::<GeneratedExteriorObject>()
            .remove::<DepositSiteMember>()
            .remove::<SurfaceMineralDeposit>()
            .remove::<GeneratedObjectId>();
    }
}

/// Story 5.5: when a player-added exterior object is picked up, remove its
/// record from `ChunkPlayerAdditions` and strip its ownership marker.
///
/// This is the additive counterpart to `release_collected_generated_objects`.
/// The generated version records a removal delta (subtractive); this version
/// deletes the addition record entirely, because the object is no longer
/// "something the player left in that chunk."
///
/// If the player drops the object again later, `claim_exterior_drops` will
/// create a fresh record — possibly in a different chunk.
// Bevy system — same wide-query pattern as release_collected_generated_objects.
#[allow(clippy::type_complexity)]
fn release_collected_player_added_objects(
    mut commands: Commands,
    mut spawned_chunks: ResMut<ActiveExteriorChunkSpawns>,
    mut player_additions: ResMut<ChunkPlayerAdditions>,
    collected_query: Query<
        (Entity, &PlayerAddedExteriorObject),
        Or<(With<HeldItem>, With<InCarry>)>,
    >,
) {
    for (entity, player_added) in collected_query.iter() {
        // Remove the record from chunk state. The object is now "in the
        // player's hands" and no longer belongs to any chunk.
        if let Some(records) = player_additions
            .added_by_chunk
            .get_mut(&player_added.home_chunk)
        {
            records.retain(|r| r.id != player_added.player_added_id);
        }

        if let Some(chunk_entities) = spawned_chunks
            .spawned_entities_by_chunk
            .get_mut(&player_added.home_chunk)
        {
            chunk_entities.retain(|tracked| *tracked != entity);
        }

        commands
            .entity(entity)
            .remove::<PlayerAddedExteriorObject>();
    }
}

/// Story 5.5: detect newly-dropped material objects in the exterior and record
/// them in chunk state.
///
/// This system runs after both release systems to avoid claiming an object in
/// the same frame it was released. It looks for "unclaimed" `MaterialObject`
/// entities — those without any ownership marker (`HeldItem`, `InCarry`,
/// `GeneratedExteriorObject`, `PlayerAddedExteriorObject`) — and checks if
/// their world position falls within the exterior ground patch bounds.
///
/// Objects inside the room (workbench materials, shelf items) are excluded
/// because their positions do not fall within the `ExteriorGroundPatch` bounds.
///
/// ## Why not use RemovedComponents<HeldItem>?
///
/// Bevy's `RemovedComponents` is frame-sensitive and would miss objects that
/// were dropped in a previous frame but not yet claimed (e.g. if claim ran
/// before process_place). A query-based approach is more robust: it catches
/// any unclaimed exterior object regardless of when it appeared.
// Bevy system — query tuple is wide to match any unclaimed exterior object.
#[allow(clippy::type_complexity)]
fn claim_exterior_drops(
    mut commands: Commands,
    exterior_patch: Res<ExteriorGroundPatch>,
    world_profile: Option<Res<WorldProfile>>,
    mut player_additions: ResMut<ChunkPlayerAdditions>,
    mut id_counter: ResMut<PlayerAddedIdCounter>,
    unclaimed_query: Query<
        (Entity, &GameMaterial, &Transform),
        (
            With<MaterialObject>,
            Without<HeldItem>,
            Without<InCarry>,
            Without<GeneratedExteriorObject>,
            Without<PlayerAddedExteriorObject>,
        ),
    >,
) {
    let Some(world_profile) = world_profile else {
        return;
    };
    for (entity, material, transform) in unclaimed_query.iter() {
        let pos = transform.translation;

        // Check if the object is within the exterior ground patch bounds.
        // Objects on the room floor, workbench, or shelves will not match.
        if !is_within_exterior_bounds(pos, &exterior_patch.bounds_xz) {
            continue;
        }

        let chunk = world_position_to_chunk_coord(
            PositionXZ::new(pos.x, pos.z),
            world_profile.chunk_size_world_units,
        );

        let id = id_counter.next();
        let record = PlayerAddedObjectRecord {
            id,
            material: material.clone(),
            position: [pos.x, pos.y, pos.z],
            visual_scale: transform.scale.x, // uniform scale
        };

        player_additions
            .added_by_chunk
            .entry(chunk)
            .or_default()
            .push(record);

        commands.entity(entity).insert(PlayerAddedExteriorObject {
            home_chunk: chunk,
            player_added_id: id,
        });
    }
}

/// Check if a world-space position falls within the exterior ground patch.
///
/// This is a simple 2D bounds check on the X/Z plane. The Y coordinate is
/// ignored because objects may rest at slightly different heights depending on
/// their shape and the surface.
fn is_within_exterior_bounds(pos: Vec3, bounds: &RectXZ) -> bool {
    pos.x >= bounds.min_x && pos.x <= bounds.max_x && pos.z >= bounds.min_z && pos.z <= bounds.max_z
}

/// Filter baseline placements through removal deltas, producing the final
/// spawn list.
///
/// This is a pure function (no ECS, no side effects) so it can be unit-tested
/// in isolation. The pipeline is:
///
/// ```text
/// generate_surface_mineral_chunk_baseline()  →  full deterministic baseline
///                  ↓
/// apply_removal_deltas()                     →  baseline minus picked-up objects
///                  ↓
/// spawn loop in sync_active_exterior_chunks  →  live Bevy entities
/// ```
///
/// If `removals` is `None` or empty, the baseline passes through unchanged —
/// no allocation or filtering overhead beyond the emptiness check.
fn apply_removal_deltas(
    baseline: Vec<GeneratedSurfaceMineralPlacement>,
    removals: Option<&HashSet<GeneratedObjectId>>,
) -> Vec<GeneratedSurfaceMineralPlacement> {
    match removals {
        Some(removed) if !removed.is_empty() => baseline
            .into_iter()
            .filter(|placement| !removed.contains(&placement.generated_id))
            .collect(),
        _ => baseline,
    }
}

/// Generate the deterministic baseline surface mineral placements for one chunk.
///
/// This is the top-level pure-Rust generation entry point. It takes a
/// [`SurfaceProvider`] instead of a concrete surface type so that:
/// - the live game passes a [`FlatSurface`] built from `ExteriorGroundPatch`
/// - tests pass synthetic flat, sloped, or stepped surfaces
/// - the generation logic never knows or cares which surface shape it is placing on
///
/// ## Deterministic retry / rejection behavior
///
/// When a candidate placement is rejected (surface invalid or too steep), the
/// candidate index still advances. This means a rejection does not shift all
/// subsequent placements — the remaining candidates keep their deterministic
/// positions regardless of which earlier candidates were rejected. This is
/// intentional: it preserves generation stability so adding a cliff in one
/// corner of the chunk does not reorganize deposits in the other corners.
fn generate_surface_mineral_chunk_baseline(
    profile: &WorldProfile,
    catalog: &SurfaceMineralDepositCatalog,
    surface: &dyn SurfaceProvider,
    chunk_coord: ChunkCoord,
    biome: &ChunkBiome,
) -> Vec<GeneratedSurfaceMineralPlacement> {
    let deposit_sites =
        generate_surface_mineral_deposit_sites(profile, catalog, surface, chunk_coord, biome);
    let mut placements = Vec::new();

    // Story 5.2 produced one object per accepted point sample.
    // Story 5.2b inserts a site layer in between:
    // 1. generate a few deterministic deposit sites
    // 2. expand each site into clustered child minerals
    //
    // That extra layer is what makes the outside read like deposits or veins
    // instead of unrelated loose pieces sprinkled around the patch.
    //
    // Story 5.3: each child mineral now queries the surface independently
    // because the surface height and slope may vary across the deposit radius.
    for site in deposit_sites {
        placements.extend(expand_deposit_site_into_cluster(profile, &site, surface));
    }

    placements
}

/// Generate deposit site candidates for a single chunk.
///
/// Each candidate grid cell queries the [`SurfaceProvider`] to determine:
/// 1. Whether the surface exists at the site center (valid check)
/// 2. Whether the surface is flat enough for a deposit (slope check)
/// 3. What height the deposit center sits at
///
/// A location is **accepted** if `is_placement_valid` returns true (surface
/// valid AND slope ≤ `DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS`). Rejected
/// candidates still advance the candidate index so later candidates keep their
/// deterministic identity regardless of earlier rejections.
fn generate_surface_mineral_deposit_sites(
    profile: &WorldProfile,
    catalog: &SurfaceMineralDepositCatalog,
    surface: &dyn SurfaceProvider,
    chunk_coord: ChunkCoord,
    biome: &ChunkBiome,
) -> Vec<GeneratedSurfaceMineralDepositSite> {
    let generation_key = derive_chunk_generation_key(profile, chunk_coord);
    let chunk_origin_xz = chunk_origin_xz(chunk_coord, profile.chunk_size_world_units);
    let spacing = catalog.site_spacing_world_units;
    let columns = (profile.chunk_size_world_units / spacing).ceil() as u32;
    let rows = (profile.chunk_size_world_units / spacing).ceil() as u32;
    let mut sites: Vec<GeneratedSurfaceMineralDepositSite> = Vec::new();
    let mut local_site_index = 0_u32;

    // Story 5a.2: the biome's density modifier scales the spawn threshold.
    // A density_modifier > 1.0 lowers the effective threshold, admitting more
    // candidates (denser biome). A modifier < 1.0 raises it, rejecting more
    // candidates (sparser biome). We clamp to avoid division by zero.
    let effective_threshold =
        catalog.site_spawn_threshold / biome.density_modifier.max(f32::EPSILON);

    for row in 0..rows {
        for column in 0..columns {
            let cell_center_x = chunk_origin_xz.x + (column as f32 + 0.5) * spacing;
            let cell_center_z = chunk_origin_xz.z + (row as f32 + 0.5) * spacing;
            let cell_center_xz = PositionXZ::new(cell_center_x, cell_center_z);

            // Story 5.3: query the surface at the cell center BEFORE doing
            // density field evaluation. If the surface is invalid here (out of
            // bounds, void, etc.) we skip early. We don't check slope at the
            // grid-cell level because jitter will move the actual center — slope
            // is checked at the final jittered position below.
            let cell_surface = surface.query_surface(cell_center_x, cell_center_z);
            if !cell_surface.valid {
                local_site_index += 1;
                continue;
            }

            let density = continuous_value_field_01(
                generation_key.placement_density_key,
                cell_center_xz,
                catalog.site_density_field_scale_world_units,
            );
            // Story 5a.2: use biome-adjusted threshold instead of catalog baseline.
            if density < effective_threshold {
                local_site_index += 1;
                continue;
            }

            // Story 5a.2: pass biome weight modifiers to deposit selection so
            // different biomes favor different materials.
            let Some(definition) = choose_deposit_definition(
                &catalog.deposits,
                generation_key.placement_variation_key,
                chunk_coord,
                local_site_index,
                &biome.deposit_weight_modifiers,
            ) else {
                local_site_index += 1;
                continue;
            };

            let jitter_offset = jitter_offset_xz(
                generation_key.placement_variation_key,
                chunk_coord,
                local_site_index,
                spacing * catalog.site_jitter_fraction,
            );
            let center_xz = PositionXZ::new(
                cell_center_xz.x + jitter_offset.x,
                cell_center_xz.z + jitter_offset.z,
            );

            // Story 5.3: query the surface at the final jittered position.
            // This is where we check both validity AND slope, because this is
            // where the deposit will actually be placed.
            let center_surface = surface.query_surface(center_xz.x, center_xz.z);
            if !is_placement_valid(&center_surface, DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS) {
                local_site_index += 1;
                continue;
            }

            let radius_mix = unit_interval_01(mix_candidate_input(
                generation_key.placement_variation_key,
                chunk_coord,
                local_site_index,
                0x6600_0000_0000_0001,
            ));
            let radius_world_units = lerp(
                definition.deposit_radius_min,
                definition.deposit_radius_max,
                radius_mix,
            );

            let child_count_mix = unit_interval_01(mix_candidate_input(
                generation_key.placement_variation_key,
                chunk_coord,
                local_site_index,
                0x6600_0000_0000_0002,
            ));
            let child_count = lerp(
                definition.child_count_min as f32,
                definition.child_count_max as f32,
                child_count_mix,
            )
            .round() as u32;

            // A deposit only feels like its own formation if it has some air
            // around it. This deterministic overlap check rejects sites that
            // would collapse visually into an existing site, preserving visible
            // gaps between deposits.
            let overlaps_existing_site = sites.iter().any(|existing| {
                let distance = distance_xz(center_xz, existing.center_xz);
                let min_distance = radius_world_units
                    + existing.radius_world_units
                    + catalog.site_min_gap_world_units;
                distance < min_distance
            });
            if overlaps_existing_site {
                local_site_index += 1;
                continue;
            }

            sites.push(GeneratedSurfaceMineralDepositSite {
                site_id: GeneratedDepositSiteId {
                    planet_seed: profile.planet_seed.0,
                    chunk_coord,
                    definition_key: definition.key.clone(),
                    local_site_index,
                    generator_version: SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION,
                },
                definition_key: definition.key.clone(),
                material_seed: choose_material_seed_from_palette(
                    &biome.material_palette,
                    generation_key.placement_variation_key,
                    chunk_coord,
                    local_site_index,
                ),
                center_xz,
                radius_world_units,
                child_count: child_count.max(1),
                surface_y: center_surface.position_y,
                surface_normal: center_surface.normal,
                scale_min: definition.scale_min,
                scale_max: definition.scale_max,
                cluster_compactness: definition.cluster_compactness.clamp(0.0, 1.0),
            });

            local_site_index += 1;
        }
    }

    sites
}

/// Expand a deposit site into its individual child mineral placements.
///
/// Each child queries the surface independently at its own position because the
/// surface height and slope may vary across the deposit radius. On a flat
/// surface every child gets the same height; on terrain with micro-variation
/// each child sits at its own elevation.
///
/// Children placed on invalid or too-steep surface points are skipped. The child
/// index still advances to preserve determinism for remaining children.
fn expand_deposit_site_into_cluster(
    profile: &WorldProfile,
    site: &GeneratedSurfaceMineralDepositSite,
    surface: &dyn SurfaceProvider,
) -> Vec<GeneratedSurfaceMineralPlacement> {
    let mut placements = Vec::new();

    for local_child_index in 0..site.child_count {
        let angle = unit_interval_01(mix_child_input(
            site,
            local_child_index,
            0x7700_0000_0000_0001,
        )) * std::f32::consts::TAU;
        let radial_mix = unit_interval_01(mix_child_input(
            site,
            local_child_index,
            0x7700_0000_0000_0002,
        ));
        // Higher compactness keeps more child minerals near the center so the
        // player reads one deposit instead of a sparse ring.
        let radial_exponent = lerp(2.6, 1.1, 1.0 - site.cluster_compactness);
        let radial_distance = site.radius_world_units * radial_mix.powf(radial_exponent);
        let child_x = site.center_xz.x + angle.cos() * radial_distance;
        let child_z = site.center_xz.z + angle.sin() * radial_distance;
        let position_xz = PositionXZ::new(child_x, child_z);

        // Story 5.3: query the surface at each child's position independently.
        // On flat terrain every child gets the same result, but on non-flat
        // terrain each child may sit at a different height or be rejected if
        // the local slope is too steep.
        let child_surface = surface.query_surface(child_x, child_z);
        if !is_placement_valid(&child_surface, DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS) {
            continue;
        }

        let scale_mix = unit_interval_01(mix_child_input(
            site,
            local_child_index,
            0x7700_0000_0000_0003,
        ));
        let visual_scale = lerp(site.scale_min, site.scale_max, scale_mix);

        placements.push(GeneratedSurfaceMineralPlacement {
            generated_id: derive_generated_object_id(
                profile,
                site.site_id.chunk_coord,
                site.definition_key.clone(),
                (site.site_id.local_site_index << 16) | local_child_index,
                SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION,
            ),
            deposit_site_id: site.site_id.clone(),
            definition_key: site.definition_key.clone(),
            material_seed: site.material_seed,
            position_xz,
            surface_y: child_surface.position_y,
            surface_normal: child_surface.normal,
            visual_scale,
            local_child_index,
        });
    }

    placements
}

fn distance_xz(a: PositionXZ, b: PositionXZ) -> f32 {
    let dx = a.x - b.x;
    let dz = a.z - b.z;
    (dx * dx + dz * dz).sqrt()
}

/// Choose a deposit definition from the catalog using weighted random selection.
///
/// Story 5a.2: biome weight modifiers are applied multiplicatively to each
/// definition's base `selection_weight`. A modifier of `2.0` doubles the
/// chance that material is selected; `0.0` guarantees it is never selected
/// in this biome. Definitions not present in the modifier map default to
/// `1.0` (unchanged).
fn choose_deposit_definition<'a>(
    definitions: &'a [SurfaceMineralDepositDefinition],
    variation_key: u64,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
    weight_modifiers: &HashMap<String, f32>,
) -> Option<&'a SurfaceMineralDepositDefinition> {
    // Compute biome-adjusted weights: base weight × biome modifier.
    let effective_weights: Vec<f32> = definitions
        .iter()
        .map(|def| {
            let modifier = weight_modifiers.get(&def.key).copied().unwrap_or(1.0);
            (def.selection_weight * modifier).max(0.0)
        })
        .collect();

    let total_weight: f32 = effective_weights.iter().sum();
    if definitions.is_empty() || total_weight <= f32::EPSILON {
        return None;
    }

    let roll = unit_interval_01(mix_candidate_input(
        variation_key,
        chunk_coord,
        local_candidate_index,
        0x2200_0000_0000_0001,
    )) * total_weight;

    let mut running = 0.0;
    for (definition, &weight) in definitions.iter().zip(effective_weights.iter()) {
        running += weight;
        if roll <= running {
            return Some(definition);
        }
    }

    definitions.last()
}

/// Choose a material seed from the biome's material palette using weighted
/// random selection.
///
/// Returns `0` if the palette is empty (the spawning system will skip deposits
/// with seed `0` since no valid material can be derived). The deterministic
/// roll uses a distinct channel (`0x4400_0000_0000_0001`) so it does not
/// correlate with the deposit-definition selection roll.
fn choose_material_seed_from_palette(
    palette: &[PaletteMaterial],
    variation_key: u64,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
) -> u64 {
    if palette.is_empty() {
        return 0;
    }

    let total_weight: f32 = palette
        .iter()
        .map(|entry| entry.selection_weight.max(0.0))
        .sum();
    if total_weight <= f32::EPSILON {
        return 0;
    }

    let roll = unit_interval_01(mix_candidate_input(
        variation_key,
        chunk_coord,
        local_candidate_index,
        0x4400_0000_0000_0001,
    )) * total_weight;

    let mut running = 0.0_f32;
    for entry in palette {
        running += entry.selection_weight.max(0.0);
        if roll <= running {
            return entry.material_seed;
        }
    }

    // Fallback to last entry (float rounding edge case).
    palette.last().map(|e| e.material_seed).unwrap_or(0)
}

fn jitter_offset_xz(
    variation_key: u64,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
    max_offset: f32,
) -> PositionXZ {
    let jitter_x = signed_unit_interval(mix_candidate_input(
        variation_key,
        chunk_coord,
        local_candidate_index,
        0x3300_0000_0000_0001,
    )) * max_offset;
    let jitter_z = signed_unit_interval(mix_candidate_input(
        variation_key,
        chunk_coord,
        local_candidate_index,
        0x3300_0000_0000_0002,
    )) * max_offset;

    PositionXZ::new(jitter_x, jitter_z)
}

/// Sample a continuous field on the X/Z plane using deterministic value noise.
///
/// This is the "coherent place rather than random soup" part of the story.
/// We are not rolling a fresh random value per chunk and per candidate. Instead
/// we sample a field defined over world space:
/// - nearby world positions produce nearby values
/// - crossing a chunk boundary does not reset the field
/// - chunk identity still matters because the same world field is being sampled
///   at different locations on the same planet
pub(super) fn continuous_value_field_01(
    seed: u64,
    position_xz: PositionXZ,
    scale_world_units: f32,
) -> f32 {
    let sample_x = position_xz.x / scale_world_units;
    let sample_z = position_xz.z / scale_world_units;

    let cell_min_x = sample_x.floor() as i32;
    let cell_min_z = sample_z.floor() as i32;
    let frac_x = sample_x - cell_min_x as f32;
    let frac_z = sample_z - cell_min_z as f32;

    let v00 = corner_noise_01(seed, cell_min_x, cell_min_z);
    let v10 = corner_noise_01(seed, cell_min_x + 1, cell_min_z);
    let v01 = corner_noise_01(seed, cell_min_x, cell_min_z + 1);
    let v11 = corner_noise_01(seed, cell_min_x + 1, cell_min_z + 1);

    let sx = smoothstep(frac_x);
    let sz = smoothstep(frac_z);
    let ix0 = lerp(v00, v10, sx);
    let ix1 = lerp(v01, v11, sx);
    lerp(ix0, ix1, sz)
}

fn corner_noise_01(seed: u64, lattice_x: i32, lattice_z: i32) -> f32 {
    unit_interval_01(mix_lattice_coord(seed, lattice_x, lattice_z))
}

fn mix_lattice_coord(seed: u64, lattice_x: i32, lattice_z: i32) -> u64 {
    let packed_x = lattice_x as u32 as u64;
    let packed_z = lattice_z as u32 as u64;
    let packed = (packed_x << 32) | packed_z;
    splitmix64(seed.wrapping_add(packed.wrapping_mul(0x9E37_79B9_7F4A_7C15)))
}

fn mix_candidate_input(
    base: u64,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
    channel: u64,
) -> u64 {
    let packed_chunk = ((chunk_coord.x as u32 as u64) << 32) | (chunk_coord.z as u32 as u64);
    splitmix64(
        base.wrapping_add(packed_chunk)
            .wrapping_add(local_candidate_index as u64)
            .wrapping_add(channel),
    )
}

fn mix_child_input(
    site: &GeneratedSurfaceMineralDepositSite,
    local_child_index: u32,
    channel: u64,
) -> u64 {
    splitmix64(
        site.site_id
            .planet_seed
            .wrapping_add(site.site_id.chunk_coord.x as u32 as u64)
            .wrapping_add((site.site_id.chunk_coord.z as u32 as u64) << 32)
            .wrapping_add(site.site_id.local_site_index as u64)
            .wrapping_add(local_child_index as u64)
            .wrapping_add(channel),
    )
}

fn splitmix64(mut z: u64) -> u64 {
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

fn unit_interval_01(value: u64) -> f32 {
    (value as f64 / u64::MAX as f64) as f32
}

fn signed_unit_interval(value: u64) -> f32 {
    unit_interval_01(value) * 2.0 - 1.0
}

fn smoothstep(t: f32) -> f32 {
    t * t * (3.0 - 2.0 * t)
}

// ── Story 5.6: Delta-Sync Architecture Validation ───────────────────────
//
// ## Purpose
//
// This section contains pure-logic functions that validate the delta model's
// correctness for future multiplayer merge scenarios. No ECS wiring — these
// functions operate entirely on the `ChunkRemovalDeltas` and
// `ChunkPlayerAdditions` types defined above.
//
// These types and functions are currently only exercised by tests. They will
// be promoted to production use when multiplayer (Epic 22) lands. The
// `allow(dead_code)` annotations will be removed at that point.
//
// ## Why this matters
//
// When two players independently modify the same base (Epic 22), the server
// must merge their deltas. Removals are set-based and naturally commutative.
// Player additions can conflict spatially — two objects in the same building
// cell. Rather than silently resolving these, the system flags them so the
// base owner can decide.
//
// ## Building cells
//
// A building cell is a 3D grid cell addressed by `(i64, i64, i64)`. Unlike
// chunks (2D XZ columns), building cells include the Y axis to discriminate
// vertically stacked structures. The cell key is computed as:
//   `(floor(x / cell_size), floor(y / cell_size), floor(z / cell_size))`
//
// Cell size is configurable. Two objects in the same cell are considered
// spatially overlapping. Grid-cell collision avoids the edge cases of
// radius-based overlap with non-circular footprints (e.g. square buildings).

/// A 3D building cell coordinate, unique across the solar system.
///
/// Computed by quantizing a world-space position into a grid with a
/// configurable cell size. Two player-added objects in the same cell are
/// considered to be in spatial conflict when merging deltas from different
/// sources.
///
/// Unlike [`ChunkCoord`] (which is 2D on the XZ ground plane), building cells
/// include the Y axis so vertically stacked structures occupy distinct cells.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[allow(dead_code)]
struct BuildingCell {
    pub x: i64,
    pub y: i64,
    pub z: i64,
}

impl BuildingCell {
    /// Quantize a world-space position into a building cell.
    ///
    /// Each axis is independently divided by `cell_size` and floored to produce
    /// a signed integer cell coordinate. This means a `cell_size` of 1.0 gives
    /// meter-resolution cells; a `cell_size` of 0.5 gives half-meter cells.
    ///
    /// ## Panics
    ///
    /// Panics if `cell_size` is not positive and finite. A zero or negative
    /// cell size has no physical meaning and would produce nonsensical or
    /// infinite coordinates.
    fn from_position(position: [f32; 3], cell_size: f32) -> Self {
        assert!(
            cell_size > 0.0 && cell_size.is_finite(),
            "building cell size must be positive and finite, got {cell_size}"
        );
        Self {
            x: (position[0] as f64 / cell_size as f64).floor() as i64,
            y: (position[1] as f64 / cell_size as f64).floor() as i64,
            z: (position[2] as f64 / cell_size as f64).floor() as i64,
        }
    }
}

/// A single spatial conflict detected during delta merging.
///
/// When two player-added objects from different sources occupy the same
/// [`BuildingCell`], neither is automatically discarded. Instead, a conflict
/// record is created so the base owner can resolve it manually (Epic 22).
///
/// The record identifies the conflicting cell and the IDs + source labels
/// of both objects involved.
#[derive(Clone, Debug)]
#[allow(dead_code)]
struct DeltaMergeConflict {
    /// The chunk containing the conflicting cell.
    pub chunk: ChunkCoord,
    /// The building cell where both objects overlap.
    pub cell: BuildingCell,
    /// The player-added object ID from source A.
    pub id_a: u64,
    /// Identifying label for the first delta source (e.g. player name).
    pub source_a: String,
    /// The player-added object ID from source B.
    pub id_b: u64,
    /// Identifying label for the second delta source.
    pub source_b: String,
}

/// The result of merging two sets of player additions.
///
/// Non-conflicting additions are combined into `merged`. Any same-cell
/// overlaps are reported in `conflicts` for the base owner to resolve.
#[allow(dead_code)]
struct MergedPlayerAdditions {
    /// The combined player additions (excluding conflicting entries).
    pub merged: ChunkPlayerAdditions,
    /// Spatial conflicts that require human resolution.
    pub conflicts: Vec<DeltaMergeConflict>,
}

/// Merge two removal delta sets into one.
///
/// Removal deltas are set-based (each entry is a `GeneratedObjectId` that was
/// removed). Merging is a per-chunk set union. This operation is:
/// - **Commutative:** `merge(A, B) == merge(B, A)` — union is symmetric.
/// - **Idempotent:** removing the same object from both sources produces the
///   same result as removing it from one.
///
/// No conflicts are possible with removals — if both sources removed the same
/// generated object, the merged result simply contains that removal once.
#[allow(dead_code)]
fn merge_removal_deltas(a: &ChunkRemovalDeltas, b: &ChunkRemovalDeltas) -> ChunkRemovalDeltas {
    let mut merged = a.clone();
    for (chunk, ids) in &b.removed_by_chunk {
        merged
            .removed_by_chunk
            .entry(*chunk)
            .or_default()
            .extend(ids.iter().cloned());
    }
    merged
}

/// Merge two sets of player additions, detecting spatial conflicts.
///
/// Non-conflicting additions (different building cells) are combined into the
/// merged result. When two additions from different sources occupy the same
/// [`BuildingCell`], both are excluded from the merged result and a
/// [`DeltaMergeConflict`] is recorded instead.
///
/// ## Conflict detection strategy
///
/// For each chunk, every addition is mapped to its `BuildingCell` via
/// `floor(position / cell_size)`. If an addition from source B lands in a cell
/// already occupied by source A, that pair is a conflict. Within a single
/// source, objects in the same cell are allowed (the player placed them
/// intentionally and is aware of the overlap).
///
/// ## Parameters
///
/// - `a`, `b`: the two addition sets to merge
/// - `source_a_label`, `source_b_label`: human-readable labels identifying
///   each source (e.g. player names) for the conflict records
/// - `cell_size`: the building cell size used for spatial quantization
#[allow(dead_code)]
fn merge_player_additions(
    a: &ChunkPlayerAdditions,
    b: &ChunkPlayerAdditions,
    source_a_label: &str,
    source_b_label: &str,
    cell_size: f32,
) -> MergedPlayerAdditions {
    let mut merged = ChunkPlayerAdditions::default();
    let mut conflicts = Vec::new();

    // Collect all chunk coords from both sources.
    let all_chunks: HashSet<ChunkCoord> = a
        .added_by_chunk
        .keys()
        .chain(b.added_by_chunk.keys())
        .copied()
        .collect();

    for chunk in all_chunks {
        let records_a = a.added_by_chunk.get(&chunk);
        let records_b = b.added_by_chunk.get(&chunk);

        // Build a cell → record-ID index for source A's objects in this chunk.
        let mut cells_a: HashMap<BuildingCell, u64> = HashMap::new();
        let mut merged_records: Vec<PlayerAddedObjectRecord> = Vec::new();

        if let Some(recs) = records_a {
            for rec in recs {
                let cell = BuildingCell::from_position(rec.position, cell_size);
                cells_a.insert(cell, rec.id);
                merged_records.push(rec.clone());
            }
        }

        if let Some(recs) = records_b {
            for rec in recs {
                let cell = BuildingCell::from_position(rec.position, cell_size);
                if let Some(&existing_id) = cells_a.get(&cell) {
                    // Spatial conflict: source B's object lands in a cell
                    // already occupied by source A. Exclude both from the
                    // merged result and record the conflict.
                    merged_records.retain(|r| r.id != existing_id);
                    conflicts.push(DeltaMergeConflict {
                        chunk,
                        cell,
                        id_a: existing_id,
                        source_a: source_a_label.to_string(),
                        id_b: rec.id,
                        source_b: source_b_label.to_string(),
                    });
                } else {
                    merged_records.push(rec.clone());
                }
            }
        }

        if !merged_records.is_empty() {
            merged.added_by_chunk.insert(chunk, merged_records);
        }
    }

    MergedPlayerAdditions { merged, conflicts }
}

#[cfg(test)]
#[path = "exterior_tests.rs"]
mod tests;
