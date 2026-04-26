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

#[derive(Component, Default)]
struct Journal {
    fabrication_log: Vec<String>,
    entries: BTreeMap<u64, JournalEntry>,
}

#[derive(Clone, Debug, Default)]
struct JournalEntry {
    name: String,
    surface_observations: Vec<String>,
    thermal_observation: Option<String>,
    weight_observation: Option<String>,
    fabrication_history: Vec<String>,
}

impl Journal {
    fn ensure_entry(&mut self, seed: u64, name: &str) -> &mut JournalEntry {
        self.entries.entry(seed).or_insert_with(|| JournalEntry {
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
    commands.entity(player).insert(Journal::default());
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
    mut player_query: Query<&mut Journal, With<Player>>,
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
    mut player_query: Query<&mut Journal, With<Player>>,
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
    mut player_query: Query<&mut Journal, With<Player>>,
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
    mut player_query: Query<&mut Journal, With<Player>>,
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
    player_query: Query<&Journal, With<Player>>,
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

fn build_journal_text(journal: &Journal) -> String {
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

    let mut entries: Vec<&JournalEntry> = journal.entries.values().collect();
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
        let mut journal = Journal::default();
        let entry = journal.ensure_entry(1, "Ferrite");
        entry.surface_observations.push("Weight: Heavy".into());

        let text = build_journal_text(&journal);
        assert!(text.contains("Weight: Heavy"));
        assert!(!text.contains("Heat:"));
    }

    #[test]
    fn journal_includes_fabrication_history() {
        let mut journal = Journal::default();
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
        let mut journal = Journal::default();
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
        let mut journal = Journal::default();
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
}
