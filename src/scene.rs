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
        app.init_resource::<RoomShellCollision>()
            .add_systems(PreStartup, load_scene_config)
            .add_systems(Startup, setup_scene);
    }
}

// ── Marker components (Epic 3+ query targets) ───────────────────────────

/// Marks the central workbench mesh — future fabricator anchor (Epic 3).
#[derive(Component)]
pub(crate) struct Workbench;

/// A placement plane: the actual top of a piece of furniture where objects
/// can be set down. Spawned as its own entity at the true surface Y so
/// the placement system never needs offset math.
#[derive(Component)]
pub(crate) struct Surface {
    pub half_extent_x: f32,
    pub half_extent_z: f32,
}

/// Distinguishes storage shelves from the experiment workbench so initial
/// material spawning only targets shelves.
#[derive(Component)]
pub(crate) struct Shelf;

/// Ground-plane position using world X/Z coordinates.
///
/// Bevy uses Y as vertical, so any "flat" room-shell collision math happens on
/// the X/Z plane instead of the X/Y plane familiar from CAD or 3D printing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub(crate) struct PositionXZ {
    pub x: f32,
    pub z: f32,
}

impl PositionXZ {
    pub(crate) fn new(x: f32, z: f32) -> Self {
        Self { x, z }
    }
}

/// Axis-aligned rectangle on the world X/Z plane.
#[derive(Clone, Copy, Debug)]
pub(crate) struct RectXZ {
    pub min_x: f32,
    pub max_x: f32,
    pub min_z: f32,
    pub max_z: f32,
}

#[derive(Clone, Debug)]
pub(crate) struct WallCollider {
    pub footprint_xz: RectXZ,
}

impl WallCollider {
    fn blocks_circle_xz(&self, position_xz: PositionXZ, radius: f32) -> bool {
        let clamped_x = position_xz
            .x
            .clamp(self.footprint_xz.min_x, self.footprint_xz.max_x);
        let clamped_z = position_xz
            .z
            .clamp(self.footprint_xz.min_z, self.footprint_xz.max_z);

        // We treat the player as a circle on the X/Z plane and each wall
        // segment as a finite rectangle on that same plane. First find the
        // nearest point on the wall footprint to the player's center. Then ask
        // whether a same-radius circle centered on that wall point would reach
        // the player center. That gives the same overlap answer as asking
        // whether the player's circle overlaps the wall rectangle, while also
        // handling the ends of split wall segments around the doorway.
        let dx = position_xz.x - clamped_x;
        let dz = position_xz.z - clamped_z;
        dx * dx + dz * dz < radius * radius
    }
}

#[derive(Resource, Clone, Debug, Default)]
pub(crate) struct RoomShellCollision {
    pub wall_colliders: Vec<WallCollider>,
}

impl RoomShellCollision {
    pub(crate) fn blocks_circle_xz(&self, position_xz: PositionXZ, radius: f32) -> bool {
        self.wall_colliders
            .iter()
            .any(|collider| collider.blocks_circle_xz(position_xz, radius))
    }
}

/// The currently authored flat exterior patch outside the doorway.
///
/// Epic 5's first generation stories are intentionally scoped to the exterior
/// patch that already exists in the scene. This resource makes that patch
/// explicit so generation code can ask "is this candidate on the current
/// playable exterior surface?" instead of hardcoding scene dimensions a second
/// time in a different module.
#[derive(Resource, Clone, Debug)]
pub(crate) struct ExteriorGroundPatch {
    pub bounds_xz: RectXZ,
    pub surface_y: f32,
}

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
    #[serde(default)]
    pub fabricator: FabricatorSceneConfig,
    #[serde(default)]
    pub heat_source: HeatSourceConfig,
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

// ── Fabricator config ────────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct FabricatorSceneConfig {
    #[serde(default = "default_fab_slot_offset_x")]
    pub slot_offset_x: f32,
    #[serde(default = "default_fab_slot_spacing_z")]
    pub slot_spacing_z: f32,
    #[serde(default = "default_fab_slot_radius")]
    pub slot_radius: f32,
    #[serde(default = "default_fab_slot_height")]
    pub slot_height: f32,
    #[serde(default = "default_fab_output_offset_x")]
    pub output_offset_x: f32,
    #[serde(default = "default_fab_output_offset_z")]
    pub output_offset_z: f32,
    #[serde(default = "default_fab_output_radius")]
    pub output_radius: f32,
    #[serde(default = "default_fab_output_height")]
    pub output_height: f32,
    #[serde(default = "default_fab_process_seconds")]
    pub process_seconds: f32,
}

fn default_fab_slot_offset_x() -> f32 {
    -0.45
}
fn default_fab_slot_spacing_z() -> f32 {
    0.3
}
fn default_fab_slot_radius() -> f32 {
    0.07
}
fn default_fab_slot_height() -> f32 {
    0.02
}
fn default_fab_output_offset_x() -> f32 {
    0.0
}
fn default_fab_output_offset_z() -> f32 {
    -0.25
}
fn default_fab_output_radius() -> f32 {
    0.09
}
fn default_fab_output_height() -> f32 {
    0.02
}
fn default_fab_process_seconds() -> f32 {
    2.5
}

impl Default for FabricatorSceneConfig {
    fn default() -> Self {
        Self {
            slot_offset_x: default_fab_slot_offset_x(),
            slot_spacing_z: default_fab_slot_spacing_z(),
            slot_radius: default_fab_slot_radius(),
            slot_height: default_fab_slot_height(),
            output_offset_x: default_fab_output_offset_x(),
            output_offset_z: default_fab_output_offset_z(),
            output_radius: default_fab_output_radius(),
            output_height: default_fab_output_height(),
            process_seconds: default_fab_process_seconds(),
        }
    }
}

// ── Heat source config ──────────────────────────────────────────────────

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct HeatSourceConfig {
    #[serde(default = "default_hs_offset_x")]
    pub offset_x: f32,
    #[serde(default = "default_hs_offset_z")]
    pub offset_z: f32,
    #[serde(default = "default_hs_radius")]
    pub radius: f32,
    #[serde(default = "default_hs_zone_radius")]
    pub zone_radius: f32,
    #[serde(default = "default_hs_light_intensity")]
    pub light_intensity: f32,
    /// Seconds of exposure before a material fully reacts.
    #[serde(default = "default_hs_reaction_seconds")]
    pub reaction_seconds: f32,
    /// Seconds of exposure before thermal_resistance is revealed.
    #[serde(default = "default_hs_reveal_seconds")]
    pub reveal_seconds: f32,
}

fn default_hs_offset_x() -> f32 {
    0.55
}
fn default_hs_offset_z() -> f32 {
    0.0
}
fn default_hs_radius() -> f32 {
    0.1
}
fn default_hs_zone_radius() -> f32 {
    1.0
}
fn default_hs_light_intensity() -> f32 {
    40_000.0
}
fn default_hs_reaction_seconds() -> f32 {
    2.5
}
fn default_hs_reveal_seconds() -> f32 {
    3.5
}

impl Default for HeatSourceConfig {
    fn default() -> Self {
        Self {
            offset_x: default_hs_offset_x(),
            offset_z: default_hs_offset_z(),
            radius: default_hs_radius(),
            zone_radius: default_hs_zone_radius(),
            light_intensity: default_hs_light_intensity(),
            reaction_seconds: default_hs_reaction_seconds(),
            reveal_seconds: default_hs_reveal_seconds(),
        }
    }
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

const DOORWAY_WIDTH: f32 = 1.6;
const DOORWAY_HEIGHT: f32 = 2.25;

fn west_wall_center_x(room_half_width: f32, wall_thickness: f32) -> f32 {
    -room_half_width - wall_thickness * 0.5
}

fn east_wall_center_x(room_half_width: f32, wall_thickness: f32) -> f32 {
    room_half_width + wall_thickness * 0.5
}

fn south_wall_center_z(room_half_depth: f32, wall_thickness: f32) -> f32 {
    -room_half_depth - wall_thickness * 0.5
}

fn north_wall_center_z(room_half_depth: f32, wall_thickness: f32) -> f32 {
    room_half_depth + wall_thickness * 0.5
}

pub(crate) fn build_room_shell_collision(
    room_half_width: f32,
    room_half_depth: f32,
    wall_thickness: f32,
) -> RoomShellCollision {
    let full_depth_with_walls = room_half_depth * 2.0 + wall_thickness * 2.0;
    let full_width_with_walls = room_half_width * 2.0 + wall_thickness * 2.0;
    let doorway_half_width = DOORWAY_WIDTH * 0.5;
    let side_wall_width = room_half_width - doorway_half_width;
    let south_z = south_wall_center_z(room_half_depth, wall_thickness);
    let north_z = north_wall_center_z(room_half_depth, wall_thickness);
    let west_x = west_wall_center_x(room_half_width, wall_thickness);
    let east_x = east_wall_center_x(room_half_width, wall_thickness);

    RoomShellCollision {
        wall_colliders: vec![
            WallCollider {
                footprint_xz: RectXZ {
                    min_x: west_x - wall_thickness * 0.5,
                    max_x: west_x + wall_thickness * 0.5,
                    min_z: -full_depth_with_walls * 0.5,
                    max_z: full_depth_with_walls * 0.5,
                },
            },
            WallCollider {
                footprint_xz: RectXZ {
                    min_x: east_x - wall_thickness * 0.5,
                    max_x: east_x + wall_thickness * 0.5,
                    min_z: -full_depth_with_walls * 0.5,
                    max_z: full_depth_with_walls * 0.5,
                },
            },
            WallCollider {
                footprint_xz: RectXZ {
                    min_x: -full_width_with_walls * 0.5,
                    max_x: full_width_with_walls * 0.5,
                    min_z: north_z - wall_thickness * 0.5,
                    max_z: north_z + wall_thickness * 0.5,
                },
            },
            WallCollider {
                footprint_xz: RectXZ {
                    min_x: -(doorway_half_width + side_wall_width),
                    max_x: -doorway_half_width,
                    min_z: south_z - wall_thickness * 0.5,
                    max_z: south_z + wall_thickness * 0.5,
                },
            },
            WallCollider {
                footprint_xz: RectXZ {
                    min_x: doorway_half_width,
                    max_x: doorway_half_width + side_wall_width,
                    min_z: south_z - wall_thickness * 0.5,
                    max_z: south_z + wall_thickness * 0.5,
                },
            },
        ],
    }
}

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
    let doorway_half_width = DOORWAY_WIDTH * 0.5;
    let side_wall_width = hx - doorway_half_width;
    let lintel_height = (h - DOORWAY_HEIGHT).max(t);
    commands.insert_resource(build_room_shell_collision(hx, hz, t));

    // West (-X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(t, h, west_east_depth))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(west_wall_center_x(hx, t), wall_y, 0.0),
    ));
    // East (+X)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(t, h, west_east_depth))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(east_wall_center_x(hx, t), wall_y, 0.0),
    ));
    // South (-Z) with a centered doorway opening.
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(side_wall_width, h, t))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(
            -(doorway_half_width + side_wall_width * 0.5),
            wall_y,
            south_wall_center_z(hz, t),
        ),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(side_wall_width, h, t))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(
            doorway_half_width + side_wall_width * 0.5,
            wall_y,
            south_wall_center_z(hz, t),
        ),
    ));
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(DOORWAY_WIDTH, lintel_height, t))),
        MeshMaterial3d(wall_mat.clone()),
        Transform::from_xyz(
            0.0,
            DOORWAY_HEIGHT + lintel_height * 0.5,
            south_wall_center_z(hz, t),
        ),
    ));
    // North (+Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(north_south_width, h, t))),
        MeshMaterial3d(wall_mat),
        Transform::from_xyz(0.0, wall_y, north_wall_center_z(hz, t)),
    ));

    let exterior_ground_size_x = hx * 6.0;
    let exterior_ground_size_z = hz * 8.0;
    let exterior_ground_center_z = -hz - exterior_ground_size_z * 0.5;
    let exterior_surface_y = -0.01;
    let exterior_ground_mat = materials.add(StandardMaterial {
        base_color: Color::srgb(0.26, 0.3, 0.22),
        perceptual_roughness: 0.98,
        ..default()
    });
    commands.insert_resource(ExteriorGroundPatch {
        bounds_xz: RectXZ {
            min_x: -exterior_ground_size_x * 0.5,
            max_x: exterior_ground_size_x * 0.5,
            min_z: exterior_ground_center_z - exterior_ground_size_z * 0.5,
            max_z: exterior_ground_center_z + exterior_ground_size_z * 0.5,
        },
        surface_y: exterior_surface_y,
    });
    commands.spawn((
        Mesh3d(
            meshes.add(
                Plane3d::default()
                    .mesh()
                    .size(exterior_ground_size_x, exterior_ground_size_z),
            ),
        ),
        MeshMaterial3d(exterior_ground_mat),
        Transform::from_xyz(0.0, exterior_surface_y, exterior_ground_center_z),
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
    // Placement plane at the true top of the workbench.
    commands.spawn((
        Surface {
            half_extent_x: fur.workbench_width * 0.5,
            half_extent_z: fur.workbench_depth * 0.5,
        },
        Transform::from_xyz(fur.workbench_x, fur.workbench_height, fur.workbench_z),
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
            Mesh3d(meshes.add(Cuboid::new(shelf_w, shelf_h, shelf_d))),
            MeshMaterial3d(shelf_mat.clone()),
            Transform::from_xyz(shelf.x, shelf.y - shelf_half_y, shelf.z),
        ));
        // Placement plane at the true top of each shelf.
        commands.spawn((
            Surface {
                half_extent_x: shelf_w * 0.5,
                half_extent_z: shelf_d * 0.5,
            },
            Shelf,
            Transform::from_xyz(shelf.x, shelf.y, shelf.z),
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
