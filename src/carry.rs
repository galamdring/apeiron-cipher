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

use crate::input::InputAction;
use crate::interaction::HeldItem;
use crate::journal::{JournalKey, Observation, ObservationCategory, RecordObservation};
use crate::materials::{GameMaterial, MaterialObject};
use crate::observation::{ConfidenceLevel, ConfidenceTracker};
use crate::player::{Player, PlayerCamera, cursor_is_captured};
use leafwing_input_manager::prelude::*;

const CONFIG_PATH: &str = "assets/config/carry.toml";

pub struct CarryPlugin;

impl Plugin for CarryPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<StashIntent>()
            .add_message::<CycleCarryIntent>()
            .add_message::<CarryWeightChanged>()
            .add_message::<CarryActionRejected>()
            .add_message::<StashHeldForPickup>()
            .add_message::<ObserveWeight>()
            .init_resource::<CarryConfig>()
            .init_resource::<ActiveCarryProfile>()
            .init_resource::<CarryMovementState>()
            .add_systems(PreStartup, load_carry_config)
            .add_systems(
                Startup,
                attach_carry_state_to_player.after(crate::player::spawn_player),
            )
            .add_systems(
                Update,
                (
                    update_carry_movement_state,
                    update_carry_strength,
                    emit_stash_intent,
                    emit_cycle_carry_intent,
                    process_stash_intent,
                    process_stash_held_for_pickup,
                    process_observe_weight,
                    process_cycle_carry_intent.after(process_stash_intent),
                ),
            );
    }
}

// ── Intent messages for carry actions ─────────────────────────────────────

/// Emitted when the player wants to move the held item into carry.
#[derive(Message)]
struct StashIntent;

/// Emitted when the player wants to cycle the next carried item to hand.
#[derive(Message)]
struct CycleCarryIntent;

// TODO: Incomplete refactor — DropCarryIntent and its handler were partially
// decoupled from interaction.rs but never wired up. Commented out to fix
// dead-code lint. Restore when the drop-from-carry flow is completed.
// #[derive(Message)]
// pub(crate) struct DropCarryIntent;

/// Emitted when a carry action fails so downstream systems can provide diegetic
/// feedback (visual strain, item bounce-back, audio cue, etc.).
///
/// The game never tells the player — it *shows* them. A silent no-op on a failed
/// stash leaves the player confused. This event is the hook that lets future
/// visual/audio systems translate "you can't do that" into something observable.
#[derive(Message, Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) struct CarryActionRejected {
    pub reason: CarryRejectionReason,
}

/// Why a carry action was rejected.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub(crate) enum CarryRejectionReason {
    /// Stash attempted but nothing is held in hand.
    NothingHeld,
    /// Stash attempted but adding the item would exceed effective capacity.
    OverCapacity,
    /// Cycle or drop attempted but carry container is empty.
    CarryEmpty,
    /// The next entity in carry order has been despawned (evicted from carry).
    StaleEntity,
}

/// Interaction emits this when picking up a new material while already holding one.
/// Carry handles the stash mutation and weight observation for the held item.
#[derive(Message)]
pub struct StashHeldForPickup {
    pub held_entity: Entity,
    pub held_material: GameMaterial,
    pub picked_material: GameMaterial,
}

/// Request carry to record a weight observation for a material the player just
/// interacted with (pickup, examine, etc.). Carry handles confidence tracking
/// and journal recording.
#[derive(Message)]
pub struct ObserveWeight {
    pub material: GameMaterial,
}

/// Emitted whenever carry weight changes so movement/stamina systems can
/// respond without polling.
#[derive(Message, Clone, Copy, Debug, PartialEq)]
struct CarryWeightChanged {
    pub current_weight: f32,
    pub effective_capacity: f32,
}

/// Current movement-facing interpretation of carry consequences.
///
/// `CarryState` is the source of truth for inventory mass. This resource is the
/// source of truth for *how that mass affects locomotion right now*. Keeping the
/// two separated lets later stories change the feedback model without rewriting
/// how carry contents are tracked.
#[derive(Clone, Debug, Resource, PartialEq)]
pub struct CarryMovementState {
    pub speed_modifier: f32,
    pub stamina_drain_multiplier: f32,
    pub encumbrance_ratio: f32,
    pub creative_mode: bool,
    /// Sprint speed multiplier sourced from the active carry profile config.
    pub sprint_speed_multiplier: f32,
    /// Maximum stamina from the active carry profile config.
    pub base_stamina: f32,
    /// Stamina drain per second (before the weight-based multiplier) from config.
    pub stamina_drain_per_second: f32,
    /// Stamina regen per second from config.
    pub stamina_regen_per_second: f32,
}

impl Default for CarryMovementState {
    fn default() -> Self {
        Self {
            speed_modifier: 1.0,
            stamina_drain_multiplier: 1.0,
            encumbrance_ratio: 0.0,
            creative_mode: false,
            sprint_speed_multiplier: default_sprint_speed_multiplier(),
            base_stamina: default_base_stamina(),
            stamina_drain_per_second: default_stamina_drain_per_second(),
            stamina_regen_per_second: default_stamina_regen_per_second(),
        }
    }
}

/// Marks a material entity as being in the player's carry container rather than
/// physically present in the world.
///
/// This matters because Epic 4's carry loop is not a second copy of materials.
/// The same entity moves between three states:
/// - world object (`MaterialObject`)
/// - held in hand (`HeldItem`)
/// - stashed in carry (`InCarry`)
///
/// Making that state explicit keeps later systems from accidentally treating a
/// stashed item like a world object that can still be raycast, heated, or placed.
#[derive(Component, Clone, Copy, Debug, PartialEq, Eq)]
pub struct InCarry;

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
pub struct CarriedItem {
    pub entity: Entity,
}

impl CarriedItem {
    pub fn new(entity: Entity) -> Self {
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
pub struct CarryState {
    pub current_weight: f32,
    pub effective_capacity: f32,
    pub hard_limit_enabled: bool,
    pub carried_items: Vec<CarriedItem>,
}

impl CarryState {
    pub fn new(effective_capacity: f32, hard_limit_enabled: bool) -> Self {
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
    pub fn add_material(&mut self, entity: Entity, material: &GameMaterial) {
        self.carried_items.push(CarriedItem::new(entity));
        self.current_weight += material.density.value;
    }

    /// Removes one carried material and subtracts that material's density cost.
    ///
    /// We search by entity because runtime carry is presently keyed by the live
    /// world entity. A future persistence story may need a richer identity model,
    /// but runtime in-session carry can safely start here.
    pub fn remove_material(
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

    /// Check whether there is room to stash one more material given the current
    /// weight and capacity rules.
    ///
    /// Delegates to [`Self::can_accept`] using the material's density as
    /// the weight cost so both call sites share a single epsilon
    /// tolerance and capacity policy — adding a new method that diverges
    /// here is a bug.
    pub fn can_stash(&self, material: &GameMaterial) -> bool {
        self.can_accept(material.density.value)
    }

    /// Select which carried entity should be returned next when cycling or dropping.
    ///
    /// FIFO means "oldest stashed item first." LIFO means "most recently stashed
    /// item first." We return the entity without mutating here so higher-level
    /// systems can decide the order of multi-step operations like "stash current
    /// hand item, then retrieve an older carried item."
    pub fn next_carried_entity(&self, cycle_order: CarryCycleOrder) -> Option<Entity> {
        match cycle_order {
            CarryCycleOrder::Fifo => self.carried_items.first().map(|item| item.entity),
            CarryCycleOrder::Lifo => self.carried_items.last().map(|item| item.entity),
        }
    }

    /// Returns true when the carry container can accept an item of the given weight.
    ///
    /// When `hard_limit_enabled` is true, the item is rejected if it would push
    /// `current_weight` above `effective_capacity`. When hard limits are off, the
    /// container always accepts (soft-limit feedback is handled elsewhere).
    ///
    /// The comparison uses an `f32::EPSILON` tolerance so that an item
    /// whose weight exactly fills the remaining capacity is accepted
    /// despite IEEE-754 rounding error.  This is the single canonical
    /// capacity check — [`Self::can_stash`] and [`can_stash_material`]
    /// both delegate here so all callers share the same accept/reject
    /// boundary.
    pub(crate) fn can_accept(&self, weight: f32) -> bool {
        if !self.hard_limit_enabled {
            return true;
        }
        (self.current_weight + weight) <= (self.effective_capacity + f32::EPSILON)
    }

    /// Remove a carried item by entity without needing the material reference.
    ///
    /// Used to evict stale/despawned entities from carry state. We cannot look up
    /// the material's density for a despawned entity, so weight is left unchanged.
    /// This prevents the carry from soft-locking on a dead entity while accepting
    /// that weight accounting may drift slightly. A future integrity-check system
    /// can reconcile weight by scanning remaining items.
    pub(crate) fn evict_stale_entity(&mut self, entity: Entity) -> bool {
        let Some(index) = self
            .carried_items
            .iter()
            .position(|item| item.entity == entity)
        else {
            return false;
        };
        self.carried_items.remove(index);
        true
    }
}

/// Current player carry strength.
///
/// The growth behavior itself lands in Story 4.4. This story only ensures the
/// player starts with an explicit, configurable strength value instead of future
/// stories inventing one ad hoc. Growth rate is owned by [`CarryConfig`], not
/// duplicated here, since it is a tuning constant rather than mutable player state.
#[derive(Component, Clone, Copy, Debug, PartialEq)]
pub struct CarryStrength {
    pub current: f32,
}

/// Runtime state describing whether a configured carry-enabling item is required
/// and whether the player currently has it.
///
/// This is intentionally modeled as item identity rather than a boolean because
/// the design direction is "carry may come from a real fabricated or acquired
/// object later," not "carry is always an innate stat."
#[derive(Component, Clone, Debug, PartialEq, Eq, Default)]
struct CarryDeviceState {
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
pub struct CarryConfig {
    /// Camera-relative offset for held items. Default: `[0.2, -0.15, -0.5]`.
    ///
    /// Previously hardcoded as `HOLD_OFFSET` in interaction.rs; now data-driven
    /// so artists can tweak the hold position without recompiling.
    #[serde(default = "default_hold_offset")]
    pub hold_offset: [f32; 3],
    #[serde(default)]
    pub active_profile: CarryProfileSelection,
    #[serde(default = "default_starting_capacity")]
    pub starting_capacity: f32,
    #[serde(default = "default_starting_strength")]
    pub starting_strength: f32,
    #[serde(default = "default_growth_rate")]
    pub growth_rate: f32,
    #[serde(default)]
    pub growth_curve: CarryGrowthCurveConfig,
    #[serde(default)]
    pub carry_device_item_key: Option<String>,
    #[serde(default)]
    pub grant_starting_device: bool,
    #[serde(default)]
    pub cycle_order: CarryCycleOrder,
    #[serde(default = "default_weight_descriptions")]
    pub weight_descriptions: Vec<WeightDescriptionBand>,
    #[serde(default)]
    pub weight_cues: CarryCueConfig,
    #[serde(default)]
    pub profiles: CarryProfilesConfig,
}

impl Default for CarryConfig {
    fn default() -> Self {
        Self {
            hold_offset: default_hold_offset(),
            active_profile: CarryProfileSelection::default(),
            starting_capacity: default_starting_capacity(),
            starting_strength: default_starting_strength(),
            growth_rate: default_growth_rate(),
            growth_curve: CarryGrowthCurveConfig::default(),
            carry_device_item_key: None,
            grant_starting_device: false,
            cycle_order: CarryCycleOrder::default(),
            weight_descriptions: default_weight_descriptions(),
            weight_cues: CarryCueConfig::default(),
            profiles: CarryProfilesConfig::default(),
        }
    }
}

impl CarryConfig {
    /// Convenience accessor that converts the TOML-friendly `[f32; 3]` into a
    /// `Vec3` for use in transform operations.
    pub fn hold_offset_vec3(&self) -> Vec3 {
        Vec3::from_array(self.hold_offset)
    }
}

fn default_hold_offset() -> [f32; 3] {
    [0.2, -0.15, -0.5]
}

/// Which carry tuning profile is active.
///
/// Stored as an enum (rather than a free-form string) so the config
/// loader cannot end up pointing at a profile name that no profile in
/// [`CarryProfilesConfig`] actually defines.  Adding a new profile means
/// adding a variant here, a field on `CarryProfilesConfig`, and an arm
/// in [`ActiveCarryProfile::from_config`] — the compiler enforces all
/// three at once.
///
/// `serde` uses snake_case so `carry.toml` keeps the existing
/// `active_profile = "default"` / `"relaxed"` / `"creative"` spellings.
/// Unknown values from a hand-edited TOML file fall back to
/// [`Self::Default`] via `#[serde(other)]` rather than failing the
/// whole load — the same lenient behaviour the previous string-based
/// match had.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CarryProfileSelection {
    /// Standard difficulty: hard capacity limit, normal stamina drain.
    #[default]
    Default,
    /// Forgiving variant for casual play.
    Relaxed,
    /// Carry weight has no movement or stamina effect (sandbox/testing).
    Creative,
    /// Unknown selector from a hand-edited TOML — treated as
    /// [`Self::Default`] at resolution time.
    #[serde(other)]
    Unknown,
}

impl CarryProfileSelection {
    /// Stable string used for save-data debugging, log lines, and the
    /// creative-mode toggle in `update_carry_strength` (it preserves
    /// the previous behaviour of comparing the active profile name to
    /// `"creative"`).
    pub fn as_str(self) -> &'static str {
        match self {
            CarryProfileSelection::Default => "default",
            CarryProfileSelection::Relaxed => "relaxed",
            CarryProfileSelection::Creative => "creative",
            CarryProfileSelection::Unknown => "unknown",
        }
    }
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

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CarryGrowthCurveConfig {
    #[serde(default)]
    pub kind: CarryGrowthCurveKind,
    #[serde(default = "default_growth_curve_cap")]
    pub max_strength: f32,
}

impl Default for CarryGrowthCurveConfig {
    fn default() -> Self {
        Self {
            kind: CarryGrowthCurveKind::default(),
            max_strength: default_growth_curve_cap(),
        }
    }
}

fn default_growth_curve_cap() -> f32 {
    8.0
}

#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CarryGrowthCurveKind {
    #[default]
    Linear,
    Logarithmic,
    Asymptotic,
}

#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct WeightDescriptionBand {
    pub max_ratio: f32,
    pub text: String,
}

fn default_weight_descriptions() -> Vec<WeightDescriptionBand> {
    vec![
        WeightDescriptionBand {
            max_ratio: 0.1,
            text: "Almost weightless".into(),
        },
        WeightDescriptionBand {
            max_ratio: 0.3,
            text: "Light enough to carry easily".into(),
        },
        WeightDescriptionBand {
            max_ratio: 0.5,
            text: "Solid weight".into(),
        },
        WeightDescriptionBand {
            max_ratio: 0.7,
            text: "Heavy but manageable".into(),
        },
        WeightDescriptionBand {
            max_ratio: 0.9,
            text: "Straining under the weight".into(),
        },
        WeightDescriptionBand {
            max_ratio: f32::INFINITY,
            text: "Barely able to lift".into(),
        },
    ]
}

/// Config for Story 4.5's subtle sensory carry cues.
///
/// These values intentionally live alongside the rest of the carry tuning in
/// `carry.toml`, because the goal is "weight feels physical through multiple
/// channels" rather than "camera math and audio live in unrelated systems."
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CarryCueConfig {
    #[serde(default = "default_footstep_interval_seconds")]
    pub footstep_interval_seconds: f32,
    #[serde(default = "default_footstep_base_volume")]
    pub footstep_base_volume: f32,
    #[serde(default = "default_footstep_max_volume")]
    pub footstep_max_volume: f32,
    #[serde(default = "default_footstep_light_speed")]
    pub footstep_light_speed: f32,
    #[serde(default = "default_footstep_heavy_speed")]
    pub footstep_heavy_speed: f32,
    #[serde(default = "default_bob_base_amplitude")]
    pub bob_base_amplitude: f32,
    #[serde(default = "default_bob_weight_amplitude")]
    pub bob_weight_amplitude: f32,
    #[serde(default = "default_bob_frequency")]
    pub bob_frequency: f32,
    #[serde(default = "default_bob_sprint_multiplier")]
    pub bob_sprint_multiplier: f32,
    #[serde(default = "default_breathing_start_ratio")]
    pub breathing_start_ratio: f32,
    #[serde(default = "default_breathing_full_ratio")]
    pub breathing_full_ratio: f32,
    #[serde(default = "default_breathing_max_volume")]
    pub breathing_max_volume: f32,
    #[serde(default = "default_breathing_base_speed")]
    pub breathing_base_speed: f32,
    #[serde(default = "default_breathing_heavy_speed")]
    pub breathing_heavy_speed: f32,
    #[serde(default = "default_footstep_tone_hz")]
    pub footstep_tone_hz: f32,
    #[serde(default = "default_footstep_duration_ms")]
    pub footstep_duration_ms: u64,
    #[serde(default = "default_breathing_tone_hz")]
    pub breathing_tone_hz: f32,
    #[serde(default = "default_breathing_cycle_ms")]
    pub breathing_cycle_ms: u64,
    #[serde(default = "default_bob_forward_ratio")]
    pub bob_forward_ratio: f32,
    #[serde(default = "default_footstep_sprint_cadence")]
    pub footstep_sprint_cadence: f32,
    /// Exponential decay rate (per second) for the camera bob when the player
    /// stops moving. Higher values mean a faster snap back to neutral.
    #[serde(default = "default_bob_decay_rate")]
    pub bob_decay_rate: f32,
}

impl Default for CarryCueConfig {
    fn default() -> Self {
        Self {
            footstep_interval_seconds: default_footstep_interval_seconds(),
            footstep_base_volume: default_footstep_base_volume(),
            footstep_max_volume: default_footstep_max_volume(),
            footstep_light_speed: default_footstep_light_speed(),
            footstep_heavy_speed: default_footstep_heavy_speed(),
            bob_base_amplitude: default_bob_base_amplitude(),
            bob_weight_amplitude: default_bob_weight_amplitude(),
            bob_frequency: default_bob_frequency(),
            bob_sprint_multiplier: default_bob_sprint_multiplier(),
            breathing_start_ratio: default_breathing_start_ratio(),
            breathing_full_ratio: default_breathing_full_ratio(),
            breathing_max_volume: default_breathing_max_volume(),
            breathing_base_speed: default_breathing_base_speed(),
            breathing_heavy_speed: default_breathing_heavy_speed(),
            footstep_tone_hz: default_footstep_tone_hz(),
            footstep_duration_ms: default_footstep_duration_ms(),
            breathing_tone_hz: default_breathing_tone_hz(),
            breathing_cycle_ms: default_breathing_cycle_ms(),
            bob_forward_ratio: default_bob_forward_ratio(),
            footstep_sprint_cadence: default_footstep_sprint_cadence(),
            bob_decay_rate: default_bob_decay_rate(),
        }
    }
}

fn default_footstep_interval_seconds() -> f32 {
    0.42
}

fn default_footstep_base_volume() -> f32 {
    0.02
}

fn default_footstep_max_volume() -> f32 {
    0.06
}

fn default_footstep_light_speed() -> f32 {
    1.2
}

fn default_footstep_heavy_speed() -> f32 {
    0.8
}

fn default_bob_base_amplitude() -> f32 {
    0.01
}

fn default_bob_weight_amplitude() -> f32 {
    0.015
}

fn default_bob_frequency() -> f32 {
    8.0
}

fn default_bob_sprint_multiplier() -> f32 {
    1.35
}

fn default_breathing_start_ratio() -> f32 {
    0.75
}

fn default_breathing_full_ratio() -> f32 {
    1.0
}

fn default_breathing_max_volume() -> f32 {
    0.035
}

fn default_breathing_base_speed() -> f32 {
    0.9
}

fn default_breathing_heavy_speed() -> f32 {
    1.15
}

fn default_footstep_tone_hz() -> f32 {
    180.0
}

fn default_footstep_duration_ms() -> u64 {
    65
}

fn default_breathing_tone_hz() -> f32 {
    110.0
}

fn default_breathing_cycle_ms() -> u64 {
    1100
}

fn default_bob_forward_ratio() -> f32 {
    0.35
}

fn default_footstep_sprint_cadence() -> f32 {
    0.78
}

/// Derived so the exponential decay matches the original per-frame factor of
/// 0.18 at 60 fps: rate = -ln(1 - 0.18) / (1/60) ≈ 11.9.
fn default_bob_decay_rate() -> f32 {
    11.9
}

/// How carry retrieval should behave once Story 4.2 starts cycling items.
#[derive(Clone, Copy, Debug, Serialize, Deserialize, PartialEq, Eq, Default)]
#[serde(rename_all = "snake_case")]
pub enum CarryCycleOrder {
    #[default]
    Fifo,
    Lifo,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CarryProfilesConfig {
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
pub struct CarryProfileConfig {
    #[serde(default)]
    pub speed_curve: CarryCurveConfig,
    #[serde(default = "default_stamina_cost_multiplier")]
    pub stamina_cost_multiplier: f32,
    #[serde(default = "default_hard_limit_enabled")]
    pub hard_limit_enabled: bool,
    /// When true, carry weight has no effect on movement or stamina.
    #[serde(default)]
    pub creative_mode: bool,
    #[serde(default = "default_sprint_speed_multiplier")]
    pub sprint_speed_multiplier: f32,
    #[serde(default = "default_base_stamina")]
    pub base_stamina: f32,
    #[serde(default = "default_stamina_drain_per_second")]
    pub stamina_drain_per_second: f32,
    #[serde(default = "default_stamina_regen_per_second")]
    pub stamina_regen_per_second: f32,
}

fn default_profile_config() -> CarryProfileConfig {
    CarryProfileConfig {
        speed_curve: CarryCurveConfig::default(),
        stamina_cost_multiplier: default_stamina_cost_multiplier(),
        hard_limit_enabled: default_hard_limit_enabled(),
        creative_mode: false,
        sprint_speed_multiplier: default_sprint_speed_multiplier(),
        base_stamina: default_base_stamina(),
        stamina_drain_per_second: default_stamina_drain_per_second(),
        stamina_regen_per_second: default_stamina_regen_per_second(),
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
        creative_mode: false,
        sprint_speed_multiplier: default_sprint_speed_multiplier(),
        base_stamina: default_base_stamina(),
        stamina_drain_per_second: default_stamina_drain_per_second(),
        stamina_regen_per_second: default_stamina_regen_per_second(),
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
        creative_mode: true,
        sprint_speed_multiplier: default_sprint_speed_multiplier(),
        base_stamina: default_base_stamina(),
        stamina_drain_per_second: default_stamina_drain_per_second(),
        stamina_regen_per_second: default_stamina_regen_per_second(),
    }
}

fn default_stamina_cost_multiplier() -> f32 {
    1.4
}

fn default_hard_limit_enabled() -> bool {
    true
}
fn default_sprint_speed_multiplier() -> f32 {
    1.45
}
fn default_base_stamina() -> f32 {
    100.0
}
fn default_stamina_drain_per_second() -> f32 {
    22.0
}
fn default_stamina_regen_per_second() -> f32 {
    14.0
}

/// Config shape for speed degradation curves.
///
/// Defines how carry weight translates into movement speed penalties.
/// Values are loaded from `carry.toml` and resolved into the active profile.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct CarryCurveConfig {
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
pub enum CarryCurveKind {
    #[default]
    Linear,
    Exponential,
}

/// The resolved profile selected from [`CarryConfig::active_profile`].
///
/// This gives later systems one stable resource to read without making every
/// caller re-implement "which profile name did the config select?" logic.
#[derive(Clone, Debug, Resource, PartialEq)]
struct ActiveCarryProfile {
    pub selection: CarryProfileSelection,
    pub tuning: CarryProfileConfig,
}

impl Default for ActiveCarryProfile {
    fn default() -> Self {
        Self {
            selection: CarryProfileSelection::default(),
            tuning: default_profile_config(),
        }
    }
}

impl ActiveCarryProfile {
    fn from_config(config: &CarryConfig) -> Self {
        // `Unknown` falls back to the default tuning, mirroring the
        // previous string-based `_ =>` branch.  Keeping the selection
        // value as-recorded preserves the diagnostic information so a
        // misconfigured TOML file is still visible in logs/state dumps.
        let tuning = match config.active_profile {
            CarryProfileSelection::Relaxed => config.profiles.relaxed.clone(),
            CarryProfileSelection::Creative => config.profiles.creative.clone(),
            CarryProfileSelection::Default | CarryProfileSelection::Unknown => {
                config.profiles.default.clone()
            }
        };

        Self {
            selection: config.active_profile,
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
    // Normalize TOML sentinel for the final weight band. TOML doesn't support
    // infinity literals, so authors use a large number like 9999.0. We convert
    // to f32::INFINITY so band lookup never falls through for extreme ratios.
    let mut config = config;
    config.weight_descriptions.sort_by(|a, b| {
        a.max_ratio
            .partial_cmp(&b.max_ratio)
            .unwrap_or(std::cmp::Ordering::Equal)
    });
    if let Some(last) = config.weight_descriptions.last_mut()
        && last.max_ratio >= 9999.0
    {
        last.max_ratio = f32::INFINITY;
    }
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

/// Convert current carry state into movement-facing consequences.
///
/// Only runs the recomputation when `CarryState` or `ActiveCarryProfile`
/// actually change, avoiding redundant writes on idle frames.
fn update_carry_movement_state(
    active_profile: Res<ActiveCarryProfile>,
    mut movement_state: ResMut<CarryMovementState>,
    player_query: Query<(Ref<CarryState>,), With<Player>>,
) {
    let Ok((carry_ref,)) = player_query.single() else {
        return;
    };

    // Skip recomputation when neither input changed this frame.
    if !carry_ref.is_changed() && !active_profile.is_changed() {
        return;
    }

    let carry_state = &*carry_ref;

    // Always propagate the stamina tuning knobs from the active profile so
    // player.rs never needs its own hardcoded copies.
    let sprint_speed_multiplier = active_profile.tuning.sprint_speed_multiplier;
    let base_stamina = active_profile.tuning.base_stamina;
    let stamina_drain_per_second = active_profile.tuning.stamina_drain_per_second;
    let stamina_regen_per_second = active_profile.tuning.stamina_regen_per_second;

    if active_profile.tuning.creative_mode {
        *movement_state = CarryMovementState {
            speed_modifier: 1.0,
            stamina_drain_multiplier: 1.0,
            encumbrance_ratio: 0.0,
            creative_mode: true,
            sprint_speed_multiplier,
            base_stamina,
            stamina_drain_per_second,
            stamina_regen_per_second,
        };
        return;
    }

    let encumbrance_ratio = if carry_state.effective_capacity <= f32::EPSILON {
        if carry_state.current_weight > 0.0 {
            1.0
        } else {
            0.0
        }
    } else {
        (carry_state.current_weight / carry_state.effective_capacity).max(0.0)
    };

    let speed_modifier = evaluate_speed_curve(
        &active_profile.tuning.speed_curve,
        encumbrance_ratio,
        carry_state.hard_limit_enabled,
    );
    let stamina_drain_multiplier =
        1.0 + encumbrance_ratio.max(0.0) * (active_profile.tuning.stamina_cost_multiplier - 1.0);

    *movement_state = CarryMovementState {
        speed_modifier,
        stamina_drain_multiplier,
        encumbrance_ratio,
        creative_mode: false,
        sprint_speed_multiplier,
        base_stamina,
        stamina_drain_per_second,
        stamina_regen_per_second,
    };
}

fn update_carry_strength(
    time: Res<Time>,
    active_profile: Res<ActiveCarryProfile>,
    config: Res<CarryConfig>,
    mut player_query: Query<(&CarryState, &mut CarryStrength), With<Player>>,
) {
    if active_profile.selection == CarryProfileSelection::Creative {
        return;
    }

    let Ok((carry_state, mut strength)) = player_query.single_mut() else {
        return;
    };
    if carry_state.current_weight <= f32::EPSILON {
        return;
    }

    let delta = carry_strength_delta(
        carry_state.current_weight,
        strength.current,
        config.growth_rate,
        &config.growth_curve,
        time.delta_secs(),
    );
    strength.current = (strength.current + delta).min(config.growth_curve.max_strength);
}

fn evaluate_speed_curve(
    curve: &CarryCurveConfig,
    encumbrance_ratio: f32,
    hard_limit_enabled: bool,
) -> f32 {
    let clamped_ratio = if hard_limit_enabled {
        encumbrance_ratio.clamp(0.0, 1.0)
    } else {
        encumbrance_ratio.max(0.0)
    };

    let falloff = match curve.kind {
        CarryCurveKind::Linear => clamped_ratio.powf(curve.exponent.max(0.01)),
        CarryCurveKind::Exponential => 1.0 - (-clamped_ratio * curve.exponent.max(0.01)).exp(),
    };

    let base = 1.0 - (1.0 - curve.min_multiplier) * falloff;
    if hard_limit_enabled {
        base.max(curve.min_multiplier)
    } else {
        base.max(0.1)
    }
}

fn carry_strength_delta(
    current_weight: f32,
    current_strength: f32,
    growth_rate: f32,
    growth_curve: &CarryGrowthCurveConfig,
    delta_seconds: f32,
) -> f32 {
    // Defensive clamps: all inputs must be non-negative. A misconfigured TOML
    // (e.g. negative growth_rate) or future bug (negative strength) must not
    // cause strength to decrease or produce NaN via ln_1p().
    let base = current_weight.max(0.0) * growth_rate.max(0.0) * delta_seconds.max(0.0);
    let safe_strength = current_strength.max(0.0);
    match growth_curve.kind {
        CarryGrowthCurveKind::Linear => base,
        // Accelerating growth — strength gains are slow when weak and speed up
        // as the player practices.  Uses a power-curve with a 10 % floor so
        // the very first lift still yields *some* progress.
        //
        // Shape: base × (0.1 + 0.9 × (s/max)²)
        CarryGrowthCurveKind::Logarithmic => {
            if growth_curve.max_strength <= f32::EPSILON {
                return 0.0;
            }
            let t = (safe_strength / growth_curve.max_strength).clamp(0.0, 1.0);
            base * (0.1 + 0.9 * t * t)
        }
        // S-curve growth — slow start *and* slow finish, with the fastest
        // gains in the middle of the strength range.  This is the "practice
        // makes perfect … up to a point" curve.
        //
        // Shape: base × 4 × t × (1 − t)   (parabola peaking at t = 0.5)
        CarryGrowthCurveKind::Asymptotic => {
            if growth_curve.max_strength <= f32::EPSILON {
                return 0.0;
            }
            let t = (safe_strength / growth_curve.max_strength).clamp(0.0, 1.0);
            base * 4.0 * t * (1.0 - t)
        }
    }
}

fn describe_weight_observation(
    density: f32,
    carry_strength: f32,
    confidence: ConfidenceLevel,
    bands: &[WeightDescriptionBand],
) -> String {
    let ratio = if carry_strength <= f32::EPSILON {
        f32::INFINITY
    } else {
        density / carry_strength
    };
    let base = bands
        .iter()
        .find(|band| ratio <= band.max_ratio)
        .map(|band| band.text.as_str())
        .unwrap_or("Barely able to lift");

    match confidence {
        ConfidenceLevel::Tentative => format!("Seemed {}", base.to_lowercase()),
        ConfidenceLevel::Observed | ConfidenceLevel::Confident => base.to_string(),
    }
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

// ── Input → carry intents ────────────────────────────────────────────────

/// Shared guard: returns the player's action state only if cursor is captured.
fn player_action_if_captured<'a>(
    player_query: &'a Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: &bevy::window::CursorOptions,
) -> Option<&'a ActionState<InputAction>> {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return None;
    }
    player_query.single().ok()
}

/// Generate a Bevy system that fires a unit-struct message when a specific
/// [`InputAction`] is `just_pressed` and the cursor is captured.
///
/// Every carry-intent emitter follows the same three-step pattern:
/// 1. Guard on cursor capture via [`player_action_if_captured`].
/// 2. Check `just_pressed` for one [`InputAction`] variant.
/// 3. Write one unit-struct message.
///
/// Adding a new intent (e.g. a future `DropIntent`) is a single macro call.
macro_rules! emit_intent_system {
    ($fn_name:ident, $action:expr, $msg:ident) => {
        fn $fn_name(
            player_query: Query<&ActionState<InputAction>, With<Player>>,
            cursor_options: Single<&bevy::window::CursorOptions>,
            mut writer: MessageWriter<$msg>,
        ) {
            if let Some(action) = player_action_if_captured(&player_query, &cursor_options)
                && action.just_pressed(&$action)
            {
                writer.write($msg);
            }
        }
    };
}

emit_intent_system!(emit_stash_intent, InputAction::Stash, StashIntent);
emit_intent_system!(
    emit_cycle_carry_intent,
    InputAction::CycleCarry,
    CycleCarryIntent
);

// ── Carry mutation helpers ───────────────────────────────────────────────

/// Free-function alias of [`CarryState::can_stash`] kept for callers that
/// already pattern-match on `(state, material)` parameters.  Delegates to
/// the method so all capacity decisions go through one epsilon-tolerant
/// comparison ([`CarryState::can_accept`]).
pub fn can_stash_material(carry_state: &CarryState, material: &GameMaterial) -> bool {
    carry_state.can_stash(material)
}

/// Convert a held world material into a stashed carry item.
///
/// The entity itself stays alive. We are not cloning or re-instantiating the
/// material for carry. Instead, we move the same entity out of the world-facing
/// state and into an inventory-facing state:
/// - remove `HeldItem` because it is no longer in hand
/// - remove `MaterialObject` because it should no longer behave like a world prop
/// - add `InCarry` to make the state explicit for later systems
/// - hide the entity so it stops rendering
pub fn stash_entity_into_carry(
    commands: &mut Commands,
    carry_state: &mut CarryState,
    entity: Entity,
    material: &GameMaterial,
) {
    carry_state.add_material(entity, material);
    commands
        .entity(entity)
        .remove::<HeldItem>()
        .remove::<MaterialObject>()
        .remove_parent_in_place()
        .insert(InCarry)
        .insert(Visibility::Hidden);
}

pub fn record_weight_observation(
    material: &GameMaterial,
    carry_strength: f32,
    config: &CarryConfig,
    tracker: &mut ConfidenceTracker,
    journal_writer: &mut MessageWriter<RecordObservation>,
) {
    tracker.record(material.seed, "weight");
    let confidence = tracker.level(material.seed, "weight");
    let description = describe_weight_observation(
        material.density.value,
        carry_strength,
        confidence,
        &config.weight_descriptions,
    );
    journal_writer.write(RecordObservation {
        key: JournalKey::Material {
            seed: material.seed,
        },
        name: material.name.clone(),
        observation: Observation {
            category: ObservationCategory::Weight,
            confidence,
            description,
            recorded_at: 0,
        },
    });
}

/// Convert a stashed carry item back into the player's hand.
///
/// We restore the entity into the world-facing material state because the hand
/// interaction loop already understands `HeldItem + MaterialObject`. Reusing that
/// path keeps Epic 4 from inventing a second representation for "the material in
/// front of the camera."
fn move_entity_from_carry_to_hand(
    commands: &mut Commands,
    camera_entity: Entity,
    entity: Entity,
    hold_offset: Vec3,
) {
    commands
        .entity(entity)
        .remove::<InCarry>()
        .insert(MaterialObject)
        .insert(HeldItem)
        .insert(Visibility::Inherited)
        .set_parent_in_place(camera_entity)
        .insert(Transform::from_translation(hold_offset));
}

fn emit_carry_weight_changed(
    writer: &mut MessageWriter<CarryWeightChanged>,
    carry_state: &CarryState,
) {
    writer.write(CarryWeightChanged {
        current_weight: carry_state.current_weight,
        effective_capacity: carry_state.effective_capacity,
    });
}

// ── Server-side carry processing ─────────────────────────────────────────

// This system now spans input, carry state, observation confidence, and journal
// recording. Keeping the parameters explicit is still easier to read than
// hiding the ECS touch points behind wrapper structs.
#[allow(clippy::too_many_arguments)]
fn process_stash_intent(
    mut commands: Commands,
    mut reader: MessageReader<StashIntent>,
    mut weight_writer: MessageWriter<CarryWeightChanged>,
    mut reject_writer: MessageWriter<CarryActionRejected>,
    mut journal_writer: MessageWriter<RecordObservation>,
    mut tracker: ResMut<ConfidenceTracker>,
    config: Res<CarryConfig>,
    mut player_query: Query<(&mut CarryState, &CarryStrength), With<Player>>,
    held_query: Query<(Entity, &GameMaterial), With<HeldItem>>,
) {
    for _intent in reader.read() {
        let Some((held_entity, held_material)) = held_query.iter().next() else {
            reject_writer.write(CarryActionRejected {
                reason: CarryRejectionReason::NothingHeld,
            });
            continue;
        };
        let Ok((mut carry_state, carry_strength)) = player_query.single_mut() else {
            continue;
        };
        // Single capacity gate — `can_stash_material`, `CarryState::can_stash`,
        // and `CarryState::can_accept` all share one epsilon-tolerant
        // comparison so we cannot get inconsistent accept/reject decisions
        // between the two former pre-checks that used to live here.
        if !can_stash_material(&carry_state, held_material) {
            reject_writer.write(CarryActionRejected {
                reason: CarryRejectionReason::OverCapacity,
            });
            continue;
        }

        stash_entity_into_carry(&mut commands, &mut carry_state, held_entity, held_material);
        record_weight_observation(
            held_material,
            carry_strength.current,
            &config,
            &mut tracker,
            &mut journal_writer,
        );
        emit_carry_weight_changed(&mut weight_writer, &carry_state);
    }
}

/// Handle interaction's request to stash the held item so a new pickup can proceed.
///
/// This keeps all carry-state mutation inside the carry module. Interaction only
/// needs to read `CarryState` for the capacity gate and emit this message.
#[allow(clippy::too_many_arguments)]
fn process_stash_held_for_pickup(
    mut commands: Commands,
    mut reader: MessageReader<StashHeldForPickup>,
    mut weight_writer: MessageWriter<CarryWeightChanged>,
    mut journal_writer: MessageWriter<RecordObservation>,
    mut tracker: ResMut<ConfidenceTracker>,
    config: Res<CarryConfig>,
    mut player_query: Query<(&mut CarryState, &CarryStrength), With<Player>>,
) {
    for request in reader.read() {
        let Ok((mut carry_state, carry_strength)) = player_query.single_mut() else {
            continue;
        };

        stash_entity_into_carry(
            &mut commands,
            &mut carry_state,
            request.held_entity,
            &request.held_material,
        );
        record_weight_observation(
            &request.held_material,
            carry_strength.current,
            &config,
            &mut tracker,
            &mut journal_writer,
        );
        // Also record weight for the newly picked material.
        record_weight_observation(
            &request.picked_material,
            carry_strength.current,
            &config,
            &mut tracker,
            &mut journal_writer,
        );
        emit_carry_weight_changed(&mut weight_writer, &carry_state);
    }
}

/// Handle standalone weight observation requests from interaction (pickup without stash).
fn process_observe_weight(
    mut reader: MessageReader<ObserveWeight>,
    mut journal_writer: MessageWriter<RecordObservation>,
    mut tracker: ResMut<ConfidenceTracker>,
    config: Res<CarryConfig>,
    player_query: Query<&CarryStrength, With<Player>>,
) {
    for request in reader.read() {
        let Ok(carry_strength) = player_query.single() else {
            continue;
        };
        record_weight_observation(
            &request.material,
            carry_strength.current,
            &config,
            &mut tracker,
            &mut journal_writer,
        );
    }
}

// Bevy system signatures get wide when they touch both input-derived state and
// ECS mutation points. Keeping the queries explicit is more readable than
// hiding them behind wrapper resources or tuple aliases here.
#[allow(clippy::too_many_arguments)]
fn process_cycle_carry_intent(
    mut commands: Commands,
    mut reader: MessageReader<CycleCarryIntent>,
    mut weight_writer: MessageWriter<CarryWeightChanged>,
    mut reject_writer: MessageWriter<CarryActionRejected>,
    mut journal_writer: MessageWriter<RecordObservation>,
    mut tracker: ResMut<ConfidenceTracker>,
    config: Res<CarryConfig>,
    mut player_query: Query<(&mut CarryState, &CarryStrength), With<Player>>,
    camera_query: Query<Entity, With<PlayerCamera>>,
    held_query: Query<(Entity, &GameMaterial), With<HeldItem>>,
    carried_material_query: Query<&GameMaterial, With<InCarry>>,
) {
    for _intent in reader.read() {
        let Ok((mut carry_state, carry_strength)) = player_query.single_mut() else {
            continue;
        };

        let Some(next_entity) = carry_state.next_carried_entity(config.cycle_order) else {
            reject_writer.write(CarryActionRejected {
                reason: CarryRejectionReason::CarryEmpty,
            });
            continue;
        };

        let Ok(next_material) = carried_material_query.get(next_entity) else {
            // Entity was despawned — evict it so carry doesn't soft-lock.
            carry_state.evict_stale_entity(next_entity);
            reject_writer.write(CarryActionRejected {
                reason: CarryRejectionReason::StaleEntity,
            });
            continue;
        };
        let Ok(camera_entity) = camera_query.single() else {
            continue;
        };

        // Extract held item data before mutating carry state.
        // Only density is needed for the capacity check; the full &GameMaterial
        // reference is re-acquired after stashing for the weight observation.
        let held_info = held_query
            .iter()
            .next()
            .map(|(entity, material)| (entity, material.clone()));
        if let Some((held_entity, held_material)) = held_info.as_ref() {
            if !carry_state.can_stash(held_material) {
                continue;
            }
            // Re-acquire the material ref — entity is still alive, query is
            // still valid since we only mutated CarryState (separate component).
            let Ok((_, held_material)) = held_query.get(*held_entity) else {
                continue;
            };
            stash_entity_into_carry(&mut commands, &mut carry_state, *held_entity, held_material);
            record_weight_observation(
                held_material,
                carry_strength.current,
                &config,
                &mut tracker,
                &mut journal_writer,
            );
        }

        let Some(_removed) = carry_state.remove_material(next_entity, next_material) else {
            continue;
        };

        move_entity_from_carry_to_hand(
            &mut commands,
            camera_entity,
            next_entity,
            config.hold_offset_vec3(),
        );
        record_weight_observation(
            next_material,
            carry_strength.current,
            &config,
            &mut tracker,
            &mut journal_writer,
        );
        emit_carry_weight_changed(&mut weight_writer, &carry_state);
    }
}

// TODO: Incomplete refactor — `process_drop_carry_intent` was partially
// decoupled from interaction.rs but `floor_drop_position` and
// `move_entity_from_carry_to_floor` were never ported. Commented out to fix
// dead-code lint. Restore when the drop-from-carry flow is completed.
//
// #[allow(clippy::too_many_arguments)]
// fn process_drop_carry_intent(
//     mut commands: Commands,
//     mut reader: MessageReader<DropCarryIntent>,
//     mut weight_writer: MessageWriter<CarryWeightChanged>,
//     mut reject_writer: MessageWriter<CarryActionRejected>,
//     config: Res<CarryConfig>,
//     scene: Res<SceneConfig>,
//     mut player_query: Query<(&GlobalTransform, &mut CarryState), With<Player>>,
//     carried_material_query: Query<&GameMaterial, With<InCarry>>,
// ) {
//     ...
// }

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
        assert_eq!(config.active_profile, CarryProfileSelection::Default);
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
        assert_eq!(config.active_profile, CarryProfileSelection::Relaxed);
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
        // `Unknown` is the variant `serde(other)` deserialises any
        // unrecognised TOML value into, so it is the right stand-in for
        // a hand-edited config that points at a profile name no longer
        // defined.  Resolution must keep the recorded selection (for
        // diagnostics) but fall back to the default tuning.
        let config = CarryConfig {
            active_profile: CarryProfileSelection::Unknown,
            ..CarryConfig::default()
        };

        let active = ActiveCarryProfile::from_config(&config);
        assert_eq!(active.selection, CarryProfileSelection::Unknown);
        assert_eq!(active.tuning, default_profile_config());
    }

    #[test]
    fn unknown_active_profile_string_in_toml_deserialises_to_unknown() {
        // Defends the lenient-loader contract: a typo in
        // `assets/config/carry.toml` must not crash the loader; it
        // should land in `CarryProfileSelection::Unknown` so the
        // resolver can fall back to the default tuning.
        let toml = r#"active_profile = "mystery""#;
        let config: CarryConfig = toml::from_str(toml).expect("config parses");
        assert_eq!(config.active_profile, CarryProfileSelection::Unknown);
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

    #[test]
    fn can_stash_respects_hard_limit_capacity() {
        let material = material_with_density(0.6);
        let mut state = CarryState::new(1.0, true);
        state.current_weight = 0.5;

        assert!(!state.can_stash(&material));
    }

    #[test]
    fn can_stash_ignores_capacity_when_hard_limit_disabled() {
        let material = material_with_density(0.6);
        let mut state = CarryState::new(1.0, false);
        state.current_weight = 0.9;

        assert!(state.can_stash(&material));
    }

    #[test]
    fn next_carried_entity_uses_fifo_order() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut state = CarryState::new(5.0, true);
        state.carried_items = vec![CarriedItem::new(first), CarriedItem::new(second)];

        assert_eq!(
            state.next_carried_entity(CarryCycleOrder::Fifo),
            Some(first)
        );
    }

    #[test]
    fn next_carried_entity_uses_lifo_order() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut state = CarryState::new(5.0, true);
        state.carried_items = vec![CarriedItem::new(first), CarriedItem::new(second)];

        assert_eq!(
            state.next_carried_entity(CarryCycleOrder::Lifo),
            Some(second)
        );
    }

    #[test]
    fn can_stash_allows_within_capacity() {
        let state = CarryState::new(5.0, true);
        let light = material_with_density(4.9);
        let exact = material_with_density(5.0);
        assert!(state.can_stash(&light));
        assert!(state.can_stash(&exact));
    }

    #[test]
    fn can_stash_rejects_over_capacity_when_hard_limit_enabled() {
        let mut state = CarryState::new(5.0, true);
        state.current_weight = 4.5;
        let too_heavy = material_with_density(0.6);
        assert!(!state.can_stash(&too_heavy));
    }

    #[test]
    fn can_stash_allows_over_capacity_when_hard_limit_disabled() {
        let mut state = CarryState::new(5.0, false);
        state.current_weight = 4.5;
        let very_heavy = material_with_density(10.0);
        assert!(state.can_stash(&very_heavy));
    }

    #[test]
    fn evict_stale_entity_removes_from_carried_items() {
        let first = Entity::from_bits(1);
        let second = Entity::from_bits(2);
        let mut state = CarryState::new(5.0, true);
        state.carried_items = vec![CarriedItem::new(first), CarriedItem::new(second)];

        assert!(state.evict_stale_entity(first));
        assert_eq!(state.carried_items, vec![CarriedItem::new(second)]);
    }

    #[test]
    fn evict_stale_entity_returns_false_for_unknown() {
        let mut state = CarryState::new(5.0, true);
        assert!(!state.evict_stale_entity(Entity::from_bits(999)));
    }

    #[test]
    fn linear_speed_curve_clamps_at_min_multiplier_when_hard_limit_is_enabled() {
        let curve = CarryCurveConfig {
            kind: CarryCurveKind::Linear,
            min_multiplier: 0.45,
            exponent: 1.0,
        };

        assert!((evaluate_speed_curve(&curve, 3.0, true) - 0.45).abs() < f32::EPSILON);
    }

    #[test]
    fn linear_speed_curve_continues_degrading_when_hard_limit_is_disabled() {
        let curve = CarryCurveConfig {
            kind: CarryCurveKind::Linear,
            min_multiplier: 0.45,
            exponent: 1.0,
        };

        assert!(evaluate_speed_curve(&curve, 3.0, false) < 0.45);
    }

    #[test]
    fn exponential_speed_curve_degrades_faster_at_high_encumbrance() {
        let curve = CarryCurveConfig {
            kind: CarryCurveKind::Exponential,
            min_multiplier: 0.45,
            exponent: 2.0,
        };

        let at_25 = evaluate_speed_curve(&curve, 0.25, true);
        let at_75 = evaluate_speed_curve(&curve, 0.75, true);

        // Exponential curve should produce meaningful degradation.
        assert!(at_25 > at_75, "higher encumbrance should be slower");
        // At 75% load the multiplier should sit between min and the 25% value.
        assert!(at_75 >= 0.45, "should not drop below min_multiplier");
        assert!(at_75 < at_25, "75% load slower than 25% load");
        // At 0% load there should be no penalty.
        let at_0 = evaluate_speed_curve(&curve, 0.0, true);
        assert!((at_0 - 1.0).abs() < f32::EPSILON, "zero load = full speed");
    }

    #[test]
    fn asymptotic_growth_slows_near_strength_cap() {
        let growth_curve = CarryGrowthCurveConfig {
            kind: CarryGrowthCurveKind::Asymptotic,
            max_strength: 8.0,
        };

        let early = carry_strength_delta(2.0, 1.0, 0.1, &growth_curve, 1.0);
        let late = carry_strength_delta(2.0, 7.5, 0.1, &growth_curve, 1.0);

        assert!(early > late);
        assert!(late > 0.0);
    }

    /// Logarithmic growth accelerates with strength — early gains are slower
    /// than later gains, matching the "practice makes you better" design.
    #[test]
    fn logarithmic_growth_accelerates_with_strength() {
        let curve = CarryGrowthCurveConfig {
            kind: CarryGrowthCurveKind::Logarithmic,
            max_strength: 8.0,
        };
        let weight = 2.0;
        let rate = 0.1;
        let dt = 1.0;

        let at_low = carry_strength_delta(weight, 1.0, rate, &curve, dt);
        let at_mid = carry_strength_delta(weight, 4.0, rate, &curve, dt);
        let at_high = carry_strength_delta(weight, 7.0, rate, &curve, dt);

        // Monotonically increasing: low < mid < high.
        assert!(
            at_low < at_mid,
            "growth at strength 1 ({at_low}) should be less than at 4 ({at_mid})"
        );
        assert!(
            at_mid < at_high,
            "growth at strength 4 ({at_mid}) should be less than at 7 ({at_high})"
        );
        // Floor: even at strength=0 there is some growth (10% floor).
        let at_zero = carry_strength_delta(weight, 0.0, rate, &curve, dt);
        assert!(
            at_zero > 0.0,
            "zero-strength floor should still yield growth"
        );
        let base = weight * rate * dt;
        let expected_floor = base * 0.1;
        assert!(
            (at_zero - expected_floor).abs() < f32::EPSILON,
            "zero-strength growth should be 10% of base"
        );
    }

    /// Asymptotic S-curve peaks at midpoint and tapers at both extremes.
    #[test]
    fn asymptotic_growth_is_s_curve() {
        let curve = CarryGrowthCurveConfig {
            kind: CarryGrowthCurveKind::Asymptotic,
            max_strength: 8.0,
        };
        let weight = 2.0;
        let rate = 0.1;
        let dt = 1.0;

        let at_start = carry_strength_delta(weight, 0.0, rate, &curve, dt);
        let at_mid = carry_strength_delta(weight, 4.0, rate, &curve, dt);
        let at_end = carry_strength_delta(weight, 8.0, rate, &curve, dt);

        // S-curve: zero at both extremes, peak at midpoint.
        assert!(
            at_start.abs() < f32::EPSILON,
            "growth at strength=0 should be ~0 (was {at_start})"
        );
        assert!(
            at_end.abs() < f32::EPSILON,
            "growth at strength=max should be ~0 (was {at_end})"
        );
        assert!(
            at_mid > at_start && at_mid > at_end,
            "midpoint ({at_mid}) should be the peak"
        );
        // Peak equals base (parabola normalised to 4×0.5×0.5 = 1.0).
        let base = weight * rate * dt;
        assert!(
            (at_mid - base).abs() < f32::EPSILON,
            "midpoint growth should equal base rate ({base}), got {at_mid}"
        );
    }

    /// All curve kinds return zero when max_strength is zero or negative.
    #[test]
    fn growth_curves_zero_max_strength() {
        for kind in [
            CarryGrowthCurveKind::Logarithmic,
            CarryGrowthCurveKind::Asymptotic,
        ] {
            let curve = CarryGrowthCurveConfig {
                kind,
                max_strength: 0.0,
            };
            let result = carry_strength_delta(2.0, 1.0, 0.1, &curve, 1.0);
            assert!(
                result.abs() < f32::EPSILON,
                "{kind:?} with max_strength=0 should return 0, got {result}"
            );
        }
    }

    /// Linear growth is unaffected by strength level.
    #[test]
    fn linear_growth_is_constant() {
        let curve = CarryGrowthCurveConfig {
            kind: CarryGrowthCurveKind::Linear,
            max_strength: 8.0,
        };
        let at_low = carry_strength_delta(2.0, 1.0, 0.1, &curve, 1.0);
        let at_high = carry_strength_delta(2.0, 7.0, 0.1, &curve, 1.0);
        assert!(
            (at_low - at_high).abs() < f32::EPSILON,
            "linear growth should be constant regardless of strength"
        );
    }

    /// Parameterized sweep of `evaluate_speed_curve` across the full
    /// encumbrance range, both curve kinds, and hard-limit on/off.
    /// Verifies: monotonic degradation, boundary values, and floor behavior.
    #[test]
    fn speed_curve_parameterized_sweep() {
        struct Case {
            label: &'static str,
            kind: CarryCurveKind,
            min_multiplier: f32,
            exponent: f32,
            hard_limit: bool,
        }

        let cases = vec![
            Case {
                label: "linear/hard",
                kind: CarryCurveKind::Linear,
                min_multiplier: 0.4,
                exponent: 1.0,
                hard_limit: true,
            },
            Case {
                label: "linear/soft",
                kind: CarryCurveKind::Linear,
                min_multiplier: 0.4,
                exponent: 1.0,
                hard_limit: false,
            },
            Case {
                label: "exponential/hard",
                kind: CarryCurveKind::Exponential,
                min_multiplier: 0.4,
                exponent: 2.0,
                hard_limit: true,
            },
            Case {
                label: "exponential/soft",
                kind: CarryCurveKind::Exponential,
                min_multiplier: 0.4,
                exponent: 2.0,
                hard_limit: false,
            },
        ];

        let ratios = [0.0, 0.25, 0.5, 0.75, 1.0, 1.5];

        for case in &cases {
            let curve = CarryCurveConfig {
                kind: case.kind,
                min_multiplier: case.min_multiplier,
                exponent: case.exponent,
            };

            // Zero encumbrance must always yield full speed.
            let at_zero = evaluate_speed_curve(&curve, 0.0, case.hard_limit);
            assert!(
                (at_zero - 1.0).abs() < f32::EPSILON,
                "{}: zero load must give 1.0, got {}",
                case.label,
                at_zero
            );

            // Monotonic: speed must not increase as encumbrance rises.
            let mut prev = at_zero;
            for &ratio in &ratios[1..] {
                let speed = evaluate_speed_curve(&curve, ratio, case.hard_limit);
                assert!(
                    speed <= prev + f32::EPSILON,
                    "{}: speed must be monotonically non-increasing (ratio {}: {} > prev {})",
                    case.label,
                    ratio,
                    speed,
                    prev
                );
                prev = speed;
            }

            // Hard-limit: speed at ratio=1.0 must equal min_multiplier.
            if case.hard_limit {
                let at_max = evaluate_speed_curve(&curve, 1.0, true);
                assert!(
                    at_max >= case.min_multiplier - f32::EPSILON,
                    "{}: hard-limit speed at 1.0 ({}) must be >= min_multiplier ({})",
                    case.label,
                    at_max,
                    case.min_multiplier
                );

                // Beyond 1.0 should clamp (same as 1.0) with hard limit.
                let at_over = evaluate_speed_curve(&curve, 1.5, true);
                assert!(
                    (at_over - at_max).abs() < f32::EPSILON,
                    "{}: hard-limit must clamp beyond 1.0 ({} vs {})",
                    case.label,
                    at_over,
                    at_max
                );
            }

            // Soft-limit: speed at 1.5 should be lower than at 1.0.
            if !case.hard_limit {
                let at_100 = evaluate_speed_curve(&curve, 1.0, false);
                let at_150 = evaluate_speed_curve(&curve, 1.5, false);
                assert!(
                    at_150 <= at_100 + f32::EPSILON,
                    "{}: soft-limit speed should keep degrading past 1.0 ({} vs {})",
                    case.label,
                    at_150,
                    at_100
                );
                // Global floor of 0.1 in soft mode.
                assert!(
                    at_150 >= 0.1 - f32::EPSILON,
                    "{}: soft-limit must not go below 0.1, got {}",
                    case.label,
                    at_150
                );
            }
        }
    }

    #[test]
    fn weight_observation_uses_tentative_language_for_first_carry() {
        let text = describe_weight_observation(
            0.8,
            1.0,
            ConfidenceLevel::Tentative,
            &default_weight_descriptions(),
        );

        assert!(text.starts_with("Seemed "));
    }

    #[test]
    fn weight_observation_strengthens_with_confidence() {
        let text = describe_weight_observation(
            0.8,
            1.0,
            ConfidenceLevel::Observed,
            &default_weight_descriptions(),
        );

        assert_eq!(text, "Straining under the weight");
    }
}
