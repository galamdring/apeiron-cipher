//! Fabricator plugin — the workbench combination device.
//!
//! The fabricator has two input slots and one output slot, all positioned on
//! the workbench surface. Players place materials into input slots, activate
//! the fabricator, and receive a combined output material.
//!
//! State machine: Idle → Processing(timer) → Complete.
//! Activation requires both slots filled. Processing runs for `process_seconds`
//! with visual feedback (emissive glow on input slots). On completion the input
//! materials are consumed and a placeholder output is spawned.
//!
//! Slot targeting and material placement routing live in the interaction plugin.

use bevy::prelude::*;

use crate::journal::{JournalKey, Observation, ObservationCategory};
use crate::materials::{
    GameMaterial, MATERIAL_SURFACE_GAP, MaterialCatalog, MaterialObject, MaterialProperty,
    PropertyVisibility,
};
use crate::observation::{ConfidenceConfig, RecordObservation};
use crate::scene::{FabricatorSceneConfig, FurnitureConfig, Workbench};

/// Registers the fabricator workbench systems for combining materials.
pub struct FabricatorPlugin;

impl Plugin for FabricatorPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<ActivateIntent>()
            .init_resource::<FabricatorState>()
            .add_systems(PostStartup, spawn_fabricator_slots)
            .add_systems(
                Update,
                (
                    process_activation,
                    tick_processing.after(process_activation),
                    apply_processing_visuals.after(tick_processing),
                ),
            );
    }
}

// ── Messages ────────────────────────────────────────────────────────────

#[derive(Message)]
/// Message requesting the fabricator to begin processing its input slots.
pub struct ActivateIntent;

// ── State ───────────────────────────────────────────────────────────────

#[derive(Resource, Default, Debug, PartialEq)]
enum FabricatorState {
    #[default]
    Idle,
    Processing {
        elapsed: f32,
    },
}

// ── Components ──────────────────────────────────────────────────────────

/// Marks a fabricator input receptacle. `index` distinguishes slot 0 from slot 1.\
/// `material` holds the entity of the material currently seated in this slot.
///
/// The `InputSlot` entity is a logical parent with no mesh of its own.
/// The visual geometry (floor panel + 4 wall panels) lives on child entities,
/// each tagged with [`ChamberPanel`] so the emissive glow system can find them.
#[derive(Component, Debug)]
pub struct InputSlot {
    /// Numeric identifier distinguishing slot 0 from slot 1.
    // Used in debug logging and future UI to identify which slot is which.
    #[allow(dead_code)]
    pub index: usize,
    /// Entity of the material currently placed in this slot, if any.
    pub material: Option<Entity>,
    /// World-space Y coordinate of the floor interior surface of this slot.
    ///
    /// Interaction systems use this to position material objects on the chamber floor
    /// rather than floating in mid-air or clipping through walls.
    pub top_y: f32,
}

/// Marks a mesh panel that is part of an input chamber's visual geometry.
///
/// Each [`InputSlot`] entity has 5 child entities tagged with this component:
/// one floor panel and four wall panels. The emissive glow system queries
/// `With<ChamberPanel>` to apply activation feedback to all chamber surfaces.
#[derive(Component)]
struct ChamberPanel;

/// Marks the fabricator output receptacle where the combined material appears.
#[derive(Component, Debug)]
pub struct OutputSlot {
    /// Entity of the material currently placed in the output slot, if any.
    pub material: Option<Entity>,
    /// World-space Y coordinate of the top surface of this slot.
    pub top_y: f32,
}

// ── Slot spawning ───────────────────────────────────────────────────────

fn spawn_fabricator_slots(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    fab: Res<FabricatorSceneConfig>,
    fur: Res<FurnitureConfig>,
    workbench_query: Query<&Transform, With<Workbench>>,
) {
    let Ok(wb_tf) = workbench_query.single() else {
        warn!("No workbench found — fabricator slots will not be spawned");
        return;
    };

    let wb_top_y = wb_tf.translation.y + fur.workbench_height * 0.5;
    let wb_center = wb_tf.translation;

    // Shared material for input chamber walls and floor — dark metallic.
    let slot_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.25, 0.28, 0.35),
        perceptual_roughness: 0.3,
        metallic: 0.7,
        ..default()
    });

    let output_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.35, 0.32, 0.25),
        perceptual_roughness: 0.3,
        metallic: 0.7,
        ..default()
    });

    // Chamber geometry dimensions.
    //
    // The chamber is a hollow box with an open top:
    //   - `ext` is the half-extent of the INTERIOR footprint (square cross-section).
    //   - `wt`  is the wall-panel thickness.
    //   - `wh`  is the wall height above the floor surface — the cavity depth.
    //   - `ft`  is the floor-panel thickness.
    //
    // Materials rest on the floor interior surface. `top_y` is set to the floor
    // interior surface Y so interaction.rs positions materials correctly inside
    // the chamber rather than on top of the outer shell.
    let ext = fab.slot_radius; // interior half-extent (X and Z)
    let wt = fab.chamber_wall_thickness; // wall thickness
    let wh = fab.chamber_wall_height; // wall height = cavity depth
    let ft = fab.slot_height; // floor thickness

    // Full outer half-extents of the chamber assembly:
    let outer_ext = ext + wt;

    for i in 0..2 {
        let z_sign = if i == 0 { 1.0 } else { -1.0 };

        // Parent entity origin sits at the bottom of the floor panel.
        // The floor panel occupies [origin_y, origin_y + ft].
        // The floor interior surface (where materials rest) is at origin_y + ft.
        // The walls extend from origin_y + ft up to origin_y + ft + wh.
        let origin_y = wb_top_y;
        let pos = Vec3::new(
            wb_center.x + fab.slot_offset_x,
            origin_y,
            wb_center.z + fab.slot_spacing_z * z_sign,
        );

        // `top_y`: the Y position of the floor interior surface where materials sit.
        // This is what interaction.rs uses to place material objects inside the chamber.
        let floor_interior_y = origin_y + ft;

        // ── Floor panel ────────────────────────────────────────────────────
        // A full-width cuboid that seals the bottom of the chamber.
        // Width/depth covers the full outer footprint so walls have solid footing.
        let floor_mesh = meshes.add(Cuboid::new(outer_ext * 2.0, ft, outer_ext * 2.0));
        // The floor panel's center is at ft * 0.5 above the parent origin.
        let floor_child_y = ft * 0.5;

        // ── Wall panels ────────────────────────────────────────────────────
        // Four walls, each a thin cuboid. The walls sit on top of the floor surface.
        // Wall center Y (local) = ft + wh * 0.5  (floor thickness + half wall height)
        let wall_center_local_y = ft + wh * 0.5;

        // +X wall (right side of chamber interior)
        // Spans the full outer Z extent, thickness wt on the +X side.
        let wall_px_mesh = meshes.add(Cuboid::new(wt, wh, outer_ext * 2.0));
        // -X wall
        let wall_nx_mesh = meshes.add(Cuboid::new(wt, wh, outer_ext * 2.0));
        // +Z wall — spans only the INTERIOR X extent to avoid corner overlap with X walls.
        // Inner Z walls fit between the outer X walls so there are no doubled corners.
        let wall_pz_mesh = meshes.add(Cuboid::new(ext * 2.0, wh, wt));
        // -Z wall
        let wall_nz_mesh = meshes.add(Cuboid::new(ext * 2.0, wh, wt));

        // X-wall center X offset = outer_ext - wt * 0.5  (inner face flush with interior)
        let wall_x_offset = outer_ext - wt * 0.5;
        // Z-wall center Z offset = the interior half-extent ext (inner face flush)
        let wall_z_offset = ext + wt * 0.5;

        let slot_entity = commands
            .spawn((
                InputSlot {
                    index: i,
                    material: None,
                    // Materials rest on the floor interior surface.
                    top_y: floor_interior_y,
                },
                // The parent entity carries the logical slot identity; mesh panels are children.
                // No `Mesh3d` here — the emissive glow system targets `MeshMaterial3d` children.
                Transform::from_translation(pos),
                Visibility::Inherited,
            ))
            .id();

        // Spawn the five chamber panels as children of the slot entity.
        // Each child uses a local Transform relative to the parent origin.
        // `ChamberPanel` is the query anchor for the emissive glow system.
        commands.entity(slot_entity).with_children(|parent| {
            // Floor panel — center at (0, ft*0.5, 0) local
            parent.spawn((
                ChamberPanel,
                Mesh3d(floor_mesh),
                MeshMaterial3d(slot_mat.clone()),
                Transform::from_xyz(0.0, floor_child_y, 0.0),
            ));
            // +X wall
            parent.spawn((
                ChamberPanel,
                Mesh3d(wall_px_mesh),
                MeshMaterial3d(slot_mat.clone()),
                Transform::from_xyz(wall_x_offset, wall_center_local_y, 0.0),
            ));
            // -X wall
            parent.spawn((
                ChamberPanel,
                Mesh3d(wall_nx_mesh),
                MeshMaterial3d(slot_mat.clone()),
                Transform::from_xyz(-wall_x_offset, wall_center_local_y, 0.0),
            ));
            // +Z wall
            parent.spawn((
                ChamberPanel,
                Mesh3d(wall_pz_mesh),
                MeshMaterial3d(slot_mat.clone()),
                Transform::from_xyz(0.0, wall_center_local_y, wall_z_offset),
            ));
            // -Z wall
            parent.spawn((
                ChamberPanel,
                Mesh3d(wall_nz_mesh),
                MeshMaterial3d(slot_mat.clone()),
                Transform::from_xyz(0.0, wall_center_local_y, -wall_z_offset),
            ));
        });

        info!(
            "Spawned input chamber {i} at ({}, {}, {}) — interior floor Y: {floor_interior_y:.4}",
            pos.x, pos.y, pos.z
        );
    }

    // ── Output slot ────────────────────────────────────────────────────────
    // The output slot keeps its flat cylinder shape — output is presented on a
    // tray-like surface, not consumed into a chamber.
    let output_pos = Vec3::new(
        wb_center.x + fab.output_offset_x,
        wb_top_y + fab.output_height * 0.5,
        wb_center.z + fab.output_offset_z,
    );

    commands.spawn((
        OutputSlot {
            material: None,
            top_y: output_pos.y + fab.output_height * 0.5,
        },
        Mesh3d(meshes.add(Cylinder::new(fab.output_radius, fab.output_height))),
        MeshMaterial3d(output_mat),
        Transform::from_translation(output_pos),
    ));

    info!(
        "Spawned output slot at ({}, {}, {})",
        output_pos.x, output_pos.y, output_pos.z
    );
}

// ── Activation ──────────────────────────────────────────────────────────

fn process_activation(
    mut reader: MessageReader<ActivateIntent>,
    mut state: ResMut<FabricatorState>,
    slots: Query<&InputSlot>,
) {
    for _intent in reader.read() {
        if *state != FabricatorState::Idle {
            continue;
        }

        let both_filled = slots.iter().all(|s| s.material.is_some());
        if both_filled {
            *state = FabricatorState::Processing { elapsed: 0.0 };
            info!("Fabricator activated — processing started");
        }
    }
}

// ── Processing timer ────────────────────────────────────────────────────

// Bevy systems that handle completion need access to commands, time, config,
// state, both slot types, material data, combination rules, and mesh/material assets.
#[allow(clippy::too_many_arguments)]
fn tick_processing(
    mut commands: Commands,
    time: Res<Time>,
    cfg: Res<FabricatorSceneConfig>,
    confidence_config: Res<ConfidenceConfig>,
    mut journal_writer: MessageWriter<RecordObservation>,
    mut state: ResMut<FabricatorState>,
    mut catalog: ResMut<MaterialCatalog>,
    mut slots: Query<&mut InputSlot>,
    material_query: Query<&GameMaterial, With<MaterialObject>>,
    mut output_slot: Query<(&GlobalTransform, &mut OutputSlot)>,
    mut meshes: ResMut<Assets<Mesh>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    let FabricatorState::Processing { ref mut elapsed } = *state else {
        return;
    };

    *elapsed += time.delta_secs();

    if *elapsed < cfg.process_seconds {
        return;
    }

    // Collect input materials before mutating slots.
    let input_mats: Vec<GameMaterial> = slots
        .iter()
        .filter_map(|s| s.material.and_then(|e| material_query.get(e).ok()).cloned())
        .collect();

    if input_mats.len() < 2 {
        warn!("Processing completed but input materials missing — resetting");
        *state = FabricatorState::Idle;
        return;
    }

    // Despawn input material entities and clear the slots.
    for mut slot in &mut slots {
        if let Some(mat_entity) = slot.material.take() {
            commands.entity(mat_entity).despawn();
        }
    }

    // Property-math combination.
    let output_mat = property_combine(&input_mats[0], &input_mats[1]);

    // Register the fabricated material in the catalog so it is discoverable
    // by seed/name lookups (e.g. journal, future recipes).  The catalog may
    // disambiguate the name if it collides with an existing entry, so we use
    // the *registered* version for the spawned entity — not the pre-registration
    // clone.  (Fixes #311: spawned entity had a stale, potentially colliding name.)
    let output_mat = catalog.register_fabricated(output_mat).clone();

    // Spawn the output material on the output slot.
    let Ok((output_gtf, mut out_slot)) = output_slot.single_mut() else {
        warn!("No output slot found — cannot spawn result");
        *state = FabricatorState::Idle;
        return;
    };

    let out_pos = output_gtf.translation();
    let mesh = output_mat.mesh_for_density(&mut meshes);
    let render_mat = std_materials.add(StandardMaterial {
        base_color: output_mat.bevy_color(),
        perceptual_roughness: 0.5,
        metallic: if output_mat.conductivity.value() > 0.6 {
            0.6
        } else {
            0.1
        },
        ..default()
    });

    let output_entity = commands
        .spawn((
            MaterialObject,
            output_mat.clone(),
            Mesh3d(mesh),
            MeshMaterial3d(render_mat),
            Transform::from_xyz(
                out_pos.x,
                out_slot.top_y + output_mat.support_height() + MATERIAL_SURFACE_GAP,
                out_pos.z,
            ),
        ))
        .id();

    out_slot.material = Some(output_entity);

    // Record the fabrication result in the player's journal so the player
    // accumulates knowledge about what combinations produce which outputs.
    let input_names: Vec<&str> = input_mats.iter().map(|m| m.name.as_str()).collect();
    let description = format!(
        "Combined {} and {} to produce {}.",
        input_names[0], input_names[1], output_mat.name
    );
    journal_writer.write(RecordObservation {
        key: JournalKey::Fabrication {
            output_seed: output_mat.seed,
        },
        name: output_mat.name.clone(),
        material_seed: None,
        observation: Observation {
            category: ObservationCategory::FabricationResult,
            confidence: crate::observation::Confidence::new(
                confidence_config.initial_observation_confidence,
            ), // Configured in confidence.toml
            description,
            recorded_at: 0,
        },
        // Pass input material seeds so the knowledge graph can wire DerivedFrom
        // edges from the output concept to each input material (Story 10.5).
        planet_seed: None,
        input_seeds: input_mats
            .iter()
            .map(|m| crate::materials::MaterialSeed(m.seed))
            .collect(),
        context_location: None,
    });

    info!("Fabrication complete — produced '{}'", output_mat.name);
    *state = FabricatorState::Idle;
}

// ── Processing visual feedback ──────────────────────────────────────────

/// Apply the violet pulse emissive to all chamber panel surfaces during fabrication.
///
/// Runs in `Update`, after `tick_processing`. Queries `ChamberPanel` children (not the
/// logical `InputSlot` parent, which carries no mesh) to drive emissive intensity on
/// the material handles. During `Processing` the pulse is a sine-wave modulated violet;
/// during `Idle` the emissive is zeroed so the chambers return to their base color.
fn apply_processing_visuals(
    state: Res<FabricatorState>,
    cfg: Res<FabricatorSceneConfig>,
    // Query the chamber panel children — they carry MeshMaterial3d, not the InputSlot parent.
    panel_query: Query<&MeshMaterial3d<StandardMaterial>, With<ChamberPanel>>,
    mut std_materials: ResMut<Assets<StandardMaterial>>,
) {
    let glow = match *state {
        FabricatorState::Processing { elapsed } => {
            let frac = (elapsed / cfg.process_seconds).clamp(0.0, 1.0);
            let pulse = (frac * std::f32::consts::TAU * 3.0).sin().abs();
            LinearRgba::new(pulse * 60.0, pulse * 40.0, pulse * 80.0, 1.0)
        }
        FabricatorState::Idle => LinearRgba::BLACK,
    };

    for mat_handle in &panel_query {
        if let Some(std_mat) = std_materials.get_mut(mat_handle) {
            std_mat.emissive = glow;
        }
    }
}

// ── Property-math combination ─────────────────────────────────────────────

/// Deterministic pseudo-random float in \[-1.0, 1.0\] from a seed+channel.
/// Splitmix64 single iteration — fast, deterministic, no external crate needed.
fn seeded_noise(seed: u64, channel: u64) -> f32 {
    let mut z = seed.wrapping_add(channel.wrapping_mul(0x9E37_79B9_7F4A_7C15));
    z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
    z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
    z ^= z >> 31;
    // Map to [-1.0, 1.0]
    (z as i64 as f64 / i64::MAX as f64) as f32
}

/// Small deterministic perturbation so outputs are interesting but reproducible.
/// The player should get the same result every time they combine the same pair —
/// but not a perfectly clean average, encouraging measurement.
const PERTURBATION_SCALE: f32 = 0.04;

fn perturb(base: f32, seed: u64, channel: u64) -> f32 {
    (base + seeded_noise(seed, channel) * PERTURBATION_SCALE).clamp(0.0, 1.0)
}

// ── Procedural naming ────────────────────────────────────────────────────
pub use crate::naming::procedural_name;

// ── Color blending ───────────────────────────────────────────────────────

/// Shift hue toward warmer tones — used when a reactive pair produces
/// a visually distinct output (high reactivity synergy).
fn hue_shift(color: [f32; 3], amount: f32) -> [f32; 3] {
    let (r, g, b) = (color[0], color[1], color[2]);
    [
        (r + amount * (1.0 - r)).clamp(0.0, 1.0),
        (g - amount * g * 0.5).clamp(0.0, 1.0),
        (b + amount * (1.0 - b) * 0.3).clamp(0.0, 1.0),
    ]
}

fn blend_color(a: &[f32; 3], b: &[f32; 3], reactive: bool) -> [f32; 3] {
    let blended = [
        (a[0] + b[0]) * 0.5,
        (a[1] + b[1]) * 0.5,
        (a[2] + b[2]) * 0.5,
    ];
    if reactive {
        hue_shift(blended, 0.15)
    } else {
        blended
    }
}

/// Derives the canonical fabricated output seed for an unordered pair of input seeds.
///
/// Fabricator slots are physical placement affordances, not recipe semantics: combining
/// A+B must be the same experiment as combining B+A. Sorting the seeds before applying
/// the stable arithmetic formula makes the fabricated material identity independent of
/// which input slot happened to hold each constituent.
fn combined_material_seed(seed_a: u64, seed_b: u64) -> u64 {
    let seed_min = seed_a.min(seed_b);
    let seed_max = seed_a.max(seed_b);
    seed_min.wrapping_mul(31).wrapping_add(seed_max)
}

/// Combine two materials using pure property math — no external rule tables.
///
/// Input order does not affect the output: the two input seeds are sorted before
/// the fabricated seed is derived, so placing A in slot 0 and B in slot 1 yields
/// the same material as placing B in slot 0 and A in slot 1.
///
/// # Property formulas
///
/// | Property | Formula | Rationale |
/// |---|---|---|
/// | `density` | density-weighted blend | denser input dominates mass |
/// | `thermal_resistance` | `max(a, b)` | more resistant material sets the floor |
/// | `reactivity` | `min + a*b*synergy` | reactive materials amplify each other |
/// | `conductivity` | derived from `1 - thermal_resistance` | physically coupled |
/// | `toxicity` | `max(a, b)` | worst-case — contamination doesn't average out |
///
/// All outputs receive a small deterministic perturbation from the combined
/// seed so results are reproducible but not perfectly clean averages.
pub fn property_combine(a: &GameMaterial, b: &GameMaterial) -> GameMaterial {
    let combined_seed = combined_material_seed(a.seed, b.seed);
    let name = crate::naming::compositional_name(&a.name, &b.name);

    // ── Density: weighted by each input's density (denser dominates) ──
    let total_d = a.density.value() + b.density.value();
    let raw_density = if total_d < f32::EPSILON {
        (a.density.value() + b.density.value()) * 0.5
    } else {
        (a.density.value() * a.density.value() + b.density.value() * b.density.value()) / total_d
    };
    let density_val = perturb(raw_density, combined_seed, 0);

    // ── Thermal resistance: max — most resistant material sets the floor ──
    let thermal_val = perturb(
        a.thermal_resistance
            .value()
            .max(b.thermal_resistance.value()),
        combined_seed,
        1,
    );

    // ── Reactivity: synergistic — reactive inputs amplify each other ──
    // base = min(a, b) so at least one must be reactive; synergy term is a*b
    let react_base = a.reactivity.value().min(b.reactivity.value());
    let synergy = a.reactivity.value() * b.reactivity.value() * 0.5;
    let reactivity_val = perturb((react_base + synergy).clamp(0.0, 1.0), combined_seed, 2);

    // ── Conductivity: physically coupled to thermal resistance ──
    // Raw blend, then pulled toward (1 - thermal_resistance) to enforce physics.
    let raw_cond = (a.conductivity.value() + b.conductivity.value()) * 0.5;
    let thermal_cond = 1.0 - thermal_val;
    let conductivity_val = perturb(((raw_cond * 2.0) + thermal_cond) / 3.0, combined_seed, 3);

    // ── Toxicity: max — contamination is worst-case, not averaged ──
    let toxicity_val = perturb(a.toxicity.value().max(b.toxicity.value()), combined_seed, 4);

    // ── Reactive pair gets a hue shift so players notice the synergy ──
    let reactive = reactivity_val > 0.6;
    let color = blend_color(&a.color, &b.color, reactive);

    GameMaterial {
        name,
        seed: combined_seed,
        color,
        // Fabricated materials have no planet origin.
        origin_planet_seed: None,
        density: MaterialProperty::new(density_val, PropertyVisibility::Observable),
        thermal_resistance: MaterialProperty::new(thermal_val, PropertyVisibility::Hidden),
        reactivity: MaterialProperty::new(reactivity_val, PropertyVisibility::Hidden),
        conductivity: MaterialProperty::new(conductivity_val, PropertyVisibility::Hidden),
        toxicity: MaterialProperty::new(toxicity_val, PropertyVisibility::Hidden),
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    fn test_material(name: &str, seed: u64, density: f32) -> GameMaterial {
        let prop = |v: f32| MaterialProperty::new(v, PropertyVisibility::Hidden);
        GameMaterial {
            name: name.into(),
            seed,
            color: [0.5, 0.5, 0.5],
            origin_planet_seed: None,
            density: MaterialProperty::new(density, PropertyVisibility::Observable),
            thermal_resistance: prop(0.4),
            reactivity: prop(0.6),
            conductivity: prop(0.3),
            toxicity: prop(0.1),
        }
    }

    #[test]
    fn property_combine_output_is_deterministic() {
        let a = test_material("Ferrite", 100, 0.8);
        let b = test_material("Silite", 200, 0.2);
        let r1 = property_combine(&a, &b);
        let r2 = property_combine(&a, &b);
        assert_eq!(r1.seed, r2.seed);
        assert_eq!(r1.name, r2.name);
        assert!((r1.density.value() - r2.density.value()).abs() < f32::EPSILON);
        assert!(
            (r1.thermal_resistance.value() - r2.thermal_resistance.value()).abs() < f32::EPSILON
        );
    }

    #[test]
    fn property_combine_order_independent() {
        // Fabricator input slots are not semantic recipe operands. The output must
        // be identical no matter which slot receives which constituent material.
        let a = test_material("Alpha", 1, 0.8);
        let b = test_material("Beta", 2, 0.3);

        let ab = property_combine(&a, &b);
        let ba = property_combine(&b, &a);

        assert_eq!(ab.seed, ba.seed, "combined seed should ignore slot order");
        assert_eq!(ab.name, ba.name, "display name should ignore slot order");
        assert_eq!(ab.color, ba.color, "color should ignore slot order");
        assert_eq!(
            ab.origin_planet_seed, ba.origin_planet_seed,
            "origin metadata should ignore slot order"
        );

        assert_eq!(
            ab.density.value(),
            ba.density.value(),
            "density should ignore slot order"
        );
        assert_eq!(
            ab.density.visibility, ba.density.visibility,
            "density visibility should ignore slot order"
        );
        assert_eq!(
            ab.thermal_resistance.value(),
            ba.thermal_resistance.value(),
            "thermal resistance should ignore slot order"
        );
        assert_eq!(
            ab.thermal_resistance.visibility, ba.thermal_resistance.visibility,
            "thermal resistance visibility should ignore slot order"
        );
        assert_eq!(
            ab.reactivity.value(),
            ba.reactivity.value(),
            "reactivity should ignore slot order"
        );
        assert_eq!(
            ab.reactivity.visibility, ba.reactivity.visibility,
            "reactivity visibility should ignore slot order"
        );
        assert_eq!(
            ab.conductivity.value(),
            ba.conductivity.value(),
            "conductivity should ignore slot order"
        );
        assert_eq!(
            ab.conductivity.visibility, ba.conductivity.visibility,
            "conductivity visibility should ignore slot order"
        );
        assert_eq!(
            ab.toxicity.value(),
            ba.toxicity.value(),
            "toxicity should ignore slot order"
        );
        assert_eq!(
            ab.toxicity.visibility, ba.toxicity.visibility,
            "toxicity visibility should ignore slot order"
        );
    }

    #[test]
    fn density_dominated_by_denser_input() {
        // When one input is much denser, the output should skew toward it.
        let a = test_material("Heavy", 10, 0.9);
        let b = test_material("Light", 20, 0.1);
        let result = property_combine(&a, &b);
        // Density-weighted blend skews toward 0.9 (0.9^2/(0.9+0.1) = 0.81)
        assert!(
            result.density.value() > 0.5,
            "denser input should dominate: got {}",
            result.density.value()
        );
    }

    #[test]
    fn thermal_resistance_is_max_of_inputs() {
        let mut a = test_material("A", 10, 0.5);
        let mut b = test_material("B", 20, 0.5);
        a.thermal_resistance.set_value(0.3);
        b.thermal_resistance.set_value(0.8);
        let result = property_combine(&a, &b);
        // Should be near max(0.3, 0.8) = 0.8 (plus small perturbation)
        assert!(
            result.thermal_resistance.value() > 0.7,
            "thermal resistance should be near max: got {}",
            result.thermal_resistance.value()
        );
    }

    #[test]
    fn high_reactivity_pair_gets_hue_shift() {
        let mut a = test_material("A", 10, 0.5);
        let mut b = test_material("B", 20, 0.5);
        a.reactivity.set_value(0.9);
        b.reactivity.set_value(0.9);
        let result = property_combine(&a, &b);
        let plain_blend = [
            (a.color[0] + b.color[0]) * 0.5,
            (a.color[1] + b.color[1]) * 0.5,
            (a.color[2] + b.color[2]) * 0.5,
        ];
        assert_ne!(
            result.color, plain_blend,
            "high reactivity pair should shift hue"
        );
    }

    #[test]
    fn toxicity_is_worst_case() {
        let mut a = test_material("A", 10, 0.5);
        let mut b = test_material("B", 20, 0.5);
        a.toxicity.set_value(0.1);
        b.toxicity.set_value(0.9);
        let result = property_combine(&a, &b);
        assert!(
            result.toxicity.value() > 0.5,
            "toxicity should be near max: got {}",
            result.toxicity.value()
        );
    }

    #[test]
    fn procedural_name_deterministic() {
        assert_eq!(procedural_name(42), procedural_name(42));
    }

    #[test]
    fn procedural_name_varies_by_seed() {
        assert_ne!(procedural_name(1000), procedural_name(999_999));
    }

    #[test]
    fn seeded_noise_deterministic() {
        let a = seeded_noise(12345, 0);
        let b = seeded_noise(12345, 0);
        assert!((a - b).abs() < f32::EPSILON);
    }

    #[test]
    fn seeded_noise_varies_by_channel() {
        let a = seeded_noise(12345, 0);
        let b = seeded_noise(12345, 1);
        assert!(
            (a - b).abs() > f32::EPSILON,
            "different channels should produce different noise"
        );
    }

    #[test]
    fn fabricated_density_is_observable() {
        let a = test_material("A", 100, 0.8);
        let b = test_material("B", 200, 0.2);
        let result = property_combine(&a, &b);
        assert_eq!(result.density.visibility, PropertyVisibility::Observable);
        assert_eq!(
            result.thermal_resistance.visibility,
            PropertyVisibility::Hidden
        );
        assert_eq!(result.reactivity.visibility, PropertyVisibility::Hidden);
        assert_eq!(result.conductivity.visibility, PropertyVisibility::Hidden);
        assert_eq!(result.toxicity.visibility, PropertyVisibility::Hidden);
    }

    #[test]
    fn fabricated_conductivity_tracks_thermal_direction() {
        let mut a = test_material("A", 1, 0.5);
        let mut b = test_material("B", 2, 0.5);
        // Low thermal resistance → high thermal conductivity
        a.thermal_resistance.set_value(0.1);
        b.thermal_resistance.set_value(0.1);
        a.conductivity.set_value(0.2);
        b.conductivity.set_value(0.2);
        let result = property_combine(&a, &b);
        assert!(
            result.conductivity.value() > 0.3,
            "conductivity should track thermal conductivity (1 - low_thermal_resistance): got {}",
            result.conductivity.value()
        );
    }

    #[test]
    fn combined_seed_does_not_collide_with_biome_palette_seeds() {
        let palette_seeds: Vec<u64> = (1001..=1010).collect();
        let palette_set: std::collections::HashSet<u64> = palette_seeds.iter().copied().collect();

        for &a in &palette_seeds {
            for &b in &palette_seeds {
                let combined = combined_material_seed(a, b);
                assert!(
                    !palette_set.contains(&combined),
                    "fabrication of ({a}, {b}) → {combined} collides with a palette seed"
                );
            }
        }
    }

    #[test]
    fn fabricated_materials_register_cleanly_in_catalog() {
        use crate::materials::MaterialCatalog;
        let mut catalog = MaterialCatalog::default();
        let a = test_material("InputA", 1001, 0.4);
        let b = test_material("InputB", 1002, 0.6);
        let output = property_combine(&a, &b);
        let seed = output.seed;
        let density = output.density.value();
        let registered = catalog.register_fabricated(output);
        assert_eq!(registered.seed, seed);
        assert!((registered.density.value() - density).abs() < f32::EPSILON);
    }
}
