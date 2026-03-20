//! Interaction plugin — raycasting, pickup/place, and crosshair UI.
//!
//! Provides the hands-on loop for material interaction: the player looks at
//! objects, picks them up, carries them, and puts them down on surfaces.
//!
//! Architecture follows the server-authoritative pattern: input systems emit
//! intent messages, separate systems process those intents and mutate state.
//!
//! Systems:
//! - `update_interaction_target`: raycast from camera center, track closest hit
//! - `emit_pickup_intent` / `emit_place_intent`: read input actions, emit messages
//! - `process_pickup`: pick up targeted material (re-parent to camera)
//! - `process_place`: place held material on nearest surface
//! - `update_held_position`: keep held item in front of camera
//! - `spawn_crosshair`: UI overlay at screen center
//! - `update_crosshair`: colour change when targeting an interactable

use bevy::picking::mesh_picking::ray_cast::{MeshRayCast, MeshRayCastSettings, RayCastVisibility};
use bevy::prelude::*;
use bevy::window::CursorGrabMode;

use crate::input::InputAction;
use crate::materials::MaterialObject;
use crate::player::{Player, PlayerCamera};
use crate::scene::Surface;

use leafwing_input_manager::prelude::*;

const INTERACTION_RANGE: f32 = 3.0;
const HOLD_OFFSET: Vec3 = Vec3::new(0.2, -0.15, -0.5);

pub(crate) struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractionTarget>().add_systems(
            Update,
            (
                update_interaction_target,
                emit_pickup_intent.after(update_interaction_target),
                emit_place_intent.after(update_interaction_target),
                process_pickup.after(emit_pickup_intent),
                process_place.after(emit_place_intent),
                update_held_position.after(process_pickup),
                update_crosshair.after(update_interaction_target),
            ),
        );
        app.add_systems(Startup, spawn_crosshair);
    }
}

// ── Intent messages (client → server boundary) ───────────────────────────

#[derive(Message)]
struct PickupIntent;

#[derive(Message)]
struct PlaceIntent;

// ── State ────────────────────────────────────────────────────────────────

/// Tracks what the player's center-screen ray is currently hitting.
#[derive(Resource, Default)]
struct InteractionTarget {
    entity: Option<Entity>,
}

/// Marks a material entity as currently held by the player.
#[derive(Component)]
pub(crate) struct HeldItem;

// ── UI markers ───────────────────────────────────────────────────────────

#[derive(Component)]
struct Crosshair;

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

// ── Intent emission ──────────────────────────────────────────────────────

fn emit_pickup_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut writer: MessageWriter<PickupIntent>,
) {
    if cursor_options.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok(action) = player_query.single() else {
        return;
    };
    if action.just_pressed(&InputAction::Interact) {
        writer.write(PickupIntent);
    }
}

fn emit_place_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut writer: MessageWriter<PlaceIntent>,
) {
    if cursor_options.grab_mode != CursorGrabMode::Locked {
        return;
    }
    let Ok(action) = player_query.single() else {
        return;
    };
    if action.just_pressed(&InputAction::Place) {
        writer.write(PlaceIntent);
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

fn process_place(
    mut commands: Commands,
    mut reader: MessageReader<PlaceIntent>,
    held_query: Query<Entity, With<HeldItem>>,
    camera_query: Query<&GlobalTransform, With<PlayerCamera>>,
    surfaces: Query<(Entity, &GlobalTransform), With<Surface>>,
) {
    for _intent in reader.read() {
        let Some(held_entity) = held_query.iter().next() else {
            continue;
        };

        let Ok(cam_gtf) = camera_query.single() else {
            continue;
        };

        let cam_pos = cam_gtf.translation();
        let cam_forward = cam_gtf.forward();
        let place_target = cam_pos + *cam_forward * 1.5;

        let closest_surface = surfaces.iter().min_by(|(_, a), (_, b)| {
            let da = a.translation().distance_squared(place_target);
            let db = b.translation().distance_squared(place_target);
            da.partial_cmp(&db).unwrap_or(std::cmp::Ordering::Equal)
        });

        let Some((_surface_entity, surface_gtf)) = closest_surface else {
            continue;
        };

        commands
            .entity(held_entity)
            .remove::<HeldItem>()
            .remove_parent_in_place();

        let surface_pos = surface_gtf.translation();
        commands.entity(held_entity).insert(Transform::from_xyz(
            surface_pos.x,
            surface_pos.y + 0.15,
            surface_pos.z,
        ));
    }
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
    held_query: Query<(), With<HeldItem>>,
    mut crosshair_query: Query<&mut TextColor, With<Crosshair>>,
) {
    let Ok(mut color) = crosshair_query.single_mut() else {
        return;
    };

    let holding = held_query.iter().next().is_some();
    let targeting = target.entity.is_some();

    color.0 = if targeting {
        Color::srgba(0.2, 1.0, 0.4, 0.9)
    } else if holding {
        Color::srgba(1.0, 0.85, 0.3, 0.8)
    } else {
        Color::srgba(1.0, 1.0, 1.0, 0.6)
    };
}
