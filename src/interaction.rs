//! Interaction plugin — raycasting, pickup/place, examine, and crosshair UI.
//!
//! Provides the hands-on loop for material interaction: the player looks at
//! objects, picks them up, carries them, puts them down on surfaces, and
//! examines observable properties.
//!
//! Architecture follows the server-authoritative pattern: input systems emit
//! intent messages, separate systems process those intents and mutate state.
//!
//! Systems:
//! - `update_interaction_target`: raycast from camera center, track closest hit
//! - `emit_pickup_intent` / `emit_place_intent` / `emit_examine_intent`: input → messages
//! - `process_pickup`: pick up targeted material (re-parent to camera)
//! - `process_place`: place held material on nearest surface
//! - `process_examine`: toggle examine overlay for held material
//! - `update_held_position`: keep held item in front of camera
//! - `spawn_crosshair`: UI overlay at screen center
//! - `update_crosshair`: colour change when targeting an interactable

use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility};
use bevy::prelude::*;

use crate::carry::{CarryConfig, CarryState, ObserveWeight, StashHeldForPickup};
use crate::descriptions::{
    describe_color, describe_density, describe_thermal_observation, describe_value,
};
use crate::fabricator::{ActivateIntent, InputSlot, OutputSlot};
use crate::input::InputAction;
use crate::journal::{JournalKey, Observation, ObservationCategory, RecordObservation};
use crate::materials::{GameMaterial, MATERIAL_SURFACE_GAP, MaterialObject, PropertyVisibility};
use crate::observation::{ConfidenceLevel, ConfidenceTracker, PropertyName};
use crate::player::{Player, PlayerCamera, cursor_is_captured};
use crate::scene::{PlayerSceneConfig, Surface};
use crate::world_generation::{PlanetSurface, WorldGenerationConfig, WorldProfile};

use leafwing_input_manager::prelude::*;

const INTERACTION_RANGE: f32 = 3.0;

/// Plugin that handles object pickup, placement, and examination interactions.
pub struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<PickupIntent>()
            .add_message::<PlaceIntent>()
            .add_message::<ExamineIntent>()
            .init_resource::<InteractionTarget>()
            .init_resource::<SlotTarget>()
            .init_resource::<ExamineState>()
            .add_systems(
                Update,
                (
                    update_interaction_target,
                    update_slot_target,
                    emit_pickup_intent.after(update_interaction_target),
                    emit_place_intent.after(update_interaction_target),
                    emit_examine_intent.after(update_interaction_target),
                    emit_activate_intent,
                    process_pickup.after(emit_pickup_intent),
                    process_place
                        .after(emit_place_intent)
                        .after(update_slot_target),
                    process_examine.after(emit_examine_intent),
                    update_held_position.after(process_pickup),
                    update_crosshair
                        .after(update_interaction_target)
                        .after(update_slot_target),
                    update_examine_panel.after(process_examine),
                ),
            );
        app.add_systems(Startup, (spawn_crosshair, spawn_examine_panel));
    }
}

// ── Intent messages (client → server boundary) ───────────────────────────

#[derive(Message)]
struct PickupIntent;

#[derive(Message)]
struct PlaceIntent;

#[derive(Message)]
struct ExamineIntent;

// ── State ────────────────────────────────────────────────────────────────

/// Tracks what the player's center-screen ray is currently hitting (material objects).
#[derive(Resource, Default)]
struct InteractionTarget {
    entity: Option<Entity>,
}

/// Tracks whether the player's ray is hitting a fabricator input slot.
#[derive(Resource, Default)]
struct SlotTarget {
    entity: Option<Entity>,
}

/// Marks a material entity as currently held by the player.
#[derive(Component)]
pub struct HeldItem;

/// Whether the examine overlay is currently visible.
#[derive(Resource, Default)]
struct ExamineState {
    visible: bool,
}

// ── UI markers ───────────────────────────────────────────────────────────

#[derive(Component)]
struct Crosshair;

#[derive(Component)]
struct ExaminePanel;

#[derive(Component)]
struct ExamineText;

// ── Crosshair setup ──────────────────────────────────────────────────────

fn spawn_crosshair(mut commands: Commands) {
    commands
        .spawn(Node {
            width: Val::Percent(100.0),
            height: Val::Percent(100.0),
            justify_content: JustifyContent::Center,
            align_items: AlignItems::Center,
            ..default()
        })
        .with_children(|parent| {
            parent.spawn((
                Crosshair,
                Text::new("·"),
                TextFont {
                    font_size: 32.0,
                    ..default()
                },
                TextColor(Color::srgba(1.0, 1.0, 1.0, 0.6)),
            ));
        });
}

// ── Raycast ──────────────────────────────────────────────────────────────

fn update_interaction_target(
    mut target: ResMut<InteractionTarget>,
    camera_query: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut ray_cast: MeshRayCast,
    material_query: Query<&GameMaterial, With<MaterialObject>>,
    held_query: Query<(), With<HeldItem>>,
    mut encounter_writer: MessageWriter<RecordObservation>,
) {
    let previous_target = target.entity;
    target.entity = None;

    let Ok((camera, cam_gtf)) = camera_query.single() else {
        return;
    };

    let Some(viewport_size) = camera.logical_viewport_size() else {
        return;
    };
    let center = viewport_size * 0.5;

    let Ok(ray) = camera.viewport_to_world(cam_gtf, center) else {
        return;
    };

    let filter = |entity: Entity| material_query.contains(entity) && !held_query.contains(entity);
    let settings = MeshRayCastSettings::default()
        .with_filter(&filter)
        .with_visibility(RayCastVisibility::Any);

    let hits = ray_cast.cast_ray(ray, &settings);

    if let Some(&(entity, ref hit)) = hits.first()
        && hit.distance <= INTERACTION_RANGE
    {
        target.entity = Some(entity);
    }

    if target.entity != previous_target
        && let Some(entity) = target.entity
        && let Ok(material) = material_query.get(entity)
    {
        encounter_writer.write(RecordObservation {
            key: JournalKey::Material {
                seed: material.seed,
                // Planet seed is automatically resolved by the journal
                // ingestion system from the current WorldProfile resource.
                // This eliminates the need for manual extraction and
                // prevents silent failures when observation sites forget
                // the extraction pattern.
                planet_seed: None,
            },
            name: material.name.clone(),
            observation: Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: format!(
                    "Color: {}\nWeight: {}",
                    describe_color(&material.color),
                    describe_density(material.density.value)
                ),
                recorded_at: 0,
            },
        });
    }
}

// ── Slot raycast ────────────────────────────────────────────────────────

fn update_slot_target(
    mut target: ResMut<SlotTarget>,
    camera_query: Query<(&Camera, &GlobalTransform), With<PlayerCamera>>,
    mut ray_cast: MeshRayCast,
    slot_query: Query<(), With<InputSlot>>,
) {
    target.entity = None;

    let Ok((camera, cam_gtf)) = camera_query.single() else {
        return;
    };

    let Some(viewport_size) = camera.logical_viewport_size() else {
        return;
    };
    let center = viewport_size * 0.5;

    let Ok(ray) = camera.viewport_to_world(cam_gtf, center) else {
        return;
    };

    let filter = |entity: Entity| slot_query.contains(entity);
    let settings = MeshRayCastSettings::default()
        .with_filter(&filter)
        .with_visibility(RayCastVisibility::Any);

    let hits = ray_cast.cast_ray(ray, &settings);

    if let Some(&(entity, ref hit)) = hits.first()
        && hit.distance <= INTERACTION_RANGE
    {
        target.entity = Some(entity);
    }
}

// ── Intent emission ──────────────────────────────────────────────────────

fn should_emit_pickup(interact_pressed: bool, targeting_material: bool) -> bool {
    interact_pressed && targeting_material
}

fn should_emit_place(
    interact_pressed: bool,
    place_pressed: bool,
    holding: bool,
    targeting_material: bool,
) -> bool {
    holding && (place_pressed || (interact_pressed && !targeting_material))
}

fn emit_pickup_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    target: Res<InteractionTarget>,
    mut writer: MessageWriter<PickupIntent>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }
    let Ok(action) = player_query.single() else {
        return;
    };
    if should_emit_pickup(
        action.just_pressed(&InputAction::Interact),
        target.entity.is_some(),
    ) {
        writer.write(PickupIntent);
    }
}

fn emit_place_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    target: Res<InteractionTarget>,
    held_query: Query<(), With<HeldItem>>,
    mut writer: MessageWriter<PlaceIntent>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }
    let holding = held_query.iter().next().is_some();
    let Ok(action) = player_query.single() else {
        return;
    };
    if should_emit_place(
        action.just_pressed(&InputAction::Interact),
        action.just_pressed(&InputAction::Place),
        holding,
        target.entity.is_some(),
    ) {
        writer.write(PlaceIntent);
    }
}

fn emit_activate_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut writer: MessageWriter<ActivateIntent>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }
    let Ok(action) = player_query.single() else {
        return;
    };
    if action.just_pressed(&InputAction::Activate) {
        writer.write(ActivateIntent);
    }
}

// ── Server-side processing ───────────────────────────────────────────────

#[allow(clippy::too_many_arguments)]
fn process_pickup(
    mut commands: Commands,
    mut reader: MessageReader<PickupIntent>,
    target: Res<InteractionTarget>,
    carry_config: Res<CarryConfig>,
    mut stash_writer: MessageWriter<StashHeldForPickup>,
    mut observe_writer: MessageWriter<ObserveWeight>,
    player_query: Query<&CarryState, With<Player>>,
    held_query: Query<(Entity, &GameMaterial), With<HeldItem>>,
    material_query: Query<&GameMaterial, With<MaterialObject>>,
    camera_query: Query<Entity, With<PlayerCamera>>,
    mut input_slots: Query<&mut InputSlot>,
    mut output_slots: Query<&mut OutputSlot>,
) {
    for _intent in reader.read() {
        let Some(target_entity) = target.entity else {
            continue;
        };

        let Ok(camera_entity) = camera_query.single() else {
            continue;
        };
        let Ok(carry_state) = player_query.single() else {
            continue;
        };

        let Ok(target_material) = material_query.get(target_entity) else {
            continue;
        };

        if let Some((held_entity, held_material)) = held_query.iter().next() {
            if !carry_state.can_stash(held_material) {
                continue;
            }
            // Carry module handles stash mutation + weight observations for both items.
            stash_writer.write(StashHeldForPickup {
                held_entity,
                held_material: held_material.clone(),
                picked_material: target_material.clone(),
            });
        } else {
            // No held item — just observe weight for the pickup target.
            observe_writer.write(ObserveWeight {
                material: target_material.clone(),
            });
        }

        // Fabricator slot state must follow the actual object in the world.
        for mut slot in &mut input_slots {
            clear_input_slot_reference(&mut slot, target_entity);
        }
        for mut output in &mut output_slots {
            clear_output_slot_reference(&mut output, target_entity);
        }

        commands
            .entity(target_entity)
            .remove::<MaterialObject>()
            .insert(HeldItem)
            .set_parent_in_place(camera_entity)
            .insert(Transform::from_translation(carry_config.hold_offset_vec3()));
    }
}

/// Small gap above the surface so objects sit on top without z-fighting.
/// Maximum XZ distance from the ray's intersection to a surface center for
/// placement to be considered valid.
const PLACE_REACH: f32 = 2.5;

#[derive(Clone, Copy, Debug, PartialEq)]
struct OccupiedMaterialFootprint {
    entity: Entity,
    position: Vec3,
    radius: f32,
}

fn clear_input_slot_reference(slot: &mut InputSlot, entity: Entity) {
    if slot.material == Some(entity) {
        slot.material = None;
    }
}

fn clear_output_slot_reference(slot: &mut OutputSlot, entity: Entity) {
    if slot.material == Some(entity) {
        slot.material = None;
    }
}

// Bevy system — parameter count is driven by ECS query requirements, not design smell.
#[allow(clippy::too_many_arguments)]
fn process_place(
    mut commands: Commands,
    mut reader: MessageReader<PlaceIntent>,
    held_query: Query<(Entity, &GameMaterial), With<HeldItem>>,
    camera_query: Query<&GlobalTransform, With<PlayerCamera>>,
    player_query: Query<&GlobalTransform, With<Player>>,
    surfaces: Query<(Entity, &GlobalTransform, &Surface)>,
    material_positions: Query<(Entity, &GlobalTransform, &GameMaterial), With<MaterialObject>>,
    slot_target: Res<SlotTarget>,
    mut slot_query: Query<(&GlobalTransform, &mut InputSlot)>,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    surface_registry: Res<crate::surface::SurfaceOverrideRegistry>,
    scene: Res<PlayerSceneConfig>,
) {
    let Some(world_profile) = world_profile else {
        return;
    };
    for _intent in reader.read() {
        let Some((held_entity, held_material)) = held_query.iter().next() else {
            continue;
        };
        let occupied = collect_material_footprints(&material_positions);

        // Priority: if looking at an empty input slot, seat the material there.
        if let Some(slot_entity) = slot_target.entity
            && let Ok((slot_gtf, mut slot)) = slot_query.get_mut(slot_entity)
            && slot.material.is_none()
        {
            let slot_pos = slot_gtf.translation();
            slot.material = Some(held_entity);

            commands
                .entity(held_entity)
                .remove::<HeldItem>()
                .insert(MaterialObject)
                .remove_parent_in_place()
                .insert(Transform::from_xyz(
                    slot_pos.x,
                    slot.top_y + held_material.support_height() + MATERIAL_SURFACE_GAP,
                    slot_pos.z,
                ));
            continue;
        }

        // Fallback: place on the nearest surface the player is looking at.
        let Ok(cam_gtf) = camera_query.single() else {
            continue;
        };
        let Ok(player_gtf) = player_query.single() else {
            continue;
        };

        let planet_surface = PlanetSurface::new_from_profile(&world_profile, &world_gen_config);

        commands
            .entity(held_entity)
            .remove::<HeldItem>()
            .insert(MaterialObject)
            .remove_parent_in_place();

        let drop_position = if let Some((_entity, surface_gtf, surface)) =
            best_surface_for_ray(cam_gtf, &surfaces)
        {
            let surface_pos = surface_gtf.translation();
            let cam_pos = cam_gtf.translation();
            let cam_fwd = *cam_gtf.forward();

            let hit = ray_horizontal_intersection(cam_pos, cam_fwd, surface_pos.y);
            let place_x = hit.map_or(surface_pos.x, |p| {
                p.x.clamp(
                    surface_pos.x - surface.half_extent_x,
                    surface_pos.x + surface.half_extent_x,
                )
            });
            let place_z = hit.map_or(surface_pos.z, |p| {
                p.z.clamp(
                    surface_pos.z - surface.half_extent_z,
                    surface_pos.z + surface.half_extent_z,
                )
            });
            let candidate = Vec3::new(
                place_x,
                held_material.resting_center_y(surface_pos.y),
                place_z,
            );
            if can_place_material(held_entity, held_material, candidate, &occupied) {
                candidate
            } else {
                floor_drop_position(
                    player_gtf,
                    held_entity,
                    held_material,
                    &occupied,
                    &planet_surface,
                    &surface_registry,
                    scene.drop_surface_reach,
                )
            }
        } else {
            floor_drop_position(
                player_gtf,
                held_entity,
                held_material,
                &occupied,
                &planet_surface,
                &surface_registry,
                scene.drop_surface_reach,
            )
        };

        commands
            .entity(held_entity)
            .insert(Transform::from_translation(drop_position));
    }
}

/// Intersect a ray with the horizontal plane at the given Y.
/// Returns `None` if the ray is parallel or the intersection is behind the origin.
fn ray_horizontal_intersection(origin: Vec3, direction: Vec3, plane_y: f32) -> Option<Vec3> {
    if direction.y.abs() < 1e-6 {
        return None;
    }
    let t = (plane_y - origin.y) / direction.y;
    if t < 0.0 {
        return None;
    }
    Some(origin + direction * t)
}

/// Pick the surface the player is most likely aiming at by intersecting the
/// camera ray with each surface's horizontal plane and choosing the one whose
/// center is closest to the intersection point.
fn best_surface_for_ray<'a>(
    cam_gtf: &GlobalTransform,
    surfaces: &'a Query<(Entity, &GlobalTransform, &Surface)>,
) -> Option<(Entity, &'a GlobalTransform, &'a Surface)> {
    let origin = cam_gtf.translation();
    let direction = *cam_gtf.forward();

    surfaces
        .iter()
        .filter_map(|(entity, sgtf, surface)| {
            let s_pos = sgtf.translation();
            let hit = ray_horizontal_intersection(origin, direction, s_pos.y)?;
            let dx = hit.x - s_pos.x;
            let dz = hit.z - s_pos.z;
            if dx.abs() > surface.half_extent_x || dz.abs() > surface.half_extent_z {
                return None;
            }
            let xz_dist = Vec2::new(dx, dz).length();
            if xz_dist > PLACE_REACH {
                return None;
            }
            Some((entity, sgtf, surface, xz_dist))
        })
        .min_by(|a, b| a.3.partial_cmp(&b.3).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(entity, sgtf, surface, _)| (entity, sgtf, surface))
}

fn collect_material_footprints(
    material_positions: &Query<(Entity, &GlobalTransform, &GameMaterial), With<MaterialObject>>,
) -> Vec<OccupiedMaterialFootprint> {
    material_positions
        .iter()
        .map(|(entity, gtf, material)| OccupiedMaterialFootprint {
            entity,
            position: gtf.translation(),
            radius: material.footprint_radius(),
        })
        .collect()
}

fn can_place_material(
    held_entity: Entity,
    held_material: &GameMaterial,
    candidate: Vec3,
    occupied: &[OccupiedMaterialFootprint],
) -> bool {
    let held_radius = held_material.footprint_radius();
    occupied.iter().all(|footprint| {
        if footprint.entity == held_entity {
            return true;
        }

        let same_level = (footprint.position.y - candidate.y).abs() < 0.25;
        if !same_level {
            return true;
        }

        let required_gap = held_radius + footprint.radius;
        let xz_dist = Vec2::new(
            footprint.position.x - candidate.x,
            footprint.position.z - candidate.z,
        )
        .length();
        xz_dist >= required_gap
    })
}

fn floor_drop_position(
    player_gtf: &GlobalTransform,
    held_entity: Entity,
    material: &GameMaterial,
    occupied: &[OccupiedMaterialFootprint],
    surface: &PlanetSurface,
    surface_registry: &crate::surface::SurfaceOverrideRegistry,
    drop_surface_reach: f32,
) -> Vec3 {
    let origin = player_gtf.translation();
    let forward = *player_gtf.forward();
    let right = *player_gtf.right();
    let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let right_xz = Vec3::new(right.x, 0.0, right.z).normalize_or_zero();
    let fallback_forward = if forward_xz == Vec3::ZERO {
        Vec3::NEG_Z
    } else {
        forward_xz
    };
    let fallback_right = if right_xz == Vec3::ZERO {
        Vec3::X
    } else {
        right_xz
    };
    let feet_y = origin.y - 1.7; // approximate feet from eye position
    let max_y = feet_y + drop_surface_reach;
    let terrain_y = surface.sample_elevation(origin.x, origin.z);
    let standing_y = crate::surface::resolve_standing_surface(
        origin.x,
        origin.z,
        max_y,
        terrain_y,
        surface_registry,
    );
    let base = Vec3::new(origin.x, material.resting_center_y(standing_y), origin.z);
    let forward_steps = [0.35_f32, 0.6, 0.85, 1.1];
    let lateral_steps = [0.0_f32, -0.35, 0.35, -0.7, 0.7];

    for forward_step in forward_steps {
        for lateral_step in lateral_steps {
            let candidate_xz =
                base + fallback_forward * forward_step + fallback_right * lateral_step;
            let candidate_terrain_y = surface.sample_elevation(candidate_xz.x, candidate_xz.z);
            let candidate_standing_y = crate::surface::resolve_standing_surface(
                candidate_xz.x,
                candidate_xz.z,
                max_y,
                candidate_terrain_y,
                surface_registry,
            );
            let candidate = Vec3::new(
                candidate_xz.x,
                material.resting_center_y(candidate_standing_y),
                candidate_xz.z,
            );
            if can_place_material(held_entity, material, candidate, occupied) {
                return candidate;
            }
        }
    }

    let fallback_xz = base + fallback_forward * 0.35;
    let fallback_terrain_y = surface.sample_elevation(fallback_xz.x, fallback_xz.z);
    let fallback_standing_y = crate::surface::resolve_standing_surface(
        fallback_xz.x,
        fallback_xz.z,
        max_y,
        fallback_terrain_y,
        surface_registry,
    );
    Vec3::new(
        fallback_xz.x,
        material.resting_center_y(fallback_standing_y),
        fallback_xz.z,
    )
}

// ── Held item tracking ───────────────────────────────────────────────────

fn update_held_position(
    carry_config: Res<CarryConfig>,
    mut held_query: Query<&mut Transform, With<HeldItem>>,
) {
    let offset = carry_config.hold_offset_vec3();
    for mut tf in &mut held_query {
        tf.translation = offset;
        tf.rotation = Quat::IDENTITY;
    }
}

// ── Crosshair feedback ──────────────────────────────────────────────────

fn update_crosshair(
    target: Res<InteractionTarget>,
    slot_target: Res<SlotTarget>,
    held_query: Query<(), With<HeldItem>>,
    mut crosshair_query: Query<&mut TextColor, With<Crosshair>>,
) {
    let Ok(mut color) = crosshair_query.single_mut() else {
        return;
    };

    let holding = held_query.iter().next().is_some();
    let targeting_material = target.entity.is_some();
    let targeting_slot = slot_target.entity.is_some();

    color.0 = if targeting_slot && holding {
        // Slot + holding → ready to place into slot (cyan).
        Color::srgba(0.3, 0.9, 1.0, 0.95)
    } else if targeting_material {
        Color::srgba(0.2, 1.0, 0.4, 0.9)
    } else if holding {
        Color::srgba(1.0, 0.85, 0.3, 0.8)
    } else {
        Color::srgba(1.0, 1.0, 1.0, 0.6)
    };
}

// ── Examine panel setup ─────────────────────────────────────────────────

fn spawn_examine_panel(mut commands: Commands) {
    commands
        .spawn((
            ExaminePanel,
            Node {
                position_type: PositionType::Absolute,
                bottom: Val::Px(80.0),
                left: Val::Percent(50.0),
                padding: UiRect::all(Val::Px(14.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.08, 0.08, 0.12, 0.85)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                ExamineText,
                Text::new(""),
                TextFont {
                    font_size: 18.0,
                    ..default()
                },
                TextColor(Color::srgba(0.9, 0.92, 0.88, 1.0)),
            ));
        });
}

// ── Examine intent ──────────────────────────────────────────────────────

fn emit_examine_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut writer: MessageWriter<ExamineIntent>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }
    let Ok(action) = player_query.single() else {
        return;
    };
    if action.just_pressed(&InputAction::Examine) {
        writer.write(ExamineIntent);
    }
}

// ── Examine processing ──────────────────────────────────────────────────

fn process_examine(
    mut reader: MessageReader<ExamineIntent>,
    mut state: ResMut<ExamineState>,
    target: Res<InteractionTarget>,
    held_query: Query<&GameMaterial, With<HeldItem>>,
    material_query: Query<&GameMaterial, With<MaterialObject>>,
) {
    for _intent in reader.read() {
        let held_material = held_query.iter().next();
        let targeted_material = target
            .entity
            .and_then(|entity| material_query.get(entity).ok());

        if held_material.or(targeted_material).is_some() {
            state.visible = !state.visible;
        } else {
            state.visible = false;
        }
    }
}

fn update_examine_panel(
    state: Res<ExamineState>,
    target: Res<InteractionTarget>,
    tracker: Res<ConfidenceTracker>,
    held_query: Query<&GameMaterial, With<HeldItem>>,
    material_query: Query<&GameMaterial, With<MaterialObject>>,
    mut panel_query: Query<&mut Visibility, With<ExaminePanel>>,
    mut text_query: Query<&mut Text, With<ExamineText>>,
) {
    let Ok(mut vis) = panel_query.single_mut() else {
        return;
    };
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    if !state.visible {
        *vis = Visibility::Hidden;
        return;
    }

    // Prefer held item; fall back to whatever the player is looking at.
    let mat = held_query
        .iter()
        .next()
        .or_else(|| target.entity.and_then(|e| material_query.get(e).ok()));

    let Some(mat) = mat else {
        *vis = Visibility::Hidden;
        return;
    };

    *vis = Visibility::Visible;
    text.0 = build_examine_text(mat, &tracker);
}

fn build_examine_text(mat: &GameMaterial, tracker: &ConfidenceTracker) -> String {
    let mut lines = vec![mat.name.clone()];
    lines.push(String::new());

    append_prop(&mut lines, "Weight", &mat.density, describe_density);
    append_thermal_prop(&mut lines, mat, tracker);
    append_prop(&mut lines, "Reactivity", &mat.reactivity, describe_value);
    append_prop(
        &mut lines,
        "Conductivity",
        &mat.conductivity,
        describe_value,
    );
    append_prop(&mut lines, "Toxicity", &mat.toxicity, describe_value);

    lines.join("\n")
}

fn append_prop(
    lines: &mut Vec<String>,
    label: &str,
    prop: &crate::materials::MaterialProperty,
    describer: fn(f32) -> &'static str,
) {
    if prop.visibility == PropertyVisibility::Observable
        || prop.visibility == PropertyVisibility::Revealed
    {
        lines.push(format!("{label}: {}", describer(prop.value)));
    } else {
        lines.push(format!("{label}: ???"));
    }
}

fn append_thermal_prop(lines: &mut Vec<String>, mat: &GameMaterial, tracker: &ConfidenceTracker) {
    let seed_has_thermal_knowledge = tracker.count(mat.seed, PropertyName::ThermalResistance) > 0;
    match mat.thermal_resistance.visibility {
        PropertyVisibility::Hidden if !seed_has_thermal_knowledge => {
            lines.push("Heat response: ???".to_string())
        }
        PropertyVisibility::Hidden
        | PropertyVisibility::Observable
        | PropertyVisibility::Revealed => {
            let confidence = tracker.level(mat.seed, PropertyName::ThermalResistance);
            let description =
                describe_thermal_observation(mat.thermal_resistance.value, confidence);
            lines.push(format!("Heat response: {description}"));
        }
    }
}

// ── Tests ────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::MaterialProperty;
    use crate::scene::SceneConfig;

    /// A flat surface at y=0 for unit tests that don't care about terrain.
    fn flat_surface() -> PlanetSurface {
        PlanetSurface {
            elevation_seed: 0,
            base_y: 0.0,
            amplitude: 0.0,
            frequency: 1.0,
            octaves: 1,
            detail_weight: 0.0,
            detail_seed: 0,
            detail_frequency: 1.0,
            detail_octaves: 1,
            planet_surface_diameter: 10,
            chunk_size_world_units: 45.0,
        }
    }

    fn test_material() -> GameMaterial {
        GameMaterial {
            name: "TestMat".into(),
            seed: 1,
            color: [0.5, 0.5, 0.5],
            density: MaterialProperty {
                value: 0.78,
                visibility: PropertyVisibility::Observable,
            },
            thermal_resistance: MaterialProperty {
                value: 0.65,
                visibility: PropertyVisibility::Hidden,
            },
            reactivity: MaterialProperty {
                value: 0.35,
                visibility: PropertyVisibility::Hidden,
            },
            conductivity: MaterialProperty {
                value: 0.72,
                visibility: PropertyVisibility::Revealed,
            },
            toxicity: MaterialProperty {
                value: 0.05,
                visibility: PropertyVisibility::Hidden,
            },
        }
    }

    #[test]
    fn examine_text_shows_observable_and_revealed_hides_hidden() {
        let mat = test_material();
        let tracker = ConfidenceTracker::default();
        let text = build_examine_text(&mat, &tracker);

        assert!(text.contains("TestMat"));
        assert!(text.contains("Weight: Very heavy"));
        assert!(text.contains("Conductivity: Very high"));
        assert!(text.contains("Heat response: ???"));
        assert!(text.contains("Reactivity: ???"));
        assert!(text.contains("Toxicity: ???"));
    }

    #[test]
    fn examine_text_name_is_first_line() {
        let mat = test_material();
        let tracker = ConfidenceTracker::default();
        let text = build_examine_text(&mat, &tracker);
        let first_line = text.lines().next().unwrap();
        assert_eq!(first_line, "TestMat");
    }

    #[test]
    fn examine_text_uses_confidence_language_for_revealed_heat_response() {
        let mut mat = test_material();
        mat.thermal_resistance.visibility = PropertyVisibility::Revealed;

        let mut tracker = ConfidenceTracker::default();
        let tentative = build_examine_text(&mat, &tracker);
        assert!(tentative.contains("Heat response: Seemed to hold together under heat"));

        tracker.record(mat.seed, PropertyName::ThermalResistance);
        tracker.record(mat.seed, PropertyName::ThermalResistance);
        let observed = build_examine_text(&mat, &tracker);
        assert!(observed.contains("Heat response: Hold together under heat"));

        tracker.record(mat.seed, PropertyName::ThermalResistance);
        tracker.record(mat.seed, PropertyName::ThermalResistance);
        let confident = build_examine_text(&mat, &tracker);
        assert!(confident.contains("Heat response: Reliably hold together under heat"));
    }

    #[test]
    fn examine_text_uses_seed_level_thermal_knowledge_even_if_entity_is_hidden() {
        let mat = test_material();
        let mut tracker = ConfidenceTracker::default();
        tracker.record(mat.seed, PropertyName::ThermalResistance);

        let text = build_examine_text(&mat, &tracker);
        assert!(!text.contains("Heat response: ???"));
        assert!(text.contains("Heat response: Seemed to hold together under heat"));
    }

    #[test]
    fn interact_picks_up_only_when_not_holding() {
        assert!(should_emit_pickup(true, true));
        assert!(!should_emit_pickup(false, true));
        assert!(!should_emit_pickup(true, false));
    }

    #[test]
    fn interact_or_place_can_drop_when_holding() {
        assert!(should_emit_place(true, false, true, false));
        assert!(should_emit_place(false, true, true, false));
        assert!(!should_emit_place(true, false, true, true));
        assert!(!should_emit_place(false, false, true, false));
        assert!(!should_emit_place(true, false, false, false));
    }

    #[test]
    fn floor_drop_position_does_not_snap_back_into_room_bounds() {
        let player = GlobalTransform::from(Transform::from_xyz(100.0, 1.7, 100.0));
        let material = test_material();
        let surface = flat_surface();
        let registry = crate::surface::SurfaceOverrideRegistry::default();
        let dropped = floor_drop_position(
            &player,
            Entity::from_bits(99),
            &material,
            &[],
            &surface,
            &registry,
            1.5,
        );

        assert!(dropped.x > 4.0);
        assert!(dropped.z > 4.0);
        assert!((dropped.y - material.resting_center_y(0.0)).abs() < f32::EPSILON);
    }

    #[test]
    fn floor_drop_position_uses_player_floor_position() {
        let player = GlobalTransform::from(Transform::from_xyz(1.25, 1.7, -0.75));
        let material = test_material();
        let surface = flat_surface();
        let registry = crate::surface::SurfaceOverrideRegistry::default();
        let dropped = floor_drop_position(
            &player,
            Entity::from_bits(99),
            &material,
            &[],
            &surface,
            &registry,
            1.5,
        );

        assert!((dropped.x - 1.25).abs() < f32::EPSILON);
        assert!((dropped.z - (-1.10)).abs() < f32::EPSILON);
    }

    #[test]
    fn pickup_then_place_returns_item_to_same_slot_spot() {
        let mut app = App::new();
        app.add_message::<PickupIntent>()
            .add_message::<PlaceIntent>()
            .add_message::<StashHeldForPickup>()
            .add_message::<ObserveWeight>()
            .insert_resource(InteractionTarget::default())
            .insert_resource(SlotTarget::default())
            .insert_resource(SceneConfig::default())
            .insert_resource(WorldProfile::from_config(&WorldGenerationConfig::default()).unwrap())
            .insert_resource(WorldGenerationConfig::default())
            .insert_resource(crate::surface::SurfaceOverrideRegistry::default())
            .insert_resource(PlayerSceneConfig::default())
            .insert_resource(CarryConfig::default())
            .add_systems(Update, (process_pickup, process_place));

        let camera = app
            .world_mut()
            .spawn((PlayerCamera, GlobalTransform::default()))
            .id();

        // process_pickup queries for a Player entity with CarryState.
        // Without this, the pickup silently no-ops because the query returns Err.
        app.world_mut().spawn((
            Player,
            CarryState::new(100.0, false),
            Transform::default(),
            GlobalTransform::default(),
        ));

        let slot_pos = Vec3::ZERO;
        let slot_entity = app
            .world_mut()
            .spawn((
                InputSlot {
                    index: 0,
                    material: None,
                    top_y: slot_pos.y,
                },
                Transform::from_translation(slot_pos),
                GlobalTransform::from_translation(slot_pos),
            ))
            .id();

        let mat = test_material();
        let start_pos = Vec3::new(
            slot_pos.x,
            slot_pos.y + mat.support_height() + MATERIAL_SURFACE_GAP,
            slot_pos.z,
        );
        let item = app
            .world_mut()
            .spawn((
                MaterialObject,
                mat,
                Transform::from_translation(start_pos),
                GlobalTransform::from_translation(start_pos),
            ))
            .id();

        app.world_mut().resource_mut::<InteractionTarget>().entity = Some(item);
        app.world_mut().write_message(PickupIntent);
        app.update();

        assert!(app.world().entity(item).contains::<HeldItem>());
        assert!(app.world().get::<ChildOf>(item).is_some());

        app.world_mut().resource_mut::<SlotTarget>().entity = Some(slot_entity);
        app.world_mut().write_message(PlaceIntent);
        app.update();

        assert!(!app.world().entity(item).contains::<HeldItem>());
        assert!(app.world().get::<ChildOf>(item).is_none());

        let slot = app.world().get::<InputSlot>(slot_entity).unwrap();
        assert_eq!(slot.material, Some(item));

        let end_pos = app.world().get::<Transform>(item).unwrap().translation;
        let epsilon = 1e-4;
        assert!((end_pos.x - start_pos.x).abs() <= epsilon);
        assert!((end_pos.z - start_pos.z).abs() <= epsilon);
        assert!((end_pos.y - start_pos.y).abs() <= epsilon);

        // Keep at least one sanity-check that pickup parented to the camera.
        let _ = camera;
    }

    #[test]
    fn floor_drop_position_spreads_away_from_occupied_drop_points() {
        let player = GlobalTransform::from(Transform::from_xyz(1.25, 1.7, -0.75));
        let material = test_material();
        let occupied = [OccupiedMaterialFootprint {
            entity: Entity::from_bits(1),
            position: Vec3::new(1.25, material.resting_center_y(0.0), -1.10),
            radius: material.footprint_radius(),
        }];
        let dropped = floor_drop_position(
            &player,
            Entity::from_bits(99),
            &material,
            &occupied,
            &flat_surface(),
            &crate::surface::SurfaceOverrideRegistry::default(),
            1.5,
        );

        assert_ne!(
            dropped,
            Vec3::new(1.25, material.resting_center_y(0.0), -1.10)
        );
    }

    #[test]
    fn clearing_input_slot_reference_only_removes_matching_entity() {
        let target = Entity::from_bits(7);
        let mut slot = InputSlot {
            index: 0,
            material: Some(target),
            top_y: 1.0,
        };
        clear_input_slot_reference(&mut slot, target);
        assert_eq!(slot.material, None);

        let other = Entity::from_bits(8);
        let mut slot = InputSlot {
            index: 1,
            material: Some(other),
            top_y: 1.0,
        };
        clear_input_slot_reference(&mut slot, target);
        assert_eq!(slot.material, Some(other));
    }

    #[test]
    fn clearing_output_slot_reference_only_removes_matching_entity() {
        let target = Entity::from_bits(11);
        let mut slot = OutputSlot {
            material: Some(target),
            top_y: 1.0,
        };
        clear_output_slot_reference(&mut slot, target);
        assert_eq!(slot.material, None);

        let other = Entity::from_bits(12);
        let mut slot = OutputSlot {
            material: Some(other),
            top_y: 1.0,
        };
        clear_output_slot_reference(&mut slot, target);
        assert_eq!(slot.material, Some(other));
    }

    // ── Tests for pure utility functions ──────────────────────────────────

    #[test]
    fn ray_horizontal_intersection_basic_case() {
        let origin = Vec3::new(0.0, 5.0, 0.0);
        let direction = Vec3::new(0.0, -1.0, 0.0); // pointing down
        let plane_y = 0.0;
        
        let result = ray_horizontal_intersection(origin, direction, plane_y);
        assert_eq!(result, Some(Vec3::new(0.0, 0.0, 0.0)));
    }

    #[test]
    fn ray_horizontal_intersection_diagonal_ray() {
        let origin = Vec3::new(1.0, 2.0, 1.0);
        let direction = Vec3::new(1.0, -1.0, 1.0).normalize(); // diagonal down
        let plane_y = 0.0;
        
        let result = ray_horizontal_intersection(origin, direction, plane_y).unwrap();
        assert!((result.y - 0.0).abs() < f32::EPSILON);
        assert!(result.x > 1.0); // moved forward in x
        assert!(result.z > 1.0); // moved forward in z
    }

    #[test]
    fn ray_horizontal_intersection_horizontal_ray_returns_none() {
        let origin = Vec3::new(0.0, 5.0, 0.0);
        let direction = Vec3::new(1.0, 0.0, 0.0); // horizontal
        let plane_y = 0.0;
        
        let result = ray_horizontal_intersection(origin, direction, plane_y);
        assert_eq!(result, None);
    }

    #[test]
    fn ray_horizontal_intersection_nearly_horizontal_returns_none() {
        let origin = Vec3::new(0.0, 5.0, 0.0);
        let direction = Vec3::new(1.0, 1e-7, 0.0); // nearly horizontal
        let plane_y = 0.0;
        
        let result = ray_horizontal_intersection(origin, direction, plane_y);
        assert_eq!(result, None);
    }

    #[test]
    fn ray_horizontal_intersection_upward_ray_returns_none() {
        let origin = Vec3::new(0.0, 5.0, 0.0);
        let direction = Vec3::new(0.0, 1.0, 0.0); // pointing up
        let plane_y = 0.0; // plane below origin
        
        let result = ray_horizontal_intersection(origin, direction, plane_y);
        assert_eq!(result, None); // t would be negative
    }

    #[test]
    fn ray_horizontal_intersection_plane_above_origin() {
        let origin = Vec3::new(0.0, 5.0, 0.0);
        let direction = Vec3::new(0.0, 1.0, 0.0); // pointing up
        let plane_y = 10.0;
        
        let result = ray_horizontal_intersection(origin, direction, plane_y);
        assert_eq!(result, Some(Vec3::new(0.0, 10.0, 0.0)));
    }

    #[test]
    fn should_emit_pickup_logic() {
        // Both conditions must be true
        assert!(should_emit_pickup(true, true));
        
        // Missing either condition should fail
        assert!(!should_emit_pickup(false, true));
        assert!(!should_emit_pickup(true, false));
        assert!(!should_emit_pickup(false, false));
    }

    #[test]
    fn should_emit_place_logic() {
        // Place pressed while holding (regardless of targeting)
        assert!(should_emit_place(false, true, true, false));
        assert!(should_emit_place(false, true, true, true));
        
        // Interact pressed while holding and not targeting material
        assert!(should_emit_place(true, false, true, false));
        
        // Should not place when interact pressed while targeting material
        assert!(!should_emit_place(true, false, true, true));
        
        // Should not place when not holding
        assert!(!should_emit_place(true, true, false, false));
        assert!(!should_emit_place(false, true, false, false));
        
        // Should not place when no input
        assert!(!should_emit_place(false, false, true, false));
    }

    #[test]
    fn append_prop_observable_property() {
        let mut lines = Vec::new();
        let prop = MaterialProperty {
            value: 0.8,
            visibility: PropertyVisibility::Observable,
        };
        
        append_prop(&mut lines, "Test", &prop, |v| {
            if v > 0.5 { "High" } else { "Low" }
        });
        
        assert_eq!(lines, vec!["Test: High"]);
    }

    #[test]
    fn append_prop_revealed_property() {
        let mut lines = Vec::new();
        let prop = MaterialProperty {
            value: 0.3,
            visibility: PropertyVisibility::Revealed,
        };
        
        append_prop(&mut lines, "Weight", &prop, |v| {
            if v > 0.5 { "Heavy" } else { "Light" }
        });
        
        assert_eq!(lines, vec!["Weight: Light"]);
    }

    #[test]
    fn append_prop_hidden_property() {
        let mut lines = Vec::new();
        let prop = MaterialProperty {
            value: 0.9,
            visibility: PropertyVisibility::Hidden,
        };
        
        append_prop(&mut lines, "Secret", &prop, |_| "Should not see this");
        
        assert_eq!(lines, vec!["Secret: ???"]);
    }

    #[test]
    fn append_thermal_prop_hidden_no_knowledge() {
        let mut lines = Vec::new();
        let mat = test_material(); // thermal_resistance is Hidden
        let tracker = ConfidenceTracker::default(); // no observations
        
        append_thermal_prop(&mut lines, &mat, &tracker);
        
        assert_eq!(lines, vec!["Heat response: ???"]);
    }

    #[test]
    fn append_thermal_prop_hidden_with_knowledge() {
        let mut lines = Vec::new();
        let mat = test_material(); // thermal_resistance is Hidden
        let mut tracker = ConfidenceTracker::default();
        tracker.record(mat.seed, PropertyName::ThermalResistance);
        
        append_thermal_prop(&mut lines, &mat, &tracker);
        
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("Heat response: "));
        assert!(!lines[0].contains("???"));
    }

    #[test]
    fn append_thermal_prop_observable() {
        let mut lines = Vec::new();
        let mut mat = test_material();
        mat.thermal_resistance.visibility = PropertyVisibility::Observable;
        let tracker = ConfidenceTracker::default();
        
        append_thermal_prop(&mut lines, &mat, &tracker);
        
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("Heat response: "));
        assert!(!lines[0].contains("???"));
    }

    #[test]
    fn append_thermal_prop_revealed() {
        let mut lines = Vec::new();
        let mut mat = test_material();
        mat.thermal_resistance.visibility = PropertyVisibility::Revealed;
        let tracker = ConfidenceTracker::default();
        
        append_thermal_prop(&mut lines, &mat, &tracker);
        
        assert_eq!(lines.len(), 1);
        assert!(lines[0].starts_with("Heat response: "));
        assert!(!lines[0].contains("???"));
    }
}
