//! Deterministic exterior baseline generation for Epic 5 Story 5.2.
//!
//! Story 5.1 established "which world / chunk are we talking about?"
//! Story 5.2 answers the next question:
//! "what untouched baseline content appears in that chunk?"
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

use crate::carry::InCarry;
use crate::interaction::HeldItem;
use crate::materials::{MaterialCatalog, MaterialObject};
use crate::scene::{ExteriorGroundPatch, PositionXZ, RectXZ};
use crate::world_generation::{
    ActiveChunkNeighborhood, ChunkCoord, GeneratedObjectId, WorldProfile, chunk_origin_xz,
    derive_chunk_generation_key, derive_generated_object_id,
};

const DEPOSIT_CONFIG_PATH: &str = "assets/exterior/surface_mineral_deposits.toml";
const SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION: u32 = 1;

pub(crate) struct ExteriorGenerationPlugin;

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
pub(crate) struct SurfaceMineralDepositDefinition {
    pub key: String,
    pub material_key: String,
    pub selection_weight: f32,
    pub scale_min: f32,
    pub scale_max: f32,
}

/// Dedicated data source for Story 5.2 baseline exterior generation.
///
/// The story explicitly calls for a separate exterior-object data file instead
/// of hiding this in generator constants. We also keep the candidate spacing and
/// threshold here because they are part of "what this exterior object family
/// looks like in the world" rather than generic world foundation config.
#[derive(Clone, Debug, PartialEq, Resource, Serialize, Deserialize)]
pub(crate) struct SurfaceMineralDepositCatalog {
    #[serde(default = "default_candidate_spacing_world_units")]
    pub candidate_spacing_world_units: f32,
    #[serde(default = "default_density_field_scale_world_units")]
    pub density_field_scale_world_units: f32,
    #[serde(default = "default_spawn_threshold")]
    pub spawn_threshold: f32,
    #[serde(default = "default_jitter_fraction")]
    pub jitter_fraction: f32,
    #[serde(default = "default_surface_mineral_deposits")]
    pub deposits: Vec<SurfaceMineralDepositDefinition>,
}

impl Default for SurfaceMineralDepositCatalog {
    fn default() -> Self {
        Self {
            candidate_spacing_world_units: default_candidate_spacing_world_units(),
            density_field_scale_world_units: default_density_field_scale_world_units(),
            spawn_threshold: default_spawn_threshold(),
            jitter_fraction: default_jitter_fraction(),
            deposits: default_surface_mineral_deposits(),
        }
    }
}

fn default_candidate_spacing_world_units() -> f32 {
    6.0
}

fn default_density_field_scale_world_units() -> f32 {
    18.0
}

fn default_spawn_threshold() -> f32 {
    0.62
}

fn default_jitter_fraction() -> f32 {
    0.34
}

fn default_surface_mineral_deposits() -> Vec<SurfaceMineralDepositDefinition> {
    vec![
        SurfaceMineralDepositDefinition {
            key: "ferrite_surface_deposit".into(),
            material_key: "Ferrite".into(),
            selection_weight: 1.0,
            scale_min: 0.9,
            scale_max: 1.2,
        },
        SurfaceMineralDepositDefinition {
            key: "silite_surface_deposit".into(),
            material_key: "Silite".into(),
            selection_weight: 0.8,
            scale_min: 0.85,
            scale_max: 1.15,
        },
        SurfaceMineralDepositDefinition {
            key: "prismate_surface_deposit".into(),
            material_key: "Prismate".into(),
            selection_weight: 0.45,
            scale_min: 0.8,
            scale_max: 1.05,
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
pub(crate) struct GeneratedExteriorObject {
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
pub(crate) struct SurfaceMineralDeposit {
    pub definition_key: String,
}

/// Baseline generated placement before it becomes a live Bevy entity.
///
/// This separation is important for testing. The deterministic generation logic
/// should be testable as pure Rust data without rendering, ECS world setup, or
/// scene spawning.
#[derive(Clone, Debug, PartialEq)]
struct GeneratedSurfaceMineralPlacement {
    generated_id: GeneratedObjectId,
    definition_key: String,
    material_key: String,
    position_xz: PositionXZ,
    surface_y: f32,
    visual_scale: f32,
}

/// Active chunk baseline entities currently spawned into the world.
#[derive(Resource, Default)]
pub(crate) struct ActiveExteriorChunkSpawns {
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
            &exterior_patch,
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

            let entity = commands
                .spawn((
                    MaterialObject,
                    deposit_material.clone(),
                    SurfaceMineralDeposit {
                        definition_key: placement.definition_key.clone(),
                    },
                    GeneratedExteriorObject { home_chunk: chunk },
                    placement.generated_id.clone(),
                    Mesh3d(mesh),
                    MeshMaterial3d(render_material),
                    Transform::from_xyz(
                        placement.position_xz.x,
                        deposit_material.resting_center_y(placement.surface_y),
                        placement.position_xz.z,
                    )
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
            .remove::<SurfaceMineralDeposit>()
            .remove::<GeneratedObjectId>();
    }
}

fn generate_surface_mineral_chunk_baseline(
    profile: &WorldProfile,
    catalog: &SurfaceMineralDepositCatalog,
    exterior_patch: &ExteriorGroundPatch,
    chunk_coord: ChunkCoord,
) -> Vec<GeneratedSurfaceMineralPlacement> {
    let generation_key = derive_chunk_generation_key(profile, chunk_coord);
    let chunk_origin_xz = chunk_origin_xz(chunk_coord, profile.chunk_size_world_units);
    let spacing = catalog.candidate_spacing_world_units;
    let columns = (profile.chunk_size_world_units / spacing).ceil() as u32;
    let rows = (profile.chunk_size_world_units / spacing).ceil() as u32;
    let mut placements = Vec::new();
    let mut local_candidate_index = 0_u32;

    // The chunk uses a fixed candidate grid so "same chunk" always means the
    // same set of candidate identities. Continuous spatial variation then
    // decides which of those candidates actually become visible deposits.
    for row in 0..rows {
        for column in 0..columns {
            let cell_center_x = chunk_origin_xz.x + (column as f32 + 0.5) * spacing;
            let cell_center_z = chunk_origin_xz.z + (row as f32 + 0.5) * spacing;
            let cell_center_xz = PositionXZ::new(cell_center_x, cell_center_z);

            // The current exterior is a flat authored patch south of the room.
            // We only generate baseline deposits where that patch actually
            // exists. This keeps Story 5.2 honest: it is about deterministic
            // chunk content, not about pretending the whole future planet is
            // already physically rendered.
            if !rect_contains_xz(&exterior_patch.bounds_xz, cell_center_xz) {
                local_candidate_index += 1;
                continue;
            }

            // This density field is the "adjacent chunks feel related" part.
            // It samples a continuous world-position field, so nearby world
            // positions produce nearby values even when they live in different
            // chunks.
            let density = continuous_value_field_01(
                generation_key.placement_density_key,
                cell_center_xz,
                catalog.density_field_scale_world_units,
            );
            if density < catalog.spawn_threshold {
                local_candidate_index += 1;
                continue;
            }

            let Some(definition) = choose_deposit_definition(
                &catalog.deposits,
                generation_key.placement_variation_key,
                chunk_coord,
                local_candidate_index,
            ) else {
                local_candidate_index += 1;
                continue;
            };

            let jitter_offset = jitter_offset_xz(
                generation_key.placement_variation_key,
                chunk_coord,
                local_candidate_index,
                spacing * catalog.jitter_fraction,
            );
            let final_position_xz = PositionXZ::new(
                cell_center_xz.x + jitter_offset.x,
                cell_center_xz.z + jitter_offset.z,
            );

            if !rect_contains_xz(&exterior_patch.bounds_xz, final_position_xz) {
                local_candidate_index += 1;
                continue;
            }

            let scale_mix = unit_interval_01(mix_candidate_input(
                generation_key.placement_variation_key,
                chunk_coord,
                local_candidate_index,
                0x5500_0000_0000_0001,
            ));
            let visual_scale = lerp(definition.scale_min, definition.scale_max, scale_mix);

            placements.push(GeneratedSurfaceMineralPlacement {
                generated_id: derive_generated_object_id(
                    profile,
                    chunk_coord,
                    definition.key.clone(),
                    local_candidate_index,
                    SURFACE_MINERAL_DEPOSIT_GENERATOR_VERSION,
                ),
                definition_key: definition.key.clone(),
                material_key: definition.material_key.clone(),
                position_xz: final_position_xz,
                surface_y: exterior_patch.surface_y,
                visual_scale,
            });

            local_candidate_index += 1;
        }
    }

    placements
}

fn rect_contains_xz(bounds_xz: &RectXZ, position_xz: PositionXZ) -> bool {
    position_xz.x >= bounds_xz.min_x
        && position_xz.x <= bounds_xz.max_x
        && position_xz.z >= bounds_xz.min_z
        && position_xz.z <= bounds_xz.max_z
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
    use crate::world_generation::WorldGenerationConfig;

    fn sample_profile() -> WorldProfile {
        WorldProfile::from_config(&WorldGenerationConfig {
            planet_seed: 2026,
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
        })
    }

    fn sample_catalog() -> SurfaceMineralDepositCatalog {
        SurfaceMineralDepositCatalog {
            spawn_threshold: 0.0,
            ..SurfaceMineralDepositCatalog::default()
        }
    }

    fn sample_patch() -> ExteriorGroundPatch {
        ExteriorGroundPatch {
            bounds_xz: RectXZ {
                min_x: -12.0,
                max_x: 12.0,
                min_z: -36.0,
                max_z: -4.0,
            },
            surface_y: -0.01,
        }
    }

    #[test]
    fn same_chunk_regenerates_identically() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let patch = sample_patch();

        let a = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &patch,
            ChunkCoord::new(0, -1),
        );
        let b = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &patch,
            ChunkCoord::new(0, -1),
        );

        assert_eq!(a, b);
    }

    #[test]
    fn different_chunks_produce_different_baselines() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let patch = sample_patch();

        let a = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &patch,
            ChunkCoord::new(0, -1),
        );
        let b = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &patch,
            ChunkCoord::new(1, -1),
        );

        assert_ne!(a, b);
    }

    #[test]
    fn generated_object_ids_are_stable_from_explicit_inputs() {
        let profile = sample_profile();
        let catalog = sample_catalog();
        let patch = sample_patch();

        let placements = generate_surface_mineral_chunk_baseline(
            &profile,
            &catalog,
            &patch,
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
    fn deposit_catalog_toml_parses() {
        let toml_str = r#"
candidate_spacing_world_units = 6.0
density_field_scale_world_units = 18.0
spawn_threshold = 0.62
jitter_fraction = 0.34

[[deposits]]
key = "ferrite_surface_deposit"
material_key = "Ferrite"
selection_weight = 1.0
scale_min = 0.9
scale_max = 1.2
"#;

        let catalog: SurfaceMineralDepositCatalog =
            toml::from_str(toml_str).expect("surface deposit catalog should parse");

        assert_eq!(catalog.deposits.len(), 1);
        assert_eq!(catalog.deposits[0].material_key, "Ferrite");
    }
}
