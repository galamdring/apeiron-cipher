//! World-generation foundation for Epic 5.
//!
//! This module is intentionally the identity layer for the exterior world, not
//! the content layer. Story 5.1 does **not** place mineral deposits yet. It
//! answers the quieter questions that later stories will depend on:
//!
//! - Which planet are we on?
//! - How big is one exterior chunk?
//! - Which chunk contains a given world-space position?
//! - Which chunks are logically active around the player right now?
//! - How do later systems derive per-chunk deterministic inputs without
//!   sneaking in ambient randomness?
//!
//! The code is commented heavily on purpose. Deterministic generation is the
//! kind of system that can feel "obvious" when you just wrote it and opaque a
//! week later when you are trying to prove that nothing is secretly random.

use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::Player;
use crate::scene::PositionXZ;

const CONFIG_PATH: &str = "assets/config/world_generation.toml";
const PLACEMENT_DENSITY_CHANNEL: u64 = 0xD3E5_17A1_0000_0001;
const PLACEMENT_VARIATION_CHANNEL: u64 = 0xD3E5_17A1_0000_0002;
const OBJECT_IDENTITY_CHANNEL: u64 = 0xD3E5_17A1_0000_0003;

pub(crate) struct WorldGenerationPlugin;

impl Plugin for WorldGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGenerationConfig>()
            .init_resource::<WorldProfile>()
            .init_resource::<ActiveChunkNeighborhood>()
            .add_systems(PreStartup, load_world_generation_config)
            .add_systems(Update, update_active_chunk_neighborhood);
    }
}

/// Stable identifier for the currently loaded world / planet.
///
/// We load the initial seed from config in the POC rather than generating it at
/// runtime. That choice is deliberate: the point of Story 5.1 is to make
/// determinism obvious and testable. A config-backed seed means anyone can read
/// the world seed, rerun the game, and get the same foundational world state.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct PlanetSeed(pub u64);

/// Signed chunk coordinate on the exterior X/Z ground plane.
///
/// The coordinate is signed because we are treating the current exterior as a
/// local patch of a future planet surface. The long-term planet may wrap or
/// project these coordinates differently, but the first useful model is simply
/// "infinite signed grid on X/Z" rather than "special-case only positive
/// chunks near the room."
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub(crate) struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub(crate) const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// Runtime world-generation config loaded from `assets/config/world_generation.toml`.
///
/// Story 5.1 keeps this intentionally small:
/// - `planet_seed`: identity for the whole generated world
/// - `chunk_size_world_units`: how wide/deep one chunk is in Bevy world units
/// - `active_chunk_radius`: how many chunks around the player's chunk are
///   considered logically active
#[derive(Clone, Debug, Resource, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorldGenerationConfig {
    #[serde(default = "default_planet_seed")]
    pub planet_seed: u64,
    #[serde(default = "default_chunk_size_world_units")]
    pub chunk_size_world_units: f32,
    #[serde(default = "default_active_chunk_radius")]
    pub active_chunk_radius: i32,
}

impl Default for WorldGenerationConfig {
    fn default() -> Self {
        Self {
            planet_seed: default_planet_seed(),
            chunk_size_world_units: default_chunk_size_world_units(),
            active_chunk_radius: default_active_chunk_radius(),
        }
    }
}

fn default_planet_seed() -> u64 {
    20_260_408
}

fn default_chunk_size_world_units() -> f32 {
    // The story calls for 45 world units as the shipped default.
    //
    // Why 45?
    // - It is large enough that the current room + nearby exterior patch fit
    //   comfortably inside a single chunk.
    // - It is small enough that chunk boundaries are still meaningful once the
    //   player starts walking further into the exterior.
    // - It gives a useful "local neighborhood" scale for chunk activation
    //   without immediately forcing visual streaming work.
    45.0
}

fn default_active_chunk_radius() -> i32 {
    // Radius 1 means "player chunk plus the eight neighbors around it" for a
    // simple 3x3 active neighborhood. That is enough to prove the model
    // without pretending we already have full streaming and persistence.
    1
}

/// Derived deterministic world profile.
///
/// The profile exists so later stories do not have to keep reverse engineering
/// "which seed should I use for this purpose?" from the raw planet seed. We
/// derive explicit sub-seeds up front and document what each one is for.
#[derive(Clone, Debug, Resource, PartialEq, Serialize, Deserialize)]
pub(crate) struct WorldProfile {
    pub planet_seed: PlanetSeed,
    pub chunk_size_world_units: f32,
    pub active_chunk_radius: i32,
    pub placement_density_seed: u64,
    pub placement_variation_seed: u64,
    pub object_identity_seed: u64,
}

impl Default for WorldProfile {
    fn default() -> Self {
        Self::from_config(&WorldGenerationConfig::default())
    }
}

impl WorldProfile {
    pub(crate) fn from_config(config: &WorldGenerationConfig) -> Self {
        let planet_seed = PlanetSeed(config.planet_seed);

        Self {
            planet_seed,
            chunk_size_world_units: config.chunk_size_world_units,
            active_chunk_radius: config.active_chunk_radius,
            placement_density_seed: mix_seed(planet_seed.0, PLACEMENT_DENSITY_CHANNEL),
            placement_variation_seed: mix_seed(planet_seed.0, PLACEMENT_VARIATION_CHANNEL),
            object_identity_seed: mix_seed(planet_seed.0, OBJECT_IDENTITY_CHANNEL),
        }
    }
}

/// Stable per-chunk deterministic inputs that later stories can build on.
///
/// Later stories should not improvise their own "seed + coord + maybe some
/// magic constants" pattern. They should start from this explicit key so that
/// placement, object identity, and persistence all agree on what chunk they are
/// talking about.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub(crate) struct ChunkGenerationKey {
    pub chunk_coord: ChunkCoord,
    pub placement_density_key: u64,
    pub placement_variation_key: u64,
    pub object_identity_key: u64,
}

/// Stable identity for one generated baseline object.
///
/// We keep the identity fields explicit instead of hiding them behind a single
/// opaque hash. That makes later persistence stories easier to audit because a
/// saved removal record can literally say which planet, which chunk, which
/// object kind, and which deterministic local candidate it refers to.
#[derive(Clone, Debug, PartialEq, Eq, Hash, Serialize, Deserialize, Component)]
pub(crate) struct GeneratedObjectId {
    pub planet_seed: PlanetSeed,
    pub chunk_coord: ChunkCoord,
    pub object_kind_key: String,
    pub local_candidate_index: u32,
    pub generator_version: u32,
}

/// Runtime view of the chunks that are logically active around the player.
///
/// This resource is intentionally boring: it just names the player's current
/// chunk and the neighborhood around it. That is enough for future chunk
/// loading/unloading stories to build on without having to rediscover the
/// neighborhood math or re-derive it ad hoc in multiple systems.
#[derive(Clone, Debug, Default, Resource, PartialEq)]
pub(crate) struct ActiveChunkNeighborhood {
    pub center_chunk: Option<ChunkCoord>,
    pub center_chunk_origin_xz: Option<PositionXZ>,
    pub center_chunk_generation_key: Option<ChunkGenerationKey>,
    pub radius: i32,
    pub chunks: Vec<ChunkCoord>,
}

fn load_world_generation_config(mut commands: Commands) {
    let config = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<WorldGenerationConfig>(&contents) {
                Ok(config) => {
                    info!("Loaded world-generation config from {CONFIG_PATH}");
                    config
                }
                Err(error) => {
                    warn!("Malformed {CONFIG_PATH}, using defaults: {error}");
                    WorldGenerationConfig::default()
                }
            },
            Err(error) => {
                warn!("Could not read {CONFIG_PATH}, using defaults: {error}");
                WorldGenerationConfig::default()
            }
        }
    } else {
        warn!("{CONFIG_PATH} not found, using defaults");
        WorldGenerationConfig::default()
    };

    let profile = WorldProfile::from_config(&config);
    commands.insert_resource(config);
    commands.insert_resource(profile);
}

fn update_active_chunk_neighborhood(
    profile: Res<WorldProfile>,
    mut active_chunks: ResMut<ActiveChunkNeighborhood>,
    player_query: Query<&Transform, With<Player>>,
) {
    let Ok(player_transform) = player_query.single() else {
        active_chunks.center_chunk = None;
        active_chunks.center_chunk_origin_xz = None;
        active_chunks.center_chunk_generation_key = None;
        active_chunks.radius = profile.active_chunk_radius;
        active_chunks.chunks.clear();
        return;
    };

    let player_position_xz = PositionXZ::new(
        player_transform.translation.x,
        player_transform.translation.z,
    );
    let center_chunk =
        world_position_to_chunk_coord(player_position_xz, profile.chunk_size_world_units);
    let center_chunk_origin_xz = chunk_origin_xz(center_chunk, profile.chunk_size_world_units);
    let center_chunk_generation_key = derive_chunk_generation_key(&profile, center_chunk);
    let chunks = active_chunk_neighborhood(center_chunk, profile.active_chunk_radius);

    active_chunks.center_chunk = Some(center_chunk);
    active_chunks.center_chunk_origin_xz = Some(center_chunk_origin_xz);
    active_chunks.center_chunk_generation_key = Some(center_chunk_generation_key);
    active_chunks.radius = profile.active_chunk_radius;
    active_chunks.chunks = chunks;
}

/// Convert a world-space X/Z position into the containing chunk coordinate.
///
/// The important detail here is `floor`, especially for negative positions.
/// We do **not** want integer truncation toward zero:
/// - `44.9 / 45.0` should be chunk `0`
/// - `45.0 / 45.0` should be chunk `1`
/// - `-0.1 / 45.0` should be chunk `-1`
///
/// That last case is why floor matters. Truncation would incorrectly map
/// slightly-negative positions back to chunk `0`.
pub(crate) fn world_position_to_chunk_coord(
    position_xz: PositionXZ,
    chunk_size_world_units: f32,
) -> ChunkCoord {
    debug_assert!(
        chunk_size_world_units > 0.0,
        "chunk size must be positive to derive chunk coordinates"
    );

    let chunk_x = (position_xz.x / chunk_size_world_units).floor() as i32;
    let chunk_z = (position_xz.z / chunk_size_world_units).floor() as i32;
    ChunkCoord::new(chunk_x, chunk_z)
}

/// Return the world-space X/Z origin of the given chunk.
///
/// "Origin" here means the minimum X/minimum Z corner of the chunk on the
/// ground plane, not the center of the chunk.
pub(crate) fn chunk_origin_xz(chunk_coord: ChunkCoord, chunk_size_world_units: f32) -> PositionXZ {
    PositionXZ::new(
        chunk_coord.x as f32 * chunk_size_world_units,
        chunk_coord.z as f32 * chunk_size_world_units,
    )
}

/// Return the stable square neighborhood around a center chunk.
///
/// We intentionally use a square neighborhood because chunk activation is a
/// grid concern, not a radial-distance concern. Radius 1 means:
/// - one chunk to the west/east
/// - one chunk to the north/south
/// - and the four diagonal neighbors
///
/// The nested loop order is stable, so any later story that iterates this list
/// gets the same ordering every run.
pub(crate) fn active_chunk_neighborhood(center_chunk: ChunkCoord, radius: i32) -> Vec<ChunkCoord> {
    let mut chunks = Vec::new();

    for dz in -radius..=radius {
        for dx in -radius..=radius {
            chunks.push(ChunkCoord::new(center_chunk.x + dx, center_chunk.z + dz));
        }
    }

    chunks
}

/// Derive stable per-chunk generation keys from the world profile and chunk.
///
/// We mix the profile's purpose-specific seeds with the chunk coordinate so that:
/// - the same planet + same chunk always gets the same keys
/// - different chunks on the same planet get different keys
/// - later systems can tell which key is meant for which job
pub(crate) fn derive_chunk_generation_key(
    profile: &WorldProfile,
    chunk_coord: ChunkCoord,
) -> ChunkGenerationKey {
    let chunk_mixer = mix_chunk_coord(profile.planet_seed, chunk_coord);

    ChunkGenerationKey {
        chunk_coord,
        placement_density_key: mix_seed(profile.placement_density_seed, chunk_mixer),
        placement_variation_key: mix_seed(profile.placement_variation_seed, chunk_mixer),
        object_identity_key: mix_seed(profile.object_identity_seed, chunk_mixer),
    }
}

/// Deterministically mix a base seed and a channel into a new 64-bit value.
///
/// This is a SplitMix64-style bit mixer. The algorithm is deterministic, cheap,
/// and requires no external crate. We are not using it as a cryptographic hash.
/// We are using it to avalanche nearby integer inputs into well-mixed outputs
/// so that later generation systems do not accidentally treat "similar number"
/// as "similar world feature."
fn mix_seed(base: u64, channel: u64) -> u64 {
    let mut z = base.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^ (z >> 31)
}

/// Pack the signed chunk coordinate into a deterministic mixer input.
///
/// We cast each signed axis through `u32` before widening to `u64` so the exact
/// bit pattern of negative coordinates is preserved in a stable, explicit way.
/// That makes `(-1, 0)` a genuinely different chunk identity from `(0, 0)` or
/// `(1, 0)` instead of relying on ambiguous string formatting or ad hoc math.
fn mix_chunk_coord(planet_seed: PlanetSeed, chunk_coord: ChunkCoord) -> u64 {
    let packed_x = chunk_coord.x as u32 as u64;
    let packed_z = chunk_coord.z as u32 as u64;
    let packed = (packed_x << 32) | packed_z;
    mix_seed(planet_seed.0, packed)
}

pub(crate) fn derive_generated_object_id(
    profile: &WorldProfile,
    chunk_coord: ChunkCoord,
    object_kind_key: impl Into<String>,
    local_candidate_index: u32,
    generator_version: u32,
) -> GeneratedObjectId {
    GeneratedObjectId {
        planet_seed: profile.planet_seed,
        chunk_coord,
        object_kind_key: object_kind_key.into(),
        local_candidate_index,
        generator_version,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn world_profile_derivation_is_deterministic() {
        let config = WorldGenerationConfig {
            planet_seed: 123_456,
            chunk_size_world_units: 45.0,
            active_chunk_radius: 2,
        };

        let a = WorldProfile::from_config(&config);
        let b = WorldProfile::from_config(&config);

        assert_eq!(a, b);
    }

    #[test]
    fn world_profile_derives_distinct_sub_seeds() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig::default());

        assert_ne!(
            profile.placement_density_seed,
            profile.placement_variation_seed
        );
        assert_ne!(profile.placement_density_seed, profile.object_identity_seed);
        assert_ne!(
            profile.placement_variation_seed,
            profile.object_identity_seed
        );
    }

    #[test]
    fn world_position_inside_same_chunk_maps_to_same_coord() {
        let chunk_size = 45.0;
        let a = world_position_to_chunk_coord(PositionXZ::new(0.0, -10.0), chunk_size);
        let b = world_position_to_chunk_coord(PositionXZ::new(44.99, -0.01), chunk_size);

        assert_eq!(a, ChunkCoord::new(0, -1));
        assert_eq!(a, b);
    }

    #[test]
    fn world_position_crossing_chunk_boundary_changes_coord() {
        let chunk_size = 45.0;
        let before = world_position_to_chunk_coord(PositionXZ::new(44.99, 89.99), chunk_size);
        let after = world_position_to_chunk_coord(PositionXZ::new(45.0, 90.0), chunk_size);

        assert_eq!(before, ChunkCoord::new(0, 1));
        assert_eq!(after, ChunkCoord::new(1, 2));
    }

    #[test]
    fn world_position_uses_floor_for_negative_coordinates() {
        let chunk_size = 45.0;
        let slightly_negative =
            world_position_to_chunk_coord(PositionXZ::new(-0.01, -0.01), chunk_size);
        let more_negative =
            world_position_to_chunk_coord(PositionXZ::new(-45.01, -90.0), chunk_size);

        assert_eq!(slightly_negative, ChunkCoord::new(-1, -1));
        assert_eq!(more_negative, ChunkCoord::new(-2, -2));
    }

    #[test]
    fn chunk_origin_xz_returns_min_corner_of_chunk() {
        let origin = chunk_origin_xz(ChunkCoord::new(-2, 3), 45.0);
        assert_eq!(origin.x, -90.0);
        assert_eq!(origin.z, 135.0);
    }

    #[test]
    fn active_chunk_neighborhood_uses_configured_radius() {
        let center = ChunkCoord::new(5, -2);
        let chunks = active_chunk_neighborhood(center, 1);

        assert_eq!(chunks.len(), 9);
        assert_eq!(chunks.first().copied(), Some(ChunkCoord::new(4, -3)));
        assert_eq!(chunks.last().copied(), Some(ChunkCoord::new(6, -1)));
        assert!(chunks.contains(&center));
    }

    #[test]
    fn chunk_generation_key_is_deterministic_for_same_inputs() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig {
            planet_seed: 777,
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
        });
        let chunk = ChunkCoord::new(-3, 4);

        let a = derive_chunk_generation_key(&profile, chunk);
        let b = derive_chunk_generation_key(&profile, chunk);

        assert_eq!(a, b);
    }

    #[test]
    fn chunk_generation_key_changes_for_different_chunks() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig::default());
        let a = derive_chunk_generation_key(&profile, ChunkCoord::new(0, 0));
        let b = derive_chunk_generation_key(&profile, ChunkCoord::new(1, 0));

        assert_ne!(a.placement_density_key, b.placement_density_key);
        assert_ne!(a.placement_variation_key, b.placement_variation_key);
        assert_ne!(a.object_identity_key, b.object_identity_key);
    }

    #[test]
    fn generated_object_id_is_stable_from_explicit_inputs() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig {
            planet_seed: 42,
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
        });

        let a =
            derive_generated_object_id(&profile, ChunkCoord::new(-2, 3), "ferrite_surface", 7, 1);
        let b =
            derive_generated_object_id(&profile, ChunkCoord::new(-2, 3), "ferrite_surface", 7, 1);

        assert_eq!(a, b);
    }
}
