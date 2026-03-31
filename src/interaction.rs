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
use bevy::window::CursorGrabMode;

use crate::input::InputAction;
use crate::materials::{GameMaterial, MaterialObject, PropertyVisibility};
use crate::player::{Player, PlayerCamera};
use crate::scene::Surface;

use leafwing_input_manager::prelude::*;

const INTERACTION_RANGE: f32 = 3.0;
const HOLD_OFFSET: Vec3 = Vec3::new(0.2, -0.15, -0.5);

pub(crate) struct InteractionPlugin;

impl Plugin for InteractionPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<InteractionTarget>()
            .init_resource::<ExamineState>()
            .add_systems(
                Update,
                (
                    update_interaction_target,
                    emit_pickup_intent.after(update_interaction_target),
                    emit_place_intent.after(update_interaction_target),
                    emit_examine_intent.after(update_interaction_target),
                    process_pickup.after(emit_pickup_intent),
                    process_place.after(emit_place_intent),
                    process_examine.after(emit_examine_intent),
                    update_held_position.after(process_pickup),
                    update_crosshair.after(update_interaction_target),
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

/// Tracks what the player's center-screen ray is currently hitting.
#[derive(Resource, Default)]
struct InteractionTarget {
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
    if cursor_options.grab_mode != CursorGrabMode::Locked {
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
    held_query: Query<(), With<HeldItem>>,
) {
    for _intent in reader.read() {
        if held_query.iter().next().is_some() {
            state.visible = !state.visible;
        } else {
            state.visible = false;
        }
    }
}

fn update_examine_panel(
    state: Res<ExamineState>,
    held_query: Query<&GameMaterial, With<HeldItem>>,
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

    let Some(mat) = held_query.iter().next() else {
        *vis = Visibility::Hidden;
        return;
    };

    *vis = Visibility::Visible;
    text.0 = build_examine_text(mat);
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

fn build_examine_text(mat: &GameMaterial) -> String {
    let mut lines = vec![mat.name.clone()];
    lines.push(String::new());

    append_prop(&mut lines, "Weight", &mat.density, describe_density);
    append_prop(
        &mut lines,
        "Heat resistance",
        &mat.thermal_resistance,
        describe_value,
    );
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
        let text = build_examine_text(&mat);

        assert!(text.contains("TestMat"));
        assert!(text.contains("Weight: Very heavy"));
        assert!(text.contains("Conductivity: Very high"));
        assert!(text.contains("Heat resistance: ???"));
        assert!(text.contains("Reactivity: ???"));
        assert!(text.contains("Toxicity: ???"));
    }

    #[test]
    fn examine_text_name_is_first_line() {
        let mat = test_material();
        let text = build_examine_text(&mat);
        let first_line = text.lines().next().unwrap();
        assert_eq!(first_line, "TestMat");
    }
}
