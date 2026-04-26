//! Discovery journal — player-owned record of observed materials and outcomes.
//!
//! The journal is a lightweight UI overlay that records what the player has
//! personally encountered: surface observations from examination, thermal test
//! results from environmental exposure, and fabrication history from the
//! fabricator. Unknown properties are omitted entirely rather than shown as
//! placeholders.

use std::collections::BTreeMap;

use bevy::prelude::*;
use leafwing_input_manager::prelude::*;
use serde::{Deserialize, Serialize};

use crate::input::InputAction;
use crate::materials::GameMaterial;
use crate::observation::{ConfidenceLevel, describe_thermal_observation};
use crate::player::{Player, cursor_is_captured, spawn_player};

// ── Observation data model ──────────────────────────────────────────────

/// Categories of observation — extensible by adding variants.
///
/// Each variant represents a distinct *kind* of knowledge the player can
/// accumulate about a journal subject. New game systems (navigation,
/// trade, language) add variants here without touching existing match
/// arms or storage structures.
#[derive(Clone, Debug, PartialEq, Eq, Serialize, Deserialize)]
pub enum ObservationCategory {
    /// Visual or tactile surface properties noticed on first examination.
    SurfaceAppearance,
    /// How the subject reacts to heat or cold exposure.
    ThermalBehavior,
    /// Perceived heft or density when the player picks up the subject.
    Weight,
    /// Outcome of combining materials in the fabricator.
    FabricationResult,
    /// A note about a specific location (landmark, hazard, resource).
    LocationNote,
    // Future: LanguageFragment, CulturalBehavior, TradePrice, etc.
}

/// A single observation about a journal subject, timestamped.
///
/// Observations are the atomic unit of player knowledge. Each one records
/// *what* was observed ([`ObservationCategory`]), *how confident* the
/// player should be ([`ConfidenceLevel`]), a human-readable description,
/// and the game-time tick when it was recorded. Observations accumulate
/// inside a [`JournalEntry`] over time — the journal never forgets.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Observation {
    /// What kind of knowledge this observation represents.
    pub category: ObservationCategory,
    /// How certain the player is based on repeated evidence.
    pub confidence: ConfidenceLevel,
    /// Player-facing prose description of the observation.
    pub description: String,
    /// Game-time tick when this observation was recorded.
    pub recorded_at: u64,
}

// ── Journal key ─────────────────────────────────────────────────────────

/// Unique key identifying a journal subject.
///
/// Each variant encodes both the *type* of subject (material, fabrication
/// output, etc.) and the identity that distinguishes one instance from
/// another. The enum is intentionally non-exhaustive so future systems
/// (navigation, trade, language) can add variants without breaking
/// existing match arms.
///
/// `Ord` is derived so that `JournalKey` can serve as a `BTreeMap` key,
/// giving the journal a stable, deterministic iteration order.
#[derive(Clone, Debug, PartialEq, Eq, Hash, PartialOrd, Ord, Serialize, Deserialize)]
pub enum JournalKey {
    /// A raw or discovered material, keyed by its procedural seed.
    Material {
        /// The deterministic seed that uniquely identifies this material
        /// within the world generation system.
        seed: u64,
    },
    /// The output of a fabrication process, keyed by the resulting
    /// material's seed.
    Fabrication {
        /// The deterministic seed of the fabricated output material.
        output_seed: u64,
    },
}

pub struct JournalPlugin;

impl Plugin for JournalPlugin {
    fn build(&self, app: &mut App) {
        app.add_message::<RecordEncounter>()
            .add_message::<RecordFabrication>()
            .add_message::<RecordThermalObservation>()
            .add_message::<RecordWeightObservation>()
            .add_message::<ToggleJournalIntent>()
            .init_resource::<JournalUiState>()
            .add_systems(
                Startup,
                (
                    attach_journal_to_player.after(spawn_player),
                    spawn_journal_ui,
                ),
            )
            .add_systems(
                Update,
                (
                    emit_toggle_journal_intent,
                    toggle_journal_visibility.after(emit_toggle_journal_intent),
                    apply_encounter_records,
                    apply_fabrication_records,
                    apply_thermal_records,
                    apply_weight_records,
                    render_journal
                        .after(apply_encounter_records)
                        .after(apply_fabrication_records)
                        .after(apply_thermal_records)
                        .after(apply_weight_records),
                ),
            );
    }
}

// ── Messages ────────────────────────────────────────────────────────────

#[derive(Message, Clone)]
pub struct RecordEncounter {
    pub material: GameMaterial,
}

#[derive(Message, Clone)]
pub struct RecordFabrication {
    pub output_material: GameMaterial,
    pub input_a: String,
    pub input_b: String,
}

#[derive(Message, Clone)]
pub struct RecordThermalObservation {
    pub seed: u64,
    pub name: String,
    pub thermal_resistance: f32,
    pub confidence: ConfidenceLevel,
}

#[derive(Message, Clone)]
pub struct RecordWeightObservation {
    pub seed: u64,
    pub name: String,
    pub description: String,
}

// ── Player-owned journal data ───────────────────────────────────────────

/// A journal entry that accumulates observations about a single subject over time.
///
/// Each entry is keyed by a [`JournalKey`] and holds a chronologically ordered
/// vector of [`Observation`]s. The `first_observed_at` and `last_updated_at`
/// timestamps track the game-time ticks bounding the observation window, which
/// later systems (e.g., confidence decay, staleness indicators) can use without
/// re-scanning all observations.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct JournalEntry {
    /// The unique identifier for this journal subject.
    pub key: JournalKey,
    /// Player-facing display name for this subject.
    pub name: String,
    /// Chronologically ordered observations accumulated over time.
    pub observations: Vec<Observation>,
    /// Game-time tick when the player first recorded *any* observation about
    /// this subject.
    pub first_observed_at: u64,
    /// Game-time tick of the most recent observation recorded for this subject.
    pub last_updated_at: u64,
}

impl JournalEntry {
    /// Create a new journal entry with no observations yet recorded.
    ///
    /// The `tick` parameter sets both `first_observed_at` and `last_updated_at`
    /// to the same initial value; they diverge once additional observations
    /// arrive.
    pub fn new(key: JournalKey, name: String, tick: u64) -> Self {
        Self {
            key,
            name,
            observations: Vec::new(),
            first_observed_at: tick,
            last_updated_at: tick,
        }
    }

    /// Append an observation and update the `last_updated_at` timestamp.
    ///
    /// The observation's `recorded_at` tick is used as the new
    /// `last_updated_at` value, so callers must ensure observations are
    /// appended in monotonically non-decreasing tick order.
    pub fn add_observation(&mut self, observation: Observation) {
        self.last_updated_at = observation.recorded_at;
        self.observations.push(observation);
    }

    /// Return all observations matching a given category, in recorded order.
    pub fn observations_by_category(&self, category: &ObservationCategory) -> Vec<&Observation> {
        self.observations
            .iter()
            .filter(|o| &o.category == category)
            .collect()
    }
}

/// The player's journal — accumulates all observations about every subject
/// the player has investigated.
///
/// Keyed by [`JournalKey`] so lookups are O(log n) and iteration order is
/// deterministic (important for save/load reproducibility and test stability).
#[derive(Component, Default, Clone, Debug, Serialize, Deserialize)]
pub struct NewJournal {
    /// All journal entries, keyed by subject identity.
    ///
    /// Serialized as a list of entries (not a JSON object) because
    /// [`JournalKey`] is an enum and cannot serve as a JSON map key.
    #[serde(with = "journal_entries_serde")]
    pub entries: BTreeMap<JournalKey, JournalEntry>,
}

/// Serialize/deserialize a `BTreeMap<JournalKey, JournalEntry>` as a
/// `Vec<JournalEntry>`. Each entry already carries its key, so the vec
/// representation is lossless and avoids the JSON "keys must be strings"
/// limitation.
mod journal_entries_serde {
    use super::*;
    use serde::de::Deserializer;
    use serde::ser::Serializer;

    pub fn serialize<S: Serializer>(
        map: &BTreeMap<JournalKey, JournalEntry>,
        serializer: S,
    ) -> Result<S::Ok, S::Error> {
        let entries: Vec<&JournalEntry> = map.values().collect();
        entries.serialize(serializer)
    }

    pub fn deserialize<'de, D: Deserializer<'de>>(
        deserializer: D,
    ) -> Result<BTreeMap<JournalKey, JournalEntry>, D::Error> {
        let entries: Vec<JournalEntry> = Vec::deserialize(deserializer)?;
        Ok(entries.into_iter().map(|e| (e.key.clone(), e)).collect())
    }
}

impl NewJournal {
    /// Look up or create a journal entry for the given key.
    ///
    /// If no entry exists yet, one is created with the provided `name` and
    /// `tick` as the initial observation timestamp. If an entry already exists,
    /// the existing entry is returned unchanged (name is *not* overwritten —
    /// the first observer wins).
    pub fn ensure_entry(&mut self, key: JournalKey, name: &str, tick: u64) -> &mut JournalEntry {
        self.entries
            .entry(key.clone())
            .or_insert_with(|| JournalEntry::new(key, name.to_string(), tick))
    }

    /// Record an observation against a subject, creating the entry if needed.
    ///
    /// This is the primary write path that future systems (materials, heat,
    /// fabrication, navigation, trade, language) use to push knowledge into
    /// the journal.
    pub fn record(&mut self, key: JournalKey, name: &str, observation: Observation) {
        let entry = self.ensure_entry(key, name, observation.recorded_at);
        entry.add_observation(observation);
    }
}

// ── Legacy journal structs (POC — will be removed by migration stories) ─

#[derive(Component, Default)]
struct LegacyJournal {
    fabrication_log: Vec<String>,
    entries: BTreeMap<u64, LegacyJournalEntry>,
}

#[derive(Clone, Debug, Default)]
struct LegacyJournalEntry {
    name: String,
    surface_observations: Vec<String>,
    thermal_observation: Option<String>,
    weight_observation: Option<String>,
    fabrication_history: Vec<String>,
}

impl LegacyJournal {
    fn ensure_entry(&mut self, seed: u64, name: &str) -> &mut LegacyJournalEntry {
        self.entries
            .entry(seed)
            .or_insert_with(|| LegacyJournalEntry {
                name: name.to_string(),
                ..default()
            })
    }
}

// ── UI state ────────────────────────────────────────────────────────────

#[derive(Resource, Default)]
struct JournalUiState {
    visible: bool,
}

#[derive(Message)]
struct ToggleJournalIntent;

#[derive(Component)]
struct JournalPanel;

#[derive(Component)]
struct JournalText;

fn attach_journal_to_player(mut commands: Commands, player_query: Query<Entity, With<Player>>) {
    let Ok(player) = player_query.single() else {
        return;
    };
    commands.entity(player).insert(LegacyJournal::default());
}

fn spawn_journal_ui(mut commands: Commands) {
    commands
        .spawn((
            JournalPanel,
            Node {
                position_type: PositionType::Absolute,
                top: Val::Px(24.0),
                left: Val::Px(24.0),
                width: Val::Px(460.0),
                max_height: Val::Percent(80.0),
                padding: UiRect::all(Val::Px(16.0)),
                ..default()
            },
            BackgroundColor(Color::srgba(0.07, 0.07, 0.09, 0.92)),
            Visibility::Hidden,
        ))
        .with_children(|parent| {
            parent.spawn((
                JournalText,
                Text::new(""),
                TextFont {
                    font_size: 16.0,
                    ..default()
                },
                TextColor(Color::srgba(0.92, 0.92, 0.88, 1.0)),
            ));
        });
}

// ── Input ───────────────────────────────────────────────────────────────

fn emit_toggle_journal_intent(
    player_query: Query<&ActionState<InputAction>, With<Player>>,
    cursor_options: Single<&bevy::window::CursorOptions>,
    mut writer: MessageWriter<ToggleJournalIntent>,
) {
    if !cursor_is_captured(cursor_options.grab_mode) {
        return;
    }

    let Ok(action) = player_query.single() else {
        return;
    };
    if action.just_pressed(&InputAction::ToggleJournal) {
        writer.write(ToggleJournalIntent);
    }
}

fn toggle_journal_visibility(
    mut reader: MessageReader<ToggleJournalIntent>,
    mut state: ResMut<JournalUiState>,
) {
    for _ in reader.read() {
        state.visible = !state.visible;
    }
}

// ── Record ingestion ────────────────────────────────────────────────────

fn apply_encounter_records(
    mut reader: MessageReader<RecordEncounter>,
    mut player_query: Query<&mut LegacyJournal, With<Player>>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    for event in reader.read() {
        let entry = journal.ensure_entry(event.material.seed, &event.material.name);
        if entry.surface_observations.is_empty() {
            entry.surface_observations = vec![
                format!("Color: {}", describe_color(&event.material.color)),
                format!("Weight: {}", describe_density(event.material.density.value)),
            ];
        }
    }
}

fn apply_fabrication_records(
    mut reader: MessageReader<RecordFabrication>,
    mut player_query: Query<&mut LegacyJournal, With<Player>>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    for event in reader.read() {
        let history = format!(
            "Combined {} + {} -> {}",
            event.input_a, event.input_b, event.output_material.name
        );
        if !journal.fabrication_log.contains(&history) {
            journal.fabrication_log.push(history.clone());
        }

        let entry = journal.ensure_entry(event.output_material.seed, &event.output_material.name);
        if entry.surface_observations.is_empty() {
            entry.surface_observations = vec![
                format!("Color: {}", describe_color(&event.output_material.color)),
                format!(
                    "Weight: {}",
                    describe_density(event.output_material.density.value)
                ),
            ];
        }
        if !entry.fabrication_history.contains(&history) {
            entry.fabrication_history.push(history);
        }
    }
}

fn apply_thermal_records(
    mut reader: MessageReader<RecordThermalObservation>,
    mut player_query: Query<&mut LegacyJournal, With<Player>>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    for event in reader.read() {
        let entry = journal.ensure_entry(event.seed, &event.name);
        entry.thermal_observation = Some(describe_thermal_observation(
            event.thermal_resistance,
            event.confidence,
        ));
    }
}

fn apply_weight_records(
    mut reader: MessageReader<RecordWeightObservation>,
    mut player_query: Query<&mut LegacyJournal, With<Player>>,
) {
    let Ok(mut journal) = player_query.single_mut() else {
        return;
    };

    for event in reader.read() {
        let entry = journal.ensure_entry(event.seed, &event.name);
        entry.weight_observation = Some(event.description.clone());
    }
}

// ── Rendering ───────────────────────────────────────────────────────────

fn render_journal(
    state: Res<JournalUiState>,
    player_query: Query<&LegacyJournal, With<Player>>,
    mut panel_query: Query<&mut Visibility, With<JournalPanel>>,
    mut text_query: Query<&mut Text, With<JournalText>>,
) {
    let Ok(mut panel_vis) = panel_query.single_mut() else {
        return;
    };
    let Ok(mut text) = text_query.single_mut() else {
        return;
    };

    if !state.visible {
        *panel_vis = Visibility::Hidden;
        return;
    }

    let Ok(journal) = player_query.single() else {
        *panel_vis = Visibility::Hidden;
        return;
    };

    *panel_vis = Visibility::Visible;
    text.0 = build_journal_text(journal);
}

fn build_journal_text(journal: &LegacyJournal) -> String {
    if journal.entries.is_empty() {
        return "Journal\n\nNo observations yet.".to_string();
    }

    let mut out = vec!["Journal".to_string()];

    if !journal.fabrication_log.is_empty() {
        out.push(String::new());
        out.push("Recent Fabrication".to_string());
        for history in &journal.fabrication_log {
            out.push(history.clone());
        }
    }

    let mut entries: Vec<&LegacyJournalEntry> = journal.entries.values().collect();
    entries.sort_by(|a, b| a.name.cmp(&b.name));

    for entry in entries {
        out.push(String::new());
        out.push(entry.name.clone());

        for obs in &entry.surface_observations {
            out.push(format!("Surface: {obs}"));
        }

        if let Some(thermal) = &entry.thermal_observation {
            out.push(format!("Heat: {thermal}"));
        }

        if let Some(weight) = &entry.weight_observation {
            out.push(format!("Carried: {weight}"));
        }

        for history in &entry.fabrication_history {
            out.push(history.clone());
        }
    }

    out.join("\n")
}

// ── Descriptive language ────────────────────────────────────────────────

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

fn describe_color(color: &[f32; 3]) -> &'static str {
    let [r, g, b] = *color;
    let max = r.max(g).max(b);
    let min = r.min(g).min(b);

    if max - min < 0.08 {
        if max < 0.25 {
            "Dark mineral grey"
        } else if max < 0.7 {
            "Muted stone grey"
        } else {
            "Pale chalk grey"
        }
    } else if r >= g && r >= b {
        "Warm rust tone"
    } else if g >= r && g >= b {
        "Verdant green tone"
    } else {
        "Cool blue tone"
    }
}

// ── Tests ───────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn journal_omits_unknown_properties() {
        let mut journal = LegacyJournal::default();
        let entry = journal.ensure_entry(1, "Ferrite");
        entry.surface_observations.push("Weight: Heavy".into());

        let text = build_journal_text(&journal);
        assert!(text.contains("Weight: Heavy"));
        assert!(!text.contains("Heat:"));
    }

    #[test]
    fn journal_includes_fabrication_history() {
        let mut journal = LegacyJournal::default();
        journal
            .fabrication_log
            .push("Combined Ferrite + Silite -> Neoite".into());
        let entry = journal.ensure_entry(2, "Neoite");
        entry
            .fabrication_history
            .push("Combined Ferrite + Silite -> Neoite".into());

        let text = build_journal_text(&journal);
        assert!(text.contains("Combined Ferrite + Silite -> Neoite"));
        assert!(text.contains("Recent Fabrication"));
    }

    #[test]
    fn journal_shows_thermal_observation_when_present() {
        let mut journal = LegacyJournal::default();
        let entry = journal.ensure_entry(3, "TestMat");
        entry.thermal_observation = Some("Reliably hold together under heat".into());

        let text = build_journal_text(&journal);
        assert!(text.contains("Heat: Reliably hold together under heat"));
    }

    #[test]
    fn journal_key_material_equality() {
        let a = JournalKey::Material { seed: 42 };
        let b = JournalKey::Material { seed: 42 };
        let c = JournalKey::Material { seed: 99 };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn journal_key_fabrication_equality() {
        let a = JournalKey::Fabrication { output_seed: 7 };
        let b = JournalKey::Fabrication { output_seed: 7 };
        let c = JournalKey::Fabrication { output_seed: 8 };
        assert_eq!(a, b);
        assert_ne!(a, c);
    }

    #[test]
    fn journal_key_variants_are_distinct() {
        let mat = JournalKey::Material { seed: 42 };
        let fab = JournalKey::Fabrication { output_seed: 42 };
        assert_ne!(mat, fab);
    }

    #[test]
    fn journal_key_serde_round_trip() {
        let keys = vec![
            JournalKey::Material { seed: 123 },
            JournalKey::Fabrication { output_seed: 456 },
        ];
        for key in &keys {
            let json = serde_json::to_string(key).expect("JournalKey should serialize to JSON");
            let deserialized: JournalKey =
                serde_json::from_str(&json).expect("JournalKey should deserialize from JSON");
            assert_eq!(*key, deserialized);
        }
    }

    #[test]
    fn journal_key_btreemap_ordering_is_stable() {
        use std::collections::BTreeMap;
        let mut map = BTreeMap::new();
        map.insert(JournalKey::Fabrication { output_seed: 1 }, "fab");
        map.insert(JournalKey::Material { seed: 99 }, "mat99");
        map.insert(JournalKey::Material { seed: 1 }, "mat1");

        let keys: Vec<_> = map.keys().collect();
        // Derived Ord: enum variants ordered by declaration (Material < Fabrication),
        // then by field values within each variant.
        assert_eq!(*keys[0], JournalKey::Material { seed: 1 });
        assert_eq!(*keys[1], JournalKey::Material { seed: 99 });
        assert_eq!(*keys[2], JournalKey::Fabrication { output_seed: 1 });
    }

    #[test]
    fn journal_shows_weight_observation_only_when_present() {
        let mut journal = LegacyJournal::default();
        let entry = journal.ensure_entry(4, "Ferrite");
        entry
            .surface_observations
            .push("Color: Cool blue tone".into());

        let without_weight = build_journal_text(&journal);
        assert!(!without_weight.contains("Carried: Heavy but manageable"));

        let entry = journal.ensure_entry(4, "Ferrite");
        entry.weight_observation = Some("Heavy but manageable".into());

        let with_weight = build_journal_text(&journal);
        assert!(with_weight.contains("Carried: Heavy but manageable"));
    }

    // ── New data model tests ────────────────────────────────────────────

    #[test]
    fn journal_entry_new_sets_timestamps() {
        let key = JournalKey::Material { seed: 1 };
        let entry = JournalEntry::new(key.clone(), "Ferrite".into(), 100);
        assert_eq!(entry.key, key);
        assert_eq!(entry.name, "Ferrite");
        assert!(entry.observations.is_empty());
        assert_eq!(entry.first_observed_at, 100);
        assert_eq!(entry.last_updated_at, 100);
    }

    #[test]
    fn journal_entry_add_observation_updates_timestamp() {
        let key = JournalKey::Material { seed: 1 };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 20,
        });

        assert_eq!(entry.observations.len(), 1);
        assert_eq!(entry.first_observed_at, 10);
        assert_eq!(entry.last_updated_at, 20);
    }

    #[test]
    fn journal_entry_accumulates_multiple_observations() {
        let key = JournalKey::Material { seed: 1 };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Observed,
            description: "Holds together under heat".into(),
            recorded_at: 50,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::Weight,
            confidence: ConfidenceLevel::Tentative,
            description: "Heavy".into(),
            recorded_at: 55,
        });

        assert_eq!(entry.observations.len(), 3);
        assert_eq!(entry.last_updated_at, 55);
    }

    #[test]
    fn journal_entry_observations_by_category() {
        let key = JournalKey::Material { seed: 1 };
        let mut entry = JournalEntry::new(key, "Ferrite".into(), 10);

        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Observed,
            description: "Holds together under heat".into(),
            recorded_at: 20,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Observed,
            description: "Slightly rough texture".into(),
            recorded_at: 30,
        });

        let surface = entry.observations_by_category(&ObservationCategory::SurfaceAppearance);
        assert_eq!(surface.len(), 2);
        assert_eq!(surface[0].description, "Warm rust tone");
        assert_eq!(surface[1].description, "Slightly rough texture");

        let thermal = entry.observations_by_category(&ObservationCategory::ThermalBehavior);
        assert_eq!(thermal.len(), 1);

        let weight = entry.observations_by_category(&ObservationCategory::Weight);
        assert!(weight.is_empty());
    }

    #[test]
    fn new_journal_ensure_entry_creates_and_retrieves() {
        let mut journal = NewJournal::default();
        let key = JournalKey::Material { seed: 42 };

        journal.ensure_entry(key.clone(), "Ferrite", 100);
        journal.ensure_entry(key.clone(), "Ignored Name", 200);

        assert_eq!(journal.entries.len(), 1);
        let entry = journal.entries.get(&key).expect("entry should exist");
        // First name wins.
        assert_eq!(entry.name, "Ferrite");
        // Timestamps unchanged by second ensure_entry call.
        assert_eq!(entry.first_observed_at, 100);
    }

    #[test]
    fn new_journal_record_accumulates_observations() {
        let mut journal = NewJournal::default();
        let key = JournalKey::Material { seed: 42 };

        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Holds together under heat".into(),
                recorded_at: 50,
            },
        );

        let entry = journal.entries.get(&key).expect("entry should exist");
        assert_eq!(entry.observations.len(), 2);
        assert_eq!(entry.first_observed_at, 10);
        assert_eq!(entry.last_updated_at, 50);
    }

    #[test]
    fn new_journal_different_keys_coexist() {
        let mut journal = NewJournal::default();
        let mat_key = JournalKey::Material { seed: 1 };
        let fab_key = JournalKey::Fabrication { output_seed: 2 };

        journal.record(
            mat_key.clone(),
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            fab_key.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 20,
            },
        );

        assert_eq!(journal.entries.len(), 2);
        assert!(journal.entries.contains_key(&mat_key));
        assert!(journal.entries.contains_key(&fab_key));
    }

    #[test]
    fn new_journal_serde_round_trip() {
        let mut journal = NewJournal::default();
        journal.record(
            JournalKey::Material { seed: 42 },
            "Ferrite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Warm rust tone".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            JournalKey::Fabrication { output_seed: 99 },
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Ferrite + Silite -> Neoite".into(),
                recorded_at: 50,
            },
        );

        let json = serde_json::to_string(&journal).expect("NewJournal should serialize to JSON");
        let deserialized: NewJournal =
            serde_json::from_str(&json).expect("NewJournal should deserialize from JSON");

        assert_eq!(deserialized.entries.len(), 2);
        let ferrite = deserialized
            .entries
            .get(&JournalKey::Material { seed: 42 })
            .expect("Ferrite entry should exist");
        assert_eq!(ferrite.name, "Ferrite");
        assert_eq!(ferrite.observations.len(), 1);
        assert_eq!(ferrite.first_observed_at, 10);
    }

    #[test]
    fn new_journal_empty_default() {
        let journal = NewJournal::default();
        assert!(journal.entries.is_empty());
    }

    /// Every type in the journal data model serializes to JSON and deserializes
    /// back to an identical value. Covers all `JournalKey` variants, all
    /// `ObservationCategory` variants, all `ConfidenceLevel` variants, the
    /// `Observation` struct, `JournalEntry`, and a `NewJournal` containing
    /// entries of every key type with observations of every category.
    #[test]
    fn all_types_serde_round_trip() {
        // ── JournalKey variants ─────────────────────────────────────
        let keys = vec![
            JournalKey::Material { seed: 0 },
            JournalKey::Material { seed: u64::MAX },
            JournalKey::Fabrication { output_seed: 42 },
        ];
        for key in &keys {
            let json = serde_json::to_string(key).expect("JournalKey should serialize");
            let rt: JournalKey =
                serde_json::from_str(&json).expect("JournalKey should deserialize");
            assert_eq!(*key, rt);
        }

        // ── ObservationCategory variants ────────────────────────────
        let categories = vec![
            ObservationCategory::SurfaceAppearance,
            ObservationCategory::ThermalBehavior,
            ObservationCategory::Weight,
            ObservationCategory::FabricationResult,
            ObservationCategory::LocationNote,
        ];
        for cat in &categories {
            let json = serde_json::to_string(cat).expect("ObservationCategory should serialize");
            let rt: ObservationCategory =
                serde_json::from_str(&json).expect("ObservationCategory should deserialize");
            assert_eq!(*cat, rt);
        }

        // ── ConfidenceLevel variants ────────────────────────────────
        let levels = vec![
            ConfidenceLevel::Tentative,
            ConfidenceLevel::Observed,
            ConfidenceLevel::Confident,
        ];
        for level in &levels {
            let json = serde_json::to_string(level).expect("ConfidenceLevel should serialize");
            let rt: ConfidenceLevel =
                serde_json::from_str(&json).expect("ConfidenceLevel should deserialize");
            assert_eq!(*level, rt);
        }

        // ── Observation struct ──────────────────────────────────────
        let observation = Observation {
            category: ObservationCategory::ThermalBehavior,
            confidence: ConfidenceLevel::Observed,
            description: "Holds together under heat".into(),
            recorded_at: 999,
        };
        let json = serde_json::to_string(&observation).expect("Observation should serialize");
        let rt: Observation = serde_json::from_str(&json).expect("Observation should deserialize");
        assert_eq!(rt.category, observation.category);
        assert_eq!(rt.confidence, observation.confidence);
        assert_eq!(rt.description, observation.description);
        assert_eq!(rt.recorded_at, observation.recorded_at);

        // ── JournalEntry struct ─────────────────────────────────────
        let mut entry = JournalEntry::new(JournalKey::Material { seed: 7 }, "Ferrite".into(), 10);
        entry.add_observation(Observation {
            category: ObservationCategory::SurfaceAppearance,
            confidence: ConfidenceLevel::Tentative,
            description: "Warm rust tone".into(),
            recorded_at: 10,
        });
        entry.add_observation(Observation {
            category: ObservationCategory::Weight,
            confidence: ConfidenceLevel::Confident,
            description: "Very heavy".into(),
            recorded_at: 20,
        });

        let json = serde_json::to_string(&entry).expect("JournalEntry should serialize");
        let rt: JournalEntry =
            serde_json::from_str(&json).expect("JournalEntry should deserialize");
        assert_eq!(rt.key, entry.key);
        assert_eq!(rt.name, entry.name);
        assert_eq!(rt.observations.len(), 2);
        assert_eq!(rt.first_observed_at, entry.first_observed_at);
        assert_eq!(rt.last_updated_at, entry.last_updated_at);
        assert_eq!(
            rt.observations[0].category,
            ObservationCategory::SurfaceAppearance
        );
        assert_eq!(rt.observations[1].category, ObservationCategory::Weight);

        // ── NewJournal with all key types and all categories ────────
        let mut journal = NewJournal::default();

        // Material entry with surface, thermal, and weight observations.
        let mat_key = JournalKey::Material { seed: 100 };
        journal.record(
            mat_key.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::SurfaceAppearance,
                confidence: ConfidenceLevel::Tentative,
                description: "Cool blue tone".into(),
                recorded_at: 1,
            },
        );
        journal.record(
            mat_key.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::ThermalBehavior,
                confidence: ConfidenceLevel::Observed,
                description: "Softens quickly under heat".into(),
                recorded_at: 5,
            },
        );
        journal.record(
            mat_key.clone(),
            "Silite",
            Observation {
                category: ObservationCategory::Weight,
                confidence: ConfidenceLevel::Confident,
                description: "Light".into(),
                recorded_at: 8,
            },
        );

        // Fabrication entry with fabrication result and location note.
        let fab_key = JournalKey::Fabrication { output_seed: 200 };
        journal.record(
            fab_key.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::FabricationResult,
                confidence: ConfidenceLevel::Confident,
                description: "Combined Silite + Ferrite -> Neoite".into(),
                recorded_at: 10,
            },
        );
        journal.record(
            fab_key.clone(),
            "Neoite",
            Observation {
                category: ObservationCategory::LocationNote,
                confidence: ConfidenceLevel::Tentative,
                description: "Found near volcanic ridge".into(),
                recorded_at: 15,
            },
        );

        let json = serde_json::to_string(&journal).expect("NewJournal should serialize");
        let rt: NewJournal = serde_json::from_str(&json).expect("NewJournal should deserialize");

        // Verify structure preserved.
        assert_eq!(rt.entries.len(), 2);

        let silite = rt
            .entries
            .get(&mat_key)
            .expect("Material entry should exist");
        assert_eq!(silite.name, "Silite");
        assert_eq!(silite.observations.len(), 3);
        assert_eq!(silite.first_observed_at, 1);
        assert_eq!(silite.last_updated_at, 8);
        assert_eq!(
            silite.observations[0].category,
            ObservationCategory::SurfaceAppearance
        );
        assert_eq!(
            silite.observations[0].confidence,
            ConfidenceLevel::Tentative
        );
        assert_eq!(
            silite.observations[1].category,
            ObservationCategory::ThermalBehavior
        );
        assert_eq!(silite.observations[2].category, ObservationCategory::Weight);

        let neoite = rt
            .entries
            .get(&fab_key)
            .expect("Fabrication entry should exist");
        assert_eq!(neoite.name, "Neoite");
        assert_eq!(neoite.observations.len(), 2);
        assert_eq!(neoite.first_observed_at, 10);
        assert_eq!(neoite.last_updated_at, 15);
        assert_eq!(
            neoite.observations[0].category,
            ObservationCategory::FabricationResult
        );
        assert_eq!(
            neoite.observations[0].confidence,
            ConfidenceLevel::Confident
        );
        assert_eq!(
            neoite.observations[1].category,
            ObservationCategory::LocationNote
        );
        assert_eq!(
            neoite.observations[1].description,
            "Found near volcanic ridge"
        );
    }
}
