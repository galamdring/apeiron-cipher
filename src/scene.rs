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

/// Bevy plugin that loads scene configuration and spawns the room environment.
pub struct ScenePlugin;

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
pub struct Workbench;

/// A placement plane: the actual top of a piece of furniture where objects
/// can be set down. Spawned as its own entity at the true surface Y so
/// the placement system never needs offset math.
#[derive(Component)]
pub struct Surface {
    /// Half-width of the placement area along the X axis.
    pub half_extent_x: f32,
    /// Half-depth of the placement area along the Z axis.
    pub half_extent_z: f32,
}

/// Distinguishes storage shelves from the experiment workbench so initial
/// material spawning only targets shelves.
#[derive(Component)]
pub struct Shelf;

/// Ground-plane position using world X/Z coordinates.
///
/// Bevy uses Y as vertical, so any "flat" room-shell collision math happens on
/// the X/Z plane instead of the X/Y plane familiar from CAD or 3D printing.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct PositionXZ {
    /// World X coordinate.
    pub x: f32,
    /// World Z coordinate.
    pub z: f32,
}

impl PositionXZ {
    /// Creates a new ground-plane position from X and Z coordinates.
    pub fn new(x: f32, z: f32) -> Self {
        Self { x, z }
    }
}

/// Axis-aligned rectangle on the world X/Z plane.
#[derive(Clone, Copy, Debug)]
pub struct RectXZ {
    /// Minimum X boundary.
    pub min_x: f32,
    /// Maximum X boundary.
    pub max_x: f32,
    /// Minimum Z boundary.
    pub min_z: f32,
    /// Maximum Z boundary.
    pub max_z: f32,
}

/// Collision shape for a single wall segment on the X/Z plane.
#[derive(Clone, Debug)]
pub struct WallCollider {
    /// Axis-aligned footprint of this wall segment.
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

/// Collection of wall colliders forming the room shell for movement blocking.
#[derive(Resource, Clone, Debug, Default)]
pub struct RoomShellCollision {
    /// All wall segment colliders in the room.
    pub wall_colliders: Vec<WallCollider>,
}

impl RoomShellCollision {
    /// Returns `true` if any wall segment overlaps a circle at the given position.
    pub fn blocks_circle_xz(&self, position_xz: PositionXZ, radius: f32) -> bool {
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
pub struct ExteriorGroundPatch {
    /// Axis-aligned bounds of the exterior patch on the X/Z plane.
    pub bounds_xz: RectXZ,
    /// Y height of the exterior ground surface.
    pub surface_y: f32,
}

// ── Config (TOML ↔ Rust) ─────────────────────────────────────────────────

const CONFIG_PATH: &str = "assets/config/scene.toml";

/// Top-level structure of `assets/config/scene.toml`.
#[derive(Clone, Debug, Default, Serialize, Deserialize, Resource)]
pub struct SceneConfig {
    /// Room dimensions and wall geometry.
    #[serde(default)]
    pub room: RoomConfig,
    /// Player spawn position and movement parameters.
    #[serde(default)]
    pub player: PlayerSceneConfig,
    /// Ambient, directional, and spot light settings.
    #[serde(default)]
    pub lighting: LightingConfig,
    /// Workbench and shelf placement configuration.
    #[serde(default)]
    pub furniture: FurnitureConfig,
    /// Fabricator slot and output geometry.
    #[serde(default)]
    pub fabricator: FabricatorSceneConfig,
    /// Heat source position and reaction timing.
    #[serde(default)]
    pub heat_source: HeatSourceConfig,
}

/// Configuration for the enclosed room dimensions and wall geometry.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct RoomConfig {
    /// Half the room width along the X axis.
    #[serde(default = "default_half_extent_x")]
    pub half_extent_x: f32,
    /// Half the room depth along the Z axis.
    #[serde(default = "default_half_extent_z")]
    pub half_extent_z: f32,
    /// Height of the room walls in meters.
    #[serde(default = "default_wall_height")]
    pub wall_height: f32,
    /// Thickness of each wall slab in meters.
    #[serde(default = "default_wall_thickness")]
    pub wall_thickness: f32,
    /// Inward margin from walls for player movement bounds.
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

/// Configuration for the player's spawn position and movement in the scene.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct PlayerSceneConfig {
    /// Camera height above the floor in meters.
    #[serde(default = "default_eye_height")]
    pub eye_height: f32,
    /// Initial X spawn coordinate.
    #[serde(default)]
    pub spawn_x: f32,
    /// Initial Z spawn coordinate.
    #[serde(default = "default_spawn_z")]
    pub spawn_z: f32,
    /// Movement speed in meters per second.
    #[serde(default = "default_move_speed")]
    pub move_speed: f32,
    /// Max height the player can step up onto a surface (meters).
    #[serde(default = "default_step_up_tolerance")]
    pub step_up_tolerance: f32,
    /// Height above feet at which dropped items search for a surface (meters).
    #[serde(default = "default_drop_surface_reach")]
    pub drop_surface_reach: f32,
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
fn default_step_up_tolerance() -> f32 {
    0.5
}
fn default_drop_surface_reach() -> f32 {
    1.5
}

impl Default for PlayerSceneConfig {
    fn default() -> Self {
        Self {
            eye_height: default_eye_height(),
            spawn_x: 0.0,
            spawn_z: default_spawn_z(),
            move_speed: default_move_speed(),
            step_up_tolerance: default_step_up_tolerance(),
            drop_surface_reach: default_drop_surface_reach(),
        }
    }
}

/// Configuration for all scene lighting (ambient, directional, and spot).
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct LightingConfig {
    /// Ambient light brightness level.
    #[serde(default = "default_ambient_brightness")]
    pub ambient_brightness: f32,
    /// Directional light illuminance in lux.
    #[serde(default = "default_directional_illuminance")]
    pub directional_illuminance: f32,
    /// Whether the directional light casts shadows.
    #[serde(default = "default_directional_shadows")]
    pub directional_shadows: bool,
    /// Spot light intensity in candela.
    #[serde(default = "default_spot_intensity")]
    pub spot_intensity: f32,
    /// Maximum range of the spot light in meters.
    #[serde(default = "default_spot_range")]
    pub spot_range: f32,
    /// Inner cone angle of the spot light in radians.
    #[serde(default = "default_spot_inner_angle")]
    pub spot_inner_angle: f32,
    /// Outer cone angle of the spot light in radians.
    #[serde(default = "default_spot_outer_angle")]
    pub spot_outer_angle: f32,
    /// Height of the spot light above the floor.
    #[serde(default = "default_spot_height")]
    pub spot_height: f32,
    /// Y coordinate the spot light aims at.
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

/// Configuration for workbench and shelf furniture placement.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct FurnitureConfig {
    /// Width of the workbench along the X axis.
    #[serde(default = "default_workbench_width")]
    pub workbench_width: f32,
    /// Height of the workbench in meters.
    #[serde(default = "default_workbench_height")]
    pub workbench_height: f32,
    /// Depth of the workbench along the Z axis.
    #[serde(default = "default_workbench_depth")]
    pub workbench_depth: f32,
    /// X position of the workbench center.
    #[serde(default)]
    pub workbench_x: f32,
    /// Z position of the workbench center.
    #[serde(default)]
    pub workbench_z: f32,
    /// Width of each shelf along its long axis.
    #[serde(default = "default_shelf_width")]
    pub shelf_width: f32,
    /// Vertical thickness of each shelf slab.
    #[serde(default = "default_shelf_thickness")]
    pub shelf_thickness: f32,
    /// Depth of each shelf along its short axis.
    #[serde(default = "default_shelf_depth")]
    pub shelf_depth: f32,
    /// List of individual shelf placements in the room.
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

/// Position of a single shelf in world coordinates.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ShelfConfig {
    /// X position of the shelf center.
    pub x: f32,
    /// Z position of the shelf center.
    pub z: f32,
    /// Y height of the shelf top surface.
    pub y: f32,
}

// ── Fabricator config ────────────────────────────────────────────────────

/// Configuration for fabricator input slots and output tray on the workbench.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct FabricatorSceneConfig {
    /// X offset of input slots from the workbench center.
    #[serde(default = "default_fab_slot_offset_x")]
    pub slot_offset_x: f32,
    /// Z spacing between adjacent input slots.
    #[serde(default = "default_fab_slot_spacing_z")]
    pub slot_spacing_z: f32,
    /// Radius of each input slot circle.
    #[serde(default = "default_fab_slot_radius")]
    pub slot_radius: f32,
    /// Visual height of each input slot disc.
    #[serde(default = "default_fab_slot_height")]
    pub slot_height: f32,
    /// X offset of the output tray from the workbench center.
    #[serde(default = "default_fab_output_offset_x")]
    pub output_offset_x: f32,
    /// Z offset of the output tray from the workbench center.
    #[serde(default = "default_fab_output_offset_z")]
    pub output_offset_z: f32,
    /// Radius of the output tray circle.
    #[serde(default = "default_fab_output_radius")]
    pub output_radius: f32,
    /// Visual height of the output tray disc.
    #[serde(default = "default_fab_output_height")]
    pub output_height: f32,
    /// Duration in seconds for a fabrication process to complete.
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

/// Configuration for the heat source position and reaction parameters.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub struct HeatSourceConfig {
    /// X offset of the heat source from the workbench center.
    #[serde(default = "default_hs_offset_x")]
    pub offset_x: f32,
    /// Z offset of the heat source from the workbench center.
    #[serde(default = "default_hs_offset_z")]
    pub offset_z: f32,
    /// Visual radius of the heat source element.
    #[serde(default = "default_hs_radius")]
    pub radius: f32,
    /// Radius of the heat effect zone around the source.
    #[serde(default = "default_hs_zone_radius")]
    pub zone_radius: f32,
    /// Intensity of the heat source's point light.
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
    commands.insert_resource(config.room.clone());
    commands.insert_resource(config.player.clone());
    commands.insert_resource(config.lighting.clone());
    commands.insert_resource(config.furniture.clone());
    commands.insert_resource(config.fabricator.clone());
    commands.insert_resource(config.heat_source.clone());
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

/// Builds wall colliders for the room shell including the south-wall doorway gap.
pub fn build_room_shell_collision(
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

// Bevy system — parameter count is driven by ECS query requirements, not design smell.
#[allow(clippy::too_many_arguments)]
fn setup_scene(
    mut commands: Commands,
    mut meshes: ResMut<Assets<Mesh>>,
    mut materials: ResMut<Assets<StandardMaterial>>,
    room: Res<RoomConfig>,
    lighting: Res<LightingConfig>,
    fur: Res<FurnitureConfig>,
    world_profile: Res<crate::world_generation::WorldProfile>,
    world_gen_config: Res<crate::world_generation::WorldGenerationConfig>,
    mut surface_registry: ResMut<crate::surface::SurfaceOverrideRegistry>,
) {
    let hx = room.half_extent_x;
    let hz = room.half_extent_z;
    let h = room.wall_height;
    let t = room.wall_thickness;

    // Compute the floor Y from terrain elevation at the room center (0, 0).
    let surface =
        crate::world_generation::PlanetSurface::new_from_profile(&world_profile, &world_gen_config);
    let floor_y = surface.sample_elevation(0.0, 0.0);

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

    // Floor — XZ plane, centered at terrain height.
    let floor_entity = commands
        .spawn((
            Mesh3d(meshes.add(Plane3d::default().mesh().size(hx * 2.0, hz * 2.0))),
            MeshMaterial3d(floor_mat),
            Transform::from_xyz(0.0, floor_y, 0.0),
        ))
        .id();

    // Register the room floor as a surface override so the player and
    // dropped items stand on it rather than the terrain underneath.
    surface_registry.register(crate::surface::SurfaceOverride {
        owner: floor_entity,
        min_x: -hx,
        max_x: hx,
        min_z: -hz,
        max_z: hz,
        surface_y: floor_y,
    });

    // Ceiling — same plane, flipped so normals face down into the room.
    commands.spawn((
        Mesh3d(meshes.add(Plane3d::default().mesh().size(hx * 2.0, hz * 2.0))),
        MeshMaterial3d(ceiling_mat),
        Transform::from_xyz(0.0, floor_y + h, 0.0)
            .with_rotation(Quat::from_rotation_x(core::f32::consts::PI)),
    ));

    // Four walls (thin boxes along the inner perimeter).
    let wall_y = floor_y + h * 0.5;
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
            floor_y + DOORWAY_HEIGHT + lintel_height * 0.5,
            south_wall_center_z(hz, t),
        ),
    ));
    // North (+Z)
    commands.spawn((
        Mesh3d(meshes.add(Cuboid::new(north_south_width, h, t))),
        MeshMaterial3d(wall_mat),
        Transform::from_xyz(0.0, wall_y, north_wall_center_z(hz, t)),
    ));

    // Story 5a.1: the exterior ground must cover the full active chunk
    // neighborhood (currently 3×3 chunks at 45 world-units each = 135×135).
    // We size it generously so players never see the edge. The ground is
    // centered on the Z-axis just south of the room so the player walks
    // straight into the exterior from the doorway.
    //
    // Previous sizing (hx*10 × hz*14 ≈ 40×56) only covered about one chunk,
    // which meant surface mineral deposits couldn't spawn in neighbor chunks.
    let exterior_ground_size_x = 200.0;
    let exterior_ground_size_z = 200.0;
    let exterior_ground_center_z = -hz - exterior_ground_size_z * 0.5;
    let exterior_surface_y = -0.01;
    // Story 5a.2: the monolithic green ground plane is replaced by per-chunk
    // biome-colored tiles spawned in `sync_active_exterior_chunks`. We still
    // need the ExteriorGroundPatch resource for room-vs-exterior discrimination
    // in `claim_exterior_drops`.
    commands.insert_resource(ExteriorGroundPatch {
        bounds_xz: RectXZ {
            min_x: -exterior_ground_size_x * 0.5,
            max_x: exterior_ground_size_x * 0.5,
            min_z: exterior_ground_center_z - exterior_ground_size_z * 0.5,
            max_z: exterior_ground_center_z + exterior_ground_size_z * 0.5,
        },
        surface_y: exterior_surface_y,
    });

    // Workbench — lighter, lower roughness than walls (future fabricator site).
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
        Transform::from_xyz(fur.workbench_x, floor_y + wb_half_y, fur.workbench_z),
    ));
    // Placement plane at the true top of the workbench.
    commands.spawn((
        Surface {
            half_extent_x: fur.workbench_width * 0.5,
            half_extent_z: fur.workbench_depth * 0.5,
        },
        Transform::from_xyz(
            fur.workbench_x,
            floor_y + fur.workbench_height,
            fur.workbench_z,
        ),
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
            Transform::from_xyz(shelf.x, floor_y + shelf.y - shelf_half_y, shelf.z),
        ));
        // Placement plane at the true top of each shelf.
        commands.spawn((
            Surface {
                half_extent_x: shelf_w * 0.5,
                half_extent_z: shelf_d * 0.5,
            },
            Shelf,
            Transform::from_xyz(shelf.x, floor_y + shelf.y, shelf.z),
        ));
    }

    // Directional fill — angled so forms read; lower than the open-plane setup.
    commands.spawn((
        DirectionalLight {
            illuminance: lighting.directional_illuminance,
            shadows_enabled: lighting.directional_shadows,
            ..default()
        },
        Transform::from_xyz(6.0, floor_y + 9.0, 4.0)
            .looking_at(Vec3::new(0.0, floor_y + 0.6, 0.0), Vec3::Y),
    ));

    commands.spawn(AmbientLight {
        brightness: lighting.ambient_brightness,
        ..default()
    });

    // Focused spot over the workbench — contrast for future material placement.
    let spot_y = floor_y + lighting.spot_height;
    let target_y = floor_y + lighting.spot_target_y;
    commands.spawn((
        SpotLight {
            color: Color::srgb(1.0, 0.97, 0.92),
            intensity: lighting.spot_intensity,
            range: lighting.spot_range,
            shadows_enabled: true,
            inner_angle: lighting.spot_inner_angle,
            outer_angle: lighting.spot_outer_angle,
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
