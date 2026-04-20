//! Carry plugin — the foundational data model for Epic 4's personal carry system.
//!
//! Story 4.1 is intentionally the "boring but important" foundation story. It does
//! not add the full stash / cycle / drop interaction loop yet. Instead, it builds
//! the configuration and state model that later stories will consume.
//!
//! The important split in this file is:
//! - [`CarryConfig`]: raw data loaded from `assets/config/carry.toml`
//! - [`ActiveCarryProfile`]: the resolved tuning profile selected from the config
//! - [`CarryState`]: the player's current runtime carry state
//! - [`CarryStrength`]: the player's current and future growth-oriented carry strength
//! - [`CarryDeviceState`]: whether the player currently has the configured carry-enabling item
//!
//! The code is commented heavily on purpose. Carry behavior touches config-driven
//! tuning, future persistence, and future progression, so the data boundaries need
//! to be obvious before later stories start mutating them.

use std::fs;
use std::path::Path;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::materials::GameMaterial;
use crate::player::Player;

const CONFIG_PATH: &str = "assets/config/carry.toml";

pub(crate) struct CarryPlugin;

impl Plugin for CarryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StashIntent>()
            .add_message::<CycleCarryIntent>()
            .add_message::<DropCarryIntent>()
            .add_message::<CarryWeightChanged>()
            .init_resource::<CarryConfig>()
            .init_resource::<ActiveCarryProfile>()
            .add_systems(PreStartup, load_carry_config)
            .add_systems(
                Startup,
                attach_carry_state_to_player.after(crate::player::spawn_player),
            );
    }
}

// ── Intent messages reserved for later carry stories ─────────────────────

/// Story 4.2 will emit this when the player wants to move the held item into carry.
#[derive(Message)]
pub(crate) struct StashIntent;

/// Story 4.2 will emit this when the player wants to cycle the next carried item to hand.
#[derive(Message)]
pub(crate) struct CycleCarryIntent;

/// Story 4.2 will emit this when the player wants to drop an item out of carry.
#[derive(Message)]
pub(crate) struct DropCarryIntent;

/// Later stories will emit this whenever carry weight changes so movement/stamina
/// systems can respond without polling and guessing.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
pub(crate) struct CarryWeightChanged {
    pub current_weight: f32,
    pub effective_capacity: f32,
}

// ── Runtime player state ─────────────────────────────────────────────────

/// One entry in the player's carry container.
///
/// We intentionally use a dedicated struct instead of `Vec<Entity>`. Right now
/// the only thing we need is the entity reference, but future stories are very
/// likely to add metadata here:
/// - stash order / insertion order
/// - cached material identifiers
/// - future persistence or ownership tags
///
/// Starting with a struct now avoids rewriting every caller later.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CarriedItem {
    pub entity: Entity,
}

impl CarriedItem {
    pub(crate) fn new(entity: Entity) -> Self {
        Self { entity }
    }
}

/// Current runtime carry state attached to the player entity.
///
/// `current_weight` is the sum of density values for items currently in carry.
/// `effective_capacity` is the presently usable capacity after applying the
/// current carry-device rule. Later stories may change that value over time as
/// devices are equipped or strength accretes.
#[derive(Component, Clone, Debug, PartialEq)]
pub(crate) struct CarryState {
    pub current_weight: f32,
    pub effective_capacity: f32,
    pub hard_limit_enabled: bool,
    pub carried_items: Vec<CarriedItem>,
}

impl CarryState {
    pub(crate) fn new(effective_capacity: f32, hard_limit_enabled: bool) -> Self {
        Self {
            current_weight: 0.0,
            effective_capacity,
            hard_limit_enabled,
            carried_items: Vec::new(),
        }
    }

    /// Adds a material into carry using that material's density as its weight cost.
    ///
    /// Story 4.1 does not wire the stash interaction yet, but this method is the
    /// server-side accounting rule that later intent-processing systems will call.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Story 4.2 will use these helpers when carry mutations become interactive"
        )
    )]
    pub(crate) fn add_material(&mut self, entity: Entity, material: &GameMaterial) {
        self.carried_items.push(CarriedItem::new(entity));
        self.current_weight += material.density.value;
    }

    /// Removes one carried material and subtracts that material's density cost.
    ///
    /// We search by entity because runtime carry is presently keyed by the live
    /// world entity. A future persistence story may need a richer identity model,
    /// but runtime in-session carry can safely start here.
    #[cfg_attr(
        not(test),
        expect(
            dead_code,
            reason = "Story 4.2 will use these helpers when carry mutations become interactive"
        )
    )]
    pub(crate) fn remove_material(
        &mut self,
        entity: Entity,
        material: &GameMaterial,
    ) -> Option<CarriedItem> {
        let index = self
            .carried_items
            .iter()
            .position(|item| item.entity == entity)?;
        let removed = self.carried_items.remove(index);
        self.current_weight = (self.current_weight - material.density.value).max(0.0);
        // Snap to zero when carry is empty to prevent IEEE 754 drift from
        // leaving a small positive residual after many add/remove cycles.
        if self.carried_items.is_empty() {
            self.current_weight = 0.0;
        }
        Some(removed)
    }
}

/// Current player carry strength.
///
/// The growth behavior itself lands in Story 4.4. This story only ensures the
/// player starts with an explicit, configurable strength value instead of future
/// stories inventing one ad hoc. Growth rate is owned by [`CarryConfig`], not
/// duplicated here, since it is a tuning constant rather than mutable player state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub(crate) struct CarryStrength {
    pub current: f32,
}

/// Runtime state describing whether a configured carry-enabling item is required
/// and whether the player currently has it.
///
/// This is intentionally modeled as item identity rather than a boolean because
/// the design direction is "carry may come from a real fabricated or acquired
/// object later," not "carry is always an innate stat."
#[derive(Component, Clone, Debug, PartialEq, Eq, Default)]
pub(crate) struct CarryDeviceState {
    pub required_item_key: Option<String>,
    pub equipped_item_key: Option<String>,
}

impl CarryDeviceState {
    fn from_config(config: &CarryConfig) -> Self {
        let required_item_key = config.carry_device_item_key.clone();
        let equipped_item_key = if config.grant_starting_device {
            required_item_key.clone()
        } else {
            None
        };

        Self {
            required_item_key,
            equipped_item_key,
        }
    }

    fn has_required_device(&self) -> bool {
        match (&self.required_item_key, &self.equipped_item_key) {
            (Some(required), Some(equipped)) => required == equipped,
            (Some(_), None) => false,
            (None, _) => true,
        }
    }
}

// ── Config types ─────────────────────────────────────────────────────────

/// Raw carry config loaded from `assets/config/carry.toml`.
///
/// We keep the config rich even though Story 4.1 only consumes part of it at
/// runtime. That is deliberate: this story is the data-model foundation for the
/// rest of the epic.
#[derive(Clone, Debug, Serialize, Deserialize, Resource)]
pub(crate) struct CarryConfig {
    #[serde(default = "default_active_profile")]
    pub active_profile: String,
    #[serde(default = "default_starting_capacity")]
    pub starting_capacity: f32,
    #[serde(default = "default_starting_strength")]
    pub starting_strength: f32,
    #[serde(default = "default_growth_rate")]
    pub growth_rate: f32,
    #[serde(default)]
    pub carry_device_item_key: Option<String>,
    #[serde(default)]
    pub grant_starting_device: bool,
    #[serde(default)]
    pub cycle_order: CarryCycleOrder,
    #[serde(default)]
    pub profiles: CarryProfilesConfig,
}

impl Default for CarryConfig {
    fn default() -> Self {
        Self {
            active_profile: default_active_profile(),
            starting_capacity: default_starting_capacity(),
            starting_strength: default_starting_strength(),
            growth_rate: default_growth_rate(),
            carry_device_item_key: None,
            grant_starting_device: false,
            cycle_order: CarryCycleOrder::default(),
            profiles: CarryProfilesConfig::default(),
        }
    }
}

fn default_active_profile() -> String {
    "default".into()
}

fn default_starting_capacity() -> f32 {
    5.0
}

fn default_starting_strength() -> f32 {
    1.0
}

fn default_growth_rate() -> f32 {
    0.02
}

/// How carry retrieval should behave once Story 4.2 starts cycling items.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CarryCycleOrder {
    #[default]
    Fifo,
    Lifo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub(crate) struct CarryProfilesConfig {
    #[serde(default = "default_profile_config")]
    pub default: CarryProfileConfig,
    #[serde(default = "relaxed_profile_config")]
    pub relaxed: CarryProfileConfig,
    #[serde(default = "creative_profile_config")]
    pub creative: CarryProfileConfig,
}

impl Default for CarryProfilesConfig {
    fn default() -> Self {
        Self {
            default: default_profile_config(),
            relaxed: relaxed_profile_config(),
            creative: creative_profile_config(),
        }
    }
}

/// One difficulty/mode profile's carry consequences.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct CarryProfileConfig {
    #[serde(default)]
    pub speed_curve: CarryCurveConfig,
    #[serde(default = "default_stamina_cost_multiplier")]
    pub stamina_cost_multiplier: f32,
    #[serde(default = "default_hard_limit_enabled")]
    pub hard_limit_enabled: bool,
}

fn default_profile_config() -> CarryProfileConfig {
    CarryProfileConfig {
        speed_curve: CarryCurveConfig::default(),
        stamina_cost_multiplier: default_stamina_cost_multiplier(),
        hard_limit_enabled: default_hard_limit_enabled(),
    }
}

fn relaxed_profile_config() -> CarryProfileConfig {
    CarryProfileConfig {
        speed_curve: CarryCurveConfig {
            min_multiplier: 0.75,
            exponent: 1.0,
            ..CarryCurveConfig::default()
        },
        stamina_cost_multiplier: 1.15,
        hard_limit_enabled: false,
    }
}

fn creative_profile_config() -> CarryProfileConfig {
    CarryProfileConfig {
        speed_curve: CarryCurveConfig {
            min_multiplier: 1.0,
            exponent: 1.0,
            ..CarryCurveConfig::default()
        },
        stamina_cost_multiplier: 1.0,
        hard_limit_enabled: false,
    }
}

fn default_stamina_cost_multiplier() -> f32 {
    1.4
}

fn default_hard_limit_enabled() -> bool {
    true
}

/// Config shape for future speed degradation curves.
///
/// Story 4.3 will be the first real consumer. Story 4.1 just proves these
/// values live in config and resolve deterministically into the active profile.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub(crate) struct CarryCurveConfig {
    #[serde(default)]
    pub kind: CarryCurveKind,
    #[serde(default = "default_min_multiplier")]
    pub min_multiplier: f32,
    #[serde(default = "default_curve_exponent")]
    pub exponent: f32,
}

impl Default for CarryCurveConfig {
    fn default() -> Self {
        Self {
            kind: CarryCurveKind::default(),
            min_multiplier: default_min_multiplier(),
            exponent: default_curve_exponent(),
        }
    }
}

fn default_min_multiplier() -> f32 {
    0.45
}

fn default_curve_exponent() -> f32 {
    1.35
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub(crate) enum CarryCurveKind {
    #[default]
    Linear,
    Exponential,
}

/// The resolved profile selected from [`CarryConfig::active_profile`].
///
/// This gives later systems one stable resource to read without making every
/// caller re-implement "which profile name did the config select?" logic.
#[derive(Clone, Debug, Resource, PartialEq)]
pub(crate) struct ActiveCarryProfile {
    pub profile_name: String,
    pub tuning: CarryProfileConfig,
}

impl Default for ActiveCarryProfile {
    fn default() -> Self {
        Self {
            profile_name: default_active_profile(),
            tuning: default_profile_config(),
        }
    }
}

impl ActiveCarryProfile {
    fn from_config(config: &CarryConfig) -> Self {
        let tuning = match config.active_profile.as_str() {
            "relaxed" => config.profiles.relaxed.clone(),
            "creative" => config.profiles.creative.clone(),
            _ => config.profiles.default.clone(),
        };

        Self {
            profile_name: config.active_profile.clone(),
            tuning,
        }
    }
}

// ── Systems ──────────────────────────────────────────────────────────────

fn load_carry_config(mut commands: Commands) {
    let config = if Path::new(CONFIG_PATH).exists() {
        match fs::read_to_string(CONFIG_PATH) {
            Ok(contents) => match toml::from_str::<CarryConfig>(&contents) {
                Ok(config) => {
                    info!("Loaded carry config from {CONFIG_PATH}");
                    config
                }
                Err(error) => {
                    warn!("Malformed {CONFIG_PATH}, using defaults: {error}");
                    CarryConfig::default()
                }
            },
            Err(error) => {
                warn!("Could not read {CONFIG_PATH}, using defaults: {error}");
                CarryConfig::default()
            }
        }
    } else {
        warn!("{CONFIG_PATH} not found, using defaults");
        CarryConfig::default()
    };

    let active_profile = ActiveCarryProfile::from_config(&config);
    commands.insert_resource(config);
    commands.insert_resource(active_profile);
}

fn attach_carry_state_to_player(
    mut commands: Commands,
    config: Res<CarryConfig>,
    active_profile: Res<ActiveCarryProfile>,
    player_query: Query<Entity, With<Player>>,
) {
    let Ok(player_entity) = player_query.single() else {
        return;
    };

    let device_state = CarryDeviceState::from_config(&config);
    let effective_capacity = compute_effective_capacity(config.starting_capacity, &device_state);

    commands.entity(player_entity).insert((
        CarryState::new(effective_capacity, active_profile.tuning.hard_limit_enabled),
        CarryStrength {
            current: config.starting_strength,
        },
        device_state,
    ));
}

/// Capacity depends on both the configured base capacity and the carry-device rule.
///
/// If the config names a carry-enabling item and the player does not have that
/// item equipped, the effective capacity is zero. Otherwise the configured base
/// capacity is usable immediately.
fn compute_effective_capacity(base_capacity: f32, device_state: &CarryDeviceState) -> f32 {
    if device_state.has_required_device() {
        base_capacity
    } else {
        0.0
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::materials::{GameMaterial, MaterialProperty, PropertyVisibility};

    fn material_with_density(value: f32) -> GameMaterial {
        let property = |density: f32| MaterialProperty {
            value: density,
            visibility: PropertyVisibility::Observable,
        };

        GameMaterial {
            name: "Testite".into(),
            seed: 7,
            color: [0.1, 0.2, 0.3],
            density: property(value),
            thermal_resistance: property(0.5),
            reactivity: property(0.5),
            conductivity: property(0.5),
            toxicity: property(0.5),
        }
    }

    #[test]
    fn carry_config_defaults_are_valid() {
        let config = CarryConfig::default();
        assert_eq!(config.active_profile, "default");
        assert_eq!(config.starting_capacity, 5.0);
        assert_eq!(config.cycle_order, CarryCycleOrder::Fifo);
    }

    #[test]
    fn carry_config_toml_parses_profiles_and_starting_state() {
        let toml = r#"
active_profile = "relaxed"
starting_capacity = 2.5
starting_strength = 1.2
growth_rate = 0.07
carry_device_item_key = "satchel_basic"
grant_starting_device = true
cycle_order = "lifo"

[profiles.default.speed_curve]
kind = "linear"
min_multiplier = 0.5
exponent = 1.2

[profiles.relaxed.speed_curve]
kind = "exponential"
min_multiplier = 0.8
exponent = 1.0

[profiles.creative.speed_curve]
kind = "linear"
min_multiplier = 1.0
exponent = 1.0
"#;

        let config: CarryConfig = toml::from_str(toml).expect("carry.toml should parse");
        assert_eq!(config.active_profile, "relaxed");
        assert_eq!(config.starting_capacity, 2.5);
        assert_eq!(
            config.carry_device_item_key.as_deref(),
            Some("satchel_basic")
        );
        assert!(config.grant_starting_device);
        assert_eq!(config.cycle_order, CarryCycleOrder::Lifo);
        assert_eq!(
            config.profiles.relaxed.speed_curve.kind,
            CarryCurveKind::Exponential
        );
    }

    #[test]
    fn active_profile_falls_back_to_default_when_unknown() {
        let config = CarryConfig {
            active_profile: "mystery".into(),
            ..CarryConfig::default()
        };

        let active = ActiveCarryProfile::from_config(&config);
        assert_eq!(active.profile_name, "mystery");
        assert_eq!(active.tuning, default_profile_config());
    }

    #[test]
    fn capacity_is_zero_when_required_device_is_missing() {
        let device_state = CarryDeviceState {
            required_item_key: Some("satchel_basic".into()),
            equipped_item_key: None,
        };

        assert_eq!(compute_effective_capacity(5.0, &device_state), 0.0);
    }

    #[test]
    fn capacity_uses_base_value_when_required_device_is_present() {
        let device_state = CarryDeviceState {
            required_item_key: Some("satchel_basic".into()),
            equipped_item_key: Some("satchel_basic".into()),
        };

        assert_eq!(compute_effective_capacity(5.0, &device_state), 5.0);
    }

    #[test]
    fn add_material_increases_weight_and_tracks_entity() {
        let entity = Entity::from_bits(123);
        let material = material_with_density(0.8);
        let mut state = CarryState::new(5.0, true);

        state.add_material(entity, &material);

        assert_eq!(state.current_weight, 0.8);
        assert_eq!(state.carried_items, vec![CarriedItem::new(entity)]);
    }

    #[test]
    fn remove_material_decreases_weight_and_removes_entity() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let light = material_with_density(0.2);
        let heavy = material_with_density(0.9);
        let mut state = CarryState::new(5.0, true);
        state.add_material(first, &light);
        state.add_material(second, &heavy);

        let removed = state.remove_material(second, &heavy);

        assert_eq!(removed, Some(CarriedItem::new(second)));
        assert!((state.current_weight - 0.2).abs() < f32::EPSILON);
        assert_eq!(state.carried_items, vec![CarriedItem::new(first)]);
    }
}
