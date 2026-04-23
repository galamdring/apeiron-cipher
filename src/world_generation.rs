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
//!
//! ## Surface Query Abstraction (Story 5.3)
//!
//! The placement pipeline does **not** assume `y = 0` or any fixed height.
//! Instead it queries a [`SurfaceProvider`] trait to resolve the surface at
//! any X/Z position. This trait is pure Rust — no Bevy `Query`, no ECS — so
//! the entire placement pipeline is unit-testable against synthetic surfaces
//! (flat, sloped, stepped, whatever) without rendering terrain or spinning up
//! an `App`.
//!
//! The current exterior is still flat, so [`FlatSurface`] is the live
//! implementation. But the placement code never knows that. When non-flat
//! terrain arrives later, a new [`SurfaceProvider`] implementation can slot in
//! without touching generation logic.

pub mod exterior;

use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::Player;
use crate::scene::PositionXZ;

// ── Surface Query Abstraction (Story 5.3) ────────────────────────────────
//
// WHY THIS IS PURE RUST AND NOT AN ECS QUERY:
//
// The story explicitly requires that placement logic be unit-testable against
// synthetic surfaces without rendering terrain. A trait with no Bevy dependency
// in its signature achieves that: tests implement the trait with whatever
// surface shape they want, and the same generation functions run in both tests
// and the live game.
//
// WHY PLACEMENT DOES NOT ASSUME y = 0:
//
// The current exterior happens to be flat at surface_y ≈ −0.01, but that is an
// implementation detail of FlatSurface. The generation pipeline never reads a
// hardcoded height — it always asks the SurfaceProvider. When the game adds
// hills, craters, or planet curvature, a new provider answers differently and
// the placement pipeline keeps working.

/// Result of querying the surface at a given X/Z world-space position.
///
/// This operates in **world space**, not sampling space. The caller provides a
/// world-space X/Z coordinate; the provider returns the world-space Y height
/// and the surface normal at that point.
///
/// ## Coordinate Convention
///
/// - `position_y`: the world-space height of the surface directly below (or at)
///   the queried X/Z. For a flat surface this is constant. For terrain with
///   elevation changes this varies per query.
/// - `normal`: the unit-length surface normal at the queried point. For a flat
///   horizontal surface this is `(0, 1, 0)`. For a slope tilted 45° toward +X
///   this would be approximately `(−0.707, 0.707, 0)`.
/// - `valid`: whether the surface exists and is usable at this location. A
///   query outside the playable area, over a void, or on geometry that has no
///   meaningful surface should return `valid = false`.
#[derive(Clone, Debug, PartialEq)]
pub struct SurfaceQueryResult {
    /// World-space Y height of the surface at the queried X/Z.
    pub position_y: f32,
    /// Unit-length surface normal at the queried point.
    ///
    /// Stored as `[x, y, z]` rather than a Bevy `Vec3` so the trait stays
    /// pure Rust with no Bevy dependency in its signature.
    pub normal: [f32; 3],
    /// Whether the surface is usable for placement at this location.
    ///
    /// `false` means "there is no ground here" — the caller should skip or
    /// retry this placement candidate.
    pub valid: bool,
}

impl SurfaceQueryResult {
    /// Compute the slope angle in radians between the surface normal and
    /// straight up `(0, 1, 0)`.
    ///
    /// Returns 0.0 for a perfectly flat horizontal surface.
    /// Returns π/2 (90°) for a vertical wall.
    ///
    /// The math: `cos(angle) = dot(normal, up)` where `up = (0, 1, 0)`,
    /// so `cos(angle) = normal.y`. We clamp to `[-1, 1]` before `acos` to
    /// guard against floating-point drift outside the valid domain.
    pub fn slope_angle_radians(&self) -> f32 {
        self.normal[1].clamp(-1.0, 1.0).acos()
    }
}

/// Pure Rust abstraction for querying the world surface at any X/Z position.
///
/// ## Why a trait instead of a function pointer or closure
///
/// A trait lets each implementation carry its own state (bounds, heightmap data,
/// config) without the generation functions needing to know what that state is.
/// Tests implement this with a two-line struct; the live game implements it from
/// `ExteriorGroundPatch`; future terrain systems implement it from heightmap
/// data. The generation code never changes.
///
/// ## Contract
///
/// - Implementations MUST be deterministic: the same X/Z input MUST produce the
///   same result every time. Non-deterministic surfaces break seed reproducibility.
/// - The normal MUST be unit-length (or very close). Placement code uses it for
///   slope checks and object orientation.
pub trait SurfaceProvider {
    /// Query the surface at the given world-space X/Z position.
    ///
    /// The provider returns the surface height, normal, and validity at that
    /// point. See [`SurfaceQueryResult`] for field semantics.
    fn query_surface(&self, x: f32, z: f32) -> SurfaceQueryResult;
}

/// The current flat exterior surface.
///
/// This is the live implementation of [`SurfaceProvider`] for the POC exterior.
/// It models a perfectly horizontal plane at `surface_y` within `bounds_xz`.
/// Any query outside the bounds returns `valid = false`.
///
/// When the game eventually adds non-flat terrain, this struct is not deleted —
/// it remains a valid (if boring) implementation. A new provider takes over for
/// terrain chunks that have actual elevation data.
#[derive(Clone, Debug)]
pub struct FlatSurface {
    /// The constant world-space Y height of the flat surface.
    pub surface_y: f32,
    /// The X/Z bounding rectangle where this surface is valid.
    ///
    /// Queries outside this rectangle return `valid = false` because there is
    /// no playable ground there.
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

impl SurfaceProvider for FlatSurface {
    fn query_surface(&self, x: f32, z: f32) -> SurfaceQueryResult {
        // A flat horizontal surface always has normal straight up and a
        // constant height. The only variable is whether the query point is
        // within the playable bounds.
        let in_bounds = x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z;
        SurfaceQueryResult {
            position_y: self.surface_y,
            normal: [0.0, 1.0, 0.0],
            valid: in_bounds,
        }
    }
}

/// Maximum slope angle (in radians) above which placement is rejected.
///
/// This default is approximately 40°. The value lives here as a module constant
/// rather than in a config file because it is a generation-pipeline parameter,
/// not a tuning knob the player or designer adjusts. If that changes later,
/// move it to the deposit catalog config.
///
/// Why 40°? It is steep enough that gentle hills and rolling terrain still get
/// deposits, but steep enough to reject cliff faces and near-vertical surfaces
/// where a "resting on the ground" deposit would look absurd.
pub const DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS: f32 = 0.6981; // ~40°

/// Check whether a surface query result is acceptable for object placement.
///
/// A candidate is rejected if:
/// - the surface is invalid at that location (`valid == false`)
/// - the slope exceeds the maximum allowed angle
///
/// This function is intentionally separate from [`SurfaceProvider`] so that
/// different object types could use different thresholds in the future without
/// changing the provider itself.
pub fn is_placement_valid(result: &SurfaceQueryResult, max_slope_radians: f32) -> bool {
    result.valid && result.slope_angle_radians() <= max_slope_radians
}

/// Compute the world-space up-axis rotation that aligns an object to a surface
/// normal.
///
/// For a flat surface (normal = `[0, 1, 0]`), this returns the identity
/// quaternion — no rotation needed. For a tilted surface, the object is rotated
/// so its local "up" axis matches the surface normal, giving placed objects a
/// natural lean on slopes.
///
/// The math: we compute the rotation from `(0, 1, 0)` to `normal` using the
/// cross-product (rotation axis) and dot-product (cosine of rotation angle).
/// The special case where the normal points straight down (antiparallel to up)
/// uses an arbitrary perpendicular axis since the cross product would be zero.
pub fn surface_alignment_rotation(normal: [f32; 3]) -> [f32; 4] {
    let up = [0.0_f32, 1.0, 0.0];
    let dot = normal[1]; // dot(up, normal) = normal.y since up = (0,1,0)

    // If normal ≈ up, no rotation needed.
    if dot > 0.9999 {
        return [0.0, 0.0, 0.0, 1.0]; // identity quaternion [x, y, z, w]
    }

    // If normal ≈ −up (surface points straight down), pick an arbitrary axis.
    if dot < -0.9999 {
        return [0.0, 0.0, 1.0, 0.0]; // 180° around Z
    }

    // Cross product: up × normal
    let cx = up[1] * normal[2] - up[2] * normal[1]; // = normal[2]
    let cy = up[2] * normal[0] - up[0] * normal[2]; // = 0
    let cz = up[0] * normal[1] - up[1] * normal[0]; // = -normal[0]

    // Quaternion from axis-angle: q = (axis * sin(θ/2), cos(θ/2))
    // Using the half-angle identity: q = (cross, 1 + dot) normalized.
    let w = 1.0 + dot;
    let len = (cx * cx + cy * cy + cz * cz + w * w).sqrt();
    let inv_len = 1.0 / len;

    [cx * inv_len, cy * inv_len, cz * inv_len, w * inv_len]
}

/// A stepped / terraced surface for testing non-flat terrain.
///
/// This divides the X axis into steps of `step_width` world units. Each step
/// has a different height: `base_y + step_index * step_height`. The surface
/// normal on each flat terrace is straight up `(0, 1, 0)`, but at the step
/// edges the normal tilts to indicate the slope transition.
///
/// This exists purely for testing AC2 and AC3. It is not used in the live game.
#[cfg(test)]
#[derive(Clone, Debug)]
pub struct SteppedSurface {
    /// Y height of the lowest step.
    pub base_y: f32,
    /// Width of each step along the X axis in world units.
    pub step_width: f32,
    /// Height difference between adjacent steps.
    pub step_height: f32,
    /// X/Z bounds — queries outside return `valid = false`.
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
    /// Width of the transition zone at each step edge where the normal tilts.
    /// Within this zone the surface linearly interpolates between step heights
    /// and the normal reflects the slope.
    pub edge_transition_width: f32,
}

#[cfg(test)]
impl SurfaceProvider for SteppedSurface {
    fn query_surface(&self, x: f32, z: f32) -> SurfaceQueryResult {
        let in_bounds = x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z;
        if !in_bounds {
            return SurfaceQueryResult {
                position_y: self.base_y,
                normal: [0.0, 1.0, 0.0],
                valid: false,
            };
        }

        // Which step are we on? Steps increase along +X.
        // step_index 0 starts at min_x.
        let relative_x = x - self.min_x;
        let step_float = relative_x / self.step_width;
        let step_index = step_float.floor();
        let frac_within_step = step_float - step_index;
        let position_within_step = frac_within_step * self.step_width;

        // The edge transition zone is at the END of each step (the riser
        // leading up to the next step).
        let flat_width = self.step_width - self.edge_transition_width;

        if position_within_step <= flat_width || self.edge_transition_width <= f32::EPSILON {
            // On the flat part of the step — normal is straight up.
            let y = self.base_y + step_index * self.step_height;
            SurfaceQueryResult {
                position_y: y,
                normal: [0.0, 1.0, 0.0],
                valid: true,
            }
        } else {
            // In the transition zone between this step and the next.
            // Linearly interpolate height and compute the corresponding slope.
            let t = (position_within_step - flat_width) / self.edge_transition_width;
            let y_low = self.base_y + step_index * self.step_height;
            let y_high = self.base_y + (step_index + 1.0) * self.step_height;
            let y = y_low + t * (y_high - y_low);

            // The slope of the riser: rise = step_height over run = edge_transition_width.
            // Normal perpendicular to slope direction (in the X/Y plane):
            // slope direction = (edge_transition_width, step_height, 0) normalized
            // normal = rotate 90° in X/Y plane = (-step_height, edge_transition_width, 0) normalized
            let nx = -self.step_height;
            let ny = self.edge_transition_width;
            let len = (nx * nx + ny * ny).sqrt();
            let normal = if len > f32::EPSILON {
                [nx / len, ny / len, 0.0]
            } else {
                [0.0, 1.0, 0.0]
            };

            SurfaceQueryResult {
                position_y: y,
                normal,
                valid: true,
            }
        }
    }
}

/// A simple tilted plane for testing slope rejection.
///
/// The surface tilts along the X axis: at `x = min_x` the height is `base_y`,
/// at `x = max_x` the height is `base_y + slope * (max_x - min_x)`. The
/// normal is constant everywhere (perpendicular to the slope direction).
///
/// This exists purely for testing. It is not used in the live game.
#[cfg(test)]
#[derive(Clone, Debug)]
pub struct TiltedSurface {
    /// Y height at `min_x`.
    pub base_y: f32,
    /// Rise per unit of X distance. A slope of 1.0 means 45° tilt.
    pub slope: f32,
    /// X/Z bounds.
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

#[cfg(test)]
impl SurfaceProvider for TiltedSurface {
    fn query_surface(&self, x: f32, z: f32) -> SurfaceQueryResult {
        let in_bounds = x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z;
        if !in_bounds {
            return SurfaceQueryResult {
                position_y: self.base_y,
                normal: [0.0, 1.0, 0.0],
                valid: false,
            };
        }

        let relative_x = x - self.min_x;
        let y = self.base_y + self.slope * relative_x;

        // The slope direction in the X/Y plane is (1, slope, 0).
        // The normal perpendicular to this is (-slope, 1, 0) normalized.
        let nx = -self.slope;
        let ny = 1.0;
        let len = (nx * nx + ny * ny).sqrt();
        let normal = [nx / len, ny / len, 0.0];

        SurfaceQueryResult {
            position_y: y,
            normal,
            valid: true,
        }
    }
}

const CONFIG_PATH: &str = "assets/config/world_generation.toml";
const PLACEMENT_DENSITY_CHANNEL: u64 = 0xD3E5_17A1_0000_0001;
const PLACEMENT_VARIATION_CHANNEL: u64 = 0xD3E5_17A1_0000_0002;
const OBJECT_IDENTITY_CHANNEL: u64 = 0xD3E5_17A1_0000_0003;

pub struct WorldGenerationPlugin;

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
pub struct PlanetSeed(pub u64);

/// Signed chunk coordinate on the exterior X/Z ground plane.
///
/// The coordinate is signed because we are treating the current exterior as a
/// local patch of a future planet surface. The long-term planet may wrap or
/// project these coordinates differently, but the first useful model is simply
/// "infinite signed grid on X/Z" rather than "special-case only positive
/// chunks near the room."
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq, Hash, Serialize, Deserialize)]
pub struct ChunkCoord {
    pub x: i32,
    pub z: i32,
}

impl ChunkCoord {
    pub const fn new(x: i32, z: i32) -> Self {
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
pub struct WorldGenerationConfig {
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
pub struct WorldProfile {
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
    pub fn from_config(config: &WorldGenerationConfig) -> Self {
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
pub struct ChunkGenerationKey {
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
pub struct GeneratedObjectId {
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
pub struct ActiveChunkNeighborhood {
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
pub fn world_position_to_chunk_coord(
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
pub fn chunk_origin_xz(chunk_coord: ChunkCoord, chunk_size_world_units: f32) -> PositionXZ {
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
fn active_chunk_neighborhood(center_chunk: ChunkCoord, radius: i32) -> Vec<ChunkCoord> {
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
pub fn derive_chunk_generation_key(
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

pub fn derive_generated_object_id(
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

    // ── Surface abstraction tests (Story 5.3) ────────────────────────────

    #[test]
    fn flat_surface_returns_constant_height_and_up_normal() {
        let surface = FlatSurface {
            surface_y: -0.01,
            min_x: -10.0,
            max_x: 10.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        let result = surface.query_surface(0.0, 0.0);
        assert!(result.valid);
        assert_eq!(result.position_y, -0.01);
        assert_eq!(result.normal, [0.0, 1.0, 0.0]);
        assert!((result.slope_angle_radians()).abs() < 0.001);
    }

    #[test]
    fn flat_surface_out_of_bounds_returns_invalid() {
        let surface = FlatSurface {
            surface_y: 0.0,
            min_x: -5.0,
            max_x: 5.0,
            min_z: -5.0,
            max_z: 5.0,
        };
        let result = surface.query_surface(100.0, 0.0);
        assert!(!result.valid);
    }

    #[test]
    fn tilted_surface_slope_angle_correct() {
        // slope = 1.0 means 45° tilt
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 1.0,
            min_x: -10.0,
            max_x: 10.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        let result = surface.query_surface(0.0, 0.0);
        assert!(result.valid);
        let angle_degrees = result.slope_angle_radians().to_degrees();
        assert!(
            (angle_degrees - 45.0).abs() < 1.0,
            "slope=1.0 should produce ~45° angle, got {angle_degrees}°"
        );
    }

    #[test]
    fn tilted_surface_height_varies_with_x() {
        let surface = TiltedSurface {
            base_y: 0.0,
            slope: 0.5,
            min_x: 0.0,
            max_x: 20.0,
            min_z: -10.0,
            max_z: 10.0,
        };
        let at_0 = surface.query_surface(0.0, 0.0);
        let at_10 = surface.query_surface(10.0, 0.0);
        assert_eq!(at_0.position_y, 0.0);
        assert_eq!(at_10.position_y, 5.0);
    }

    #[test]
    fn stepped_surface_flat_terrace_is_horizontal() {
        let surface = SteppedSurface {
            base_y: 0.0,
            step_width: 10.0,
            step_height: 2.0,
            min_x: 0.0,
            max_x: 40.0,
            min_z: -10.0,
            max_z: 10.0,
            edge_transition_width: 1.0,
        };
        // Middle of the first step (well before the transition zone)
        let result = surface.query_surface(3.0, 0.0);
        assert!(result.valid);
        assert_eq!(result.normal, [0.0, 1.0, 0.0]);
        assert_eq!(result.position_y, 0.0);
    }

    #[test]
    fn stepped_surface_riser_has_steep_normal() {
        let surface = SteppedSurface {
            base_y: 0.0,
            step_width: 10.0,
            step_height: 10.0, // very tall riser
            min_x: 0.0,
            max_x: 40.0,
            min_z: -10.0,
            max_z: 10.0,
            edge_transition_width: 1.0,
        };
        // In the transition zone near the end of the first step
        let result = surface.query_surface(9.5, 0.0);
        assert!(result.valid);
        let angle = result.slope_angle_radians().to_degrees();
        assert!(
            angle > 40.0,
            "steep riser should have slope > 40°, got {angle}°"
        );
    }

    #[test]
    fn is_placement_valid_accepts_flat_surface() {
        let result = SurfaceQueryResult {
            position_y: 0.0,
            normal: [0.0, 1.0, 0.0],
            valid: true,
        };
        assert!(is_placement_valid(
            &result,
            DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS
        ));
    }

    #[test]
    fn is_placement_valid_rejects_invalid_surface() {
        let result = SurfaceQueryResult {
            position_y: 0.0,
            normal: [0.0, 1.0, 0.0],
            valid: false,
        };
        assert!(!is_placement_valid(
            &result,
            DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS
        ));
    }

    #[test]
    fn is_placement_valid_rejects_steep_slope() {
        // 60° slope
        let cos60 = 0.5_f32;
        let sin60 = (1.0 - cos60 * cos60).sqrt();
        let result = SurfaceQueryResult {
            position_y: 0.0,
            normal: [-sin60, cos60, 0.0],
            valid: true,
        };
        assert!(!is_placement_valid(
            &result,
            DEFAULT_MAX_PLACEMENT_SLOPE_RADIANS
        ));
    }

    #[test]
    fn surface_alignment_rotation_identity_for_flat() {
        let [x, y, z, w] = surface_alignment_rotation([0.0, 1.0, 0.0]);
        assert!(
            (x.abs() + y.abs() + z.abs()) < 0.001,
            "should be near identity"
        );
        assert!((w - 1.0).abs() < 0.001);
    }

    #[test]
    fn surface_alignment_rotation_nontrivial_for_slope() {
        // A surface tilted ~30° toward +X
        let nx = -0.5_f32;
        let ny = (1.0 - nx * nx).sqrt();
        let [qx, qy, qz, qw] = surface_alignment_rotation([nx, ny, 0.0]);
        // Quaternion should not be identity
        let is_identity =
            qx.abs() < 0.001 && qy.abs() < 0.001 && qz.abs() < 0.001 && (qw - 1.0).abs() < 0.001;
        assert!(
            !is_identity,
            "tilted surface should produce non-identity rotation"
        );
        // Should be unit quaternion
        let len = (qx * qx + qy * qy + qz * qz + qw * qw).sqrt();
        assert!((len - 1.0).abs() < 0.01);
    }
}
