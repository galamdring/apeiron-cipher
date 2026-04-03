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

use crate::fabricator::{ActivateIntent, InputSlot};
use crate::input::InputAction;
use crate::journal::RecordEncounter;
use crate::materials::{GameMaterial, MATERIAL_SURFACE_GAP, MaterialObject, PropertyVisibility};
use crate::observation::{ConfidenceTracker, describe_thermal_observation};
use crate::player::{Player, PlayerCamera, cursor_is_captured};
use crate::scene::{SceneConfig, Surface};

use leafwing_input_manager::prelude::*;

const INTERACTION_RANGE: f32 = 3.0;
const HOLD_OFFSET: Vec3 = Vec3::new(0.2, -0.15, -0.5);

pub(crate) struct InteractionPlugin;

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
pub(crate) struct HeldItem;

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
    material_query: Query<(), With<MaterialObject>>,
    held_query: Query<(), With<HeldItem>>,
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

fn should_emit_pickup(interact_pressed: bool, holding: bool) -> bool {
    interact_pressed && !holding
}

fn should_emit_place(interact_pressed: bool, place_pressed: bool, holding: bool) -> bool {
    holding && (place_pressed || interact_pressed)
}

fn emit_pickup_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    held_query: Query<(), With<HeldItem>>,
    mut writer: MessageWriter<PickupIntent>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }
    let holding = held_query.iter().next().is_some();
    let Ok(action) = player_query.single() else {
        return;
    };
    if should_emit_pickup(action.just_pressed(&InputAction::Interact), holding) {
        writer.write(PickupIntent);
    }
}

fn emit_place_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
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

fn process_pickup(
    mut commands: Commands,
    mut reader: MessageReader<PickupIntent>,
    target: Res<InteractionTarget>,
    held_query: Query<Entity, With<HeldItem>>,
    camera_query: Query<Entity, With<PlayerCamera>>,
) {
    for _intent in reader.read() {
        if held_query.iter().next().is_some() {
            continue;
        }

        let Some(target_entity) = target.entity else {
            continue;
        };

        let Ok(camera_entity) = camera_query.single() else {
            continue;
        };

        commands
            .entity(target_entity)
            .insert(HeldItem)
            .set_parent_in_place(camera_entity)
            .insert(Transform::from_translation(HOLD_OFFSET));
    }
}

/// Small gap above the surface so objects sit on top without z-fighting.
/// Maximum XZ distance from the ray's intersection to a surface center for
/// placement to be considered valid.
const PLACE_REACH: f32 = 2.5;

#[allow(clippy::too_many_arguments)]
fn process_place(
    mut commands: Commands,
    mut reader: MessageReader<PlaceIntent>,
    scene: Res<SceneConfig>,
    held_query: Query<(Entity, &GameMaterial), With<HeldItem>>,
    camera_query: Query<&GlobalTransform, With<PlayerCamera>>,
    player_query: Query<&GlobalTransform, With<Player>>,
    surfaces: Query<(Entity, &GlobalTransform), With<Surface>>,
    slot_target: Res<SlotTarget>,
    mut slot_query: Query<(&GlobalTransform, &mut InputSlot)>,
) {
    for _intent in reader.read() {
        let Some((held_entity, held_material)) = held_query.iter().next() else {
            continue;
        };

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

        commands
            .entity(held_entity)
            .remove::<HeldItem>()
            .remove_parent_in_place();

        let drop_position =
            if let Some((_entity, surface_gtf)) = best_surface_for_ray(cam_gtf, &surfaces) {
                let surface_pos = surface_gtf.translation();
                let cam_pos = cam_gtf.translation();
                let cam_fwd = *cam_gtf.forward();

                let hit = ray_horizontal_intersection(cam_pos, cam_fwd, surface_pos.y);
                let place_x = hit.map_or(surface_pos.x, |p| p.x);
                let place_z = hit.map_or(surface_pos.z, |p| p.z);
                Vec3::new(
                    place_x,
                    held_material.resting_center_y(surface_pos.y),
                    place_z,
                )
            } else {
                floor_drop_position(player_gtf, &scene, held_material)
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
    surfaces: &'a Query<(Entity, &GlobalTransform), With<Surface>>,
) -> Option<(Entity, &'a GlobalTransform)> {
    let origin = cam_gtf.translation();
    let direction = *cam_gtf.forward();

    surfaces
        .iter()
        .filter_map(|(entity, sgtf)| {
            let s_pos = sgtf.translation();
            let hit = ray_horizontal_intersection(origin, direction, s_pos.y)?;
            let xz_dist = Vec2::new(hit.x - s_pos.x, hit.z - s_pos.z).length();
            if xz_dist > PLACE_REACH {
                return None;
            }
            Some((entity, sgtf, xz_dist))
        })
        .min_by(|a, b| a.2.partial_cmp(&b.2).unwrap_or(std::cmp::Ordering::Equal))
        .map(|(entity, sgtf, _)| (entity, sgtf))
}

fn floor_drop_position(
    player_gtf: &GlobalTransform,
    scene: &SceneConfig,
    material: &GameMaterial,
) -> Vec3 {
    let origin = player_gtf.translation();
    let forward = *player_gtf.forward();
    let forward_xz = Vec3::new(forward.x, 0.0, forward.z).normalize_or_zero();
    let fallback_forward = if forward_xz == Vec3::ZERO {
        Vec3::NEG_Z
    } else {
        forward_xz
    };
    let mut position = origin + fallback_forward * 0.6;
    let margin = scene.room.boundary_margin;
    let max_x = scene.room.half_extent_x - margin;
    let max_z = scene.room.half_extent_z - margin;
    position.x = position.x.clamp(-max_x, max_x);
    position.z = position.z.clamp(-max_z, max_z);
    position.y = material.resting_center_y(0.0);
    position
}

// ── Held item tracking ───────────────────────────────────────────────────

fn update_held_position(mut held_query: Query<&mut Transform, With<HeldItem>>) {
    for mut tf in &mut held_query {
        tf.translation = HOLD_OFFSET;
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
    mut encounter_writer: MessageWriter<RecordEncounter>,
) {
    for _intent in reader.read() {
        let held_material = held_query.iter().next();
        let targeted_material = target
            .entity
            .and_then(|entity| material_query.get(entity).ok());

        if let Some(mat) = held_material.or(targeted_material) {
            state.visible = !state.visible;
            encounter_writer.write(RecordEncounter {
                material: mat.clone(),
            });
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

// ── Property description ─────────────────────────────────────────────────

/// Converts a normalised 0–1 property value into a descriptive word.
fn describe_value(value: f32) -> &'static str {
    if value < 0.15 {
        "Negligible"
    } else if value < 0.3 {
        "Very low"
    } else if value < 0.45 {
        "Low"
    } else if value < 0.55 {
        "Moderate"
    } else if value < 0.7 {
        "High"
    } else if value < 0.85 {
        "Very high"
    } else {
        "Extreme"
    }
}

fn describe_density(value: f32) -> &'static str {
    if value < 0.15 {
        "Almost weightless"
    } else if value < 0.3 {
        "Very light"
    } else if value < 0.45 {
        "Light"
    } else if value < 0.55 {
        "Medium weight"
    } else if value < 0.7 {
        "Heavy"
    } else if value < 0.85 {
        "Very heavy"
    } else {
        "Extremely dense"
    }
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
    match mat.thermal_resistance.visibility {
        PropertyVisibility::Hidden => lines.push("Heat response: ???".to_string()),
        PropertyVisibility::Observable | PropertyVisibility::Revealed => {
            let confidence = tracker.level(mat.seed, "thermal_resistance");
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

    #[test]
    fn describe_value_covers_full_range() {
        assert_eq!(describe_value(0.0), "Negligible");
        assert_eq!(describe_value(0.2), "Very low");
        assert_eq!(describe_value(0.35), "Low");
        assert_eq!(describe_value(0.5), "Moderate");
        assert_eq!(describe_value(0.6), "High");
        assert_eq!(describe_value(0.75), "Very high");
        assert_eq!(describe_value(0.9), "Extreme");
    }

    #[test]
    fn describe_density_covers_full_range() {
        assert_eq!(describe_density(0.1), "Almost weightless");
        assert_eq!(describe_density(0.25), "Very light");
        assert_eq!(describe_density(0.4), "Light");
        assert_eq!(describe_density(0.5), "Medium weight");
        assert_eq!(describe_density(0.65), "Heavy");
        assert_eq!(describe_density(0.78), "Very heavy");
        assert_eq!(describe_density(0.95), "Extremely dense");
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

        tracker.record(mat.seed, "thermal_resistance");
        tracker.record(mat.seed, "thermal_resistance");
        let observed = build_examine_text(&mat, &tracker);
        assert!(observed.contains("Heat response: Hold together under heat"));

        tracker.record(mat.seed, "thermal_resistance");
        tracker.record(mat.seed, "thermal_resistance");
        let confident = build_examine_text(&mat, &tracker);
        assert!(confident.contains("Heat response: Reliably hold together under heat"));
    }

    #[test]
    fn interact_picks_up_only_when_not_holding() {
        assert!(should_emit_pickup(true, false));
        assert!(!should_emit_pickup(false, false));
        assert!(!should_emit_pickup(true, true));
    }

    #[test]
    fn interact_or_place_can_drop_when_holding() {
        assert!(should_emit_place(true, false, true));
        assert!(should_emit_place(false, true, true));
        assert!(!should_emit_place(false, false, true));
        assert!(!should_emit_place(true, false, false));
    }

    #[test]
    fn floor_drop_position_clamps_inside_room_bounds() {
        let scene = SceneConfig::default();
        let player = GlobalTransform::from(Transform::from_xyz(100.0, 1.7, 100.0));
        let material = test_material();
        let dropped = floor_drop_position(&player, &scene, &material);
        let max_x = scene.room.half_extent_x - scene.room.boundary_margin;
        let max_z = scene.room.half_extent_z - scene.room.boundary_margin;

        assert!(dropped.x <= max_x);
        assert!(dropped.z <= max_z);
        assert!((dropped.y - material.resting_center_y(0.0)).abs() < f32::EPSILON);
    }
}
