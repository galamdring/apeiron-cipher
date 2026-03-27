//! Scene setup plugin — the physical environment the player exists in.
//!
//! Loads [`SceneConfig`] from `assets/config/scene.toml`, spawns the enclosed room
//! (floor, walls, ceiling), furniture marked with [`Workbench`] and [`Surface`],
//! and lighting tuned for material contrast. The player plugin reads the same
//! config for spawn position and movement bounds.

use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

pub(crate) struct ScenePlugin;

impl Plugin for ScenePlugin {
    fn build(&self, app: &mut App) {
        app.add_systems(PreStartup, load_scene_config)
            .add_systems(Startup, setup_scene);
    }
}

// ── Marker components (Epic 3+ query targets) ───────────────────────────

/// Marks the central workbench — future fabricator anchor (Epic 3).
#[derive(Component)]
pub(crate) struct Workbench;

/// Marks shelf or table surfaces for placing materials (later stories).
#[derive(Component)]
pub(crate) struct Surface;

// ── Config (TOML ↔ Rust) ─────────────────────────────────────────────────

const CONFIG_PATH: &str = "assets/config/scene.toml";

/// Top-level structure of `assets/config/scene.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Resource)]
pub(crate) struct SceneConfig {
    #[serde(default)]
    pub room: RoomConfig,
    #[serde(default)]
    pub player: PlayerSceneConfig,
    #[serde(default)]
    pub lighting: LightingConfig,
    #[serde(default)]
    pub furniture: FurnitureConfig,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct RoomConfig {
    #[serde(default = "default_half_extent_x")]
    pub half_extent_x: f32,
    #[serde(default = "default_half_extent_z")]
    pub half_extent_z: f32,
    #[serde(default = "default_wall_height")]
    pub wall_height: f32,
    #[serde(default = "default_wall_thickness")]
    pub wall_thickness: f32,
    #[serde(default = "default_boundary_margin")]
    pub boundary_margin: f32,
}

fn default_half_extent_x() -> f32 {
    4.0
}
fn default_half_extent_z() -> f32 {
    4.0
}
fn default_wall_height() -> f32 {
    3.0
}
fn default_wall_thickness() -> f32 {
    0.2
}
fn default_boundary_margin() -> f32 {
    0.12
}

impl Default for RoomConfig {
    fn default() -> Self {
        Self {
            half_extent_x: default_half_extent_x(),
            half_extent_z: default_half_extent_z(),
            wall_height: default_wall_height(),
            wall_thickness: default_wall_thickness(),
            boundary_margin: default_boundary_margin(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct PlayerSceneConfig {
    #[serde(default = "default_eye_height")]
    pub eye_height: f32,
    #[serde(default)]
    pub spawn_x: f32,
    #[serde(default = "default_spawn_z")]
    pub spawn_z: f32,
    #[serde(default = "default_move_speed")]
    pub move_speed: f32,
}

fn default_eye_height() -> f32 {
    1.7
}
fn default_spawn_z() -> f32 {
    2.0
}
fn default_move_speed() -> f32 {
    5.0
}

impl Default for PlayerSceneConfig {
    fn default() -> Self {
        Self {
            eye_height: default_eye_height(),
            spawn_x: 0.0,
            spawn_z: default_spawn_z(),
            move_speed: default_move_speed(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct LightingConfig {
    #[serde(default = "default_ambient_brightness")]
    pub ambient_brightness: f32,
    #[serde(default = "default_directional_illuminance")]
    pub directional_illuminance: f32,
    #[serde(default = "default_directional_shadows")]
    pub directional_shadows: bool,
    #[serde(default = "default_spot_intensity")]
    pub spot_intensity: f32,
    #[serde(default = "default_spot_range")]
    pub spot_range: f32,
    #[serde(default = "default_spot_inner_angle")]
    pub spot_inner_angle: f32,
    #[serde(default = "default_spot_outer_angle")]
    pub spot_outer_angle: f32,
    #[serde(default = "default_spot_height")]
    pub spot_height: f32,
    #[serde(default = "default_spot_target_y")]
    pub spot_target_y: f32,
}

fn default_ambient_brightness() -> f32 {
    14.0
}
fn default_directional_illuminance() -> f32 {
    1100.0
}
fn default_directional_shadows() -> bool {
    true
}
fn default_spot_intensity() -> f32 {
    280_000.0
}
fn default_spot_range() -> f32 {
    12.0
}
fn default_spot_inner_angle() -> f32 {
    0.28
}
fn default_spot_outer_angle() -> f32 {
    0.48
}
fn default_spot_height() -> f32 {
    2.75
}
fn default_spot_target_y() -> f32 {
    0.45
}

impl Default for LightingConfig {
    fn default() -> Self {
        Self {
            ambient_brightness: default_ambient_brightness(),
            directional_illuminance: default_directional_illuminance(),
            directional_shadows: default_directional_shadows(),
            spot_intensity: default_spot_intensity(),
            spot_range: default_spot_range(),
            spot_inner_angle: default_spot_inner_angle(),
            spot_outer_angle: default_spot_outer_angle(),
            spot_height: default_spot_height(),
            spot_target_y: default_spot_target_y(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FurnitureConfig {
    #[serde(default = "default_workbench_width")]
    pub workbench_width: f32,
    #[serde(default = "default_workbench_height")]
    pub workbench_height: f32,
    #[serde(default = "default_workbench_depth")]
    pub workbench_depth: f32,
    #[serde(default)]
    pub workbench_x: f32,
    #[serde(default)]
    pub workbench_z: f32,
    #[serde(default = "default_shelf_width")]
    pub shelf_width: f32,
    #[serde(default = "default_shelf_thickness")]
    pub shelf_thickness: f32,
    #[serde(default = "default_shelf_depth")]
    pub shelf_depth: f32,
    #[serde(default = "default_shelves")]
    pub shelves: Vec<ShelfConfig>,
}

fn default_workbench_width() -> f32 {
    2.0
}
fn default_workbench_height() -> f32 {
    0.88
}
fn default_workbench_depth() -> f32 {
    1.0
}
fn default_shelf_width() -> f32 {
    1.35
}
fn default_shelf_thickness() -> f32 {
    0.07
}
fn default_shelf_depth() -> f32 {
    0.55
}

fn default_shelves() -> Vec<ShelfConfig> {
    vec![
        ShelfConfig {
            x: -3.15,
            z: 0.7,
            y: 0.92,
        },
        ShelfConfig {
            x: 3.15,
            z: -0.7,
            y: 0.92,
        },
        ShelfConfig {
            x: 0.6,
            z: -3.15,
            y: 0.92,
        },
    ]
}

impl Default for FurnitureConfig {
    fn default() -> Self {
        Self {
            workbench_width: default_workbench_width(),
            workbench_height: default_workbench_height(),
            workbench_depth: default_workbench_depth(),
            workbench_x: 0.0,
            workbench_z: 0.0,
            shelf_width: default_shelf_width(),
            shelf_thickness: default_shelf_thickness(),
            shelf_depth: default_shelf_depth(),
            shelves: default_shelves(),
        }
    }
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct ShelfConfig {
    pub x: f32,
    pub z: f32,
    pub y: f32,
}

// ── Load ────────────────────────────────────────────────────────────────

fn load_scene_config(mut commands: Commands) {
    let config = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<SceneConfig>(&contents) {
                Ok(cfg) => {
                    info!("Loaded scene config from {CONFIG_PATH}");
                    cfg
                }
                Err(e) => {
                    warn!("Malformed {CONFIG_PATH}, using defaults: {e}");
                    SceneConfig::default()
                }
            },
            Err(e) => {
                warn!("Could not read {CONFIG_PATH}, using defaults: {e}");
                SceneConfig::default()
            }
        }
    } else {
        warn!("{CONFIG_PATH} not found, using defaults");
        SceneConfig::default()
    };
    commands.insert_resource(config);
}

// ── Scene setup ─────────────────────────────────────────────────────────

fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    cfg: Res<SceneConfig>,
) {
    let hx = cfg.room.half_extent_x;
    let hz = cfg.room.half_extent_z;
    let h = cfg.room.wall_height;
    let t = cfg.room.wall_thickness;

    // Room shell materials — darker than furniture so interactive materials read clearly.
    let floor_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.22, 0.22, 0.26),
        perceptual_roughness: 0.92,
        ..default()
    });
    let wall_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.28, 0.3, 0.34),
        perceptual_roughness: 0.88,
        ..default()
    });
    let ceiling_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.32, 0.32, 0.36),
        perceptual_roughness: 0.9,
        ..default()
    });

    // Floor — XZ plane, centered.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(hx * 2.0, hz * 2.0))),
        MeshMaterial3d(floor_mat),
    ));

    // Ceiling — same plane, flipped so normals face down into the room.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(hx * 2.0, hz * 2.0))),
        MeshMaterial3d(ceiling_mat),
        Transform::from_xyz(0.0, h, 0.0)
            .with_rotation(Quat::from_rotation_x(core::f32::consts::PI)),
    ));

    // Four walls (thin boxes along the inner perimeter).
    let wall_y = h * 0.5;
    let west_east_depth = hz * 2.0 + t * 2.0;
    let north_south_width = hx * 2.0 + t * 2.0;

    // West (-X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(t, h, west_east_depth))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(-hx - t * 0.5, wall_y, 0.0),
    ));
    // East (+X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(t, h, west_east_depth))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(hx + t * 0.5, wall_y, 0.0),
    ));
    // South (-Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(north_south_width, h, t))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(0.0, wall_y, -hz - t * 0.5),
    ));
    // North (+Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(north_south_width, h, t))),
        MeshMaterial3d(wall_mat),
        Transform::from_xyz(0.0, wall_y, hz + t * 0.5),
    ));

    // Workbench — lighter, lower roughness than walls (future fabricator site).
    let fur = &cfg.furniture;
    let wb_half_y = fur.workbench_height * 0.5;
    let workbench_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.52, 0.48, 0.42),
        perceptual_roughness: 0.55,
        metallic: 0.02,
        ..default()
    });
    commands.spawn((
        Workbench,
        Mesh3d(meshes.add(Cuboid::new(
            fur.workbench_width,
            fur.workbench_height,
            fur.workbench_depth,
        ))),
        MeshMaterial3d(workbench_mat),
        Transform::from_xyz(fur.workbench_x, wb_half_y, fur.workbench_z),
    ));

    // Shelf surfaces — warm neutral, clearly not wall paint.
    let shelf_w = fur.shelf_width;
    let shelf_h = fur.shelf_thickness;
    let shelf_d = fur.shelf_depth;
    let shelf_half_y = shelf_h * 0.5;
    let shelf_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.42, 0.36, 0.3),
        perceptual_roughness: 0.72,
        ..default()
    });

    for shelf in &fur.shelves {
        commands.spawn((
            Surface,
            Mesh3d(meshes.add(Cuboid::new(shelf_w, shelf_h, shelf_d))),
            MeshMaterial3d(shelf_mat.clone()),
            Transform::from_xyz(shelf.x, shelf.y - shelf_half_y, shelf.z),
        ));
    }

    // Directional fill — angled so forms read; lower than the open-plane setup.
    commands.spawn((
        DirectionalLight {
            illuminance: cfg.lighting.directional_illuminance,
            shadows_enabled: cfg.lighting.directional_shadows,
            ..default()
        },
        Transform::from_xyz(6.0, 9.0, 4.0).looking_at(Vec3::new(0.0, 0.6, 0.0), Vec3::Y),
    ));

    commands.spawn(AmbientLight {
        brightness: cfg.lighting.ambient_brightness,
        ..default()
    });

    // Focused spot over the workbench — contrast for future material placement.
    let spot_y = cfg.lighting.spot_height;
    let target_y = cfg.lighting.spot_target_y;
    commands.spawn((
        SpotLight {
            color: Color::srgb(1.0, 0.97, 0.92),
            intensity: cfg.lighting.spot_intensity,
            range: cfg.lighting.spot_range,
            shadows_enabled: true,
            inner_angle: cfg.lighting.spot_inner_angle,
            outer_angle: cfg.lighting.spot_outer_angle,
            ..default()
        },
        Transform::from_xyz(fur.workbench_x, spot_y, fur.workbench_z).looking_at(
            Vec3::new(fur.workbench_x, target_y, fur.workbench_z),
            Vec3::X,
        ),
    ));
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn scene_config_toml_round_trip() {
        let original = SceneConfig::default();
        let serialized = toml::to_string(&original).expect("serialize");
        let parsed: SceneConfig = toml::from_str(&serialized).expect("deserialize");
        assert!((parsed.room.half_extent_x - original.room.half_extent_x).abs() < f32::EPSILON);
        assert_eq!(parsed.furniture.shelves.len(), 3);
    }

    #[test]
    fn scene_config_partial_room_uses_defaults_elsewhere() {
        let s = "[room]\nhalf_extent_x = 3.5\n";
        let parsed: SceneConfig = toml::from_str(s).expect("parse");
        assert!((parsed.room.half_extent_x - 3.5).abs() < f32::EPSILON);
        assert!((parsed.room.half_extent_z - 4.0).abs() < f32::EPSILON);
    }
}
