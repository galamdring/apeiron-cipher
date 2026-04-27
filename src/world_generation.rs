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

/// The current flat exterior surface.
///
/// This is the live implementation of [`SurfaceProvider`] for the POC exterior.
/// It models a perfectly horizontal plane at `surface_y` within `bounds_xz`.
/// Any query outside the bounds returns `valid = false`.
///
/// When the game eventually adds non-flat terrain, this struct is not deleted —
/// it remains a valid (if boring) implementation. A new provider takes over for
/// terrain chunks that have actual elevation data.
#[cfg(test)]
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

#[cfg(test)]
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

/// Noise-based terrain surface for runtime use.
///
/// `PlanetSurface` samples a multi-octave value noise field (reusing
/// `continuous_value_field_01` at different scales) to produce elevation and
/// surface normals across the planet. It handles torus wrapping at world-
/// coordinate level so that terrain is continuous across the wrap seam.
///
/// This struct is the runtime replacement for `FlatSurface` in the exterior
/// chunk pipeline. `FlatSurface`, `SteppedSurface`, and `TiltedSurface` remain
/// available for tests.
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

    // Early-out: skip recomputation when the player hasn't changed chunks.
    // This avoids per-frame Vec allocation and prevents Bevy change detection
    // from firing on ActiveChunkNeighborhood every frame.
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
    // Clamp to minimum 1.0 to prevent division by zero in release builds.
    // Validation at config load should prevent this, but defense in depth.
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
mod tests {
    use super::*;

    #[test]
    fn world_profile_derivation_is_deterministic() {
        let config = WorldGenerationConfig {
            planet_seed: Some(123_456),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 2,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        };

        let a = WorldProfile::from_config(&config).unwrap();
        let b = WorldProfile::from_config(&config).unwrap();

        assert_eq!(a, b);
    }

    #[test]
    fn override_mode_is_not_system_derived_and_has_no_system_context() {
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            ..Default::default()
        };

        let profile = WorldProfile::from_config(&config).unwrap();

        assert!(
            !profile.is_system_derived(),
            "override mode must report is_system_derived() == false"
        );
        assert!(
            profile.system_context.is_none(),
            "override mode must have system_context == None"
        );
    }

    #[test]
    fn world_profile_with_system_context_survives_serde_round_trip() {
        use crate::solar_system::{
            OrbitalConfig, PlanetEnvironmentConfig, SolarSystemSeed, StarTypeRegistry,
        };

        // Build a WorldProfile via the full system-seed derivation chain so
        // every field is populated with realistic, derived values.
        let star_registry = StarTypeRegistry::default();
        let orbital_config = OrbitalConfig::default();
        let env_config = PlanetEnvironmentConfig::default();
        let config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_index: 0,
            planet_seed: None,
            ..Default::default()
        };

        let profile =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("system-seed derivation must succeed for index 0");

        assert!(
            profile.is_system_derived(),
            "profile must be in system-derived mode"
        );

        // JSON round-trip
        let json =
            serde_json::to_string_pretty(&profile).expect("WorldProfile must serialize to JSON");
        let deserialized: WorldProfile =
            serde_json::from_str(&json).expect("WorldProfile must deserialize from JSON");

        assert_eq!(
            profile, deserialized,
            "WorldProfile with SystemContext must survive JSON round-trip"
        );

        // Verify the SystemContext fields are actually present after round-trip
        let ctx = deserialized
            .system_context
            .as_ref()
            .expect("system_context must survive round-trip");
        assert_eq!(ctx.system_seed, SolarSystemSeed(42));
        assert_eq!(ctx.planet_orbital_index, 0);
    }

    #[test]
    fn world_profile_override_mode_survives_serde_round_trip() {
        let config = WorldGenerationConfig {
            planet_seed: Some(12345),
            ..Default::default()
        };
        let profile = WorldProfile::from_config(&config).unwrap();

        assert!(!profile.is_system_derived());

        let json =
            serde_json::to_string_pretty(&profile).expect("WorldProfile must serialize to JSON");
        let deserialized: WorldProfile =
            serde_json::from_str(&json).expect("WorldProfile must deserialize from JSON");

        assert_eq!(
            profile, deserialized,
            "WorldProfile in override mode must survive JSON round-trip"
        );
        assert!(
            deserialized.system_context.is_none(),
            "override mode must have no system_context after round-trip"
        );
    }

    #[test]
    fn world_profile_derives_distinct_sub_seeds() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig::default()).unwrap();

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
        let center = ChunkCoord::new(5, 2);
        let chunks = active_chunk_neighborhood(center, 1);

        assert_eq!(chunks.len(), 9);
        assert_eq!(chunks.first().copied(), Some(ChunkCoord::new(4, 1)));
        assert_eq!(chunks.last().copied(), Some(ChunkCoord::new(6, 3)));
        assert!(chunks.contains(&center));
    }

    #[test]
    fn chunk_generation_key_is_deterministic_for_same_inputs() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig {
            planet_seed: Some(777),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        })
        .unwrap();
        let chunk = ChunkCoord::new(-3, 4);

        let a = derive_chunk_generation_key(&profile, chunk);
        let b = derive_chunk_generation_key(&profile, chunk);

        assert_eq!(a, b);
    }

    #[test]
    fn chunk_generation_key_changes_for_different_chunks() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig::default()).unwrap();
        let a = derive_chunk_generation_key(&profile, ChunkCoord::new(0, 0));
        let b = derive_chunk_generation_key(&profile, ChunkCoord::new(1, 0));

        assert_ne!(a.placement_density_key, b.placement_density_key);
        assert_ne!(a.placement_variation_key, b.placement_variation_key);
        assert_ne!(a.object_identity_key, b.object_identity_key);
    }

    #[test]
    fn generated_object_id_is_stable_from_explicit_inputs() {
        let profile = WorldProfile::from_config(&WorldGenerationConfig {
            planet_seed: Some(42),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        })
        .unwrap();

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

    // ── Story 5a.1: Planet Surface Topology Tests ─────────────────────────

    // ── wrap_chunk_coord ──────────────────────────────────────────────────

    #[test]
    fn wrap_chunk_coord_passthrough_for_in_range_coords() {
        // Coordinates already within [0, diameter) should pass through unchanged.
        let diameter = 100;
        let coord = ChunkCoord::new(50, 75);
        let wrapped = wrap_chunk_coord(coord, diameter);
        assert_eq!(wrapped, coord);
    }

    #[test]
    fn wrap_chunk_coord_wraps_positive_overflow() {
        // A coordinate >= diameter should wrap back around.
        let diameter = 100;
        let coord = ChunkCoord::new(105, 200);
        let wrapped = wrap_chunk_coord(coord, diameter);
        assert_eq!(wrapped, ChunkCoord::new(5, 0));
    }

    #[test]
    fn wrap_chunk_coord_wraps_negative_to_positive() {
        // Negative coordinates should wrap to the positive range.
        // -1 mod 100 = 99, -50 mod 100 = 50
        let diameter = 100;
        let coord = ChunkCoord::new(-1, -50);
        let wrapped = wrap_chunk_coord(coord, diameter);
        assert_eq!(wrapped, ChunkCoord::new(99, 50));
    }

    #[test]
    fn wrap_chunk_coord_exact_boundary_wraps_to_zero() {
        // A coordinate exactly equal to the diameter should wrap to 0.
        let diameter = 100;
        let coord = ChunkCoord::new(100, 100);
        let wrapped = wrap_chunk_coord(coord, diameter);
        assert_eq!(wrapped, ChunkCoord::new(0, 0));
    }

    #[test]
    fn wrap_chunk_coord_zero_passes_through() {
        let diameter = 100;
        let coord = ChunkCoord::new(0, 0);
        let wrapped = wrap_chunk_coord(coord, diameter);
        assert_eq!(wrapped, ChunkCoord::new(0, 0));
    }

    #[test]
    fn wrap_chunk_coord_large_negative() {
        // -301 mod 100 = 99 (since -301 = -4*100 + 99)
        let diameter = 100;
        let coord = ChunkCoord::new(-301, -1);
        let wrapped = wrap_chunk_coord(coord, diameter);
        assert_eq!(wrapped, ChunkCoord::new(99, 99));
    }

    #[test]
    #[should_panic(expected = "planet surface diameter must be positive")]
    fn wrap_chunk_coord_panics_on_zero_diameter() {
        wrap_chunk_coord(ChunkCoord::new(1, 1), 0);
    }

    // ── derive_planet_surface_radius ──────────────────────────────────────

    #[test]
    fn derive_planet_surface_radius_is_deterministic() {
        let seed = PlanetSeed(42);
        let a = derive_planet_surface_radius(seed, 500, 5000);
        let b = derive_planet_surface_radius(seed, 500, 5000);
        assert_eq!(a, b);
    }

    #[test]
    fn derive_planet_surface_radius_stays_within_range() {
        // Test many seeds to increase confidence the result is always in range.
        for seed_val in 0..1000_u64 {
            let radius = derive_planet_surface_radius(PlanetSeed(seed_val), 500, 5000);
            assert!(
                (500..=5000).contains(&radius),
                "seed {seed_val} produced out-of-range radius {radius}"
            );
        }
    }

    #[test]
    fn derive_planet_surface_radius_min_equals_max_returns_exact() {
        // When min == max, the radius must be exactly that value regardless of seed.
        let radius = derive_planet_surface_radius(PlanetSeed(99999), 1000, 1000);
        assert_eq!(radius, 1000);
    }

    #[test]
    fn derive_planet_surface_radius_different_seeds_vary() {
        // Collect radii from several seeds and verify they are not all identical.
        // This is a statistical property — with 100 seeds across a range of 4501
        // values it would be astronomically unlikely for all to match.
        let radii: Vec<i32> = (0..100)
            .map(|s| derive_planet_surface_radius(PlanetSeed(s), 500, 5000))
            .collect();
        let all_same = radii.iter().all(|&r| r == radii[0]);
        assert!(
            !all_same,
            "100 different seeds all produced the same radius"
        );
    }

    #[test]
    #[should_panic(expected = "planet surface min radius must be positive")]
    fn derive_planet_surface_radius_panics_on_zero_min() {
        derive_planet_surface_radius(PlanetSeed(1), 0, 100);
    }

    #[test]
    #[should_panic(expected = "planet surface max radius must be >= min radius")]
    fn derive_planet_surface_radius_panics_when_min_exceeds_max() {
        derive_planet_surface_radius(PlanetSeed(1), 5000, 500);
    }

    // ── WorldProfile planet surface fields ────────────────────────────────

    #[test]
    fn world_profile_includes_planet_surface_fields() {
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        };
        let profile = WorldProfile::from_config(&config).unwrap();

        assert!(
            (500..=5000).contains(&profile.planet_surface_radius),
            "radius {} out of configured range",
            profile.planet_surface_radius
        );
        assert_eq!(
            profile.planet_surface_diameter,
            profile.planet_surface_radius * 2
        );
    }

    // ── active_chunk_neighborhood (raw coords for positioning) ──────────

    #[test]
    fn neighborhood_returns_raw_unwrapped_coords() {
        // Center at (0, 0) with radius 1. The neighborhood should include
        // negative coordinates — no wrapping — so that chunk_origin_xz
        // produces world-space positions near the player.
        let center = ChunkCoord::new(0, 0);
        let chunks = active_chunk_neighborhood(center, 1);

        assert_eq!(chunks.len(), 9);
        // Should contain raw (-1, -1), not wrapped to (diameter-1, diameter-1)
        assert!(
            chunks.contains(&ChunkCoord::new(-1, -1)),
            "expected raw (-1,-1), got: {chunks:?}"
        );
        assert!(
            chunks.contains(&ChunkCoord::new(-1, 0)),
            "expected raw (-1,0), got: {chunks:?}"
        );
        assert!(
            chunks.contains(&ChunkCoord::new(0, -1)),
            "expected raw (0,-1), got: {chunks:?}"
        );
        assert!(chunks.contains(&ChunkCoord::new(0, 0)));
    }

    // ── Torus wrapping in generation keys ─────────────────────────────────

    #[test]
    fn generation_key_wraps_raw_coords_to_canonical() {
        // derive_chunk_generation_key should produce identical keys for raw
        // coordinates that are equivalent under torus wrapping. This is what
        // makes the torus seamless — chunk (-1, 0) on a diameter-100 planet
        // generates the same content as chunk (99, 0).
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 50,
            planet_surface_max_radius: 50,
            ..Default::default()
        };
        let profile = WorldProfile::from_config(&config).unwrap();
        let diameter = profile.planet_surface_diameter; // 100

        let raw_negative = ChunkCoord::new(-1, -1);
        let raw_positive = ChunkCoord::new(diameter - 1, diameter - 1);

        let key_a = derive_chunk_generation_key(&profile, raw_negative);
        let key_b = derive_chunk_generation_key(&profile, raw_positive);
        assert_eq!(key_a, key_b);
    }

    #[test]
    fn generation_key_wraps_overflow_coords() {
        // A coordinate beyond the diameter should produce the same key as
        // the equivalent in-range coordinate.
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 50,
            planet_surface_max_radius: 50,
            ..Default::default()
        };
        let profile = WorldProfile::from_config(&config).unwrap();
        let diameter = profile.planet_surface_diameter; // 100

        let canonical = ChunkCoord::new(5, 10);
        let overflow = ChunkCoord::new(5 + diameter, 10 + diameter);

        let key_a = derive_chunk_generation_key(&profile, canonical);
        let key_b = derive_chunk_generation_key(&profile, overflow);
        assert_eq!(key_a, key_b);
    }

    // ── Story 5a.2: Biome derivation ─────────────────────────────────────

    fn sample_config() -> WorldGenerationConfig {
        WorldGenerationConfig {
            planet_seed: Some(2026),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 500,
            planet_surface_max_radius: 5000,
            ..Default::default()
        }
    }

    #[test]
    fn biome_derivation_is_deterministic() {
        // Same seed + coord must always produce the same biome.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = BiomeRegistry::default();
        let coord = ChunkCoord::new(7, 13);

        let a = derive_chunk_biome(&profile, &registry, coord, None);
        let b = derive_chunk_biome(&profile, &registry, coord, None);

        assert_eq!(a.biome_key, b.biome_key);
        assert_eq!(a.ground_color, b.ground_color);
        assert_eq!(a.density_modifier, b.density_modifier);
    }

    #[test]
    fn all_three_biomes_reachable() {
        // Scan a large set of coords and verify all three biome keys appear.
        // The noise field is coherent, so with enough samples we should hit
        // all defined ranges.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = BiomeRegistry::default();

        let mut found: std::collections::HashSet<String> = std::collections::HashSet::new();
        for x in -50..50 {
            for z in -50..50 {
                let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, z), None);
                found.insert(biome.biome_key.clone());
                if found.len() == 3 {
                    break;
                }
            }
            if found.len() == 3 {
                break;
            }
        }

        assert!(
            found.contains("scorched_flats"),
            "scorched_flats not found in 100×100 scan, found: {found:?}"
        );
        assert!(
            found.contains("mineral_steppe"),
            "mineral_steppe not found in 100×100 scan, found: {found:?}"
        );
        assert!(
            found.contains("frost_shelf"),
            "frost_shelf not found in 100×100 scan, found: {found:?}"
        );
    }

    #[test]
    fn fallback_biome_used_when_no_range_matches() {
        // Create a registry with a single biome that only covers a tiny corner,
        // then sample a coord that lands outside it.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = BiomeRegistry {
            noise_scale_chunks: 12.0,
            temperature_noise_channel: 0xB10E_0001_0000_0001,
            moisture_noise_channel: 0xB10E_0001_0000_0002,
            fallback_biome_key: "fallback_test".to_string(),
            biomes: vec![
                // Impossibly narrow range — almost nothing will match.
                BiomeDefinition {
                    key: "narrow".to_string(),
                    temperature_min: 0.999,
                    temperature_max: 1.0,
                    temperature_abs_min_k: None,
                    temperature_abs_max_k: None,
                    moisture_min: 0.999,
                    moisture_max: 1.0,
                    ground_color: [1.0, 0.0, 0.0],
                    density_modifier: 1.0,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: Vec::new(),
                },
                // Fallback biome.
                BiomeDefinition {
                    key: "fallback_test".to_string(),
                    temperature_min: 0.0,
                    temperature_max: 0.0,
                    temperature_abs_min_k: None,
                    temperature_abs_max_k: None,
                    moisture_min: 0.0,
                    moisture_max: 0.0,
                    ground_color: [0.5, 0.5, 0.5],
                    density_modifier: 0.5,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: Vec::new(),
                },
            ],
        };

        // Scan coords until we find one that falls back (most will).
        let mut found_fallback = false;
        for x in 0..20 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, 0), None);
            if biome.biome_key == "fallback_test" {
                found_fallback = true;
                assert_eq!(
                    biome.density_modifier, 0.5,
                    "fallback biome must use its own density modifier"
                );
                break;
            }
        }
        assert!(
            found_fallback,
            "expected at least one coord to trigger fallback biome"
        );
    }

    #[test]
    fn biome_climate_seed_is_distinct_from_other_seeds() {
        // The biome climate seed must not collide with any other sub-seed
        // in WorldProfile to avoid correlated noise fields.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();

        assert_ne!(profile.biome_climate_seed, profile.placement_density_seed);
        assert_ne!(profile.biome_climate_seed, profile.placement_variation_seed);
        assert_ne!(profile.biome_climate_seed, profile.planet_seed.0);
    }

    #[test]
    fn elevation_seed_is_distinct_from_other_seeds() {
        // The elevation seed must not collide with any other sub-seed
        // in WorldProfile to avoid correlated noise fields.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();

        assert_ne!(profile.elevation_seed, profile.placement_density_seed);
        assert_ne!(profile.elevation_seed, profile.placement_variation_seed);
        assert_ne!(profile.elevation_seed, profile.object_identity_seed);
        assert_ne!(profile.elevation_seed, profile.biome_climate_seed);
        assert_ne!(profile.elevation_seed, profile.planet_seed.0);
    }

    #[test]
    fn biome_registry_toml_round_trip() {
        // Verify BiomeRegistry serializes to TOML and back without data loss.
        let registry = BiomeRegistry::default();
        let toml_str = toml::to_string(&registry).expect("BiomeRegistry should serialize to TOML");
        let parsed: BiomeRegistry =
            toml::from_str(&toml_str).expect("BiomeRegistry should parse from TOML");

        assert_eq!(parsed.biomes.len(), registry.biomes.len());
        assert_eq!(parsed.fallback_biome_key, registry.fallback_biome_key);
        assert_eq!(parsed.noise_scale_chunks, registry.noise_scale_chunks);
        for (a, b) in registry.biomes.iter().zip(parsed.biomes.iter()) {
            assert_eq!(a.key, b.key);
            assert_eq!(a.temperature_min, b.temperature_min);
            assert_eq!(a.temperature_max, b.temperature_max);
            assert_eq!(a.density_modifier, b.density_modifier);
            assert_eq!(a.deposit_weight_modifiers, b.deposit_weight_modifiers);
            assert_eq!(a.material_palette.len(), b.material_palette.len());
            for (pa, pb) in a.material_palette.iter().zip(b.material_palette.iter()) {
                assert_eq!(pa.material_seed, pb.material_seed);
                assert_eq!(pa.selection_weight, pb.selection_weight);
            }
        }
    }

    #[test]
    fn biome_registry_toml_round_trip_with_palette_entries() {
        // Verify that material palette entries survive a TOML serialize→deserialize cycle,
        // including hex seed values and fractional weights.
        let mut registry = BiomeRegistry::default();

        // Inject palette entries into the first biome (or add a biome if none exist).
        if registry.biomes.is_empty() {
            registry.biomes.push(BiomeDefinition {
                key: "test_biome".to_string(),
                temperature_min: 0.0,
                temperature_max: 1.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.0,
                moisture_max: 1.0,
                ground_color: [0.5, 0.5, 0.5],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            });
        }
        let palette = &mut registry.biomes[0].material_palette;
        palette.clear();
        palette.push(PaletteMaterial {
            material_seed: 0xFE00_0000_0000_0001,
            selection_weight: 3.0,
        });
        palette.push(PaletteMaterial {
            material_seed: 0xFE00_0000_0000_0002,
            selection_weight: 0.5,
        });
        palette.push(PaletteMaterial {
            material_seed: 42,
            selection_weight: 1.0,
        });

        let toml_str =
            toml::to_string(&registry).expect("BiomeRegistry with palettes should serialize");
        let parsed: BiomeRegistry =
            toml::from_str(&toml_str).expect("BiomeRegistry with palettes should parse back");

        let original_palette = &registry.biomes[0].material_palette;
        let parsed_palette = &parsed.biomes[0].material_palette;
        assert_eq!(
            original_palette.len(),
            parsed_palette.len(),
            "palette length must survive round-trip"
        );
        for (orig, rt) in original_palette.iter().zip(parsed_palette.iter()) {
            assert_eq!(
                orig.material_seed, rt.material_seed,
                "material_seed must survive round-trip"
            );
            assert_eq!(
                orig.selection_weight, rt.selection_weight,
                "selection_weight must survive round-trip"
            );
        }
    }

    #[test]
    fn biome_toml_round_trip_empty_palette() {
        // A biome with an empty material_palette must round-trip cleanly.
        let mut registry = BiomeRegistry::default();
        for biome in &mut registry.biomes {
            biome.material_palette.clear();
        }
        let toml_str = toml::to_string(&registry).expect("serialize with empty palettes");
        let parsed: BiomeRegistry = toml::from_str(&toml_str).expect("parse with empty palettes");
        for (a, b) in registry.biomes.iter().zip(parsed.biomes.iter()) {
            assert!(
                b.material_palette.is_empty(),
                "biome '{}' palette should be empty after round-trip",
                a.key,
            );
        }
    }

    #[test]
    fn biome_toml_round_trip_shared_seed_across_biomes() {
        // The same material seed can appear in multiple biomes with different weights.
        let shared_seed: u64 = 0xABCD_0000_0000_0099;
        let mut registry = BiomeRegistry::default();

        // Ensure at least two biomes exist.
        while registry.biomes.len() < 2 {
            registry.biomes.push(BiomeDefinition {
                key: format!("synth_biome_{}", registry.biomes.len()),
                temperature_min: 0.0,
                temperature_max: 1.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: 0.0,
                moisture_max: 1.0,
                ground_color: [0.3, 0.3, 0.3],
                density_modifier: 1.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            });
        }

        // Place the same seed in the first two biomes with different weights.
        registry.biomes[0].material_palette = vec![PaletteMaterial {
            material_seed: shared_seed,
            selection_weight: 5.0,
        }];
        registry.biomes[1].material_palette = vec![PaletteMaterial {
            material_seed: shared_seed,
            selection_weight: 0.1,
        }];

        let toml_str = toml::to_string(&registry).expect("serialize shared-seed registry");
        let parsed: BiomeRegistry = toml::from_str(&toml_str).expect("parse shared-seed registry");

        assert_eq!(
            parsed.biomes[0].material_palette[0].material_seed,
            shared_seed
        );
        assert_eq!(parsed.biomes[0].material_palette[0].selection_weight, 5.0);
        assert_eq!(
            parsed.biomes[1].material_palette[0].material_seed,
            shared_seed
        );
        assert_eq!(parsed.biomes[1].material_palette[0].selection_weight, 0.1);
    }

    #[test]
    fn biome_toml_parses_shipped_asset_file() {
        // Verify the actual shipped biomes.toml parses correctly, including any
        // material palette entries defined there.
        let toml_content =
            std::fs::read_to_string(BIOME_CONFIG_PATH).expect("shipped biomes.toml must exist");
        let registry: BiomeRegistry =
            toml::from_str(&toml_content).expect("shipped biomes.toml must parse");

        assert!(
            !registry.biomes.is_empty(),
            "shipped biomes.toml must define at least one biome"
        );

        // Every palette entry must have a positive weight and non-zero seed.
        for biome in &registry.biomes {
            for entry in &biome.material_palette {
                assert!(
                    entry.selection_weight > 0.0,
                    "biome '{}' has palette entry with non-positive weight {}",
                    biome.key,
                    entry.selection_weight,
                );
            }
        }
    }

    #[test]
    fn biome_derivation_wraps_torus_correctly() {
        // Equivalent torus coordinates must produce the same biome.
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            chunk_size_world_units: 45.0,
            active_chunk_radius: 1,
            building_cell_size: 1.0,
            planet_surface_min_radius: 50,
            planet_surface_max_radius: 50,
            ..Default::default()
        };
        let profile = WorldProfile::from_config(&config).unwrap();
        let registry = BiomeRegistry::default();
        let diameter = profile.planet_surface_diameter;

        let raw = ChunkCoord::new(-3, 7);
        let wrapped = ChunkCoord::new(-3 + diameter, 7);

        let a = derive_chunk_biome(&profile, &registry, raw, None);
        let b = derive_chunk_biome(&profile, &registry, wrapped, None);

        assert_eq!(a.biome_key, b.biome_key);
        assert_eq!(a.ground_color, b.ground_color);
    }

    // ── Error / failure state tests ─────────────────────────────────────

    #[test]
    fn empty_registry_returns_hardcoded_neutral_default() {
        // With zero biome definitions and a fallback key that can't match,
        // `derive_chunk_biome` must return a hardcoded neutral default
        // rather than panicking.
        let config = sample_config();
        let profile = WorldProfile::from_config(&config).unwrap();
        let registry = BiomeRegistry {
            biomes: vec![],
            fallback_biome_key: "nonexistent".to_string(),
            noise_scale_chunks: 10.0,
            temperature_noise_channel: 0xB10E_0001_0000_0001,
            moisture_noise_channel: 0xB10E_0001_0000_0002,
        };

        let result = derive_chunk_biome(&profile, &registry, ChunkCoord::new(0, 0), None);

        // Should get the hardcoded neutral default values.
        assert_eq!(result.biome_key, "nonexistent");
        assert_eq!(result.ground_color, [0.26, 0.3, 0.22]);
        assert_eq!(result.density_modifier, 1.0);
        assert!(result.deposit_weight_modifiers.is_empty());
    }

    #[test]
    fn fallback_key_missing_from_registry_returns_hardcoded_default() {
        // Registry has biomes but none match AND the fallback key doesn't
        // exist in the registry. This exercises the third fallback path
        // (lines ~1206-1214).
        let config = sample_config();
        let profile = WorldProfile::from_config(&config).unwrap();

        // Define biomes that cover an impossibly narrow range so nothing
        // will match any real noise sample.
        let registry = BiomeRegistry {
            biomes: vec![BiomeDefinition {
                key: "impossible".to_string(),
                temperature_min: -999.0,
                temperature_max: -998.0,
                temperature_abs_min_k: None,
                temperature_abs_max_k: None,
                moisture_min: -999.0,
                moisture_max: -998.0,
                ground_color: [1.0, 0.0, 0.0],
                density_modifier: 5.0,
                deposit_weight_modifiers: HashMap::new(),
                material_palette: Vec::new(),
            }],
            fallback_biome_key: "does_not_exist".to_string(),
            noise_scale_chunks: 10.0,
            temperature_noise_channel: 0xB10E_0001_0000_0001,
            moisture_noise_channel: 0xB10E_0001_0000_0002,
        };

        let result = derive_chunk_biome(&profile, &registry, ChunkCoord::new(5, 5), None);

        // Must get the hardcoded neutral, not panic.
        assert_eq!(result.biome_key, "does_not_exist");
        assert_eq!(result.ground_color, [0.26, 0.3, 0.22]);
        assert_eq!(result.density_modifier, 1.0);
    }

    // ── Story 5a.4: ChunkBiome includes correct palette per biome ──────

    #[test]
    fn chunk_biome_includes_correct_palette_for_each_biome_type() {
        // Derive chunks across a large coordinate range, collecting the
        // material palette returned for each biome key. Verify that every
        // biome's palette matches the palette defined in its BiomeDefinition.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = BiomeRegistry::default();

        // Build expected palettes from the registry, keyed by biome key.
        let expected: HashMap<String, Vec<(u64, f32)>> = registry
            .biomes
            .iter()
            .map(|b| {
                let palette = b
                    .material_palette
                    .iter()
                    .map(|p| (p.material_seed, p.selection_weight))
                    .collect::<Vec<_>>();
                (b.key.clone(), palette)
            })
            .collect();

        // Track which biomes we have verified so we can assert full coverage.
        let mut verified: std::collections::HashSet<String> = std::collections::HashSet::new();

        for x in -50..50 {
            for z in -50..50 {
                let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, z), None);

                let Some(expected_palette) = expected.get(&biome.biome_key) else {
                    panic!(
                        "derive_chunk_biome returned unknown biome key '{}'",
                        biome.biome_key
                    );
                };

                let actual: Vec<(u64, f32)> = biome
                    .material_palette
                    .iter()
                    .map(|p| (p.material_seed, p.selection_weight))
                    .collect();

                assert_eq!(
                    &actual, expected_palette,
                    "palette mismatch for biome '{}' at chunk ({}, {})",
                    biome.biome_key, x, z
                );

                verified.insert(biome.biome_key);
                if verified.len() == expected.len() {
                    break;
                }
            }
            if verified.len() == expected.len() {
                break;
            }
        }

        // Ensure we actually hit all three biomes, not just a subset.
        for key in expected.keys() {
            assert!(
                verified.contains(key),
                "biome '{key}' was never reached in 100×100 scan — cannot verify its palette"
            );
        }
    }

    #[test]
    fn chunk_biome_fallback_carries_fallback_palette() {
        // When no biome range matches, the fallback biome's palette must be
        // propagated into the ChunkBiome, not an empty vec.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let fallback_palette = vec![
            PaletteMaterial {
                material_seed: 0xAAAA,
                selection_weight: 1.0,
            },
            PaletteMaterial {
                material_seed: 0xBBBB,
                selection_weight: 2.0,
            },
        ];
        let registry = BiomeRegistry {
            noise_scale_chunks: 12.0,
            temperature_noise_channel: 0xB10E_0001_0000_0001,
            moisture_noise_channel: 0xB10E_0001_0000_0002,
            fallback_biome_key: "fb".to_string(),
            biomes: vec![
                // Impossibly narrow range — almost nothing will match.
                BiomeDefinition {
                    key: "narrow".to_string(),
                    temperature_min: 0.999,
                    temperature_max: 1.0,
                    temperature_abs_min_k: None,
                    temperature_abs_max_k: None,
                    moisture_min: 0.999,
                    moisture_max: 1.0,
                    ground_color: [1.0, 0.0, 0.0],
                    density_modifier: 1.0,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: Vec::new(),
                },
                BiomeDefinition {
                    key: "fb".to_string(),
                    temperature_min: 0.0,
                    temperature_max: 0.0,
                    temperature_abs_min_k: None,
                    temperature_abs_max_k: None,
                    moisture_min: 0.0,
                    moisture_max: 0.0,
                    ground_color: [0.5, 0.5, 0.5],
                    density_modifier: 1.0,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: fallback_palette.clone(),
                },
            ],
        };

        // Most coords will miss the narrow biome and hit the fallback.
        let mut found = false;
        for x in 0..20 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, 0), None);
            if biome.biome_key == "fb" {
                assert_eq!(
                    biome.material_palette.len(),
                    fallback_palette.len(),
                    "fallback biome palette length mismatch"
                );
                for (actual, expected) in biome.material_palette.iter().zip(fallback_palette.iter())
                {
                    assert_eq!(actual.material_seed, expected.material_seed);
                    assert_eq!(actual.selection_weight, expected.selection_weight);
                }
                found = true;
                break;
            }
        }
        assert!(
            found,
            "expected at least one coord to trigger fallback biome"
        );
    }

    #[test]
    fn chunk_biome_hardcoded_default_has_reasonable_palette() {
        // When the fallback key itself is missing from the registry, the
        // hardcoded neutral default must still provide a non-empty material
        // palette so that deposits can be generated even without biomes.toml.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = BiomeRegistry {
            fallback_biome_key: "does_not_exist".to_string(),
            biomes: Vec::new(),
            noise_scale_chunks: 12.0,
            temperature_noise_channel: 0xB10E_0001_0000_0001,
            moisture_noise_channel: 0xB10E_0001_0000_0002,
        };

        let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(5, 5), None);
        assert!(
            !biome.material_palette.is_empty(),
            "hardcoded neutral default must have a non-empty material palette"
        );
        // All weights must be positive.
        for entry in &biome.material_palette {
            assert!(
                entry.selection_weight > 0.0,
                "palette entry seed {} has non-positive weight {}",
                entry.material_seed,
                entry.selection_weight
            );
        }
    }

    // ── PlanetEnvironment temperature scaling tests ─────────────────────

    /// Helper: build a registry with biomes that have absolute Kelvin thresholds
    /// for testing planet environment integration.
    fn abs_temp_registry() -> BiomeRegistry {
        BiomeRegistry {
            fallback_biome_key: "neutral_biome".to_string(),
            noise_scale_chunks: 12.0,
            temperature_noise_channel: 0xB10E_0001_0000_0001,
            moisture_noise_channel: 0xB10E_0001_0000_0002,
            biomes: vec![
                BiomeDefinition {
                    key: "hot_biome".to_string(),
                    temperature_min: 0.6,
                    temperature_max: 1.0,
                    temperature_abs_min_k: Some(350.0),
                    temperature_abs_max_k: Some(600.0),
                    moisture_min: 0.0,
                    moisture_max: 1.0,
                    ground_color: [0.8, 0.3, 0.1],
                    density_modifier: 1.0,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: Vec::new(),
                },
                BiomeDefinition {
                    key: "cold_biome".to_string(),
                    temperature_min: 0.0,
                    temperature_max: 0.5,
                    temperature_abs_min_k: Some(50.0),
                    temperature_abs_max_k: Some(220.0),
                    moisture_min: 0.0,
                    moisture_max: 1.0,
                    ground_color: [0.3, 0.3, 0.5],
                    density_modifier: 1.0,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: Vec::new(),
                },
                // Neutral fallback biome: no absolute thresholds, covers the
                // full normalized range so it catches anything filtered out by
                // absolute temperature checks on the other biomes.
                BiomeDefinition {
                    key: "neutral_biome".to_string(),
                    temperature_min: 0.0,
                    temperature_max: 1.0,
                    temperature_abs_min_k: None,
                    temperature_abs_max_k: None,
                    moisture_min: 0.0,
                    moisture_max: 1.0,
                    ground_color: [0.4, 0.4, 0.4],
                    density_modifier: 1.0,
                    deposit_weight_modifiers: HashMap::new(),
                    material_palette: Vec::new(),
                },
            ],
        }
    }

    #[test]
    fn planet_env_none_uses_normalized_matching_only() {
        // Without PlanetEnvironment, absolute thresholds are ignored and
        // biomes match purely on normalized temperature/moisture ranges.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = abs_temp_registry();

        // Scan a range of chunks — every result should match one of the two
        // biomes based on normalized ranges alone, regardless of absolute K.
        for x in 0..20 {
            let biome = derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, 0), None);
            assert!(
                biome.biome_key == "hot_biome" || biome.biome_key == "cold_biome",
                "unexpected biome: {}",
                biome.biome_key,
            );
        }
    }

    #[test]
    fn planet_env_hot_planet_filters_cold_biome() {
        // A very hot planet (min 400 K, max 700 K) maps all noise values
        // above the cold biome's absolute max of 220 K, so the cold biome
        // should never match.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = abs_temp_registry();
        let hot_env = PlanetEnvironment {
            surface_temp_min_k: 400.0,
            surface_temp_max_k: 700.0,
            atmosphere_density: 0.5,
            radiation_level: 0.8,
            surface_gravity_g: 1.2,
            in_habitable_zone: false,
        };

        for x in 0..50 {
            let biome =
                derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, x), Some(&hot_env));
            // On a 400–700 K planet, the absolute temp is always >= 400 K.
            // The cold biome requires abs <= 220 K, so it must never appear.
            assert_ne!(
                biome.biome_key, "cold_biome",
                "cold biome should not appear on a 400–700 K planet (chunk x={x})",
            );
        }
    }

    #[test]
    fn planet_env_cold_planet_filters_hot_biome() {
        // A very cold planet (min 30 K, max 100 K) maps all noise values
        // below the hot biome's absolute min of 350 K.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = abs_temp_registry();
        let cold_env = PlanetEnvironment {
            surface_temp_min_k: 30.0,
            surface_temp_max_k: 100.0,
            atmosphere_density: 0.1,
            radiation_level: 0.05,
            surface_gravity_g: 0.3,
            in_habitable_zone: false,
        };

        for x in 0..50 {
            let biome =
                derive_chunk_biome(&profile, &registry, ChunkCoord::new(x, x), Some(&cold_env));
            assert_ne!(
                biome.biome_key, "hot_biome",
                "hot biome should not appear on a 30–100 K planet (chunk x={x})",
            );
        }
    }

    #[test]
    fn planet_env_deterministic() {
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = abs_temp_registry();
        let env = PlanetEnvironment {
            surface_temp_min_k: 200.0,
            surface_temp_max_k: 500.0,
            atmosphere_density: 1.0,
            radiation_level: 0.3,
            surface_gravity_g: 1.0,
            in_habitable_zone: true,
        };
        let coord = ChunkCoord::new(7, 13);
        let a = derive_chunk_biome(&profile, &registry, coord, Some(&env));
        let b = derive_chunk_biome(&profile, &registry, coord, Some(&env));
        assert_eq!(
            a.biome_key, b.biome_key,
            "same inputs must produce same biome"
        );
    }

    #[test]
    fn hot_planet_biomes_differ_from_cold_planet_biomes() {
        // A hot planet and a cold planet should produce meaningfully different
        // biome distributions across the same set of chunk coordinates.
        let profile = WorldProfile::from_config(&sample_config()).unwrap();
        let registry = abs_temp_registry();

        let hot_env = PlanetEnvironment {
            surface_temp_min_k: 400.0,
            surface_temp_max_k: 700.0,
            atmosphere_density: 0.5,
            radiation_level: 0.8,
            surface_gravity_g: 1.2,
            in_habitable_zone: false,
        };
        let cold_env = PlanetEnvironment {
            surface_temp_min_k: 30.0,
            surface_temp_max_k: 100.0,
            atmosphere_density: 0.1,
            radiation_level: 0.05,
            surface_gravity_g: 0.3,
            in_habitable_zone: false,
        };

        let mut hot_biomes: Vec<String> = Vec::new();
        let mut cold_biomes: Vec<String> = Vec::new();

        for x in 0..100 {
            let coord = ChunkCoord::new(x, x * 3);
            hot_biomes
                .push(derive_chunk_biome(&profile, &registry, coord, Some(&hot_env)).biome_key);
            cold_biomes
                .push(derive_chunk_biome(&profile, &registry, coord, Some(&cold_env)).biome_key);
        }

        // The hot planet must never produce the cold biome (abs max 220 K)
        // and the cold planet must never produce the hot biome (abs min 350 K).
        assert!(
            !hot_biomes.contains(&"cold_biome".to_string()),
            "hot planet should not contain cold_biome",
        );
        assert!(
            !cold_biomes.contains(&"hot_biome".to_string()),
            "cold planet should not contain hot_biome",
        );

        // The distributions must actually differ — at least one coordinate
        // must resolve to a different biome key between the two planets.
        let differ_count = hot_biomes
            .iter()
            .zip(cold_biomes.iter())
            .filter(|(h, c)| h != c)
            .count();
        assert!(
            differ_count > 0,
            "hot and cold planets should produce different biome distributions, \
             but all {len} sampled chunks matched",
            len = hot_biomes.len(),
        );
    }

    #[test]
    fn hot_planet_skews_toward_scorched_flats_over_frost_shelf() {
        // Using the production biome registry, a planet close to the star
        // (surface_temp 400–700 K) should produce predominantly scorched_flats
        // and zero frost_shelf. The absolute temperature thresholds in
        // biomes.toml (scorched_flats: 350–600 K, frost_shelf: 50–220 K)
        // make it impossible for a 400+ K planet to match frost_shelf.
        let toml_content =
            std::fs::read_to_string(BIOME_CONFIG_PATH).expect("shipped biomes.toml must exist");
        let registry: BiomeRegistry =
            toml::from_str(&toml_content).expect("shipped biomes.toml must parse");
        let profile = WorldProfile::from_config(&sample_config()).unwrap();

        let hot_env = PlanetEnvironment {
            surface_temp_min_k: 400.0,
            surface_temp_max_k: 700.0,
            atmosphere_density: 0.3,
            radiation_level: 0.9,
            surface_gravity_g: 1.5,
            in_habitable_zone: false,
        };

        let cold_env = PlanetEnvironment {
            surface_temp_min_k: 30.0,
            surface_temp_max_k: 100.0,
            atmosphere_density: 0.1,
            radiation_level: 0.05,
            surface_gravity_g: 0.3,
            in_habitable_zone: false,
        };

        let mut hot_scorched = 0u32;
        let mut hot_frost = 0u32;
        let mut cold_scorched = 0u32;
        let mut cold_frost = 0u32;
        let sample_count = 200;

        for x in 0..sample_count {
            let coord = ChunkCoord::new(x, x * 7 + 3);

            let hot_biome =
                derive_chunk_biome(&profile, &registry, coord, Some(&hot_env)).biome_key;
            if hot_biome == "scorched_flats" {
                hot_scorched += 1;
            } else if hot_biome == "frost_shelf" {
                hot_frost += 1;
            }

            let cold_biome =
                derive_chunk_biome(&profile, &registry, coord, Some(&cold_env)).biome_key;
            if cold_biome == "scorched_flats" {
                cold_scorched += 1;
            } else if cold_biome == "frost_shelf" {
                cold_frost += 1;
            }
        }

        // Hot planet: frost_shelf is physically impossible (abs max 220 K < planet min 400 K).
        assert_eq!(
            hot_frost, 0,
            "frost_shelf must not appear on a 400–700 K planet",
        );
        // Hot planet must produce at least some scorched_flats.
        assert!(
            hot_scorched > 0,
            "hot planet (400–700 K) should produce scorched_flats, got none in {sample_count} samples",
        );

        // Cold planet: scorched_flats is physically impossible (abs min 350 K > planet max 100 K).
        assert_eq!(
            cold_scorched, 0,
            "scorched_flats must not appear on a 30–100 K planet",
        );
        // Cold planet must produce at least some frost_shelf.
        assert!(
            cold_frost > 0,
            "cold planet (30–100 K) should produce frost_shelf, got none in {sample_count} samples",
        );

        // The hot planet must have more scorched_flats than the cold planet (which has zero).
        assert!(
            hot_scorched > cold_scorched,
            "hot planet should have more scorched_flats ({hot_scorched}) than cold planet ({cold_scorched})",
        );
        // The cold planet must have more frost_shelf than the hot planet (which has zero).
        assert!(
            cold_frost > hot_frost,
            "cold planet should have more frost_shelf ({cold_frost}) than hot planet ({hot_frost})",
        );
    }

    // ── PlanetSurface multi-octave noise tests ──────────────────────────

    /// Helper: build a `PlanetSurface` with known parameters for testing.
    fn test_planet_surface() -> PlanetSurface {
        PlanetSurface {
            elevation_seed: 0xDEAD_BEEF,
            base_y: 0.0,
            amplitude: 10.0,
            frequency: 0.005,
            octaves: 4,
            detail_weight: 0.0,
            detail_seed: mix_seed(0xDEAD_BEEF, ELEVATION_DETAIL_CHANNEL),
            detail_frequency: 0.02,
            detail_octaves: 2,
            planet_surface_diameter: 100,
            chunk_size_world_units: 45.0,
        }
    }

    #[test]
    fn planet_surface_elevation_is_deterministic() {
        let surface = test_planet_surface();
        let a = surface.sample_elevation(123.4, 567.8);
        let b = surface.sample_elevation(123.4, 567.8);
        assert_eq!(a, b, "same inputs must produce identical elevation");
    }

    #[test]
    fn planet_surface_different_seeds_produce_different_elevation() {
        let s1 = test_planet_surface();
        let mut s2 = test_planet_surface();
        s2.elevation_seed = 0xCAFE_BABE;

        let e1 = s1.sample_elevation(50.0, 50.0);
        let e2 = s2.sample_elevation(50.0, 50.0);
        assert_ne!(e1, e2, "different seeds should produce different terrain");
    }

    #[test]
    fn planet_surface_elevation_within_amplitude() {
        let surface = test_planet_surface();
        // Sample a grid of points and verify all elevations stay within bounds.
        for ix in 0..50 {
            for iz in 0..50 {
                let x = ix as f32 * 17.3;
                let z = iz as f32 * 13.7;
                let h = surface.sample_elevation(x, z);
                assert!(
                    h >= surface.base_y - surface.amplitude
                        && h <= surface.base_y + surface.amplitude,
                    "elevation {h} out of range [{}, {}] at ({x}, {z})",
                    surface.base_y - surface.amplitude,
                    surface.base_y + surface.amplitude,
                );
            }
        }
    }

    #[test]
    fn planet_surface_torus_wrapping_continuous() {
        let surface = test_planet_surface();
        let period = surface.planet_surface_diameter as f32 * surface.chunk_size_world_units;

        // Elevation at (x, z) must equal elevation at (x + period, z).
        for i in 0..20 {
            let x = i as f32 * 37.1;
            let z = i as f32 * 23.9;
            let result_a = surface.query_surface(x, z);
            let result_b = surface.query_surface(x + period, z);
            assert!(
                (result_a.position_y - result_b.position_y).abs() < 1e-6,
                "torus wrap mismatch at x={x}: {} vs {}",
                result_a.position_y,
                result_b.position_y,
            );
            // Also verify z-direction wrapping.
            let result_c = surface.query_surface(x, z + period);
            assert!(
                (result_a.position_y - result_c.position_y).abs() < 1e-6,
                "torus wrap mismatch at z={z}: {} vs {}",
                result_a.position_y,
                result_c.position_y,
            );
        }
    }

    #[test]
    fn planet_surface_flat_region_normal_points_up() {
        // With zero amplitude the surface is perfectly flat, so the normal
        // should be straight up.
        let surface = PlanetSurface {
            amplitude: 0.0,
            ..test_planet_surface()
        };
        let result = surface.query_surface(100.0, 200.0);
        let [nx, ny, nz] = result.normal;
        assert!(
            (nx.abs() < 1e-6) && ((ny - 1.0).abs() < 1e-6) && (nz.abs() < 1e-6),
            "flat surface normal should be (0,1,0), got ({nx}, {ny}, {nz})"
        );
    }

    #[test]
    fn planet_surface_steep_region_normal_deviates_from_up() {
        // With high amplitude and high frequency, some normals must deviate
        // noticeably from straight up.
        let surface = PlanetSurface {
            amplitude: 50.0,
            frequency: 0.1,
            octaves: 1,
            ..test_planet_surface()
        };
        let mut found_steep = false;
        for ix in 0..100 {
            let x = ix as f32 * 3.7;
            let result = surface.query_surface(x, 42.0);
            if result.normal[1] < 0.99 {
                found_steep = true;
                break;
            }
        }
        assert!(
            found_steep,
            "high-amplitude terrain should have non-vertical normals"
        );
    }

    #[test]
    fn planet_surface_query_surface_always_valid() {
        let surface = test_planet_surface();
        for i in 0..50 {
            let x = (i as f32 - 25.0) * 100.0;
            let z = (i as f32 - 10.0) * 77.0;
            assert!(
                surface.query_surface(x, z).valid,
                "PlanetSurface should always return valid=true"
            );
        }
    }

    #[test]
    fn planet_surface_multiple_octaves_differ_from_single() {
        let single = PlanetSurface {
            octaves: 1,
            ..test_planet_surface()
        };
        let multi = PlanetSurface {
            octaves: 4,
            ..test_planet_surface()
        };
        // At least some samples should differ when adding more octaves.
        let mut any_different = false;
        for i in 0..50 {
            let x = i as f32 * 11.1;
            let e1 = single.sample_elevation(x, 0.0);
            let e4 = multi.sample_elevation(x, 0.0);
            if (e1 - e4).abs() > 1e-6 {
                any_different = true;
                break;
            }
        }
        assert!(
            any_different,
            "multi-octave noise should differ from single octave"
        );
    }

    #[test]
    fn planet_surface_zero_amplitude_produces_constant_base_y() {
        let base_y = 42.0;
        let surface = PlanetSurface {
            amplitude: 0.0,
            base_y,
            ..test_planet_surface()
        };
        // Sample a grid of points — every elevation must equal base_y exactly,
        // and every normal must point straight up, just like FlatSurface.
        let flat = FlatSurface {
            surface_y: base_y,
            min_x: -1000.0,
            max_x: 1000.0,
            min_z: -1000.0,
            max_z: 1000.0,
        };
        for ix in 0..20 {
            for iz in 0..20 {
                let x = ix as f32 * 23.7 - 100.0;
                let z = iz as f32 * 19.3 - 100.0;

                let planet_result = surface.query_surface(x, z);
                let flat_result = flat.query_surface(x, z);

                assert_eq!(
                    planet_result.position_y, base_y,
                    "zero-amplitude PlanetSurface must return base_y at ({x}, {z})"
                );
                assert_eq!(
                    planet_result.position_y, flat_result.position_y,
                    "zero-amplitude PlanetSurface must match FlatSurface elevation at ({x}, {z})"
                );
                assert!(
                    planet_result.valid,
                    "zero-amplitude surface should always be valid"
                );
                // Normal should point straight up (0, 1, 0).
                let n = planet_result.normal;
                assert!(
                    (n[0].abs() < 1e-6) && ((n[1] - 1.0).abs() < 1e-6) && (n[2].abs() < 1e-6),
                    "zero-amplitude normal should be (0,1,0), got ({}, {}, {}) at ({x}, {z})",
                    n[0],
                    n[1],
                    n[2]
                );
            }
        }
    }

    /// Helper that returns a `PlanetSurface` with detail noise **enabled**.
    fn test_planet_surface_with_detail() -> PlanetSurface {
        PlanetSurface {
            detail_weight: 0.3,
            ..test_planet_surface()
        }
    }

    #[test]
    fn detail_noise_elevation_is_deterministic() {
        let surface = test_planet_surface_with_detail();
        for i in 0..50 {
            let x = i as f32 * 17.3 + 3.1;
            let z = i as f32 * 11.7 + 7.9;
            let a = surface.sample_elevation(x, z);
            let b = surface.sample_elevation(x, z);
            assert_eq!(a, b, "detail noise must be deterministic at ({x}, {z})");
        }
    }

    #[test]
    fn detail_noise_torus_wrapping_continuous() {
        let surface = test_planet_surface_with_detail();
        let period = surface.planet_surface_diameter as f32 * surface.chunk_size_world_units;

        for i in 0..20 {
            let x = i as f32 * 37.1 + 5.5;
            let z = i as f32 * 23.9 + 2.3;
            let a = surface.sample_elevation(x, z);
            let b = surface.sample_elevation(x + period, z);
            assert!(
                (a - b).abs() < 1e-6,
                "detail noise breaks torus continuity at x={x}: {a} vs {b}"
            );
            let c = surface.sample_elevation(x, z + period);
            assert!(
                (a - c).abs() < 1e-6,
                "detail noise breaks torus continuity at z={z}: {a} vs {c}"
            );
        }
    }

    #[test]
    fn detail_noise_elevation_within_bounds() {
        let surface = test_planet_surface_with_detail();
        // With detail, max deviation is amplitude * (1 + detail_weight) / 2
        // since both base and detail are normalized to [-0.5, 0.5] before scaling.
        let max_deviation = surface.amplitude * (1.0 + surface.detail_weight);
        let lo = surface.base_y - max_deviation;
        let hi = surface.base_y + max_deviation;
        for ix in 0..50 {
            for iz in 0..50 {
                let x = ix as f32 * 17.3;
                let z = iz as f32 * 13.7;
                let h = surface.sample_elevation(x, z);
                assert!(
                    h >= lo && h <= hi,
                    "elevation {h} out of range [{lo}, {hi}] at ({x}, {z})"
                );
            }
        }
    }

    #[test]
    fn detail_noise_actually_changes_elevation() {
        let without = test_planet_surface();
        let with = test_planet_surface_with_detail();
        let mut any_different = false;
        for i in 0..100 {
            let x = i as f32 * 11.1;
            let e_no = without.sample_elevation(x, 42.0);
            let e_yes = with.sample_elevation(x, 42.0);
            if (e_no - e_yes).abs() > 1e-6 {
                any_different = true;
                break;
            }
        }
        assert!(
            any_different,
            "enabling detail noise should change at least some elevations"
        );
    }

    #[test]
    fn detail_weight_zero_produces_same_result_as_no_detail() {
        // A surface with detail_weight = 0 should produce identical elevations
        // and normals as one that simply has no detail layer, regardless of the
        // detail_seed, detail_frequency, or detail_octaves values.
        let baseline = test_planet_surface(); // detail_weight already 0.0

        // Build a variant with non-zero detail parameters but weight still 0.
        let zero_weight = PlanetSurface {
            detail_weight: 0.0,
            detail_seed: 0xCAFE_BABE,
            detail_frequency: 0.05,
            detail_octaves: 6,
            ..test_planet_surface()
        };

        for i in 0..200 {
            let x = i as f32 * 7.7 - 300.0;
            let z = i as f32 * 13.3 + 50.0;

            let elev_base = baseline.sample_elevation(x, z);
            let elev_zero = zero_weight.sample_elevation(x, z);
            assert_eq!(
                elev_base, elev_zero,
                "detail_weight=0 must match baseline at ({x}, {z}): {elev_base} vs {elev_zero}"
            );

            let norm_base = baseline.compute_normal(x, z);
            let norm_zero = zero_weight.compute_normal(x, z);
            assert_eq!(
                norm_base, norm_zero,
                "normals must match when detail_weight=0 at ({x}, {z})"
            );
        }
    }

    #[test]
    fn heightmap_mesh_vertex_count_matches_expected() {
        let surface = test_planet_surface();
        let chunk = ChunkCoord::new(0, 0);

        for subdivisions in [1, 2, 4, 8, 16] {
            let mesh = generate_chunk_heightmap_mesh(&surface, chunk, subdivisions);
            let expected = ((subdivisions + 1) * (subdivisions + 1)) as usize;
            let actual = mesh.count_vertices();
            assert_eq!(
                actual, expected,
                "subdivisions={subdivisions}: expected {expected} vertices, got {actual}"
            );
        }
    }

    #[test]
    fn flat_terrain_mesh_normals_all_point_up() {
        let surface = PlanetSurface {
            amplitude: 0.0,
            ..test_planet_surface()
        };

        // Test across several chunk coordinates and subdivision levels.
        let chunks = [
            ChunkCoord::new(0, 0),
            ChunkCoord::new(3, -2),
            ChunkCoord::new(-5, 7),
        ];
        for chunk in chunks {
            for subdivisions in [2, 4, 8] {
                let mesh = generate_chunk_heightmap_mesh(&surface, chunk, subdivisions);
                let normals = mesh
                    .attribute(Mesh::ATTRIBUTE_NORMAL)
                    .expect("mesh must have normals")
                    .as_float3()
                    .expect("normals must be Float32x3");

                for (i, n) in normals.iter().enumerate() {
                    assert!(
                        n[0].abs() < 1e-5 && (n[1] - 1.0).abs() < 1e-5 && n[2].abs() < 1e-5,
                        "vertex {i} in chunk {:?} (subdivisions={subdivisions}): \
                         expected normal ≈ (0,1,0), got ({}, {}, {})",
                        chunk,
                        n[0],
                        n[1],
                        n[2]
                    );
                }
            }
        }
    }

    #[test]
    fn adjacent_chunk_edges_have_identical_heights() {
        let surface = test_planet_surface();
        let subdivisions = 8u32;
        let verts_per_edge = (subdivisions + 1) as usize;

        // Test several adjacent chunk pairs along both axes.
        let pairs = [
            // (chunk_a, chunk_b, axis): axis=0 means b is +X neighbor, axis=1 means b is +Z neighbor
            (ChunkCoord::new(0, 0), ChunkCoord::new(1, 0), 0),
            (ChunkCoord::new(0, 0), ChunkCoord::new(0, 1), 1),
            (ChunkCoord::new(-3, 2), ChunkCoord::new(-2, 2), 0),
            (ChunkCoord::new(5, -1), ChunkCoord::new(5, 0), 1),
            (ChunkCoord::new(-1, -1), ChunkCoord::new(0, -1), 0),
        ];

        for (chunk_a, chunk_b, axis) in pairs {
            let mesh_a = generate_chunk_heightmap_mesh(&surface, chunk_a, subdivisions);
            let mesh_b = generate_chunk_heightmap_mesh(&surface, chunk_b, subdivisions);

            let positions_a = mesh_a
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("mesh must have positions")
                .as_float3()
                .expect("positions must be Float32x3");
            let positions_b = mesh_b
                .attribute(Mesh::ATTRIBUTE_POSITION)
                .expect("mesh must have positions")
                .as_float3()
                .expect("positions must be Float32x3");

            for i in 0..verts_per_edge {
                // For axis=0 (+X neighbor): right edge of A (ix=subdivisions) matches left edge of B (ix=0).
                // For axis=1 (+Z neighbor): bottom edge of A (iz=subdivisions) matches top edge of B (iz=0).
                let idx_a = if axis == 0 {
                    i * verts_per_edge + (verts_per_edge - 1) // right column of A
                } else {
                    (verts_per_edge - 1) * verts_per_edge + i // bottom row of A
                };
                let idx_b = if axis == 0 {
                    i * verts_per_edge // left column of B
                } else {
                    i // top row of B
                };

                let ha = positions_a[idx_a][1];
                let hb = positions_b[idx_b][1];
                assert_eq!(
                    ha, hb,
                    "Seam artifact at shared edge vertex {i}: chunk {:?} edge height {ha} != \
                     chunk {:?} edge height {hb} (axis={axis})",
                    chunk_a, chunk_b
                );
            }
        }
    }

    // ── WorldGenerationConfig::validate tests ──────────────────────────

    #[test]
    fn validate_default_config_passes() {
        WorldGenerationConfig::default()
            .validate()
            .expect("default config must pass validation");
    }

    #[test]
    fn validate_override_mode_without_planet_index_passes() {
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            planet_index: 0,
            ..Default::default()
        };
        config
            .validate()
            .expect("override mode with planet_index=0 must pass");
    }

    #[test]
    fn validate_system_derived_mode_passes() {
        let config = WorldGenerationConfig {
            planet_seed: None,
            planet_index: 3,
            ..Default::default()
        };
        config.validate().expect("system-derived mode must pass");
    }

    #[test]
    fn validate_rejects_both_planet_seed_and_planet_index() {
        let config = WorldGenerationConfig {
            planet_seed: Some(42),
            planet_index: 3,
            ..Default::default()
        };
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("planet_seed") && err.contains("planet_index"),
            "error must mention both fields, got: {err}",
        );
    }

    #[test]
    fn validate_rejects_zero_chunk_size() {
        let config = WorldGenerationConfig {
            chunk_size_world_units: 0.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_negative_active_chunk_radius() {
        let config = WorldGenerationConfig {
            active_chunk_radius: -1,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_inverted_planet_radius_bounds() {
        let config = WorldGenerationConfig {
            planet_surface_min_radius: 5000,
            planet_surface_max_radius: 500,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_nan_elevation_amplitude() {
        let config = WorldGenerationConfig {
            elevation_amplitude: f32::NAN,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_elevation_frequency() {
        let config = WorldGenerationConfig {
            elevation_frequency: 0.0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_detail_weight_above_one() {
        let config = WorldGenerationConfig {
            elevation_detail_weight: 1.5,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    #[test]
    fn validate_rejects_zero_subdivisions() {
        let config = WorldGenerationConfig {
            elevation_subdivisions: 0,
            ..Default::default()
        };
        assert!(config.validate().is_err());
    }

    // ── Config TOML parsing tests ─────────────────────────────────────

    #[test]
    fn config_with_solar_system_seed_parses_system_derived_mode() {
        let toml_str = r#"
solar_system_seed = 42
planet_index = 2
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("system-derived TOML must parse");
        assert_eq!(config.solar_system_seed, 42);
        assert_eq!(config.planet_index, 2);
        assert_eq!(
            config.planet_seed, None,
            "planet_seed must be None when omitted"
        );
        assert_eq!(config.seed_mode(), SeedMode::SystemDerived);
        config
            .validate()
            .expect("system-derived config must pass validation");
    }

    #[test]
    fn config_with_solar_system_seed_only_defaults_planet_index_to_zero() {
        let toml_str = r#"
solar_system_seed = 99
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("solar_system_seed-only TOML must parse");
        assert_eq!(config.solar_system_seed, 99);
        assert_eq!(config.planet_index, 0);
        assert_eq!(config.planet_seed, None);
        assert_eq!(config.seed_mode(), SeedMode::SystemDerived);
        config
            .validate()
            .expect("system-derived config with default planet_index must pass");
    }

    #[test]
    fn config_with_legacy_system_seed_alias_parses() {
        let toml_str = r#"
system_seed = 77
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("legacy system_seed alias must parse");
        assert_eq!(config.solar_system_seed, 77);
        assert_eq!(config.seed_mode(), SeedMode::SystemDerived);
    }

    #[test]
    fn config_with_planet_seed_parses_override_mode() {
        let toml_str = r#"
solar_system_seed = 42
planet_seed = 12345
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("override mode TOML must parse");
        assert_eq!(config.solar_system_seed, 42);
        assert_eq!(config.planet_seed, Some(12345));
        assert_eq!(config.seed_mode(), SeedMode::Override);
        config
            .validate()
            .expect("override mode config must pass validation");
    }

    /// A saved config that only has `planet_seed` (no `solar_system_seed`)
    /// must load without errors. This is the backward-compatibility guarantee
    /// for configs created before system-derived mode existed.
    #[test]
    fn config_with_only_planet_seed_loads_without_errors() {
        let toml_str = r#"
planet_seed = 12345
chunk_size_world_units = 45.0
active_chunk_radius = 1
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("planet_seed-only TOML must parse");

        // solar_system_seed falls back to its default — not an error.
        assert_eq!(config.solar_system_seed, default_solar_system_seed());
        assert_eq!(config.planet_seed, Some(12345));
        assert_eq!(config.seed_mode(), SeedMode::Override);
        config
            .validate()
            .expect("planet_seed-only config must pass validation");

        // WorldProfile can be built from this config.
        let profile = WorldProfile::from_config(&config).unwrap();
        assert_eq!(profile.planet_seed, PlanetSeed(12345));
        assert!(
            !profile.is_system_derived(),
            "planet_seed-only config must not be system-derived",
        );
    }

    /// A completely minimal config with only `planet_seed` and no other fields
    /// must also load — every field has a serde default.
    #[test]
    fn config_with_bare_planet_seed_loads_without_errors() {
        let toml_str = "planet_seed = 99999\n";
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("bare planet_seed TOML must parse");

        assert_eq!(config.planet_seed, Some(99999));
        assert_eq!(config.seed_mode(), SeedMode::Override);
        config
            .validate()
            .expect("bare planet_seed config must pass validation");

        let profile = WorldProfile::from_config(&config).unwrap();
        assert_eq!(profile.planet_seed, PlanetSeed(99999));
    }

    #[test]
    fn config_solar_system_seed_preserves_all_other_defaults() {
        let toml_str = r#"
solar_system_seed = 42
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("minimal system-derived TOML must parse");
        let defaults = WorldGenerationConfig::default();
        assert_eq!(
            config.chunk_size_world_units,
            defaults.chunk_size_world_units
        );
        assert_eq!(config.active_chunk_radius, defaults.active_chunk_radius);
        assert_eq!(config.building_cell_size, defaults.building_cell_size);
        assert_eq!(
            config.planet_surface_min_radius,
            defaults.planet_surface_min_radius
        );
        assert_eq!(
            config.planet_surface_max_radius,
            defaults.planet_surface_max_radius
        );
        assert_eq!(config.elevation_amplitude, defaults.elevation_amplitude);
        assert_eq!(config.elevation_frequency, defaults.elevation_frequency);
        assert_eq!(config.elevation_octaves, defaults.elevation_octaves);
    }

    /// When both `solar_system_seed` and `planet_seed` appear in config,
    /// `planet_seed` takes precedence (override mode). The `solar_system_seed`
    /// is still preserved — it drives star derivation — but the orbital
    /// derivation chain is skipped entirely. This is documented precedence,
    /// not silent swallowing: `seed_mode()` returns `Override`, validation
    /// passes, and both seed values are accessible for their respective roles.
    #[test]
    fn config_with_both_seeds_uses_planet_seed_precedence() {
        let toml_str = r#"
solar_system_seed = 100
planet_seed = 999
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("TOML with both seeds must parse");

        // planet_seed is present → override mode, not system-derived.
        assert_eq!(config.seed_mode(), SeedMode::Override);
        assert_eq!(
            config.planet_seed,
            Some(999),
            "planet_seed must be preserved as-is",
        );
        // solar_system_seed is still available for star derivation.
        assert_eq!(
            config.solar_system_seed, 100,
            "solar_system_seed must be preserved even in override mode",
        );
        // planet_index defaults to 0, which is fine — it is ignored in override mode.
        assert_eq!(config.planet_index, 0);

        // Validation passes: having both seeds (without planet_index) is the
        // expected override-mode configuration.
        config
            .validate()
            .expect("both seeds without planet_index must pass validation");
    }

    /// Specifying all three — `solar_system_seed`, `planet_seed`, and
    /// `planet_index` — is rejected as ambiguous. The user likely intended
    /// system-derived mode but forgot to remove `planet_seed`.
    #[test]
    fn config_with_both_seeds_and_planet_index_is_rejected() {
        let toml_str = r#"
solar_system_seed = 100
planet_seed = 999
planet_index = 3
"#;
        let config: WorldGenerationConfig =
            toml::from_str(toml_str).expect("TOML with all three must parse");
        let err = config.validate().unwrap_err();
        assert!(
            err.contains("planet_seed") && err.contains("planet_index"),
            "error must mention the conflicting fields, got: {err}",
        );
    }

    #[test]
    fn system_mode_is_system_derived_and_all_fields_populated() {
        let star_registry = StarTypeRegistry::default();
        let orbital_config = OrbitalConfig::default();
        let env_config = PlanetEnvironmentConfig::default();

        let config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };
        assert_eq!(config.seed_mode(), SeedMode::SystemDerived);

        let profile =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("from_system_seed must succeed for planet_index 0");

        assert!(
            profile.is_system_derived(),
            "system-derived mode must report is_system_derived() == true"
        );

        let ctx = profile
            .system_context
            .as_ref()
            .expect("system_context must be Some in system-derived mode");

        assert_eq!(
            ctx.system_seed,
            SolarSystemSeed(42),
            "system_context must carry the original system seed"
        );
        assert_eq!(
            ctx.planet_orbital_index, 0,
            "planet_orbital_index must match the configured planet_index"
        );
        assert!(
            !ctx.orbital_layout.planets.is_empty(),
            "orbital layout must contain at least one planet"
        );

        // Verify all WorldProfile sub-seeds are populated (non-zero is not
        // guaranteed by the mixing function, but for seed 42 they are
        // empirically distinct and non-zero — if any were zero it would
        // indicate the derivation chain is broken).
        assert_ne!(profile.placement_density_seed, 0);
        assert_ne!(profile.placement_variation_seed, 0);
        assert_ne!(profile.object_identity_seed, 0);
        assert_ne!(profile.biome_climate_seed, 0);
        assert_ne!(profile.elevation_seed, 0);
        assert!(profile.planet_surface_radius > 0);
        assert!(profile.planet_surface_diameter > 0);
        assert_eq!(
            profile.planet_surface_diameter,
            profile.planet_surface_radius * 2
        );
    }

    #[test]
    fn validate_shipped_toml_passes() {
        let contents =
            std::fs::read_to_string(CONFIG_PATH).expect("shipped world_generation.toml must exist");
        let config: WorldGenerationConfig =
            toml::from_str(&contents).expect("shipped TOML must parse");
        config
            .validate()
            .expect("shipped world_generation.toml must pass validation");
    }

    /// Full chain determinism: system seed 42 → specific star type → specific
    /// planet count → specific planet seed → specific biome at chunk (0, 0).
    ///
    /// Running the derivation twice with identical inputs must produce
    /// byte-identical results at every stage. This exercises the entire
    /// pipeline from `SolarSystemSeed` through `derive_star_profile`,
    /// `derive_orbital_layout`, `derive_planet_environment`,
    /// `WorldProfile::from_system_seed`, and `derive_chunk_biome`.
    #[test]
    fn full_chain_determinism_system_seed_to_biome_at_origin() {
        use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

        let star_registry = StarTypeRegistry::default();
        let orbital_config = OrbitalConfig::default();
        let env_config = PlanetEnvironmentConfig::default();

        let config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };

        let biome_registry = BiomeRegistry::default();
        let origin = ChunkCoord { x: 0, z: 0 };

        // Run the full derivation chain twice.
        let profile_a =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("first derivation must succeed");
        let profile_b =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("second derivation must succeed");

        // WorldProfile must be identical across runs.
        assert_eq!(profile_a, profile_b, "WorldProfile must be deterministic");

        // System context must be present and identical.
        let ctx_a = profile_a
            .system_context
            .as_ref()
            .expect("system_context must be Some");
        let ctx_b = profile_b
            .system_context
            .as_ref()
            .expect("system_context must be Some");
        assert_eq!(ctx_a, ctx_b, "SystemContext must be deterministic");

        // Verify intermediate derivation steps are concrete (not degenerate).
        // star_type is an enum — its mere presence confirms derivation ran.
        let _ = ctx_a.star.star_type; // would fail to compile if field were removed
        assert!(
            !ctx_a.orbital_layout.planets.is_empty(),
            "orbital layout must contain at least one planet"
        );
        assert!(
            ctx_a.planet_environment.surface_temp_min_k > 0.0,
            "planet environment must have a positive minimum temperature"
        );
        assert!(
            ctx_a.planet_environment.surface_temp_max_k
                > ctx_a.planet_environment.surface_temp_min_k,
            "max temperature must exceed min temperature"
        );

        // Derive biome at origin using the planet environment from the system
        // context. Both runs must produce the same biome key.
        let biome_a = derive_chunk_biome(
            &profile_a,
            &biome_registry,
            origin,
            Some(&ctx_a.planet_environment),
        );
        let biome_b = derive_chunk_biome(
            &profile_b,
            &biome_registry,
            origin,
            Some(&ctx_b.planet_environment),
        );

        assert_eq!(
            biome_a.biome_key, biome_b.biome_key,
            "biome key at origin must be deterministic"
        );
        assert_eq!(
            biome_a.ground_color, biome_b.ground_color,
            "biome ground color at origin must be deterministic"
        );
        assert_eq!(
            biome_a.density_modifier, biome_b.density_modifier,
            "biome density modifier at origin must be deterministic"
        );

        // Verify the biome key is a non-empty string — a truly exercised
        // pipeline must resolve to a concrete biome, not silently fall through
        // to an empty default.
        assert!(
            !biome_a.biome_key.is_empty(),
            "biome at origin must resolve to a named biome"
        );

        // Verify the chunk generation key is also deterministic through
        // the full chain.
        let key_a = derive_chunk_generation_key(&profile_a, origin);
        let key_b = derive_chunk_generation_key(&profile_b, origin);
        assert_eq!(
            key_a, key_b,
            "chunk generation key at origin must be deterministic"
        );
    }

    /// Planet index out of range must return a clear `Err`, not panic.
    #[test]
    fn planet_index_out_of_range_returns_error() {
        use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

        let star_registry = StarTypeRegistry::default();
        let orbital_config = OrbitalConfig::default();
        let env_config = PlanetEnvironmentConfig::default();

        // First, determine how many planets seed 42 actually produces so we
        // can request an index that is guaranteed to be out of range.
        let baseline_config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };
        let baseline = WorldProfile::from_system_seed(
            &baseline_config,
            &star_registry,
            &orbital_config,
            &env_config,
        )
        .expect("baseline derivation must succeed");
        let planet_count = baseline
            .system_context
            .as_ref()
            .expect("system_context must be Some")
            .orbital_layout
            .planets
            .len() as u32;

        // Request an index equal to planet_count (one past the last valid index).
        let bad_config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: planet_count,
            ..Default::default()
        };

        let result = WorldProfile::from_system_seed(
            &bad_config,
            &star_registry,
            &orbital_config,
            &env_config,
        );
        let err =
            result.expect_err("planet_index equal to planet count must return Err, not panic");
        assert!(
            err.contains("out of range"),
            "error message must mention 'out of range', got: {err}"
        );
        assert!(
            err.contains(&planet_count.to_string()),
            "error message must mention the invalid index, got: {err}"
        );

        // Also verify a very large index fails gracefully.
        let huge_config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: u32::MAX,
            ..Default::default()
        };
        let huge_result = WorldProfile::from_system_seed(
            &huge_config,
            &star_registry,
            &orbital_config,
            &env_config,
        );
        assert!(
            huge_result.is_err(),
            "u32::MAX planet_index must return Err, not panic"
        );
    }

    /// Selecting the last planet (index = planet_count - 1) must succeed and
    /// produce a valid, fully-populated `WorldProfile` with the correct
    /// orbital index recorded in `SystemContext`.
    #[test]
    fn planet_index_last_planet_succeeds() {
        use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

        let star_registry = StarTypeRegistry::default();
        let orbital_config = OrbitalConfig::default();
        let env_config = PlanetEnvironmentConfig::default();

        // First, derive with index 0 to discover how many planets exist.
        let baseline_config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };
        let baseline = WorldProfile::from_system_seed(
            &baseline_config,
            &star_registry,
            &orbital_config,
            &env_config,
        )
        .expect("baseline derivation must succeed");
        let planet_count = baseline
            .system_context
            .as_ref()
            .expect("system_context must be Some")
            .orbital_layout
            .planets
            .len() as u32;

        assert!(
            planet_count >= 2,
            "need at least 2 planets to meaningfully test last-index selection, got {planet_count}"
        );

        let last_index = planet_count - 1;

        // Derive using the last valid planet index.
        let last_config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: last_index,
            ..Default::default()
        };
        let profile = WorldProfile::from_system_seed(
            &last_config,
            &star_registry,
            &orbital_config,
            &env_config,
        )
        .expect("from_system_seed must succeed for the last valid planet index");

        // Verify system-derived mode.
        assert!(
            profile.is_system_derived(),
            "last-planet profile must be system-derived"
        );

        let ctx = profile
            .system_context
            .as_ref()
            .expect("system_context must be Some");

        // Orbital index matches what we requested.
        assert_eq!(
            ctx.planet_orbital_index, last_index,
            "planet_orbital_index must equal the last valid index ({last_index})"
        );

        // The orbital layout is identical regardless of which planet we select.
        assert_eq!(
            ctx.orbital_layout.planets.len() as u32,
            planet_count,
            "orbital layout planet count must be consistent across planet index selections"
        );

        // The planet seed must differ from index-0 (different orbital slot).
        assert_ne!(
            profile.planet_seed, baseline.planet_seed,
            "last planet must have a different seed than planet 0"
        );

        // Sub-seeds are populated (derivation chain is intact).
        assert_ne!(profile.placement_density_seed, 0);
        assert_ne!(profile.placement_variation_seed, 0);
        assert_ne!(profile.object_identity_seed, 0);
        assert_ne!(profile.biome_climate_seed, 0);
        assert_ne!(profile.elevation_seed, 0);
        assert!(profile.planet_surface_radius > 0);
        assert_eq!(
            profile.planet_surface_diameter,
            profile.planet_surface_radius * 2
        );
    }

    /// When OrbitalConfig is constrained to produce exactly 2 planets,
    /// planet_index 1 (the second and last planet) must yield a valid,
    /// fully-populated WorldProfile in system-derived mode.
    #[test]
    fn planet_index_one_valid_in_two_planet_system() {
        use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

        let star_registry = StarTypeRegistry::default();
        let env_config = PlanetEnvironmentConfig::default();

        // Force exactly 2 planets by setting min == max == 2.
        let orbital_config = OrbitalConfig {
            planet_count_min: 2,
            planet_count_max: 2,
            ..Default::default()
        };

        let config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 1,
            ..Default::default()
        };

        let profile =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("planet_index 1 must succeed when the system has exactly 2 planets");

        // Must be system-derived.
        assert!(
            profile.is_system_derived(),
            "profile must be in system-derived mode"
        );

        let ctx = profile
            .system_context
            .as_ref()
            .expect("system_context must be Some");

        // Orbital layout must contain exactly 2 planets.
        assert_eq!(
            ctx.orbital_layout.planets.len(),
            2,
            "orbital layout must have exactly 2 planets"
        );

        // Recorded orbital index matches the requested index.
        assert_eq!(
            ctx.planet_orbital_index, 1,
            "planet_orbital_index must be 1"
        );

        // Planet seed must differ from index 0 (different orbital slot).
        let index_zero_config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };
        let index_zero_profile = WorldProfile::from_system_seed(
            &index_zero_config,
            &star_registry,
            &orbital_config,
            &env_config,
        )
        .expect("planet_index 0 must also succeed");

        assert_ne!(
            profile.planet_seed, index_zero_profile.planet_seed,
            "planet at index 1 must have a different seed than planet at index 0"
        );

        // Sub-seeds are populated (derivation chain is intact).
        assert_ne!(profile.placement_density_seed, 0);
        assert_ne!(profile.placement_variation_seed, 0);
        assert_ne!(profile.object_identity_seed, 0);
        assert_ne!(profile.biome_climate_seed, 0);
        assert_ne!(profile.elevation_seed, 0);
        assert!(profile.planet_surface_radius > 0);
        assert_eq!(
            profile.planet_surface_diameter,
            profile.planet_surface_radius * 2
        );
    }

    /// System-derived world generates biomes influenced by planet temperature.
    ///
    /// Exercises the full chain: system seed → star → orbital layout → planet
    /// environment → biome selection, and verifies that the derived planet
    /// temperature actually gates biome assignment. The same profile is used
    /// with vs without its planet environment; absolute-temperature-aware
    /// biome definitions must produce different distributions when the planet
    /// environment is present.
    #[test]
    fn system_derived_world_generates_biomes_influenced_by_planet_temperature() {
        use crate::solar_system::{OrbitalConfig, PlanetEnvironmentConfig, StarTypeRegistry};

        let star_registry = StarTypeRegistry::default();
        let orbital_config = OrbitalConfig::default();
        let env_config = PlanetEnvironmentConfig::default();

        let config = WorldGenerationConfig {
            solar_system_seed: 42,
            planet_seed: None,
            planet_index: 0,
            ..Default::default()
        };

        let profile =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("system seed derivation must succeed");

        let ctx = profile
            .system_context
            .as_ref()
            .expect("system_context must be Some in system-derived mode");

        // Sanity: the derived planet environment has a meaningful temperature
        // range (not degenerate zero-width).
        let env = &ctx.planet_environment;
        assert!(
            env.surface_temp_max_k > env.surface_temp_min_k,
            "planet must have a non-degenerate temperature range: {}-{} K",
            env.surface_temp_min_k,
            env.surface_temp_max_k,
        );

        // Use the absolute-temperature-aware biome registry so the planet's
        // temperature band can actually influence which biomes are selected.
        let registry = abs_temp_registry();

        // Collect biome keys across a grid of chunks using the system-derived
        // planet environment (the full-chain path).
        let mut biomes_with_env: Vec<String> = Vec::new();
        for x in 0..100_i32 {
            let coord = ChunkCoord::new(x, x.wrapping_mul(7));
            let biome = derive_chunk_biome(&profile, &registry, coord, Some(env));
            biomes_with_env.push(biome.biome_key);
        }

        // Collect biome keys for the same chunks without a planet environment
        // (override / no-context mode). Without absolute Kelvin filtering,
        // all biomes that match normalized ranges can appear.
        let mut biomes_without_env: Vec<String> = Vec::new();
        for x in 0..100_i32 {
            let coord = ChunkCoord::new(x, x.wrapping_mul(7));
            let biome = derive_chunk_biome(&profile, &registry, coord, None);
            biomes_without_env.push(biome.biome_key);
        }

        // The planet temperature must actually influence biome selection:
        // at least one chunk must resolve to a different biome when the
        // planet environment is applied vs when it is absent.
        let differing_count = biomes_with_env
            .iter()
            .zip(biomes_without_env.iter())
            .filter(|(a, b)| a != b)
            .count();

        assert!(
            differing_count > 0,
            "planet temperature from the full derivation chain must influence biome selection; \
             all {} chunks produced identical biomes with and without planet environment \
             (temp range {:.0}-{:.0} K)",
            biomes_with_env.len(),
            env.surface_temp_min_k,
            env.surface_temp_max_k,
        );

        // Verify determinism: running the same derivation again must produce
        // identical results.
        let profile_again =
            WorldProfile::from_system_seed(&config, &star_registry, &orbital_config, &env_config)
                .expect("repeated derivation must succeed");
        let ctx_again = profile_again
            .system_context
            .as_ref()
            .expect("system_context must be Some");

        for x in 0..100_i32 {
            let coord = ChunkCoord::new(x, x.wrapping_mul(7));
            let biome_a =
                derive_chunk_biome(&profile, &registry, coord, Some(&ctx.planet_environment));
            let biome_b = derive_chunk_biome(
                &profile_again,
                &registry,
                coord,
                Some(&ctx_again.planet_environment),
            );
            assert_eq!(
                biome_a.biome_key, biome_b.biome_key,
                "biome at chunk ({}, {}) must be deterministic across identical derivations",
                coord.x, coord.z,
            );
        }
    }

    /// Override-mode world generates biomes identically to before (no regression).
    ///
    /// Verifies that building a `WorldProfile` via `from_config` (planet seed
    /// override) produces the exact same biome assignments as a second
    /// identically-configured profile. This guards against regressions where
    /// system-derived plumbing accidentally alters the override path.
    ///
    /// Checks:
    /// - `system_context` is `None` (override mode, no system derivation).
    /// - `is_system_derived()` returns `false`.
    /// - Sub-seeds are deterministic across two independent `from_config` calls.
    /// - Biome key, ground color, and density modifier are identical for every
    ///   chunk in a representative grid, with `planet_env` = `None` (the
    ///   override-mode call convention).
    /// - All three default biomes are still reachable (the noise field was not
    ///   inadvertently shifted).
    #[test]
    fn override_mode_biome_generation_no_regression() {
        let config = sample_config();
        assert_eq!(
            config.seed_mode(),
            SeedMode::Override,
            "sample_config must be in override mode for this test"
        );

        let profile_a = WorldProfile::from_config(&config).unwrap();
        let profile_b = WorldProfile::from_config(&config).unwrap();

        // Override mode must not carry system context.
        assert!(
            profile_a.system_context.is_none(),
            "override-mode WorldProfile must have system_context = None"
        );
        assert!(
            !profile_a.is_system_derived(),
            "override-mode WorldProfile must report is_system_derived() == false"
        );

        // Sub-seeds must be identical across independent constructions.
        assert_eq!(
            profile_a.biome_climate_seed, profile_b.biome_climate_seed,
            "biome_climate_seed must be deterministic"
        );
        assert_eq!(
            profile_a.elevation_seed, profile_b.elevation_seed,
            "elevation_seed must be deterministic"
        );
        assert_eq!(
            profile_a.placement_density_seed, profile_b.placement_density_seed,
            "placement_density_seed must be deterministic"
        );
        assert_eq!(
            profile_a.placement_variation_seed, profile_b.placement_variation_seed,
            "placement_variation_seed must be deterministic"
        );
        assert_eq!(
            profile_a.object_identity_seed, profile_b.object_identity_seed,
            "object_identity_seed must be deterministic"
        );

        let registry = BiomeRegistry::default();
        let mut found_biomes: std::collections::HashSet<String> = std::collections::HashSet::new();

        // Scan a wide grid and assert byte-identical biome results between
        // the two independently-constructed profiles.
        for x in -50..50_i32 {
            for z in -50..50_i32 {
                let coord = ChunkCoord::new(x, z);

                // Override mode always passes None for planet_env — no
                // absolute-temperature filtering.
                let biome_a = derive_chunk_biome(&profile_a, &registry, coord, None);
                let biome_b = derive_chunk_biome(&profile_b, &registry, coord, None);

                assert_eq!(
                    biome_a.biome_key, biome_b.biome_key,
                    "biome key mismatch at chunk ({x}, {z})"
                );
                assert_eq!(
                    biome_a.ground_color, biome_b.ground_color,
                    "ground_color mismatch at chunk ({x}, {z})"
                );
                assert_eq!(
                    biome_a.density_modifier, biome_b.density_modifier,
                    "density_modifier mismatch at chunk ({x}, {z})"
                );

                found_biomes.insert(biome_a.biome_key);
            }
        }

        // All three default biomes must still be reachable — a regression
        // that silently shifted noise offsets would collapse biome variety.
        assert!(
            found_biomes.contains("scorched_flats"),
            "scorched_flats must be reachable in override mode, found: {found_biomes:?}"
        );
        assert!(
            found_biomes.contains("mineral_steppe"),
            "mineral_steppe must be reachable in override mode, found: {found_biomes:?}"
        );
        assert!(
            found_biomes.contains("frost_shelf"),
            "frost_shelf must be reachable in override mode, found: {found_biomes:?}"
        );
    }
}
