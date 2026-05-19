//! Temporary debug overlay — shows terrain diagnostics at the player's feet.
//!
//! Displays the player's world position, the terrain elevation that
//! `sample_elevation` returns at that XZ, the chunk coord, and the delta
//! between the player's actual Y and the terrain surface. This helps
//! diagnose any mismatch between the visual heightmap mesh and the logical
//! surface used for placement / camera height.
//!
//! Press **F3** (default) to toggle the panel on or off. The panel starts
//! hidden — open it when you need it, close it when you don't.

use bevy::prelude::*;
use leafwing_input_manager::prelude::ActionState;

use crate::input::InputAction;
use crate::player::{Player, PlayerCamera};
use crate::scene::PositionXZ;
use crate::world_generation::{
    PlanetSurface, WorldGenerationConfig, WorldProfile, chunk_origin_xz,
    world_position_to_chunk_coord,
};

/// Plugin that renders an on-screen debug information panel.
pub struct DebugOverlayPlugin;

impl Plugin for DebugOverlayPlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(Startup, spawn_debug_panel)
            .add_systems(Update, (toggle_debug_panel, update_debug_panel).chain());
    }
}

/// Marker for the root node of the debug overlay panel.
#[derive(Component)]
struct DebugPanel;

/// Marker for the text entity inside the debug overlay panel.
#[derive(Component)]
struct DebugText;

/// Spawns the debug overlay panel in a hidden state.
///
/// Runs once at `Startup`. The panel is invisible by default — press F3
/// (or the configured `ToggleDebugOverlay` binding) to show it.
fn spawn_debug_panel(mut commands: Commands) {
    commands
        .spawn((
            DebugPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(10.0),
                left: Val::Px(10.0),
                padding: UiRect::all(Val::Px(8.0)),
                display: Display::None,
                ..default()
            },
            BackgroundColor(Color::srgba(0.0, 0.0, 0.0, 0.7)),
        ))
        .with_children(|parent| {
            parent.spawn((
                DebugText,
                Text::new(""),
                TextFont {
                    font_size: 14.0,
                    ..default()
                },
                TextColor(Color::srgba(0.0, 1.0, 0.0, 1.0)),
            ));
        });
}

/// Toggles the debug panel visibility when the `ToggleDebugOverlay` action fires.
///
/// Runs in `Update`, before `update_debug_panel` — so we skip the terrain
/// sampling work when the panel is hidden.
fn toggle_debug_panel(
    action_state: Option<Res<ActionState<InputAction>>>,
    mut panel_query: Query<&mut Node, With<DebugPanel>>,
) {
    let Some(action_state) = action_state else {
        return;
    };
    if !action_state.just_pressed(&InputAction::ToggleDebugOverlay) {
        return;
    }
    let Ok(mut node) = panel_query.single_mut() else {
        return;
    };
    node.display = if node.display == Display::None {
        Display::Flex
    } else {
        Display::None
    };
}

/// Updates the terrain diagnostic text each frame — only does work when
/// the panel is visible.
///
/// Runs in `Update`, chained after `toggle_debug_panel`.
fn update_debug_panel(
    panel_query: Query<&Node, With<DebugPanel>>,
    player_query: Query<&Transform, With<Player>>,
    camera_query: Query<&GlobalTransform, With<PlayerCamera>>,
    world_profile: Option<Res<WorldProfile>>,
    world_gen_config: Res<WorldGenerationConfig>,
    mut text_query: Query<&mut Text, With<DebugText>>,
) {
    // Skip all work if the panel is hidden.
    let Ok(panel_node) = panel_query.single() else {
        return;
    };
    if panel_node.display == Display::None {
        return;
    }

    let Ok(player_tf) = player_query.single() else {
        return;
    };
    let Some(world_profile) = world_profile else {
        return;
    };
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    let pos = player_tf.translation;
    let surface = PlanetSurface::new_from_profile(&world_profile, &world_gen_config);
    let terrain_y = surface.sample_elevation(pos.x, pos.z);

    let chunk = world_position_to_chunk_coord(
        PositionXZ::new(pos.x, pos.z),
        world_profile.chunk_size_world_units,
    );
    let chunk_origin = chunk_origin_xz(chunk, world_profile.chunk_size_world_units);

    // Sample elevation at the four corners of the current chunk to show
    // the elevation range the mesh spans.
    let cs = world_profile.chunk_size_world_units;
    let corner_nw = surface.sample_elevation(chunk_origin.x, chunk_origin.z);
    let corner_ne = surface.sample_elevation(chunk_origin.x + cs, chunk_origin.z);
    let corner_sw = surface.sample_elevation(chunk_origin.x, chunk_origin.z + cs);
    let corner_se = surface.sample_elevation(chunk_origin.x + cs, chunk_origin.z + cs);
    let chunk_min = corner_nw.min(corner_ne).min(corner_sw).min(corner_se);
    let chunk_max = corner_nw.max(corner_ne).max(corner_sw).max(corner_se);

    // Camera world Y (child of player, so global transform includes parent).
    let cam_world_y = camera_query
        .single()
        .map(|gtf| gtf.translation().y)
        .unwrap_or(f32::NAN);

    let delta_player_terrain = pos.y - terrain_y;

    text.0 = format!(
        "=== Terrain Debug ===\n\
         Player XZ:    ({:.2}, {:.2})\n\
         Player Y:     {:.4}\n\
         Camera Y:     {:.4}\n\
         Terrain Y:    {:.4}  (sample_elevation)\n\
         Delta (P-T):  {:.4}\n\
         Eye height:   {:.4}  (Y - terrain)\n\
         \n\
         Chunk:        ({}, {})\n\
         Chunk origin: ({:.1}, {:.1})\n\
         Chunk elev:   {:.3} .. {:.3}  (corners)\n\
         Chunk range:  {:.3}\n\
         \n\
         Config:\n\
         amplitude:    {:.1}\n\
         frequency:    {:.4}\n\
         octaves:      {}\n\
         detail_wt:    {:.2}\n\
         subdivisions: {}",
        pos.x,
        pos.z,
        pos.y,
        cam_world_y,
        terrain_y,
        delta_player_terrain,
        pos.y - terrain_y,
        chunk.x,
        chunk.z,
        chunk_origin.x,
        chunk_origin.z,
        chunk_min,
        chunk_max,
        chunk_max - chunk_min,
        world_gen_config.elevation_amplitude,
        world_gen_config.elevation_frequency,
        world_gen_config.elevation_octaves,
        world_gen_config.elevation_detail_weight,
        world_gen_config.elevation_subdivisions,
    );
}
