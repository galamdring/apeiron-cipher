//! Surface override registry — resolves the walkable surface at any world
//! position by layering player-placed structure floors, table tops, and other
//! horizontal surfaces on top of the procedural terrain elevation.
//!
//! ## Design
//!
//! The terrain (`PlanetSurface::sample_elevation`) provides the base ground
//! height everywhere. Structures (room floors, shelves, future player-built
//! platforms) register rectangular **surface overrides** that locally replace
//! or augment the terrain. The resolution query
//! [`resolve_standing_surface`] returns the highest surface at or below a
//! given reference Y, which lets callers choose different thresholds:
//!
//! - **Player feet:** reference Y = current feet + step tolerance (~0.5 m) —
//!   you can step up onto a curb but not a wall.
//! - **Item drops:** reference Y = current feet + hand height (~1.5 m) —
//!   items land on tables the player is standing next to.
//!
//! Each override is owned by an entity so it can be removed when the owning
//! structure is destroyed or picked up.

use bevy::prelude::*;

/// A rectangular horizontal surface that objects and the player can stand on.
///
/// Registered in [`SurfaceOverrideRegistry`] and tied to an owning entity.
/// When the owner is despawned, the override should be removed.
#[derive(Clone, Debug)]
#[allow(dead_code)] // `owner` is read by `remove_by_owner`, not yet called at runtime
pub struct SurfaceOverride {
    /// Entity that owns this surface (room floor, table, shelf, etc.).
    pub owner: Entity,
    /// Minimum X bound (world space).
    pub min_x: f32,
    /// Maximum X bound (world space).
    pub max_x: f32,
    /// Minimum Z bound (world space).
    pub min_z: f32,
    /// Maximum Z bound (world space).
    pub max_z: f32,
    /// The Y height of this walkable surface (world space).
    pub surface_y: f32,
}

impl SurfaceOverride {
    /// Returns `true` if the given world XZ falls within this surface's bounds.
    pub fn contains_xz(&self, x: f32, z: f32) -> bool {
        x >= self.min_x && x <= self.max_x && z >= self.min_z && z <= self.max_z
    }
}

/// Global registry of all active surface overrides.
///
/// The registry is intentionally a flat `Vec` — the number of overrides in a
/// typical scene is small (room floor, a few tables/shelves, player-built
/// platforms) so linear scan is fine. If this ever needs spatial indexing we
/// can swap the internals without changing the public API.
#[derive(Resource, Default, Debug)]
pub struct SurfaceOverrideRegistry {
    overrides: Vec<SurfaceOverride>,
}

impl SurfaceOverrideRegistry {
    /// Register a new surface override. Returns the index for debugging.
    pub fn register(&mut self, surface: SurfaceOverride) -> usize {
        let idx = self.overrides.len();
        self.overrides.push(surface);
        idx
    }

    /// Remove all overrides owned by the given entity.
    #[allow(dead_code)] // API for future structure removal
    pub fn remove_by_owner(&mut self, owner: Entity) {
        self.overrides.retain(|s| s.owner != owner);
    }

    /// Iterate all overrides (for queries).
    pub fn iter(&self) -> impl Iterator<Item = &SurfaceOverride> {
        self.overrides.iter()
    }

    /// Returns `true` if any registered override contains the given XZ point.
    ///
    /// Used to suppress procedural spawns (mineral deposits, flora) inside
    /// player structures — objects shouldn't sprout through floors.
    pub fn any_contains_xz(&self, x: f32, z: f32) -> bool {
        self.overrides.iter().any(|s| s.contains_xz(x, z))
    }
}

/// Resolve the effective standing surface at a world position.
///
/// Collects the terrain height and all surface overrides that contain the
/// given `(x, z)`, then returns the highest surface whose Y is at or below
/// `max_y`. This lets callers control the reach:
///
/// - For player stepping: `max_y = feet_y + step_tolerance`
/// - For item drops: `max_y = feet_y + hand_height`
///
/// If no surface (including terrain) is at or below `max_y`, returns
/// `terrain_y` as the fallback — the player shouldn't fall through the
/// world.
pub fn resolve_standing_surface(
    x: f32,
    z: f32,
    max_y: f32,
    terrain_y: f32,
    registry: &SurfaceOverrideRegistry,
) -> f32 {
    let mut best = terrain_y;

    // Only consider terrain if it's at or below the reference height.
    // If terrain is above max_y (e.g. inside a cave), we still start with
    // it as fallback but an override below max_y can win.
    if terrain_y > max_y {
        best = f32::NEG_INFINITY;
    }

    for surface in registry.iter() {
        if surface.contains_xz(x, z) && surface.surface_y <= max_y && surface.surface_y > best {
            best = surface.surface_y;
        }
    }

    // If nothing was at or below max_y (no overrides, terrain above us),
    // fall back to terrain so the player doesn't fall forever.
    if best == f32::NEG_INFINITY {
        terrain_y
    } else {
        best
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_registry() -> SurfaceOverrideRegistry {
        let mut reg = SurfaceOverrideRegistry::default();
        // Room floor at y=2.0, covering -4..4 on both axes.
        reg.register(SurfaceOverride {
            owner: Entity::from_bits(1),
            min_x: -4.0,
            max_x: 4.0,
            min_z: -4.0,
            max_z: 4.0,
            surface_y: 2.0,
        });
        // Table at y=2.8, smaller footprint.
        reg.register(SurfaceOverride {
            owner: Entity::from_bits(2),
            min_x: -1.0,
            max_x: 1.0,
            min_z: -1.0,
            max_z: 1.0,
            surface_y: 2.8,
        });
        reg
    }

    #[test]
    fn terrain_only_when_no_overrides_match() {
        let reg = SurfaceOverrideRegistry::default();
        let result = resolve_standing_surface(50.0, 50.0, 10.0, 3.0, &reg);
        assert!((result - 3.0).abs() < f32::EPSILON);
    }

    #[test]
    fn floor_override_wins_over_lower_terrain() {
        let reg = sample_registry();
        // Terrain at 1.0, floor at 2.0, player max_y at 3.0.
        // Query at (3.0, 3.0) — inside room floor but outside table bounds.
        let result = resolve_standing_surface(3.0, 3.0, 3.0, 1.0, &reg);
        assert!((result - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn table_wins_when_within_reach() {
        let reg = sample_registry();
        // Standing on floor (2.0), drop reach = 2.0 + 1.5 = 3.5.
        // Table at 2.8 is within reach and higher than floor.
        let result = resolve_standing_surface(0.0, 0.0, 3.5, 1.0, &reg);
        assert!((result - 2.8).abs() < f32::EPSILON);
    }

    #[test]
    fn table_ignored_when_above_max_y() {
        let reg = sample_registry();
        // Step tolerance from floor: max_y = 2.0 + 0.5 = 2.5.
        // Table at 2.8 is above max_y, so floor wins.
        let result = resolve_standing_surface(0.0, 0.0, 2.5, 1.0, &reg);
        assert!((result - 2.0).abs() < f32::EPSILON);
    }

    #[test]
    fn outside_override_bounds_uses_terrain() {
        let reg = sample_registry();
        let result = resolve_standing_surface(10.0, 10.0, 5.0, 1.0, &reg);
        assert!((result - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn remove_by_owner_clears_override() {
        let mut reg = sample_registry();
        reg.remove_by_owner(Entity::from_bits(1)); // Remove room floor.
        // Only table remains, but we're outside its bounds.
        let result = resolve_standing_surface(3.0, 3.0, 5.0, 1.0, &reg);
        assert!((result - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn terrain_above_max_y_still_used_as_fallback() {
        // Simulates being in a cave: terrain is above the player but there's
        // no override either. Terrain should still be returned so we don't
        // fall forever.
        let reg = SurfaceOverrideRegistry::default();
        let result = resolve_standing_surface(0.0, 0.0, 1.0, 5.0, &reg);
        assert!((result - 5.0).abs() < f32::EPSILON);
    }

    #[test]
    fn any_contains_xz_reports_hit() {
        let reg = sample_registry();
        assert!(reg.any_contains_xz(0.0, 0.0)); // inside both room and table
        assert!(reg.any_contains_xz(3.0, 3.0)); // inside room only
        assert!(!reg.any_contains_xz(10.0, 10.0)); // outside everything
    }

    #[test]
    fn cave_override_below_terrain_is_found() {
        let mut reg = SurfaceOverrideRegistry::default();
        // Cave floor at y=-2, terrain at y=5. Player is in the cave at y=-1.
        reg.register(SurfaceOverride {
            owner: Entity::from_bits(10),
            min_x: -10.0,
            max_x: 10.0,
            min_z: -10.0,
            max_z: 10.0,
            surface_y: -2.0,
        });
        // max_y = -1 + 0.5 = -0.5. Cave floor at -2 is below -0.5. Terrain at 5 is above.
        let result = resolve_standing_surface(0.0, 0.0, -0.5, 5.0, &reg);
        assert!((result - (-2.0)).abs() < f32::EPSILON);
    }
}
