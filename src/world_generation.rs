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
//! The live implementation is [`PlanetSurface`], which uses multi-octave noise
//! for elevation. Test-only synthetic surfaces live in `test_support`. The
//! placement code never assumes a specific provider — when terrain evolves, a
//! new [`SurfaceProvider`] slots in without touching generation logic.

pub mod exterior;

use std::collections::HashMap;
use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::player::Player;
use crate::scene::PositionXZ;
use crate::seed_util::{
    BIOME_CLIMATE_CHANNEL, ELEVATION_CHANNEL, ELEVATION_DETAIL_CHANNEL, OBJECT_IDENTITY_CHANNEL,
    PLACEMENT_DENSITY_CHANNEL, PLACEMENT_VARIATION_CHANNEL, PLANET_SURFACE_RADIUS_CHANNEL,
    mix_seed,
};
use crate::solar_system::{
    OrbitalConfig, OrbitalLayout, PlanetEnvironment, PlanetEnvironmentConfig,
    SolarSystemRegistries, SolarSystemSeed, StarProfile, StarTypeRegistry, derive_orbital_layout,
    derive_planet_environment, derive_star_profile,
};

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

/// Noise-based terrain surface for runtime use.
///
/// `PlanetSurface` samples a multi-octave value noise field (reusing
/// `continuous_value_field_01` at different scales) to produce elevation and
/// surface normals across the planet. It handles torus wrapping at world-
/// coordinate level so that terrain is continuous across the wrap seam.
///
/// This struct is the runtime replacement for `FlatSurface` in the exterior
/// chunk pipeline. Synthetic test surfaces live in `test_support`.
#[derive(Clone, Debug)]
pub struct PlanetSurface {
    /// Per-planet elevation seed (from `WorldProfile::elevation_seed`).
    pub elevation_seed: u64,
    /// Sea-level reference height (world units).
    pub base_y: f32,
    /// Maximum height deviation from `base_y` (world units).
    pub amplitude: f32,
    /// Base noise frequency (world units). Lower = broader features.
    pub frequency: f32,
    /// Number of fractal noise octaves layered additively.
    pub octaves: u32,
    /// Blend weight for chunk-level detail noise (0.0 = disabled).
    pub detail_weight: f32,
    /// Seed for the detail noise layer, derived from `elevation_seed` via
    /// `ELEVATION_DETAIL_CHANNEL` so it is independent of base octaves.
    pub detail_seed: u64,
    /// Base frequency for the detail noise layer. Should be higher than the
    /// base `frequency` to add fine-grained variation.
    pub detail_frequency: f32,
    /// Number of fractal noise octaves for the detail layer.
    pub detail_octaves: u32,
    /// Planet surface diameter in chunks (for torus wrapping).
    pub planet_surface_diameter: i32,
    /// Chunk edge length in world units (for torus wrapping).
    pub chunk_size_world_units: f32,
}

impl PlanetSurface {
    /// Construct a `PlanetSurface` from a derived `WorldProfile` and the raw
    /// config that carries tuning parameters.
    ///
    /// This is the single blessed constructor — every runtime use should go
    /// through here so elevation parameters are consistent.
    pub fn new_from_profile(profile: &WorldProfile, config: &WorldGenerationConfig) -> Self {
        Self {
            elevation_seed: profile.elevation_seed,
            base_y: config.elevation_base_y,
            amplitude: config.elevation_amplitude,
            frequency: config.elevation_frequency,
            octaves: config.elevation_octaves,
            detail_weight: config.elevation_detail_weight,
            detail_seed: mix_seed(profile.elevation_seed, ELEVATION_DETAIL_CHANNEL),
            detail_frequency: config.elevation_detail_frequency,
            detail_octaves: config.elevation_detail_octaves,
            planet_surface_diameter: profile.planet_surface_diameter,
            chunk_size_world_units: profile.chunk_size_world_units,
        }
    }

    /// Wrap a world-space coordinate to the canonical torus range.
    ///
    /// The planet surface spans `[0, diameter * chunk_size)` on both axes.
    /// Positions outside that range are wrapped using Euclidean modulo.
    ///
    /// **Note:** This is no longer used by `sample_elevation` (which now uses
    /// `abs()` folding for seam continuity) but is retained for other torus
    /// systems that may need canonical wrapping (e.g. chunk activation).
    #[allow(dead_code)]
    fn wrap_world_coord(&self, v: f32) -> f32 {
        let period = self.planet_surface_diameter as f32 * self.chunk_size_world_units;
        ((v % period) + period) % period
    }

    /// Fold a world-space coordinate for seamless elevation sampling.
    ///
    /// First wraps to the canonical torus range `[0, period)` via Euclidean
    /// modulo, then mirrors around the midpoint so the noise field is symmetric
    /// at the torus seam (coordinate 0 / period). The result is always in
    /// `[0, period/2]`, which keeps lattice integers small and guarantees C0
    /// continuity at the seam boundary.
    fn fold_elevation_coord(&self, v: f32) -> f32 {
        let period = self.planet_surface_diameter as f32 * self.chunk_size_world_units;
        let wrapped = ((v % period) + period) % period; // [0, period)
        let half = period * 0.5;
        // Mirror: values past the midpoint fold back.
        if wrapped > half {
            period - wrapped
        } else {
            wrapped
        }
    }

    /// Sample multi-octave elevation at an arbitrary world-space XZ.
    ///
    /// Coordinates are folded via [`fold_elevation_coord`] before noise
    /// sampling.  This wraps to the torus range and then mirrors around the
    /// midpoint, guaranteeing C0 continuity at the torus seam while keeping
    /// lattice integers within safe i32 bounds.  The underlying hash-based
    /// noise is **not** periodic, so a plain Euclidean-mod wrap produced a
    /// hard elevation discontinuity at the seam.  Folding eliminates the
    /// discontinuity at the cost of mirrored terrain shape near the seam
    /// edges — deposits, biomes, and flora still use real coordinates so
    /// visual symmetry is masked.  If a different seam strategy is needed
    /// later (e.g. bridge chunks or truly periodic noise), only this function
    /// and `compute_normal` need to change.
    ///
    /// Each octave doubles the frequency and halves the amplitude (standard fBm
    /// with lacunarity 2, persistence 0.5). The base `continuous_value_field_01`
    /// returns values in `[0, 1]`, so we center each sample around 0.5 to get
    /// positive and negative deviations from `base_y`.
    pub(crate) fn sample_elevation(&self, x: f32, z: f32) -> f32 {
        let x = self.fold_elevation_coord(x);
        let z = self.fold_elevation_coord(z);
        let mut total = 0.0_f32;
        let mut amp = 1.0_f32;
        let mut freq = self.frequency;
        let mut weight_sum = 0.0_f32;

        for octave in 0..self.octaves {
            // Each octave uses a slightly different seed so the layers are
            // independent. We mix the elevation seed with the octave index.
            let octave_seed = mix_seed(self.elevation_seed, octave as u64);
            let scale = 1.0 / freq; // continuous_value_field_01 divides by scale
            let sample =
                exterior::continuous_value_field_01(octave_seed, PositionXZ::new(x, z), scale);
            // Center around 0: value noise returns [0,1], shift to [-0.5, 0.5].
            total += (sample - 0.5) * amp;
            weight_sum += amp;
            amp *= 0.5;
            freq *= 2.0;
        }

        // Normalize so the sum of weights = 1, then scale by amplitude.
        if weight_sum > 0.0 {
            total /= weight_sum;
        }
        let base_elevation = total * self.amplitude;

        // --- Detail noise layer (chunk-level, higher frequency) ---
        // Blended additively when detail_weight > 0. Uses a separate seed
        // sub-channel so the detail pattern is independent of base octaves.
        let detail_elevation = if self.detail_weight > 0.0 {
            let mut d_total = 0.0_f32;
            let mut d_amp = 1.0_f32;
            let mut d_freq = self.detail_frequency;
            let mut d_weight_sum = 0.0_f32;

            for octave in 0..self.detail_octaves {
                let octave_seed = mix_seed(self.detail_seed, octave as u64);
                let scale = 1.0 / d_freq;
                let sample =
                    exterior::continuous_value_field_01(octave_seed, PositionXZ::new(x, z), scale);
                d_total += (sample - 0.5) * d_amp;
                d_weight_sum += d_amp;
                d_amp *= 0.5;
                d_freq *= 2.0;
            }

            if d_weight_sum > 0.0 {
                d_total /= d_weight_sum;
            }
            d_total * self.amplitude * self.detail_weight
        } else {
            0.0
        };

        self.base_y + base_elevation + detail_elevation
    }

    /// Compute the surface normal from the heightmap gradient using finite
    /// differences.
    ///
    /// We sample elevation at four points around (x, z) offset by `epsilon`,
    /// then compute the cross product of the two tangent vectors to get the
    /// surface normal. The epsilon is small enough for accuracy but large
    /// enough to avoid floating-point noise.
    fn compute_normal(&self, x: f32, z: f32) -> [f32; 3] {
        let eps = 0.1_f32;

        let hx_pos = self.sample_elevation(x + eps, z);
        let hx_neg = self.sample_elevation(x - eps, z);
        let hz_pos = self.sample_elevation(x, z + eps);
        let hz_neg = self.sample_elevation(x, z - eps);

        // Finite-difference partial derivatives:
        let dh_dx = (hx_pos - hx_neg) / (2.0 * eps);
        let dh_dz = (hz_pos - hz_neg) / (2.0 * eps);

        // The surface is parameterized as P(x,z) = (x, h(x,z), z).
        // tangent_x = (1, dh_dx, 0), tangent_z = (0, dh_dz, 1).
        // normal = cross(tangent_z, tangent_x) for upward-pointing Y:
        //        = (-dh_dx, 1, -dh_dz)
        let nx = -dh_dx;
        let ny = 1.0_f32;
        let nz = -dh_dz;
        let len = (nx * nx + ny * ny + nz * nz).sqrt();
        if len < 1e-10 {
            return [0.0, 1.0, 0.0];
        }
        let inv = 1.0 / len;
        [nx * inv, ny * inv, nz * inv]
    }
}

impl SurfaceProvider for PlanetSurface {
    fn query_surface(&self, x: f32, z: f32) -> SurfaceQueryResult {
        let position_y = self.sample_elevation(x, z);
        let normal = self.compute_normal(x, z);
        SurfaceQueryResult {
            position_y,
            normal,
            valid: true,
        }
    }
}

/// Generate a subdivided heightmap mesh for a single chunk.
///
/// The mesh is a grid of `subdivisions × subdivisions` quads
/// (`(subdivisions+1)²` vertices). Each vertex samples the elevation from
/// `surface.query_surface` so the mesh follows the terrain contour.
///
/// ## Coordinate space
///
/// The returned mesh is in **world space**. Vertex positions use absolute
/// world X/Y/Z so the caller can spawn the entity at `Transform::IDENTITY`
/// (or at the origin) — no additional translation is required beyond what
/// the caller already applies.
///
/// ## Normals
///
/// Per-vertex normals are computed from the cross product of adjacent vertex
/// differences (the heightmap gradient). This gives smooth shading across
/// the chunk and feeds into slope rejection for deposit placement.
///
/// ## UVs
///
/// UV coordinates span `[0, 1]` across the chunk so textures can be applied
/// later without revisiting mesh generation.
pub fn generate_chunk_heightmap_mesh(
    surface: &PlanetSurface,
    chunk_coord: ChunkCoord,
    subdivisions: u32,
) -> Mesh {
    let subdivisions = subdivisions.max(1);
    let verts_per_edge = subdivisions + 1;
    let num_verts = (verts_per_edge * verts_per_edge) as usize;

    let origin = chunk_origin_xz(chunk_coord, surface.chunk_size_world_units);
    let chunk_size = surface.chunk_size_world_units;
    let step = chunk_size / subdivisions as f32;

    let mut positions: Vec<[f32; 3]> = Vec::with_capacity(num_verts);
    let mut uvs: Vec<[f32; 2]> = Vec::with_capacity(num_verts);

    // First pass: sample elevation at each grid vertex to build the position
    // and UV arrays. We store heights in a flat grid so the second pass can
    // compute normals from adjacent vertex height differences.
    let mut heights: Vec<f32> = Vec::with_capacity(num_verts);

    for iz in 0..verts_per_edge {
        for ix in 0..verts_per_edge {
            let world_x = origin.x + ix as f32 * step;
            let world_z = origin.z + iz as f32 * step;
            let y = surface.sample_elevation(world_x, world_z);

            positions.push([world_x, y, world_z]);
            heights.push(y);
            uvs.push([
                ix as f32 / subdivisions as f32,
                iz as f32 / subdivisions as f32,
            ]);
        }
    }

    // Second pass: compute per-vertex normals from the cross product of
    // adjacent vertex height differences. For interior vertices we use
    // central differences; at edges we clamp to the nearest neighbor.
    let mut normals: Vec<[f32; 3]> = Vec::with_capacity(num_verts);
    let idx = |ix: u32, iz: u32| -> usize { (iz * verts_per_edge + ix) as usize };

    for iz in 0..verts_per_edge {
        for ix in 0..verts_per_edge {
            // Height differences along x-axis (central difference when possible).
            let dh_dx = if ix == 0 {
                (heights[idx(ix + 1, iz)] - heights[idx(ix, iz)]) / step
            } else if ix == subdivisions {
                (heights[idx(ix, iz)] - heights[idx(ix - 1, iz)]) / step
            } else {
                (heights[idx(ix + 1, iz)] - heights[idx(ix - 1, iz)]) / (2.0 * step)
            };

            // Height differences along z-axis.
            let dh_dz = if iz == 0 {
                (heights[idx(ix, iz + 1)] - heights[idx(ix, iz)]) / step
            } else if iz == subdivisions {
                (heights[idx(ix, iz)] - heights[idx(ix, iz - 1)]) / step
            } else {
                (heights[idx(ix, iz + 1)] - heights[idx(ix, iz - 1)]) / (2.0 * step)
            };

            // tangent_x = (step, dh_dx * step, 0), tangent_z = (0, dh_dz * step, step)
            // normal = cross(tangent_z, tangent_x) = (-dh_dx, 1, -dh_dz) (unnormalized)
            let nx = -dh_dx;
            let ny = 1.0_f32;
            let nz = -dh_dz;
            let len = (nx * nx + ny * ny + nz * nz).sqrt();
            let inv = if len < 1e-10 { 1.0 } else { 1.0 / len };
            normals.push([nx * inv, ny * inv, nz * inv]);
        }
    }

    // Build triangle indices: two triangles per quad, counter-clockwise winding.
    let num_quads = (subdivisions * subdivisions) as usize;
    let mut indices: Vec<u32> = Vec::with_capacity(num_quads * 6);

    for iz in 0..subdivisions {
        for ix in 0..subdivisions {
            let top_left = iz * verts_per_edge + ix;
            let top_right = top_left + 1;
            let bottom_left = top_left + verts_per_edge;
            let bottom_right = bottom_left + 1;

            // First triangle (top-left, bottom-left, top-right)
            indices.push(top_left);
            indices.push(bottom_left);
            indices.push(top_right);

            // Second triangle (top-right, bottom-left, bottom-right)
            indices.push(top_right);
            indices.push(bottom_left);
            indices.push(bottom_right);
        }
    }

    Mesh::new(
        bevy::render::render_resource::PrimitiveTopology::TriangleList,
        bevy::asset::RenderAssetUsages::default(),
    )
    .with_inserted_attribute(Mesh::ATTRIBUTE_POSITION, positions)
    .with_inserted_attribute(Mesh::ATTRIBUTE_NORMAL, normals)
    .with_inserted_attribute(Mesh::ATTRIBUTE_UV_0, uvs)
    .with_inserted_indices(bevy::mesh::Indices::U32(indices))
}

const CONFIG_PATH: &str = "assets/config/world_generation.toml";

/// Plugin that registers world generation resources, config loading, and chunk neighborhood systems.
pub struct WorldGenerationPlugin;

impl Plugin for WorldGenerationPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<WorldGenerationConfig>()
            .init_resource::<ActiveChunkNeighborhood>()
            .init_resource::<BiomeRegistry>()
            .add_systems(
                PreStartup,
                (load_world_generation_config, load_biome_registry),
            )
            .add_systems(Startup, resolve_system_derived_profile)
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
#[derive(
    Clone, Copy, Debug, Default, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize,
)]
pub struct ChunkCoord {
    /// Chunk index along the X axis.
    pub x: i32,
    /// Chunk index along the Z axis.
    pub z: i32,
}

impl ChunkCoord {
    /// Creates a new chunk coordinate from the given X and Z indices.
    pub const fn new(x: i32, z: i32) -> Self {
        Self { x, z }
    }
}

/// The two modes the config can operate in.
///
/// When `planet_seed` is provided in the TOML, the config is in override mode:
/// the planet seed is used directly and no solar-system derivation chain runs.
/// When `planet_seed` is absent and `solar_system_seed` is present, the full
/// chain runs: system seed → star → orbital layout → planet selection by index.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum SeedMode {
    /// Planet seed was provided directly in config (override / testing mode).
    Override,
    /// Planet seed is derived from the solar system seed + planet index.
    SystemDerived,
}

/// Runtime world-generation config loaded from `assets/config/world_generation.toml`.
///
/// Supports two mutually exclusive seeding modes:
///
/// **Override mode** (current default, for testing): set `planet_seed` directly.
/// The solar system seed is still used for star derivation and logging, but the
/// planet seed is taken as-is without running the orbital derivation chain.
///
/// **System-derived mode**: omit `planet_seed` and set `solar_system_seed` +
/// `planet_index`. The full derivation chain runs at startup: system seed →
/// star profile → orbital layout → planet selection → planet seed.
///
/// Other fields:
/// - `chunk_size_world_units`: how wide/deep one chunk is in Bevy world units
/// - `active_chunk_radius`: how many chunks around the player's chunk are
///   considered logically active
/// - `building_cell_size`: side length of 3D building cells for spatial overlap
///   detection during delta merging (Story 5.6)
#[derive(Clone, Debug, Resource, PartialEq, Serialize, Deserialize)]
pub struct WorldGenerationConfig {
    /// Solar system seed — root of all deterministic star and planet derivation.
    ///
    /// The star profile (type, luminosity, temperature, mass, habitable zone)
    /// is derived from this seed at startup. In system-derived mode, the
    /// orbital layout and planet seed are also derived from this seed.
    ///
    /// Accepts both `solar_system_seed` (new canonical name) and `system_seed`
    /// (legacy alias) in the TOML file for backward compatibility.
    #[serde(
        default = "default_solar_system_seed",
        alias = "system_seed",
        rename = "solar_system_seed"
    )]
    pub solar_system_seed: u64,
    /// Planet seed override. When present, the planet seed is used directly
    /// and no orbital derivation chain runs (override mode). When absent,
    /// the planet seed is derived from `solar_system_seed` + `planet_index`
    /// (system-derived mode).
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub planet_seed: Option<u64>,
    /// Zero-based index selecting which orbital slot to start on when in
    /// system-derived mode. Ignored in override mode.
    ///
    /// If the index is out of range for the derived orbital layout, startup
    /// fails with a descriptive error rather than silently clamping.
    #[serde(default)]
    pub planet_index: u32,
    /// Side length of a chunk in world units.
    #[serde(default = "default_chunk_size_world_units")]
    pub chunk_size_world_units: f32,
    /// Number of chunks around the player to keep active.
    #[serde(default = "default_active_chunk_radius")]
    pub active_chunk_radius: i32,
    /// Side length (in world units) of the 3D building cells used for spatial
    /// overlap detection when merging player additions from different sources.
    ///
    /// A cell is `floor(pos / cell_size)` per axis, producing an `(i64, i64,
    /// i64)` key unique across the solar system. Smaller values give finer
    /// collision granularity; larger values are more forgiving.
    #[serde(default = "default_building_cell_size")]
    pub building_cell_size: f32,
    /// Minimum planet surface radius in chunks.
    ///
    /// The actual radius is derived from the planet seed within the range
    /// `[planet_surface_min_radius, planet_surface_max_radius]`. The planet
    /// diameter in chunks is `2 * radius`, and the surface wraps in both X and
    /// Z using torus topology (walk off one edge, appear on the opposite side).
    #[serde(default = "default_planet_surface_min_radius")]
    pub planet_surface_min_radius: i32,
    /// Maximum planet surface radius in chunks.
    ///
    /// See `planet_surface_min_radius` for details on how the planet size is
    /// derived from the seed.
    #[serde(default = "default_planet_surface_max_radius")]
    pub planet_surface_max_radius: i32,
    /// Maximum terrain height deviation from base_y (in world units).
    #[serde(default = "default_elevation_amplitude")]
    pub elevation_amplitude: f32,
    /// Base frequency of the elevation noise field (in world units).
    #[serde(default = "default_elevation_frequency")]
    pub elevation_frequency: f32,
    /// Number of fractal noise octaves layered for terrain elevation.
    #[serde(default = "default_elevation_octaves")]
    pub elevation_octaves: u32,
    /// Blend weight for chunk-level detail noise added on top of the base
    /// elevation field. 0.0 means no detail layer.
    #[serde(default = "default_elevation_detail_weight")]
    pub elevation_detail_weight: f32,
    /// Base frequency for the detail noise layer. Higher than the base
    /// `elevation_frequency` to add fine-grained terrain variation.
    #[serde(default = "default_elevation_detail_frequency")]
    pub elevation_detail_frequency: f32,
    /// Number of fractal noise octaves for the detail noise layer.
    #[serde(default = "default_elevation_detail_octaves")]
    pub elevation_detail_octaves: u32,
    /// Sea-level reference height (in world units). Elevation noise is added
    /// on top of this value.
    #[serde(default = "default_elevation_base_y")]
    pub elevation_base_y: f32,
    /// Number of subdivisions per chunk edge for the heightmap mesh.
    /// An N×N grid produces (N+1)² vertices.
    #[serde(default = "default_elevation_subdivisions")]
    pub elevation_subdivisions: u32,
}

impl Default for WorldGenerationConfig {
    fn default() -> Self {
        Self {
            solar_system_seed: default_solar_system_seed(),
            planet_seed: Some(default_planet_seed()),
            planet_index: 0,
            chunk_size_world_units: default_chunk_size_world_units(),
            active_chunk_radius: default_active_chunk_radius(),
            building_cell_size: default_building_cell_size(),
            planet_surface_min_radius: default_planet_surface_min_radius(),
            planet_surface_max_radius: default_planet_surface_max_radius(),
            elevation_amplitude: default_elevation_amplitude(),
            elevation_frequency: default_elevation_frequency(),
            elevation_octaves: default_elevation_octaves(),
            elevation_detail_weight: default_elevation_detail_weight(),
            elevation_detail_frequency: default_elevation_detail_frequency(),
            elevation_detail_octaves: default_elevation_detail_octaves(),
            elevation_base_y: default_elevation_base_y(),
            elevation_subdivisions: default_elevation_subdivisions(),
        }
    }
}

impl WorldGenerationConfig {
    /// Which seeding mode this config is operating in.
    ///
    /// When `planet_seed` is `Some`, the config is in override mode — the
    /// planet seed is used directly and no orbital derivation runs. When
    /// `planet_seed` is `None`, the config is in system-derived mode and
    /// the planet seed will be derived from `solar_system_seed` + `planet_index`.
    pub fn seed_mode(&self) -> SeedMode {
        if self.planet_seed.is_some() {
            SeedMode::Override
        } else {
            SeedMode::SystemDerived
        }
    }

    /// Validate config values, particularly seed mode configuration.
    ///
    /// The config supports two mutually exclusive seeding modes. This method
    /// enforces that exactly one mode is clearly specified:
    ///
    /// - **Override mode**: `planet_seed` is set. The `solar_system_seed` is
    ///   still used for star derivation, but the planet seed bypasses orbital
    ///   derivation. `planet_index` is ignored in this mode — if it was
    ///   explicitly set alongside `planet_seed`, that is a likely
    ///   misconfiguration (the user probably meant system-derived mode).
    ///
    /// - **System-derived mode**: `planet_seed` is absent. The planet seed
    ///   is derived from `solar_system_seed` + `planet_index`.
    ///
    /// Both modes require `solar_system_seed` (always present via default).
    /// Numeric field ranges (chunk size, radii, elevation) are also validated.
    pub fn validate(&self) -> Result<(), String> {
        // Seed mode: if planet_seed is set alongside a non-default planet_index,
        // warn — the user likely intended system-derived mode but forgot to
        // remove planet_seed. This is an error, not silent precedence.
        if let Some(planet_seed) = self.planet_seed
            && self.planet_index != 0
        {
            return Err(format!(
                "planet_seed and planet_index are both set. In override mode \
                 (planet_seed present), planet_index is ignored. Either remove \
                 planet_seed to use system-derived mode, or remove planet_index \
                 to use override mode. (planet_seed={planet_seed}, planet_index={})",
                self.planet_index,
            ));
        }

        // Chunk size must be positive and finite.
        if !self.chunk_size_world_units.is_finite() || self.chunk_size_world_units <= 0.0 {
            return Err(format!(
                "chunk_size_world_units must be positive and finite, got {}",
                self.chunk_size_world_units,
            ));
        }

        // Active chunk radius must be non-negative.
        if self.active_chunk_radius < 0 {
            return Err(format!(
                "active_chunk_radius must be >= 0, got {}",
                self.active_chunk_radius,
            ));
        }

        // Building cell size must be positive and finite.
        if !self.building_cell_size.is_finite() || self.building_cell_size <= 0.0 {
            return Err(format!(
                "building_cell_size must be positive and finite, got {}",
                self.building_cell_size,
            ));
        }

        // Planet surface radius bounds.
        if self.planet_surface_min_radius < 1 {
            return Err(format!(
                "planet_surface_min_radius must be >= 1, got {}",
                self.planet_surface_min_radius,
            ));
        }
        if self.planet_surface_min_radius > self.planet_surface_max_radius {
            return Err(format!(
                "planet_surface_min_radius ({}) must be <= planet_surface_max_radius ({})",
                self.planet_surface_min_radius, self.planet_surface_max_radius,
            ));
        }

        // Elevation amplitude must be finite and non-negative.
        if !self.elevation_amplitude.is_finite() || self.elevation_amplitude < 0.0 {
            return Err(format!(
                "elevation_amplitude must be non-negative and finite, got {}",
                self.elevation_amplitude,
            ));
        }

        // Elevation frequency must be positive and finite.
        if !self.elevation_frequency.is_finite() || self.elevation_frequency <= 0.0 {
            return Err(format!(
                "elevation_frequency must be positive and finite, got {}",
                self.elevation_frequency,
            ));
        }

        // Elevation octaves must be >= 1.
        if self.elevation_octaves < 1 {
            return Err(format!(
                "elevation_octaves must be >= 1, got {}",
                self.elevation_octaves,
            ));
        }

        // Detail weight must be finite and in [0, 1].
        if !self.elevation_detail_weight.is_finite()
            || self.elevation_detail_weight < 0.0
            || self.elevation_detail_weight > 1.0
        {
            return Err(format!(
                "elevation_detail_weight must be in [0.0, 1.0], got {}",
                self.elevation_detail_weight,
            ));
        }

        // Detail frequency must be positive and finite (when detail weight > 0).
        if self.elevation_detail_weight > 0.0
            && (!self.elevation_detail_frequency.is_finite()
                || self.elevation_detail_frequency <= 0.0)
        {
            return Err(format!(
                "elevation_detail_frequency must be positive and finite when \
                 detail weight > 0, got {}",
                self.elevation_detail_frequency,
            ));
        }

        // Detail octaves must be >= 1 (when detail weight > 0).
        if self.elevation_detail_weight > 0.0 && self.elevation_detail_octaves < 1 {
            return Err(format!(
                "elevation_detail_octaves must be >= 1 when detail weight > 0, got {}",
                self.elevation_detail_octaves,
            ));
        }

        // Base Y must be finite.
        if !self.elevation_base_y.is_finite() {
            return Err(format!(
                "elevation_base_y must be finite, got {}",
                self.elevation_base_y,
            ));
        }

        // Subdivisions must be >= 1.
        if self.elevation_subdivisions < 1 {
            return Err(format!(
                "elevation_subdivisions must be >= 1, got {}",
                self.elevation_subdivisions,
            ));
        }

        Ok(())
    }
}

fn default_solar_system_seed() -> u64 {
    20_260_501
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

fn default_building_cell_size() -> f32 {
    // 1.0 world unit per cell side — roughly one meter of granularity.
    // This is a starting point for spatial overlap detection during delta
    // merging. The value is configurable in world_generation.toml so it can
    // be tuned without recompiling once building gameplay takes shape.
    1.0
}

fn default_planet_surface_min_radius() -> i32 {
    // Minimum planet radius in chunks. With a 45-unit chunk size, 500 chunks
    // gives a surface diameter of 1000 chunks × 45 = 45,000 world units
    // (~45 km). That is small enough to circumnavigate in a reasonable play
    // session but large enough that the surface wrapping is not immediately
    // obvious at ground level.
    500
}

fn default_planet_surface_max_radius() -> i32 {
    // Maximum planet radius in chunks. 5000 chunks gives a diameter of
    // 10,000 chunks × 45 = 450,000 world units (~450 km). A planet this
    // large would take real commitment to circumnavigate, making the world
    // feel genuinely expansive.
    5000
}

fn default_elevation_amplitude() -> f32 {
    // Maximum height deviation from base_y. 10 world units gives gentle
    // rolling hills that are clearly visible without being extreme.
    10.0
}

fn default_elevation_frequency() -> f32 {
    // Base noise frequency in world units. Lower values = broader features.
    // 0.005 produces features on the scale of ~200 world units (~4-5 chunks).
    0.005
}

fn default_elevation_octaves() -> u32 {
    // Number of fractal noise layers. 4 octaves give a good balance of
    // large-scale hills with smaller-scale detail.
    4
}

fn default_elevation_detail_weight() -> f32 {
    // Blend ratio for chunk-level detail noise. 0.0 means the detail layer
    // is disabled by default; later phases will tune this.
    0.0
}

fn default_elevation_detail_frequency() -> f32 {
    // Base frequency for the detail noise layer — 4× the base elevation
    // frequency so it adds finer-grained terrain texture.
    0.02
}

fn default_elevation_detail_octaves() -> u32 {
    // Two octaves of detail noise is enough for subtle variation without
    // overwhelming the base elevation shape.
    2
}

fn default_elevation_base_y() -> f32 {
    // Sea-level reference height. -0.01 matches the existing FlatSurface
    // convention used by the exterior ground patch.
    -0.01
}

fn default_elevation_subdivisions() -> u32 {
    // Number of subdivisions per chunk edge. 8 gives 64 quads per chunk
    // (9×9 = 81 vertices), a reasonable default for terrain detail.
    8
}

/// Solar system context carried through the derivation chain.
///
/// Present in `WorldProfile` when the planet seed was derived from a solar
/// system seed (system-derived mode). Absent (`None`) when the planet seed
/// was provided directly in config (override mode).
///
/// Systems that need stellar/orbital context (e.g., biome temperature
/// scaling) check `WorldProfile::system_context`. When it is `None`, they
/// fall back to defaults (preserving the pre-stellar-integration behavior).
#[derive(Clone, Debug, PartialEq, Serialize, Deserialize)]
pub struct SystemContext {
    /// The solar system seed that started the derivation chain.
    pub system_seed: SolarSystemSeed,
    /// The star profile derived from the system seed.
    pub star: StarProfile,
    /// The full orbital layout derived from the system seed.
    pub orbital_layout: OrbitalLayout,
    /// Planet-level environmental parameters derived from stellar context.
    pub planet_environment: PlanetEnvironment,
    /// The zero-based orbital index of the selected planet.
    pub planet_orbital_index: u32,
}

/// Derived deterministic world profile.
///
/// The profile exists so later stories do not have to keep reverse engineering
/// "which seed should I use for this purpose?" from the raw planet seed. We
/// derive explicit sub-seeds up front and document what each one is for.
///
/// ## Planet Surface Topology (Story 5a.1)
///
/// The planet surface uses **torus topology**: chunk coordinates wrap in both
/// the X and Z axes. Walking off one edge of the planet brings you back to the
/// opposite side. The surface is a square grid of chunks with side length
/// `planet_surface_diameter` (measured in chunks). The diameter is derived
/// deterministically from the planet seed within the configurable min/max
/// radius range, so every planet has a consistent, reproducible size.
///
/// Chunk coordinates on the planet surface are always in the range
/// `[0, planet_surface_diameter)` on both axes after wrapping. The
/// [`wrap_chunk_coord`] function handles this — all code that produces or
/// consumes chunk coordinates should pass them through wrapping to ensure
/// consistency.
#[derive(Clone, Debug, Resource, PartialEq, Serialize, Deserialize)]
pub struct WorldProfile {
    /// Seed uniquely identifying this planet.
    pub planet_seed: PlanetSeed,
    /// Side length of a chunk in world units.
    pub chunk_size_world_units: f32,
    /// Number of chunks around the player to keep active.
    pub active_chunk_radius: i32,
    /// Seed used to determine object placement density per chunk.
    pub placement_density_seed: u64,
    /// Seed used to vary object positions within a chunk.
    pub placement_variation_seed: u64,
    /// Seed used to deterministically assign object identities.
    pub object_identity_seed: u64,
    /// Per-planet biome climate seed, derived from the planet seed.
    ///
    /// This seed is mixed with temperature and moisture sub-channel constants
    /// (defined in `BiomeRegistry`) to produce two independent coherent noise
    /// fields. Each chunk samples both fields at its canonical center to
    /// determine its biome.
    pub biome_climate_seed: u64,
    /// The planet surface radius in chunks, derived from the planet seed.
    ///
    /// The full surface is a square grid of `planet_surface_diameter × diameter`
    /// chunks with torus wrapping. The radius is half the diameter.
    pub planet_surface_radius: i32,
    /// The planet surface diameter in chunks (always `2 * planet_surface_radius`).
    ///
    /// This is the wrapping period for chunk coordinates. A coordinate of
    /// `planet_surface_diameter` wraps back to `0`.
    pub planet_surface_diameter: i32,
    /// Per-planet elevation seed, derived from the planet seed via
    /// `ELEVATION_CHANNEL`. Drives the multi-octave noise field that
    /// produces terrain height variation.
    pub elevation_seed: u64,
    /// Solar system context when running in system-derived mode.
    ///
    /// `Some` when the planet seed was derived from a solar system seed via
    /// the full derivation chain. `None` when the planet seed was provided
    /// directly in config (override mode). Systems that need stellar or
    /// orbital context should check this field and fall back to defaults
    /// when it is `None`.
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub system_context: Option<SystemContext>,
}

impl WorldProfile {
    /// Build a world profile in override mode — planet seed taken directly
    /// from config, no system derivation chain.
    ///
    /// This is the constructor used when `planet_seed` is present in the
    /// config. The `system_context` field is `None`.
    ///
    /// Returns `Err` if `planet_seed` is `None` in the config (caller should
    /// check `seed_mode()` first or use `from_system_seed` for system-derived mode).
    pub fn from_config(config: &WorldGenerationConfig) -> Result<Self, String> {
        let raw_seed = config.planet_seed.ok_or_else(|| {
            "from_config requires planet_seed to be Some (override mode)".to_string()
        })?;
        let planet_seed = PlanetSeed(raw_seed);
        Self::build(planet_seed, config, None)
    }

    /// Build a world profile in system-derived mode — planet seed derived
    /// from the full solar system chain.
    ///
    /// Runs: system seed → star profile → orbital layout → select planet
    /// by index → derive planet environment → build profile.
    ///
    /// Returns `Err` with a human-readable message if the `planet_index`
    /// is out of range for the derived orbital layout.
    pub fn from_system_seed(
        config: &WorldGenerationConfig,
        star_registry: &StarTypeRegistry,
        orbital_config: &OrbitalConfig,
        env_config: &PlanetEnvironmentConfig,
    ) -> Result<Self, String> {
        let system_seed = SolarSystemSeed(config.solar_system_seed);
        let star = derive_star_profile(system_seed, star_registry);
        let orbital_layout = derive_orbital_layout(system_seed, orbital_config);

        let planet_count = orbital_layout.planets.len();
        let index = config.planet_index as usize;
        if index >= planet_count {
            return Err(format!(
                "planet_index {} is out of range: solar system seed {} produced \
                 {} planets (valid indices: 0..{})",
                config.planet_index,
                config.solar_system_seed,
                planet_count,
                planet_count.saturating_sub(1),
            ));
        }

        let slot = &orbital_layout.planets[index];
        let planet_seed = slot.planet_seed;
        let planet_environment =
            derive_planet_environment(&star, slot.orbital_distance_au, planet_seed, env_config);

        let context = SystemContext {
            system_seed,
            star,
            orbital_layout,
            planet_environment,
            planet_orbital_index: config.planet_index,
        };

        Self::build(planet_seed, config, Some(context))
    }

    /// Shared builder used by both `from_config` and `from_system_seed`.
    fn build(
        planet_seed: PlanetSeed,
        config: &WorldGenerationConfig,
        system_context: Option<SystemContext>,
    ) -> Result<Self, String> {
        // Derive the planet surface radius from the planet seed. We mix the
        // seed with a dedicated channel constant so this derivation is
        // independent of all other seed-derived values (placement density,
        // variation, identity). The result is scaled into the configured
        // [min_radius, max_radius] range using Lemire's nearly-unbiased
        // method: multiply a u32 by the range width as u64, then take the
        // upper 32 bits. This avoids modulo bias without rejection sampling.
        let planet_surface_radius = derive_planet_surface_radius(
            planet_seed,
            config.planet_surface_min_radius,
            config.planet_surface_max_radius,
        );
        let planet_surface_diameter = planet_surface_radius.checked_mul(2).ok_or_else(|| {
            format!(
                "planet_surface_radius {planet_surface_radius} overflows i32 when doubled \
                 (min_radius={}, max_radius={})",
                config.planet_surface_min_radius, config.planet_surface_max_radius,
            )
        })?;

        Ok(Self {
            planet_seed,
            chunk_size_world_units: config.chunk_size_world_units,
            active_chunk_radius: config.active_chunk_radius,
            placement_density_seed: mix_seed(planet_seed.0, PLACEMENT_DENSITY_CHANNEL),
            placement_variation_seed: mix_seed(planet_seed.0, PLACEMENT_VARIATION_CHANNEL),
            object_identity_seed: mix_seed(planet_seed.0, OBJECT_IDENTITY_CHANNEL),
            biome_climate_seed: mix_seed(planet_seed.0, BIOME_CLIMATE_CHANNEL),
            planet_surface_radius,
            planet_surface_diameter,
            elevation_seed: mix_seed(planet_seed.0, ELEVATION_CHANNEL),
            system_context,
        })
    }

    /// Whether this profile was derived from a solar system seed.
    ///
    /// Returns `true` when operating in system-derived mode (the full chain
    /// ran: system seed → star → orbital layout → planet seed). Returns
    /// `false` in override mode (planet seed was provided directly).
    ///
    /// Not yet consumed by any system — provided for story 5b.4 integration
    /// tests and downstream biome systems that will branch on mode.
    #[allow(dead_code)]
    pub fn is_system_derived(&self) -> bool {
        self.system_context.is_some()
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
    /// The chunk these keys belong to.
    pub chunk_coord: ChunkCoord,
    /// Per-chunk key for placement density noise.
    pub placement_density_key: u64,
    /// Per-chunk key for placement variation noise.
    pub placement_variation_key: u64,
    /// Per-chunk key for deterministic object identity assignment.
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
    /// Planet this object belongs to.
    pub planet_seed: PlanetSeed,
    /// Chunk containing this object.
    pub chunk_coord: ChunkCoord,
    /// String key identifying the kind of object (e.g. mineral type).
    pub object_kind_key: String,
    /// Deterministic index of this candidate within its chunk and kind.
    pub local_candidate_index: u32,
    /// Version of the generator that produced this object.
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
    /// Chunk the player currently occupies, if known.
    pub center_chunk: Option<ChunkCoord>,
    /// World-space origin of the center chunk.
    pub center_chunk_origin_xz: Option<PositionXZ>,
    /// Generation key for the center chunk.
    pub center_chunk_generation_key: Option<ChunkGenerationKey>,
    /// How many chunks outward from center to include.
    pub radius: i32,
    /// All chunk coordinates in the active neighborhood.
    pub chunks: Vec<ChunkCoord>,
}

fn load_world_generation_config(mut commands: Commands) {
    let config = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<WorldGenerationConfig>(&contents) {
                Ok(config) => match config.validate() {
                    Ok(()) => {
                        info!("Loaded world-generation config from {CONFIG_PATH}");
                        config
                    }
                    Err(validation_error) => {
                        warn!(
                            "World-generation config from {CONFIG_PATH} failed validation, \
                             using defaults: {validation_error}"
                        );
                        WorldGenerationConfig::default()
                    }
                },
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

    match config.seed_mode() {
        SeedMode::Override => {
            let Some(planet_seed) = config.planet_seed else {
                error!(
                    "BUG: seed_mode() returned Override but planet_seed is None. \
                     Config: solar_system_seed={}, planet_index={}, planet_seed={:?}. \
                     Falling back to defaults.",
                    config.solar_system_seed, config.planet_index, config.planet_seed,
                );
                commands.insert_resource(config);
                return;
            };
            info!("Seed mode: override (planet_seed={planet_seed:#018X})");

            match WorldProfile::from_config(&config) {
                Ok(profile) => {
                    commands.insert_resource(profile);
                }
                Err(err) => {
                    error!(
                        "Failed to build WorldProfile from config in override mode: {err}. \
                         Config: planet_seed={planet_seed:#018X}, \
                         solar_system_seed={}, planet_index={}. \
                         WorldProfile resource will not be available — systems that \
                         depend on it will gracefully skip until the config is corrected.",
                        config.solar_system_seed, config.planet_index,
                    );
                }
            }
        }
        SeedMode::SystemDerived => {
            info!(
                "Seed mode: system-derived (solar_system_seed={}, planet_index={}); \
                 WorldProfile will be resolved in Startup after registries are loaded",
                config.solar_system_seed, config.planet_index,
            );
            // WorldProfile will be built by resolve_system_derived_profile in Startup.
            // The init_resource default is a placeholder that gets overwritten.
        }
    }

    commands.insert_resource(config);
}

/// Resolve the `WorldProfile` from the full solar system derivation chain.
///
/// This system runs in `Startup` (after all `PreStartup` registry loaders
/// have completed). It only does work when the config is in system-derived
/// mode — when `planet_seed` is absent and the planet seed must be derived
/// from `solar_system_seed` + `planet_index`.
///
/// On success, it overwrites the default `WorldProfile` resource with the
/// fully resolved profile including `SystemContext`. On failure (e.g.,
/// `planet_index` out of range), it logs a clear error message and
/// requests a graceful application exit via [`AppExit`], rather than
/// panicking, so the user sees an actionable diagnostic instead of a
/// crash backtrace.
pub fn resolve_system_derived_profile(
    mut commands: Commands,
    config: Res<WorldGenerationConfig>,
    registries: SolarSystemRegistries,
    mut app_exit: bevy::ecs::message::MessageWriter<AppExit>,
) {
    if config.seed_mode() != SeedMode::SystemDerived {
        return;
    }

    let profile = match WorldProfile::from_system_seed(
        &config,
        &registries.star_registry,
        &registries.orbital_config,
        &registries.env_config,
    ) {
        Ok(p) => p,
        Err(err) => {
            error!(
                "Failed to resolve system-derived WorldProfile: {err}. \
                     Fix solar_system_seed / planet_index in {CONFIG_PATH} \
                     or switch to override mode by setting planet_seed directly."
            );
            app_exit.write(AppExit::error());
            return;
        }
    };

    let star_type_label = match profile.system_context.as_ref() {
        Some(ctx) => format!("{}", ctx.star.star_type),
        None => {
            error!(
                "BUG: system-derived WorldProfile has no system_context. \
                 planet_seed={:#018X}, planet_index={}. \
                 Inserting profile anyway but downstream systems may behave unexpectedly.",
                profile.planet_seed.0, config.planet_index,
            );
            "<missing>".to_string()
        }
    };

    info!(
        "Resolved system-derived WorldProfile: planet_seed={:#018X}, \
         star_type={star_type_label}, planet_index={}",
        profile.planet_seed.0, config.planet_index,
    );

    commands.insert_resource(profile);
}

fn update_active_chunk_neighborhood(
    profile: Option<Res<WorldProfile>>,
    mut active_chunks: ResMut<ActiveChunkNeighborhood>,
    player_query: Query<&Transform, With<Player>>,
) {
    let Some(profile) = profile else {
        return;
    };
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
    // Use the raw (unwrapped) chunk coordinate for world-space positioning.
    // The neighborhood must stay in the player's local coordinate space so
    // that `chunk_origin_xz` produces positions near the player. Torus
    // wrapping only matters for *generation keys* — two chunks at the same
    // canonical (wrapped) coordinate produce identical content, but they
    // must be rendered at their raw world-space positions.
    let center_chunk =
        world_position_to_chunk_coord(player_position_xz, profile.chunk_size_world_units);

    // Early-out: skip recomputation when the player has not moved into a
    // new chunk.  Without this guard, every frame would allocate a fresh
    // `Vec<ChunkCoord>` for `active_chunks.chunks` and stamp the resource
    // as changed, causing every downstream system that reacts to
    // `ActiveChunkNeighborhood` change detection to fire on every frame
    // instead of only on chunk transitions.
    if active_chunks.center_chunk == Some(center_chunk) {
        return;
    }

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
    // Defense in depth.  Config validation rejects non-positive chunk
    // sizes at load time, so a zero or negative value here means an
    // upstream bug.  In debug builds we want the panic to surface
    // immediately; in release builds we still need to avoid a divide-by-
    // zero (which would yield `NaN`/`Inf` chunk coordinates and silently
    // corrupt every system that consumes them) so we clamp to a safe
    // minimum of 1.0 and log the bug.
    debug_assert!(
        chunk_size_world_units > 0.0,
        "chunk size must be positive to derive chunk coordinates, got {chunk_size_world_units}"
    );
    let chunk_size = if chunk_size_world_units > 0.0 {
        chunk_size_world_units
    } else {
        error!("chunk_size_world_units was {chunk_size_world_units}, clamping to 1.0");
        1.0
    };

    let chunk_x = (position_xz.x / chunk_size).floor() as i32;
    let chunk_z = (position_xz.z / chunk_size).floor() as i32;
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
/// These coordinates are **raw (unwrapped)** — they stay in the player's local
/// coordinate space so that `chunk_origin_xz` produces world-space positions
/// near the player. Torus wrapping is applied later, only when deriving
/// generation keys (via [`derive_chunk_generation_key`]), so that chunks at
/// equivalent canonical positions produce identical content regardless of
/// which "lap" of the torus the player is on.
///
/// The nested loop order is stable, so any later story that iterates this
/// list gets the same ordering every run.
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
/// The input `chunk_coord` may be a raw (unwrapped) coordinate from the
/// player's local space. We wrap it to the canonical torus position before
/// mixing, so that chunk `(-1, 0)` on a diameter-1000 planet produces the
/// same generation keys as chunk `(999, 0)`. This is the **only** place
/// torus wrapping feeds into content generation — the raw coordinate is
/// preserved in the returned key for world-space positioning.
///
/// We mix the profile's purpose-specific seeds with the chunk coordinate so that:
/// - the same planet + same canonical chunk always gets the same keys
/// - different chunks on the same planet get different keys
/// - later systems can tell which key is meant for which job
pub fn derive_chunk_generation_key(
    profile: &WorldProfile,
    chunk_coord: ChunkCoord,
) -> ChunkGenerationKey {
    // Wrap to canonical torus coordinate for deterministic generation.
    // Two raw coordinates that differ by a multiple of the planet diameter
    // will produce identical keys — this is what makes the torus seamless.
    let canonical = wrap_chunk_coord(chunk_coord, profile.planet_surface_diameter);
    let chunk_mixer = mix_chunk_coord(profile.planet_seed, canonical);

    ChunkGenerationKey {
        chunk_coord: canonical,
        placement_density_key: mix_seed(profile.placement_density_seed, chunk_mixer),
        placement_variation_key: mix_seed(profile.placement_variation_seed, chunk_mixer),
        object_identity_key: mix_seed(profile.object_identity_seed, chunk_mixer),
    }
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

/// Derive the planet surface radius (in chunks) from the planet seed.
///
/// The radius is derived by mixing the planet seed with a dedicated channel
/// constant, then scaling the result into the `[min_radius, max_radius]` range
/// using Lemire's nearly-unbiased method.
///
/// ## Lemire's Method (why not modulo?)
///
/// A naïve `value % range` biases the lower values when the range doesn't
/// divide evenly into `u32::MAX`. Lemire's method avoids this: multiply the
/// random `u32` by the range width to get a `u64`, then take the upper 32 bits.
/// This is equivalent to `(value / u32::MAX) * range` but done entirely with
/// integer arithmetic — no floating point, no division, no rejection loop.
///
/// For the small ranges we use here (planet radius might span a few thousand
/// values) the bias from modulo would be negligible, but Lemire's method is
/// equally simple and has zero bias worth measuring.
fn derive_planet_surface_radius(planet_seed: PlanetSeed, min_radius: i32, max_radius: i32) -> i32 {
    debug_assert!(min_radius > 0, "planet surface min radius must be positive");
    debug_assert!(
        max_radius >= min_radius,
        "planet surface max radius must be >= min radius"
    );

    // Mix the planet seed with the dedicated channel to get a raw u64.
    let raw = mix_seed(planet_seed.0, PLANET_SURFACE_RADIUS_CHANNEL);

    // Take the lower 32 bits as a u32 for scaling.
    let raw_u32 = raw as u32;

    // Range width: how many distinct radius values are possible.
    // +1 because both endpoints are inclusive.
    let range = (max_radius - min_radius + 1) as u64;

    // Lemire's method: multiply by range, take upper 32 bits.
    // This maps the u32 space [0, 2^32) proportionally onto [0, range).
    let scaled = ((raw_u32 as u64) * range) >> 32;

    min_radius + scaled as i32
}

/// Wrap a chunk coordinate into the planet's torus surface.
///
/// The planet surface is a square grid of `diameter × diameter` chunks. Both
/// axes wrap independently using Euclidean modulo, so walking off any edge
/// brings you to the opposite side. Coordinates that are already in range
/// `[0, diameter)` pass through unchanged.
///
/// ## Why Euclidean modulo?
///
/// Rust's `%` operator is a remainder, not a modulo — it preserves the sign of
/// the dividend. `-1 % 10` gives `-1` in Rust, but we need `9`. The
/// `.rem_euclid()` method gives the mathematically correct non-negative result:
/// `-1_i32.rem_euclid(10)` gives `9`. This is exactly what we need for torus
/// wrapping where all coordinates must be in `[0, diameter)`.
pub fn wrap_chunk_coord(coord: ChunkCoord, planet_surface_diameter: i32) -> ChunkCoord {
    debug_assert!(
        planet_surface_diameter > 0,
        "planet surface diameter must be positive for wrapping"
    );
    ChunkCoord::new(
        coord.x.rem_euclid(planet_surface_diameter),
        coord.z.rem_euclid(planet_surface_diameter),
    )
}

/// Constructs a [`GeneratedObjectId`] from the world profile, chunk, kind, and candidate index.
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

// ── Biome Region Derivation (Story 5a.2) ─────────────────────────────────

const BIOME_CONFIG_PATH: &str = "assets/config/biomes.toml";

/// Registry of all biome definitions, loaded from `assets/config/biomes.toml`.
///
/// The registry defines the temperature × moisture grid that maps each chunk
/// to a biome. It is loaded once at startup and never mutated. Generation
/// systems access it via `Res<BiomeRegistry>`.
///
/// ## Noise Parameters
///
/// The two noise fields (temperature and moisture) are each sampled at the
/// chunk's canonical center in **chunk space** (not world space). The
/// `noise_scale_chunks` parameter controls how many chunks fit in one noise
/// period — larger values make biome regions bigger.
///
/// Each noise field uses its own sub-channel constant mixed with the
/// `biome_climate_seed` from `WorldProfile`, ensuring the two fields are
/// uncorrelated (no diagonal striping artifact).
#[derive(Clone, Debug, Resource, Serialize, Deserialize)]
pub struct BiomeRegistry {
    /// How many chunks fit in one period of the biome noise field.
    ///
    /// Controls biome region size: larger values → bigger regions, fewer
    /// transitions per planet circumference. A value of 12 means roughly
    /// 12 chunks between biome transitions.
    #[serde(default = "default_biome_noise_scale_chunks")]
    pub noise_scale_chunks: f32,
    /// Sub-channel mixed with `biome_climate_seed` for the temperature axis.
    #[serde(default = "default_temperature_noise_channel")]
    pub temperature_noise_channel: u64,
    /// Sub-channel mixed with `biome_climate_seed` for the moisture axis.
    #[serde(default = "default_moisture_noise_channel")]
    pub moisture_noise_channel: u64,
    /// Key of the biome used when a chunk's (temperature, moisture) pair does
    /// not fall within any defined biome's range.
    #[serde(default = "default_fallback_biome_key")]
    pub fallback_biome_key: String,
    /// Ordered list of biome definitions. The first matching biome wins when
    /// ranges overlap.
    #[serde(default)]
    pub biomes: Vec<BiomeDefinition>,
}

fn default_biome_noise_scale_chunks() -> f32 {
    12.0
}
fn default_temperature_noise_channel() -> u64 {
    0xB10E_0001_0000_0001
}
fn default_moisture_noise_channel() -> u64 {
    0xB10E_0001_0000_0002
}
fn default_fallback_biome_key() -> String {
    "mineral_steppe".to_string()
}

/// Reasonable default material palette for the hardcoded neutral fallback biome.
///
/// This mirrors the `mineral_steppe` palette from `biomes.toml` — a balanced
/// generalist selection so that even when the TOML is missing or misconfigured,
/// the player still encounters materials during exploration. The seeds here are
/// well-known values from the original 10-material catalog.
fn default_fallback_material_palette() -> Vec<PaletteMaterial> {
    vec![
        PaletteMaterial {
            material_seed: 1002,
            selection_weight: 2.0,
        }, // Calcium
        PaletteMaterial {
            material_seed: 1005,
            selection_weight: 2.5,
        }, // Verdant
        PaletteMaterial {
            material_seed: 1008,
            selection_weight: 2.0,
        }, // Cobaltine
        PaletteMaterial {
            material_seed: 1009,
            selection_weight: 1.5,
        }, // Silite
        PaletteMaterial {
            material_seed: 1001,
            selection_weight: 1.0,
        }, // Ferrite
        PaletteMaterial {
            material_seed: 1003,
            selection_weight: 0.5,
        }, // Sulfurite
        PaletteMaterial {
            material_seed: 1010,
            selection_weight: 0.8,
        }, // Phosphite
    ]
}

impl Default for BiomeRegistry {
    fn default() -> Self {
        Self {
            noise_scale_chunks: default_biome_noise_scale_chunks(),
            temperature_noise_channel: default_temperature_noise_channel(),
            moisture_noise_channel: default_moisture_noise_channel(),
            fallback_biome_key: default_fallback_biome_key(),
            biomes: default_biome_definitions(),
        }
    }
}

/// A single entry in a biome's material palette.
///
/// Each biome defines a list of `PaletteMaterial` entries that control which
/// materials can appear in that biome and how likely each one is relative to
/// the others. The `material_seed` drives deterministic property generation
/// via `derive_material_from_seed`, and `selection_weight` is used for
/// weighted random selection when placing deposits.
///
/// A given seed may appear in multiple biomes with different weights, allowing
/// materials to be common in one biome and rare in another.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct PaletteMaterial {
    /// Seed value that deterministically defines this material's properties.
    ///
    /// The same seed always produces the same `GameMaterial` (density, color,
    /// name, etc.) regardless of which biome references it.
    pub material_seed: u64,
    /// Relative likelihood of this material being selected when placing a
    /// deposit in the biome.
    ///
    /// Higher values make this material more common. The actual probability
    /// is `selection_weight / sum(all weights in palette)`. Must be positive.
    pub selection_weight: f32,
}

/// One biome definition describing a region of temperature × moisture space.
///
/// Each biome occupies a rectangular region on the two climate axes. A chunk
/// belongs to the first biome (in definition order) whose temperature and
/// moisture ranges contain the chunk's sampled values.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct BiomeDefinition {
    /// Unique key identifying this biome (e.g., `"scorched_flats"`).
    pub key: String,
    /// Minimum temperature value (0.0–1.0) for this biome's range.
    pub temperature_min: f32,
    /// Maximum temperature value (0.0–1.0) for this biome's range.
    pub temperature_max: f32,
    /// Optional absolute minimum temperature threshold in Kelvin.
    ///
    /// When present, planet-level temperature mapping uses this value instead
    /// of the normalized `temperature_min` to determine biome applicability in
    /// absolute terms. This allows hot planets to shift biome boundaries so
    /// that a "cold" biome on a hot world is still warm in absolute Kelvin.
    #[serde(default)]
    pub temperature_abs_min_k: Option<f32>,
    /// Optional absolute maximum temperature threshold in Kelvin.
    ///
    /// Counterpart to `temperature_abs_min_k`. When both absolute fields are
    /// present, they define the biome's valid absolute temperature band.
    #[serde(default)]
    pub temperature_abs_max_k: Option<f32>,
    /// Minimum moisture value (0.0–1.0) for this biome's range.
    pub moisture_min: f32,
    /// Maximum moisture value (0.0–1.0) for this biome's range.
    pub moisture_max: f32,
    /// RGB ground color for per-chunk ground tiles in this biome.
    ///
    /// Components are in linear sRGB space (0.0–1.0 per channel).
    pub ground_color: [f32; 3],
    /// Multiplier applied to the deposit spawn threshold.
    ///
    /// Values > 1.0 increase deposit density (more deposits spawn).
    /// Values < 1.0 decrease it. The modifier scales the effective
    /// spawn threshold: `effective = base_threshold / density_modifier`,
    /// so a higher modifier lowers the threshold, admitting more candidates.
    #[serde(default = "one_f32")]
    pub density_modifier: f32,
    /// Per-deposit-key weight multipliers.
    ///
    /// Each key matches a `SurfaceMineralDepositDefinition::key`. The value
    /// is multiplied with that deposit's `selection_weight` when choosing
    /// which deposit type to place. Missing keys default to 1.0 (no change).
    #[serde(default)]
    pub deposit_weight_modifiers: HashMap<String, f32>,
    /// Material palette for this biome: which material seeds can appear and at
    /// what relative weight. During deposit generation, a seed is chosen from
    /// this palette via weighted random selection. If a seed hasn't been
    /// encountered before, it is derived and registered into `MaterialCatalog`
    /// on first use.
    #[serde(default)]
    pub material_palette: Vec<PaletteMaterial>,
}

fn one_f32() -> f32 {
    1.0
}

/// Hardcoded default biome definitions used when `biomes.toml` is missing.
///
/// These match the three biomes defined in the TOML file shipped with the
/// game. The defaults exist so the game runs correctly even without asset
/// files (important for integration tests and CI).
fn default_biome_definitions() -> Vec<BiomeDefinition> {
    vec![
        BiomeDefinition {
            key: "scorched_flats".to_string(),
            temperature_min: 0.6,
            temperature_max: 1.0,
            temperature_abs_min_k: Some(350.0),
            temperature_abs_max_k: Some(600.0),
            moisture_min: 0.0,
            moisture_max: 0.4,
            ground_color: [0.55, 0.38, 0.22],
            density_modifier: 1.15,
            deposit_weight_modifiers: HashMap::from([
                ("ferrite".to_string(), 3.0),
                ("silite".to_string(), 0.8),
                ("prismate".to_string(), 0.2),
            ]),
            material_palette: vec![
                PaletteMaterial {
                    material_seed: 1001,
                    selection_weight: 3.0,
                }, // Ferrite
                PaletteMaterial {
                    material_seed: 1003,
                    selection_weight: 2.5,
                }, // Sulfurite
                PaletteMaterial {
                    material_seed: 1006,
                    selection_weight: 2.0,
                }, // Osmium
                PaletteMaterial {
                    material_seed: 1007,
                    selection_weight: 1.5,
                }, // Volatite
                PaletteMaterial {
                    material_seed: 1002,
                    selection_weight: 0.5,
                }, // Calcium
                PaletteMaterial {
                    material_seed: 1009,
                    selection_weight: 0.3,
                }, // Silite
            ],
        },
        BiomeDefinition {
            key: "mineral_steppe".to_string(),
            temperature_min: 0.3,
            temperature_max: 0.7,
            temperature_abs_min_k: Some(220.0),
            temperature_abs_max_k: Some(350.0),
            moisture_min: 0.3,
            moisture_max: 0.7,
            ground_color: [0.26, 0.3, 0.22],
            density_modifier: 1.0,
            deposit_weight_modifiers: HashMap::new(),
            material_palette: default_fallback_material_palette(),
        },
        BiomeDefinition {
            key: "frost_shelf".to_string(),
            temperature_min: 0.0,
            temperature_max: 0.4,
            temperature_abs_min_k: Some(50.0),
            temperature_abs_max_k: Some(220.0),
            moisture_min: 0.5,
            moisture_max: 1.0,
            ground_color: [0.42, 0.48, 0.56],
            density_modifier: 0.7,
            deposit_weight_modifiers: HashMap::from([
                ("ferrite".to_string(), 0.2),
                ("silite".to_string(), 1.0),
                ("prismate".to_string(), 3.0),
            ]),
            material_palette: vec![
                PaletteMaterial {
                    material_seed: 1004,
                    selection_weight: 3.0,
                }, // Prismate
                PaletteMaterial {
                    material_seed: 1009,
                    selection_weight: 2.0,
                }, // Silite
                PaletteMaterial {
                    material_seed: 1010,
                    selection_weight: 2.5,
                }, // Phosphite
                PaletteMaterial {
                    material_seed: 1008,
                    selection_weight: 1.0,
                }, // Cobaltine
                PaletteMaterial {
                    material_seed: 1005,
                    selection_weight: 0.3,
                }, // Verdant
                PaletteMaterial {
                    material_seed: 1006,
                    selection_weight: 0.5,
                }, // Osmium
            ],
        },
    ]
}

/// Result of biome derivation for a single chunk.
///
/// Contains the biome key and all generation-relevant parameters that systems
/// need to modulate chunk content based on biome. This is a value type — it
/// is computed on demand from `derive_chunk_biome()` and not stored as a
/// Component or Resource.
#[derive(Clone, Debug)]
pub struct ChunkBiome {
    /// The biome key (e.g., `"scorched_flats"`).
    pub biome_key: String,
    /// RGB ground color for this chunk's ground tile.
    pub ground_color: [f32; 3],
    /// Density modifier applied to the deposit spawn threshold.
    pub density_modifier: f32,
    /// Per-deposit-key weight multipliers for material selection.
    pub deposit_weight_modifiers: HashMap<String, f32>,
    /// Material palette copied from the matched biome definition. Chunk
    /// generation uses this to select which material seeds can appear in
    /// deposits within this biome region. Consumed by
    /// `choose_material_seed_from_palette` during deposit site generation.
    pub material_palette: Vec<PaletteMaterial>,
}

/// Derive the biome for a chunk based on its canonical position on the planet.
///
/// We sample two coherent noise fields — temperature and moisture — at the
/// chunk's canonical (wrapped) center coordinate in **chunk space**. The noise
/// fields use the same bilinear-interpolated value noise as the deposit
/// density field (`continuous_value_field_01`), but operate in chunk-space
/// rather than world-space so that biome regions scale independently of chunk
/// size.
///
/// The canonical coordinate ensures torus-wrapped chunks produce the same
/// biome regardless of the player's raw position. We sample at the chunk
/// center (coord + 0.5) rather than the corner to avoid edge artifacts where
/// four chunks meet.
///
/// ## Fallback behavior
///
/// If no biome range matches the sampled (temperature, moisture) pair, we
/// fall back to the biome identified by `registry.fallback_biome_key`. If
/// that key also doesn't exist in the registry, we return a default neutral
/// biome (olive green, no modifiers).
pub fn derive_chunk_biome(
    profile: &WorldProfile,
    registry: &BiomeRegistry,
    chunk_coord: ChunkCoord,
    planet_env: Option<&PlanetEnvironment>,
) -> ChunkBiome {
    // Wrap to canonical torus coordinate so equivalent positions on the
    // planet surface always resolve to the same biome.
    let canonical = wrap_chunk_coord(chunk_coord, profile.planet_surface_diameter);

    // Sample temperature and moisture noise at the chunk center in chunk
    // space. Using (coord + 0.5) places the sample at the center of the
    // chunk cell rather than on the corner lattice, which avoids boundary
    // artifacts where four chunks with different biomes might all share a
    // corner sample.
    let chunk_center = PositionXZ::new(canonical.x as f32 + 0.5, canonical.z as f32 + 0.5);

    let temperature_seed = mix_seed(
        profile.biome_climate_seed,
        registry.temperature_noise_channel,
    );
    let moisture_seed = mix_seed(profile.biome_climate_seed, registry.moisture_noise_channel);

    let temperature = exterior::continuous_value_field_01(
        temperature_seed,
        chunk_center,
        registry.noise_scale_chunks,
    );
    let moisture = exterior::continuous_value_field_01(
        moisture_seed,
        chunk_center,
        registry.noise_scale_chunks,
    );

    // When a PlanetEnvironment is provided, map the 0.0–1.0 temperature
    // noise into the planet's absolute Kelvin range. This lets biome
    // definitions with absolute temperature thresholds (temperature_abs_min_k
    // / temperature_abs_max_k) gate biome selection based on real stellar
    // context. A hot planet's "cold" noise region still maps to a warm
    // absolute temperature, so only biomes that tolerate that heat can match.
    let abs_temp_k: Option<f32> = planet_env.map(|env| {
        env.surface_temp_min_k + temperature * (env.surface_temp_max_k - env.surface_temp_min_k)
    });

    // Find the first biome whose range contains the sampled values.
    // Order matters — overlapping ranges resolve to the first match.
    for biome_def in &registry.biomes {
        let normalized_match = temperature >= biome_def.temperature_min
            && temperature <= biome_def.temperature_max
            && moisture >= biome_def.moisture_min
            && moisture <= biome_def.moisture_max;

        if !normalized_match {
            continue;
        }

        // If the biome defines absolute Kelvin thresholds and we have a
        // planet environment, enforce the absolute temperature band as an
        // additional filter. Biomes without absolute thresholds pass
        // unconditionally (backwards compatible).
        if let Some(abs_k) = abs_temp_k
            && let (Some(abs_min), Some(abs_max)) = (
                biome_def.temperature_abs_min_k,
                biome_def.temperature_abs_max_k,
            )
            && (abs_k < abs_min || abs_k > abs_max)
        {
            continue;
        }

        return ChunkBiome {
            biome_key: biome_def.key.clone(),
            ground_color: biome_def.ground_color,
            density_modifier: biome_def.density_modifier,
            deposit_weight_modifiers: biome_def.deposit_weight_modifiers.clone(),
            material_palette: biome_def.material_palette.clone(),
        };
    }

    // No range matched — use the fallback biome.
    if let Some(fallback) = registry
        .biomes
        .iter()
        .find(|b| b.key == registry.fallback_biome_key)
    {
        return ChunkBiome {
            biome_key: fallback.key.clone(),
            ground_color: fallback.ground_color,
            density_modifier: fallback.density_modifier,
            deposit_weight_modifiers: fallback.deposit_weight_modifiers.clone(),
            material_palette: fallback.material_palette.clone(),
        };
    }

    // Even the fallback key is missing — return a hardcoded neutral default.
    // This should never happen with a well-formed biomes.toml, but we must
    // not panic in generation code.
    warn!(
        "Biome fallback key '{}' not found in registry; using hardcoded neutral default",
        registry.fallback_biome_key
    );
    ChunkBiome {
        biome_key: registry.fallback_biome_key.clone(),
        ground_color: [0.26, 0.3, 0.22],
        density_modifier: 1.0,
        deposit_weight_modifiers: HashMap::new(),
        material_palette: default_fallback_material_palette(),
    }
}

/// Load the biome registry from TOML, falling back to hardcoded defaults.
fn load_biome_registry(mut commands: Commands) {
    let registry = if Path::new(BIOME_CONFIG_PATH).exists() {
        match fs::read_to_string(BIOME_CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<BiomeRegistry>(&contents) {
                Ok(registry) => {
                    info!(
                        "Loaded biome registry from {BIOME_CONFIG_PATH} ({} biomes)",
                        registry.biomes.len()
                    );
                    registry
                }
                Err(error) => {
                    warn!("Could not parse {BIOME_CONFIG_PATH}, using defaults: {error}");
                    BiomeRegistry::default()
                }
            },
            Err(error) => {
                warn!("Could not read {BIOME_CONFIG_PATH}, using defaults: {error}");
                BiomeRegistry::default()
            }
        }
    } else {
        warn!("{BIOME_CONFIG_PATH} not found, using defaults");
        BiomeRegistry::default()
    };

    commands.insert_resource(registry);
}

#[cfg(test)]
#[path = "world_generation_tests.rs"]
mod tests;
