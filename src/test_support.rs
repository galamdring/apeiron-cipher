//! Shared test-only utilities for the Apeiron Cipher test suite.
//!
//! This module provides synthetic [`SurfaceProvider`] implementations and
//! other reusable test infrastructure. It is compiled only under `#[cfg(test)]`
//! and is never part of a release build.
//!
//! ## Why a shared module?
//!
//! Several test files need the same fake surface providers (flat, stepped,
//! tilted). Rather than duplicating them or hiding them inside one module's
//! `#[cfg(test)]` block, they live here so any test file can import them.

use crate::world_generation::{SurfaceProvider, SurfaceQueryResult};

/// A perfectly flat horizontal surface for testing.
///
/// Models a constant-height plane at `surface_y` within `bounds_xz`.
/// Any query outside the bounds returns `valid = false`.
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

/// A stepped / terraced surface for testing non-flat terrain.
///
/// This divides the X axis into steps of `step_width` world units. Each step
/// has a different height: `base_y + step_index * step_height`. The surface
/// normal on each flat terrace is straight up `(0, 1, 0)`, but at the step
/// edges the normal tilts to indicate the slope transition.
///
/// This exists purely for testing AC2 and AC3. It is not used in the live game.
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
