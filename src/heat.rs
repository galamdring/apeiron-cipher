//! Heat plugin — environmental property revelation through thermal exposure.
//!
//! Spawns a heat source (burner) on the workbench and drives the heat-zone
//! loop: materials placed near the burner accumulate exposure, visually react
//! based on their `thermal_resistance`, and eventually have that hidden
//! property revealed for the examine panel.
//!
//! No labels, no numbers. The player watches the material change (or not) and
//! draws their own conclusions.
//!
//! Systems:
//! - `spawn_heat_source`: glowing disc + point light on the workbench
//! - `track_heat_exposure`: increment/reset exposure timers for materials in the zone
//! - `apply_thermal_reaction`: visual feedback (emissive glow, color shift, scale)
//! - `reveal_thermal_property`: flip `thermal_resistance` to `Revealed` after threshold

use bevy::prelude::*;

use crate::interaction::HeldItem;
use crate::materials::{GameMaterial, MaterialObject, PropertyVisibility};
use crate::scene::{SceneConfig, Workbench};

pub(crate) struct HeatPlugin;

impl Plugin for HeatPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PostStartup, spawn_heat_source).add_systems(
            Update,
            (
                track_heat_exposure,
                apply_thermal_reaction.after(track_heat_exposure),
                reveal_thermal_property.after(track_heat_exposure),
            ),
        );
    }
}

// ── Components ──────────────────────────────────────────────────────────

/// Marks the burner entity so systems can locate the heat source position.
#[derive(Component)]
struct HeatSource;

/// Tracks cumulative seconds a material has spent inside the heat zone.
/// Added dynamically when a material first enters the zone, persists when
/// moved away so past exposure is remembered.
#[derive(Component)]
struct HeatExposure {
    elapsed: f32,
    in_zone: bool,
}

impl HeatExposure {
    fn new() -> Self {
        Self {
            elapsed: 0.0,
            in_zone: false,
        }
    }
}

// ── Spawn ───────────────────────────────────────────────────────────────

fn spawn_heat_source(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cfg: Res<SceneConfig>,
    workbench_query: Query<&Transform, With<Workbench>>,
) {
    let Ok(wb_tf) = workbench_query.single() else {
        warn!("No workbench found — heat source will not be spawned");
        return;
    };

    let hs = &cfg.heat_source;
    let fur = &cfg.furniture;

    let pos = Vec3::new(
        wb_tf.translation.x + hs.offset_x,
        fur.workbench_height + hs.radius * 0.5,
        wb_tf.translation.z + hs.offset_z,
    );

    let burner_mesh = meshes.add(Cylinder::new(hs.radius, hs.radius * 0.3));
    let burner_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.15, 0.12, 0.12),
        emissive: LinearRgba::new(80.0, 20.0, 5.0, 1.0),
        ..default()
    });

    commands
        .spawn((
            HeatSource,
            Mesh3d(burner_mesh),
            MeshMaterial3d(burner_mat),
            Transform::from_translation(pos),
        ))
        .with_child((PointLight {
            color: Color::srgb(1.0, 0.5, 0.15),
            intensity: hs.light_intensity,
            range: hs.zone_radius * 2.0,
            shadows_enabled: false,
            ..default()
        },));

    info!("Spawned heat source at ({}, {}, {})", pos.x, pos.y, pos.z);
}

// ── Exposure tracking ───────────────────────────────────────────────────

// Bevy queries are inherently generic-heavy; a type alias would hide which
// components/filters the system accesses, making the signature harder to audit.
#[allow(clippy::type_complexity)]
fn track_heat_exposure(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<SceneConfig>,
    heat_query: Query<&GlobalTransform, With<HeatSource>>,
    mut material_query: Query<
        (Entity, &GlobalTransform, Option<&mut HeatExposure>),
        (With<MaterialObject>, Without<HeldItem>),
    >,
) {
    let Ok(heat_gtf) = heat_query.single() else {
        return;
    };
    let heat_pos = heat_gtf.translation();
    let zone_r_sq = cfg.heat_source.zone_radius * cfg.heat_source.zone_radius;
    let dt = time.delta_secs();

    for (entity, mat_gtf, exposure) in &mut material_query {
        let dist_sq = mat_gtf.translation().distance_squared(heat_pos);
        let inside = dist_sq <= zone_r_sq;

        match exposure {
            Some(mut exp) => {
                exp.in_zone = inside;
                if inside {
                    exp.elapsed += dt;
                }
            }
            None if inside => {
                commands.entity(entity).insert(HeatExposure::new());
            }
            _ => {}
        }
    }
}

// ── Thermal reaction (visual feedback) ──────────────────────────────────

/// Reaction intensity as a function of exposure progress and thermal resistance.
/// Low resistance → fast, strong reaction. High resistance → slow, weak reaction.
fn reaction_intensity(exposure_frac: f32, thermal_resistance: f32) -> f32 {
    let sensitivity = 1.0 - thermal_resistance;
    (exposure_frac * sensitivity * 1.5).clamp(0.0, 1.0)
}

/// Returns the emissive glow colour at a given reaction intensity.
fn reaction_emissive(intensity: f32) -> LinearRgba {
    LinearRgba::new(intensity * 200.0, intensity * 40.0, intensity * 5.0, 1.0)
}

/// Scale deformation: low-resistance materials "soften" (Y shrinks, XZ expands).
fn reaction_scale(intensity: f32, thermal_resistance: f32) -> Vec3 {
    if thermal_resistance > 0.7 {
        return Vec3::ONE;
    }
    let deform = intensity * (1.0 - thermal_resistance) * 0.15;
    Vec3::new(1.0 + deform, 1.0 - deform * 0.8, 1.0 + deform)
}

// Two separate queries needed: one for the material handle (shared access) and one
// for the mutable transform. Collapsing would require unsafe world access.
#[allow(clippy::type_complexity)]
fn apply_thermal_reaction(
    cfg: Res<SceneConfig>,
    exposure_query: Query<
        (
            &HeatExposure,
            &GameMaterial,
            &MeshMaterial3d<StandardMaterial>,
        ),
        With<MaterialObject>,
    >,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
    mut transform_query: Query<
        (&HeatExposure, &GameMaterial, &mut Transform),
        With<MaterialObject>,
    >,
) {
    let reaction_secs = cfg.heat_source.reaction_seconds;

    for (exp, mat, mat_handle) in &exposure_query {
        let frac = (exp.elapsed / reaction_secs).clamp(0.0, 1.0);
        let intensity = reaction_intensity(frac, mat.thermal_resistance.value);

        if let Some(std_mat) = std_materials.get_mut(mat_handle) {
            std_mat.emissive = reaction_emissive(intensity);

            let warm_shift = intensity * (1.0 - mat.thermal_resistance.value) * 0.3;
            let [r, g, b] = mat.color;
            std_mat.base_color = Color::srgb(
                (r + warm_shift).min(1.0),
                g,
                (b - warm_shift * 0.5).max(0.0),
            );
        }
    }

    for (exp, mat, mut tf) in &mut transform_query {
        let frac = (exp.elapsed / reaction_secs).clamp(0.0, 1.0);
        let intensity = reaction_intensity(frac, mat.thermal_resistance.value);
        tf.scale = reaction_scale(intensity, mat.thermal_resistance.value);
    }
}

// ── Property revelation ─────────────────────────────────────────────────

fn reveal_thermal_property(
    cfg: Res<SceneConfig>,
    mut material_query: Query<(&HeatExposure, &mut GameMaterial), With<MaterialObject>>,
) {
    let reveal_secs = cfg.heat_source.reveal_seconds;

    for (exp, mut mat) in &mut material_query {
        if exp.elapsed >= reveal_secs
            && mat.thermal_resistance.visibility == PropertyVisibility::Hidden
        {
            mat.thermal_resistance.visibility = PropertyVisibility::Revealed;
            info!(
                "'{}' thermal resistance revealed after {:.1}s exposure",
                mat.name, exp.elapsed
            );
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn reaction_intensity_zero_at_no_exposure() {
        assert!((reaction_intensity(0.0, 0.5) - 0.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reaction_intensity_increases_with_exposure() {
        let low = reaction_intensity(0.3, 0.2);
        let high = reaction_intensity(0.8, 0.2);
        assert!(high > low);
    }

    #[test]
    fn reaction_intensity_decreases_with_higher_resistance() {
        let low_resist = reaction_intensity(1.0, 0.1);
        let high_resist = reaction_intensity(1.0, 0.9);
        assert!(low_resist > high_resist);
    }

    #[test]
    fn reaction_intensity_clamped_to_one() {
        let result = reaction_intensity(2.0, 0.0);
        assert!((result - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reaction_emissive_black_at_zero() {
        let e = reaction_emissive(0.0);
        assert!(e.red.abs() < f32::EPSILON);
        assert!(e.green.abs() < f32::EPSILON);
        assert!(e.blue.abs() < f32::EPSILON);
    }

    #[test]
    fn reaction_emissive_bright_at_one() {
        let e = reaction_emissive(1.0);
        assert!(e.red > 100.0);
        assert!(e.green > 10.0);
    }

    #[test]
    fn reaction_scale_identity_for_high_resistance() {
        let s = reaction_scale(1.0, 0.8);
        assert!((s.x - 1.0).abs() < f32::EPSILON);
        assert!((s.y - 1.0).abs() < f32::EPSILON);
        assert!((s.z - 1.0).abs() < f32::EPSILON);
    }

    #[test]
    fn reaction_scale_deforms_for_low_resistance() {
        let s = reaction_scale(1.0, 0.1);
        assert!(s.x > 1.0, "XZ should expand");
        assert!(s.y < 1.0, "Y should shrink (melting)");
    }

    #[test]
    fn reaction_scale_no_deform_at_zero_intensity() {
        let s = reaction_scale(0.0, 0.1);
        assert!((s.x - 1.0).abs() < f32::EPSILON);
        assert!((s.y - 1.0).abs() < f32::EPSILON);
    }
}
