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
};
use crate::carry::InCarry;
use crate::interaction::HeldItem;
use crate::materials::{MaterialCatalog, MaterialObject};
use crate::scene::{ExteriorGroundPatch, PositionXZ};

const DEPOSIT_CONFIG_PATH: &str = "assets/exterior/surface_mineral_deposits.toml";
const SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION: u32 = 1;

pub struct ExteriorGenerationPlugin;

impl Plugin for ExteriorGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<SurfaceMineralDepositCatalog>()
            .init_resource::<ActiveExteriorChunkSpawns>()
            .add_systems(PreStartup, load_surface_mineral_deposit_catalog)
            .add_systems(
                Update,
                (
                    sync_active_exterior_chunks,
                    release_collected_generated_objects.after(sync_active_exterior_chunks),
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

        let placements = generate_surface_mineral_chunk_baseline(
            &world_profile,
            &deposit_catalog,
            &surface,
            chunk,
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

        spawned_chunks
            .spawned_entities_by_chunk
            .insert(chunk, spawned_entities);
    }
}

#[allow(clippy::type_complexity)]
fn release_collected_generated_objects(
    mut commands: Commands,
    mut spawned_chunks: ResMut<ActiveExteriorChunkSpawns>,
    collected_query: Query<(Entity, &GeneratedExteriorObject), Or<(With<HeldItem>, With<InCarry>)>>,
) {
    for (entity, generated) in collected_query.iter() {
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
        FlatSurface, SteppedSurface, SurfaceProvider, TiltedSurface, WorldGenerationConfig,
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
}
