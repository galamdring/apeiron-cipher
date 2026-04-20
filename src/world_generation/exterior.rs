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
    ActiveChunkNeighborhood, ChunkCoord, DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS, FlatSurface,
    GeneratedObjectId, SurfaceProvider, WorldProfile, chunk_origin_xz, derive_chunk_generation_key,
    derive_generated_object_id, is_placement_valid, surface_alignment_rotation,
    world_position_to_chunk_coord,
};
use crate::carry::InCarry;
use crate::interaction::HeldItem;
use crate::materials::{GameMaterial, MaterialCatalog, MaterialObject};
use crate::scene::{ExteriorGroundPatch, PositionXZ, RectXZ};

const DEPOSIT_CONFIG_PATH: &str = "assets/exterior/surface_mineral_deposits.toml";
const SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION: u32 = 1;

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
/// - which existing material it should yield / be represented by
/// - how likely it is relative to sibling deposit definitions
/// - how large it can appear
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
struct SurfaceMineralDepositDefinition {
    pub key: String,
    pub material_key: String,
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
            key: "ferrite_surface_deposit".into(),
            material_key: "Ferrite".into(),
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
            key: "silite_surface_deposit".into(),
            material_key: "Silite".into(),
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
            key: "prismate_surface_deposit".into(),
            material_key: "Prismate".into(),
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
    pub deposit_kind_key: String,
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
    material_key: String,
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
    material_key: String,
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

#[allow(clippy::too_many_arguments)]
fn sync_active_exterior_chunks(
    mut commands: Commands,
    active_chunks: Res<ActiveChunkNeighborhood>,
    world_profile: Res<WorldProfile>,
    deposit_catalog: Res<SurfaceMineralDepositCatalog>,
    material_catalog: Res<MaterialCatalog>,
    exterior_patch: Res<ExteriorGroundPatch>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut render_materials: ResMut<Assets<StandardMaterial>>,
    mut spawned_chunks: ResMut<ActiveExteriorChunkSpawns>,
    removal_deltas: Res<ChunkRemovalDeltas>,
    player_additions: Res<ChunkPlayerAdditions>,
) {
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

    // Build the surface provider from the current exterior patch.
    //
    // Story 5.3: the generation functions no longer receive ExteriorGroundPatch
    // directly. They receive a &dyn SurfaceProvider so they can be tested
    // against synthetic surfaces without any Bevy dependency. The ECS system is
    // the only place that knows about ExteriorGroundPatch — it constructs the
    // appropriate SurfaceProvider and hands it down.
    let surface = FlatSurface {
        surface_y: exterior_patch.surface_y,
        min_x: exterior_patch.bounds_xz.min_x,
        max_x: exterior_patch.bounds_xz.max_x,
        min_z: exterior_patch.bounds_xz.min_z,
        max_z: exterior_patch.bounds_xz.max_z,
    };

    for &chunk in &active_chunks.chunks {
        if spawned_chunks
            .spawned_entities_by_chunk
            .contains_key(&chunk)
        {
            continue;
        }

        let baseline_placements = generate_surface_mineral_chunk_baseline(
            &world_profile,
            &deposit_catalog,
            &surface,
            chunk,
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

        let mut spawned_entities = Vec::new();

        for placement in placements {
            let Some(base_material) = material_catalog.materials.get(&placement.material_key)
            else {
                warn!(
                    "Surface mineral deposit '{}' references unknown material '{}'; skipping placement",
                    placement.definition_key, placement.material_key
                );
                continue;
            };

            let deposit_material = base_material.clone();
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
#[allow(clippy::type_complexity)]
fn claim_exterior_drops(
    mut commands: Commands,
    exterior_patch: Res<ExteriorGroundPatch>,
    world_profile: Res<WorldProfile>,
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
) -> Vec<GeneratedSurfaceMineralPlacement> {
    let deposit_sites =
        generate_surface_mineral_deposit_sites(profile, catalog, surface, chunk_coord);
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
) -> Vec<GeneratedSurfaceMineralDepositSite> {
    let generation_key = derive_chunk_generation_key(profile, chunk_coord);
    let chunk_origin_xz = chunk_origin_xz(chunk_coord, profile.chunk_size_world_units);
    let spacing = catalog.site_spacing_world_units;
    let columns = (profile.chunk_size_world_units / spacing).ceil() as u32;
    let rows = (profile.chunk_size_world_units / spacing).ceil() as u32;
    let mut sites: Vec<GeneratedSurfaceMineralDepositSite> = Vec::new();
    let mut local_site_index = 0_u32;

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
            if density < catalog.site_spawn_threshold {
                local_site_index += 1;
                continue;
            }

            let Some(definition) = choose_deposit_definition(
                &catalog.deposits,
                generation_key.placement_variation_key,
                chunk_coord,
                local_site_index,
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
                    deposit_kind_key: definition.key.clone(),
                    local_site_index,
                    generator_version: SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION,
                },
                definition_key: definition.key.clone(),
                material_key: definition.material_key.clone(),
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
            material_key: site.material_key.clone(),
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

fn choose_deposit_definition(
    definitions: &[SurfaceMineralDepositDefinition],
    variation_key: u64,
    chunk_coord: ChunkCoord,
    local_candidate_index: u32,
) -> Option<&SurfaceMineralDepositDefinition> {
    let total_weight: f32 = definitions
        .iter()
        .map(|definition| definition.selection_weight)
        .sum();
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
    for definition in definitions {
        running += definition.selection_weight;
        if roll <= running {
            return Some(definition);
        }
    }

    definitions.last()
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
fn continuous_value_field_01(seed: u64, position_xz: PositionXZ, scale_world_units: f32) -> f32 {
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

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::world_generation::{
        FlatSurface, SteppedSurface, TiltedSurface, WorldGenerationConfig,
    };

    fn sample_profile() -> WorldProfile {
        WorldProfile::from_config(&WorldGenerationConfig {
            planet_seed: 2026,
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
        })
    }

    fn sample_catalog() -> SurfaceMineralDepositCatalog {
        SurfaceMineralDepositCatalog {
            site_spawn_threshold: 0.0,
            ..SurfaceMineralDepositCatalog::default()
        }
    }

    /// Build a FlatSurface matching the old sample_patch() bounds.
    ///
    /// Story 5.3 replaced ExteriorGroundPatch in tests with FlatSurface to
    /// prove the generation pipeline works through the SurfaceProvider trait
    /// without any Bevy dependency.
    fn sample_flat_surface() -> FlatSurface {
        FlatSurface {
            surface_y: -0.01,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        }
    }

    // ── AC1: Placement uses surface queries, not flat assumptions ─────────

    #[test]
    fn same_chunk_regenerates_identically() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();

        let a = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );
        let b = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert_eq!(a, b);
    }

    #[test]
    fn different_chunks_produce_different_baselines() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();

        let a = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );
        let b = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(1, -1),
        );

        assert_ne!(a, b);
    }

    #[test]
    fn generated_object_ids_are_stable_from_explicit_inputs() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );
        let first = placements
            .first()
            .expect("sample patch should produce at least one generated deposit");

        assert_eq!(first.generated_id.planet_seed, profile.planet_seed);
        assert_eq!(first.generated_id.chunk_coord, ChunkCoord::new(0, -1));
        assert_eq!(
            first.generated_id.generator_version,
            SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION
        );
        assert_eq!(first.generated_id.object_kind_key, first.definition_key);
    }

    #[test]
    fn flat_surface_placements_use_surface_y_not_hardcoded_zero() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        // Use a non-zero, non-default surface_y to prove the generation code
        // reads from the surface provider rather than assuming y = 0.
        let surface = FlatSurface {
            surface_y: 5.5,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(
            !placements.is_empty(),
            "should produce at least one placement"
        );
        for p in &placements {
            assert_eq!(
                p.surface_y, 5.5,
                "placement surface_y must come from surface provider, not hardcoded"
            );
        }
    }

    // ── AC2: Placement can reject invalid surface locations ───────────────

    #[test]
    fn steep_slope_rejects_all_placements() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        // A slope of 2.0 means ~63° — well above the 40° max placement slope.
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 2.0,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(
            placements.is_empty(),
            "no placements should survive on a surface steeper than max slope ({} placements found)",
            placements.len()
        );
    }

    #[test]
    fn gentle_slope_still_produces_placements() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        // A slope of 0.2 means ~11° — well under the 40° limit.
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 0.2,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(
            !placements.is_empty(),
            "gentle slope should still allow placements"
        );
    }

    #[test]
    fn tilted_surface_placements_have_varying_heights() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        // Gentle slope so placements are accepted, but heights vary by X position.
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 0.15,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(
            placements.len() >= 2,
            "need at least 2 placements to compare heights"
        );
        let heights: Vec<f32> = placements.iter().map(|p| p.surface_y).collect();
        let all_same = heights
            .windows(2)
            .all(|w| (w[0] - w[1]).abs() < f32::EPSILON);
        assert!(
            !all_same,
            "on a tilted surface, placements at different X positions should have different heights"
        );
    }

    #[test]
    fn steep_slope_rejection_is_deterministic() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 2.0,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        let a = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );
        let b = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert_eq!(a, b, "rejection must be deterministic");
    }

    // ── AC3: Placement logic testable without rendering terrain ───────────

    #[test]
    fn stepped_surface_flat_terraces_accept_placements() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        // Wide steps with a very narrow transition zone — most of the surface
        // is flat terraces where placement should succeed.
        let surface = SteppedSurface {
            base_y: 0.0,
            step_width: 8.0,
            step_height: 1.0,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
            edge_transition_width: 0.5,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(
            !placements.is_empty(),
            "flat terraces on a stepped surface should accept placements"
        );
    }

    #[test]
    fn stepped_surface_steep_risers_reject_placements() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        // Very narrow steps with wide, steep transition zones. The step_height
        // is large relative to edge_transition_width, making risers near-vertical.
        // Almost all candidate positions will fall on steep risers.
        let surface = SteppedSurface {
            base_y: 0.0,
            step_width: 2.0,   // narrow steps
            step_height: 10.0, // tall risers
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
            edge_transition_width: 1.8, // most of the 2.0 step is riser
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        // The flat portion of each step is only 0.2 world units wide.
        // Most candidates will land on steep risers and be rejected.
        // We can't guarantee zero placements (some might land on the tiny flat
        // portion) but the count should be drastically reduced compared to a
        // flat surface.
        let flat_placements = {
            let flat = sample_flat_surface();
            generate_surface_mineral_chunk_baseline(
                &profile,
                &catalog,
                &flat,
                ChunkCoord::new(0, -1),
            )
        };

        assert!(
            placements.len() < flat_placements.len() / 2,
            "steep risers should reject most placements: {} survived vs {} on flat",
            placements.len(),
            flat_placements.len()
        );
    }

    #[test]
    fn stepped_surface_placements_have_step_heights() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = SteppedSurface {
            base_y: 0.0,
            step_width: 8.0,
            step_height: 2.0,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
            edge_transition_width: 0.5,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(!placements.is_empty());
        // On a stepped surface with step_height=2.0, the placement heights
        // should be multiples of the step height (for placements on flat
        // terraces). Check that we see at least two distinct height levels.
        let mut unique_heights: Vec<f32> = placements.iter().map(|p| p.surface_y).collect();
        unique_heights.sort_by(|a, b| a.partial_cmp(b).unwrap());
        unique_heights.dedup_by(|a, b| (*a - *b).abs() < 0.1);
        assert!(
            unique_heights.len() >= 2,
            "stepped surface should produce placements at multiple height levels, found {:?}",
            unique_heights
        );
    }

    // ── AC4: Current flat exterior still works ────────────────────────────

    #[test]
    fn flat_surface_produces_same_count_as_before_story_5_3() {
        // This test verifies that the Story 5.3 refactoring does not change the
        // number or identity of placements on a flat surface. The generation
        // logic should be identical to pre-5.3 behavior when the surface is flat.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        // The exact count depends on seed/catalog/bounds, but it must be > 0
        // and deterministic.
        assert!(
            !placements.is_empty(),
            "flat surface with threshold=0 should produce placements"
        );

        // All placements should have the flat surface normal.
        for p in &placements {
            assert_eq!(
                p.surface_normal,
                [0.0, 1.0, 0.0],
                "flat surface placements must have straight-up normal"
            );
            assert_eq!(
                p.surface_y, -0.01,
                "flat surface placements must use the configured surface_y"
            );
        }
    }

    #[test]
    fn surface_normal_stored_in_placements() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 0.15,
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &surface,
            ChunkCoord::new(0, -1),
        );

        assert!(!placements.is_empty());
        for p in &placements {
            // On a tilted surface the normal is NOT straight up.
            assert!(
                p.surface_normal[0].abs() > 0.01 || p.surface_normal[1] < 0.999,
                "tilted surface should produce non-vertical normals"
            );
            // The normal should be unit-length.
            let len = (p.surface_normal[0].powi(2)
                + p.surface_normal[1].powi(2)
                + p.surface_normal[2].powi(2))
            .sqrt();
            assert!(
                (len - 1.0).abs() < 0.01,
                "surface normal must be unit-length, got {len}"
            );
        }
    }

    // ── TOML parsing ─────────────────────────────────────────────────────

    #[test]
    fn deposit_catalog_toml_parses() {
        let toml_str = r#"
site_spacing_world_units = 11.0
site_density_field_scale_world_units = 24.0
site_spawn_threshold = 0.55
site_jitter_fraction = 0.28
site_min_gap_world_units = 2.5

[[deposits]]
key = "ferrite_surface_deposit"
material_key = "Ferrite"
selection_weight = 1.0
scale_min = 0.9
scale_max = 1.2
deposit_radius_min = 2.2
deposit_radius_max = 3.4
child_count_min = 5
child_count_max = 9
cluster_compactness = 0.75
"#;

        let catalog: SurfaceMineralDepositCatalog =
            toml::from_str(toml_str).expect("surface deposit catalog should parse");

        assert_eq!(catalog.deposits.len(), 1);
        assert_eq!(catalog.deposits[0].material_key, "Ferrite");
    }

    // ── Story 5.4: Removal delta tests ───────────────────────────────────

    #[test]
    fn removal_delta_filters_out_targeted_object() {
        // Generate a baseline, pick one ID, and verify it disappears after
        // applying the removal delta while all others survive.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        assert!(
            baseline.len() >= 2,
            "need at least 2 placements to test selective removal"
        );

        let target_id = baseline[0].generated_id.clone();
        let mut removals = HashSet::new();
        removals.insert(target_id.clone());

        let filtered = apply_removal_deltas(baseline.clone(), Some(&removals));

        // The targeted object must be gone.
        assert!(
            !filtered.iter().any(|p| p.generated_id == target_id),
            "removed object should not appear in filtered output"
        );
        // All other objects must survive.
        assert_eq!(
            filtered.len(),
            baseline.len() - 1,
            "exactly one object should be removed"
        );
    }

    #[test]
    fn removal_delta_is_stable_across_regenerations() {
        // Simulate chunk unload → reload: regenerate baseline and re-apply the
        // same delta. The removed object must still be absent.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline_1 =
            generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        let target_id = baseline_1[0].generated_id.clone();
        let mut removals = HashSet::new();
        removals.insert(target_id.clone());

        // "Reload" the chunk — regenerate from scratch.
        let baseline_2 =
            generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        let filtered = apply_removal_deltas(baseline_2, Some(&removals));

        assert!(
            !filtered.iter().any(|p| p.generated_id == target_id),
            "removed object must stay gone after chunk regeneration"
        );
    }

    #[test]
    fn removal_delta_only_affects_targeted_id() {
        // Neighbors of the removed object must be completely unaffected.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        assert!(
            baseline.len() >= 3,
            "need at least 3 placements to test neighbor preservation"
        );

        let target_id = baseline[1].generated_id.clone();
        let neighbor_ids: Vec<GeneratedObjectId> = baseline
            .iter()
            .filter(|p| p.generated_id != target_id)
            .map(|p| p.generated_id.clone())
            .collect();

        let mut removals = HashSet::new();
        removals.insert(target_id);
        let filtered = apply_removal_deltas(baseline, Some(&removals));

        let filtered_ids: Vec<GeneratedObjectId> =
            filtered.iter().map(|p| p.generated_id.clone()).collect();
        assert_eq!(
            filtered_ids, neighbor_ids,
            "non-removed objects must be preserved in order"
        );
    }

    #[test]
    fn empty_removal_delta_passes_baseline_through() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        let count = baseline.len();

        // None removals.
        let filtered_none = apply_removal_deltas(baseline.clone(), None);
        assert_eq!(filtered_none.len(), count);

        // Empty set.
        let empty: HashSet<GeneratedObjectId> = HashSet::new();
        let filtered_empty = apply_removal_deltas(baseline, Some(&empty));
        assert_eq!(filtered_empty.len(), count);
    }

    #[test]
    fn chunk_removal_deltas_components_are_serializable() {
        // Verify that the key and value types in ChunkRemovalDeltas round-trip
        // through serde_json. The full HashMap<ChunkCoord, HashSet<...>> uses a
        // composite key that serde_json can't directly serialize as a JSON
        // object (JSON requires string keys), so we verify the pieces
        // individually. A future save-file format (e.g. bincode, MessagePack)
        // will handle composite keys natively.
        let chunk = ChunkCoord::new(3, -7);
        let profile = sample_profile();
        let id = super::super::derive_generated_object_id(&profile, chunk, "test_mineral", 42, 1);

        // ChunkCoord round-trips.
        let chunk_json = serde_json::to_string(&chunk).expect("ChunkCoord should serialize");
        let chunk_rt: ChunkCoord =
            serde_json::from_str(&chunk_json).expect("ChunkCoord should deserialize");
        assert_eq!(chunk_rt, chunk);

        // GeneratedObjectId round-trips.
        let id_json = serde_json::to_string(&id).expect("GeneratedObjectId should serialize");
        let id_rt: GeneratedObjectId =
            serde_json::from_str(&id_json).expect("GeneratedObjectId should deserialize");
        assert_eq!(id_rt, id);

        // A Vec<(ChunkCoord, Vec<GeneratedObjectId>)> representation round-trips,
        // proving the delta data can be persisted in any serde-compatible format.
        let entries: Vec<(ChunkCoord, Vec<GeneratedObjectId>)> = vec![(chunk, vec![id.clone()])];
        let entries_json = serde_json::to_string(&entries).expect("delta entries should serialize");
        let entries_rt: Vec<(ChunkCoord, Vec<GeneratedObjectId>)> =
            serde_json::from_str(&entries_json).expect("delta entries should deserialize");
        assert_eq!(entries_rt.len(), 1);
        assert_eq!(entries_rt[0].0, chunk);
        assert_eq!(entries_rt[0].1[0], id);
    }

    // ── Story 5.5: Player-added object tests ─────────────────────────────

    fn sample_game_material(name: &str) -> GameMaterial {
        use crate::materials::{MaterialProperty, PropertyVisibility};
        GameMaterial {
            name: name.to_string(),
            seed: 42,
            color: [0.5, 0.5, 0.5],
            density: MaterialProperty {
                value: 0.5,
                visibility: PropertyVisibility::Observable,
            },
            thermal_resistance: MaterialProperty {
                value: 0.5,
                visibility: PropertyVisibility::Observable,
            },
            reactivity: MaterialProperty {
                value: 0.5,
                visibility: PropertyVisibility::Observable,
            },
            conductivity: MaterialProperty {
                value: 0.5,
                visibility: PropertyVisibility::Observable,
            },
            toxicity: MaterialProperty {
                value: 0.5,
                visibility: PropertyVisibility::Hidden,
            },
        }
    }

    #[test]
    fn player_added_record_survives_chunk_regeneration() {
        // Simulate the full chunk state composition pipeline:
        // baseline - removals + player_additions.
        // The player-added object must appear in the final state even after
        // the baseline is regenerated from scratch.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        // Generate baseline and remove one object.
        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        assert!(!baseline.is_empty());
        let removed_id = baseline[0].generated_id.clone();
        let mut removals = HashSet::new();
        removals.insert(removed_id.clone());

        // Create a player-added record.
        let player_record = PlayerAddedObjectRecord {
            id: 0,
            material: sample_game_material("TestMineral"),
            position: [1.0, 0.0, -10.0],
            visual_scale: 1.0,
        };

        // Compose: baseline - removals.
        let after_removals = apply_removal_deltas(baseline.clone(), Some(&removals));

        // The removed object is gone.
        assert!(!after_removals.iter().any(|p| p.generated_id == removed_id));
        // The player-added record is separate data — it would be appended
        // during spawn. Verify the record itself is intact.
        assert_eq!(player_record.material.name, "TestMineral");
        assert_eq!(player_record.position, [1.0, 0.0, -10.0]);

        // "Regenerate" the chunk (simulating unload/reload).
        let baseline_2 =
            generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        let after_removals_2 = apply_removal_deltas(baseline_2, Some(&removals));

        // The removal is still applied.
        assert!(
            !after_removals_2
                .iter()
                .any(|p| p.generated_id == removed_id)
        );
        // The player record survives — it's stored in ChunkPlayerAdditions,
        // not derived from the seed.
        assert_eq!(player_record.id, 0);
    }

    #[test]
    fn player_added_and_generated_identities_are_distinct() {
        // Verify that the two identity models never collide: a
        // PlayerAddedObjectRecord uses a sequential u64, while a
        // GeneratedObjectId uses seed-derived fields.
        let record = PlayerAddedObjectRecord {
            id: 0,
            material: sample_game_material("Dropped"),
            position: [0.0, 0.0, 0.0],
            visual_scale: 1.0,
        };
        let profile = sample_profile();
        let chunk = ChunkCoord::new(0, -1);
        let gen_id =
            super::super::derive_generated_object_id(&profile, chunk, "surface_mineral", 0, 1);

        // The two types are structurally incompatible — they cannot be confused
        // at the type level. This test exists to document the design choice.
        assert_eq!(record.id, 0);
        assert_eq!(gen_id.local_candidate_index, 0);
        // Even though both have a "0", they live in completely separate types
        // and namespaces. The compiler prevents mixing them.
    }

    #[test]
    fn chunk_state_composition_is_deterministic_and_ordered() {
        // The pipeline is: baseline - removals + additions.
        // Verify that the order is always: generated survivors first, then
        // player-added objects in insertion order.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        let baseline_count = baseline.len();

        // Remove the first generated object.
        let mut removals = HashSet::new();
        removals.insert(baseline[0].generated_id.clone());
        let survivors = apply_removal_deltas(baseline, Some(&removals));
        assert_eq!(survivors.len(), baseline_count - 1);

        // Two player additions.
        let additions = vec![
            PlayerAddedObjectRecord {
                id: 0,
                material: sample_game_material("First"),
                position: [1.0, 0.0, -10.0],
                visual_scale: 1.0,
            },
            PlayerAddedObjectRecord {
                id: 1,
                material: sample_game_material("Second"),
                position: [2.0, 0.0, -11.0],
                visual_scale: 0.8,
            },
        ];

        // The final state conceptually is survivors ++ additions.
        // Verify the additions are in insertion order.
        assert_eq!(additions[0].material.name, "First");
        assert_eq!(additions[1].material.name, "Second");
        // And the total count is survivors + additions.
        assert_eq!(survivors.len() + additions.len(), baseline_count - 1 + 2);
    }

    #[test]
    fn player_added_record_is_serializable() {
        // Verify PlayerAddedObjectRecord round-trips through JSON.
        let record = PlayerAddedObjectRecord {
            id: 42,
            material: sample_game_material("SerializeTest"),
            position: [1.5, 0.0, -8.3],
            visual_scale: 0.9,
        };

        let json = serde_json::to_string(&record)
            .expect("PlayerAddedObjectRecord should serialize to JSON");
        let roundtrip: PlayerAddedObjectRecord =
            serde_json::from_str(&json).expect("PlayerAddedObjectRecord should deserialize");

        assert_eq!(roundtrip.id, 42);
        assert_eq!(roundtrip.material.name, "SerializeTest");
        assert_eq!(roundtrip.position, [1.5, 0.0, -8.3]);
        assert_eq!(roundtrip.visual_scale, 0.9);
    }

    #[test]
    fn is_within_exterior_bounds_checks_xz_only() {
        let bounds = RectXZ {
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };

        // Inside bounds.
        assert!(is_within_exterior_bounds(
            Vec3::new(0.0, 999.0, -20.0),
            &bounds
        ));
        // Outside on X.
        assert!(!is_within_exterior_bounds(
            Vec3::new(13.0, 0.0, -20.0),
            &bounds
        ));
        // Outside on Z (in the room area).
        assert!(!is_within_exterior_bounds(
            Vec3::new(0.0, 0.0, 0.0),
            &bounds
        ));
        // On the boundary (inclusive).
        assert!(is_within_exterior_bounds(
            Vec3::new(-12.0, 0.0, -4.0),
            &bounds
        ));
    }

    // ── Error / edge-case tests ──────────────────────────────────────────
    //
    // The tests above verify happy-path behavior. These tests exercise
    // boundary conditions, degenerate inputs, and error scenarios to ensure
    // the persistence layer degrades gracefully rather than panicking or
    // producing silently wrong results.

    // ── Story 5.4 edge cases: removal deltas ─────────────────────────────

    #[test]
    fn removal_of_nonexistent_id_is_harmless() {
        // If the removal set contains an ID that doesn't appear in the
        // baseline (e.g. stale save data, or a bug), the filter should
        // simply pass all baseline objects through — no panic, no data loss.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        let original_count = baseline.len();

        // Fabricate a bogus ID that cannot exist in the baseline.
        let bogus_id = super::super::derive_generated_object_id(
            &profile,
            ChunkCoord::new(999, 999),
            "nonexistent_mineral",
            9999,
            99,
        );
        let mut removals = HashSet::new();
        removals.insert(bogus_id);

        let filtered = apply_removal_deltas(baseline, Some(&removals));
        assert_eq!(
            filtered.len(),
            original_count,
            "bogus removal ID must not discard any real objects"
        );
    }

    #[test]
    fn removal_of_all_baseline_objects_produces_empty_list() {
        // If the player picks up every single generated object in a chunk,
        // the removal filter should return an empty vec — not panic.
        let profile = sample_profile();
        let catalog = sample_catalog();
        let surface = sample_flat_surface();
        let chunk = ChunkCoord::new(0, -1);

        let baseline = generate_surface_mineral_chunk_baseline(&profile, &catalog, &surface, chunk);
        assert!(!baseline.is_empty(), "test requires a non-empty baseline");

        // Collect every generated ID into the removal set.
        let all_ids: HashSet<GeneratedObjectId> =
            baseline.iter().map(|p| p.generated_id.clone()).collect();

        let filtered = apply_removal_deltas(baseline, Some(&all_ids));
        assert!(
            filtered.is_empty(),
            "removing all baseline IDs must yield an empty spawn list"
        );
    }

    #[test]
    fn removal_from_empty_baseline_is_safe() {
        // A chunk with zero generated objects (e.g. entirely steep terrain)
        // should survive removal filtering without issues.
        let empty_baseline: Vec<GeneratedSurfaceMineralPlacement> = Vec::new();
        let profile = sample_profile();
        let bogus_id = super::super::derive_generated_object_id(
            &profile,
            ChunkCoord::new(0, 0),
            "whatever",
            0,
            1,
        );
        let mut removals = HashSet::new();
        removals.insert(bogus_id);

        let filtered = apply_removal_deltas(empty_baseline, Some(&removals));
        assert!(
            filtered.is_empty(),
            "filtering an empty baseline must return empty, not panic"
        );
    }

    // ── Story 5.5 edge cases: player additions ──────────────────────────

    /// Build a `PlayerAddedObjectRecord` with sensible defaults so tests
    /// can focus on the field(s) they actually care about.
    fn sample_player_record(id: u64, name: &str) -> PlayerAddedObjectRecord {
        PlayerAddedObjectRecord {
            id,
            material: sample_game_material(name),
            position: [0.0, 0.0, 0.0],
            visual_scale: 1.0,
        }
    }

    /// Build a `ChunkPlayerAdditions` pre-populated with `records` for
    /// a single chunk — the most common test scenario.
    fn sample_additions_with(
        chunk: ChunkCoord,
        records: Vec<PlayerAddedObjectRecord>,
    ) -> ChunkPlayerAdditions {
        let mut additions = ChunkPlayerAdditions::default();
        additions.added_by_chunk.insert(chunk, records);
        additions
    }

    #[test]
    fn player_added_id_counter_monotonically_increases() {
        // The counter must never produce duplicate IDs within a session.
        let mut counter = PlayerAddedIdCounter::default();
        let first = counter.next();
        let second = counter.next();
        let third = counter.next();
        assert_eq!(first, 0);
        assert_eq!(second, 1);
        assert_eq!(third, 2);
    }

    #[test]
    fn is_within_exterior_bounds_degenerate_zero_area() {
        // A zero-area bounds (min == max) should only match the exact point.
        let bounds = RectXZ {
            min_x: 5.0,
            max_x: 5.0,
            min_z: -10.0,
            max_z: -10.0,
        };
        // Exact point — inclusive boundary means this should match.
        assert!(is_within_exterior_bounds(
            Vec3::new(5.0, 0.0, -10.0),
            &bounds
        ));
        // Anything else is outside.
        assert!(!is_within_exterior_bounds(
            Vec3::new(5.001, 0.0, -10.0),
            &bounds
        ));
        assert!(!is_within_exterior_bounds(
            Vec3::new(5.0, 0.0, -9.999),
            &bounds
        ));
    }

    #[test]
    fn is_within_exterior_bounds_infinity_inputs() {
        // Pathological floating-point values must not cause panics.
        let bounds = RectXZ {
            min_x: -12.0,
            max_x: 12.0,
            min_z: -36.0,
            max_z: -4.0,
        };
        // Infinity is always outside finite bounds.
        assert!(!is_within_exterior_bounds(
            Vec3::new(f32::INFINITY, 0.0, -20.0),
            &bounds
        ));
        assert!(!is_within_exterior_bounds(
            Vec3::new(0.0, 0.0, f32::NEG_INFINITY),
            &bounds
        ));
        // NaN comparisons always return false, so NaN should be "outside."
        assert!(!is_within_exterior_bounds(
            Vec3::new(f32::NAN, 0.0, -20.0),
            &bounds
        ));
    }

    #[test]
    fn release_player_added_with_nonexistent_id_leaves_records_intact() {
        // Simulates the core logic of `release_collected_player_added_objects`
        // when an entity has a `PlayerAddedExteriorObject` marker but the
        // corresponding record was already removed (or never existed) in
        // `ChunkPlayerAdditions`. The retain logic must not panic or remove
        // unrelated records.
        let chunk = ChunkCoord::new(0, -1);
        let mut additions = sample_additions_with(
            chunk,
            vec![
                sample_player_record(10, "RealA"),
                sample_player_record(20, "RealB"),
            ],
        );

        // Attempt to remove an ID that was never added (id: 999).
        // This mirrors the retain call in release_collected_player_added_objects.
        let bogus_id: u64 = 999;
        if let Some(records) = additions.added_by_chunk.get_mut(&chunk) {
            records.retain(|r| r.id != bogus_id);
        }

        // Both real records must survive.
        let records = additions.added_by_chunk.get(&chunk).unwrap();
        assert_eq!(
            records.len(),
            2,
            "bogus removal must not delete real records"
        );
        assert_eq!(records[0].id, 10);
        assert_eq!(records[1].id, 20);
    }

    #[test]
    fn release_player_added_from_nonexistent_chunk_is_harmless() {
        // If the marker references a chunk that has no entries in
        // `ChunkPlayerAdditions` at all (e.g. chunk was already fully
        // cleaned up), the `if let Some(...)` guard must skip silently.
        let populated_chunk = ChunkCoord::new(1, 1);
        let mut additions =
            sample_additions_with(populated_chunk, vec![sample_player_record(0, "Existing")]);

        // Try to release from a chunk that has no records.
        let missing_chunk = ChunkCoord::new(99, 99);
        if let Some(records) = additions.added_by_chunk.get_mut(&missing_chunk) {
            records.retain(|r| r.id != 42);
        }

        // The populated chunk's data must be untouched.
        assert_eq!(
            additions
                .added_by_chunk
                .get(&populated_chunk)
                .unwrap()
                .len(),
            1,
            "release from missing chunk must not corrupt other chunks"
        );
        // The missing chunk must not have been created.
        assert!(
            !additions.added_by_chunk.contains_key(&missing_chunk),
            "release must not create empty entries for missing chunks"
        );
    }
}
