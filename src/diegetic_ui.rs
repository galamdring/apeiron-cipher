//! Diegetic UI Framework — in-world information surfaces.
//!
//! # What "diegetic" means in Apeiron Cipher
//!
//! Every piece of information the player can read must exist as a physical
//! object in the game world.  No HUD overlays, no floating labels, no
//! tooltip popups.  A journal is a datapad you hold up and look at.  A
//! fabricator status is a gauge on the workbench.  A glowing mineral is its
//! own advertisement.  The player's knowledge comes from *observing the world*,
//! not from reading system messages.
//!
//! # How to add a new diegetic surface
//!
//! 1. Spawn an entity with the [`DiegeticSurface`] marker component.
//! 2. Add a [`DiegeticSurfaceKind`] component to declare how far away the
//!    surface can be perceived and interacted with.
//! 3. Add a [`DiegeticFocusState`] component (default is [`DiegeticFocusState::OutOfRange`]).
//! 4. For text-displaying surfaces, also add a [`ReadableSurfaceContent`]
//!    component.  Systems that own the information write their content there;
//!    the rendering system reads it.
//! 5. Register your rendering logic in whatever system drives your surface's
//!    visuals.  Query for `(&DiegeticFocusState, &ReadableSurfaceContent)` —
//!    only render when the state is [`DiegeticFocusState::Focused`] or
//!    [`DiegeticFocusState::Active`].
//!
//! # Surface kind selection guide
//!
//! | What you're building | Use |
//! |---|---|
//! | Text you read (journal, datapad, inscriptions) | `DiegeticSurfaceKind::Readable` |
//! | Live readouts via gauges/lights (fabricator, navigation console) | `DiegeticSurfaceKind::Instrument` |
//! | A world object whose appearance *is* the message (glowing mineral, stress cracks) | `DiegeticSurfaceKind::PhysicalIndicator` |
//!
//! # System ordering
//!
//! [`DiegeticUiPlugin`] schedules [`update_diegetic_focus`] in
//! [`DiegeticUiSet::FocusUpdate`], which runs in `Update` before any
//! rendering.  Downstream rendering systems should run after this set to see
//! up-to-date focus states.

use bevy::prelude::*;

use crate::player::Player;

// ── Plugin ───────────────────────────────────────────────────────────────────

/// Registers the diegetic UI framework systems onto the [`App`].
///
/// Adds [`DiegeticUiSet::FocusUpdate`] to `Update`.  All diegetic surface
/// rendering systems should run *after* this set.
pub struct DiegeticUiPlugin;

impl Plugin for DiegeticUiPlugin {
    fn build(&self, app: &mut App) {
        app.configure_sets(
            Update,
            (DiegeticUiSet::FocusUpdate, DiegeticUiSet::VisibilitySync).chain(),
        )
        .add_systems(
            Update,
            update_diegetic_focus.in_set(DiegeticUiSet::FocusUpdate),
        )
        .add_systems(
            Update,
            sync_readable_surface_visibility.in_set(DiegeticUiSet::VisibilitySync),
        );
    }
}

// ── System sets ─────────────────────────────────────────────────────────────

/// System sets for the diegetic UI framework.
#[derive(SystemSet, Debug, Clone, PartialEq, Eq, Hash)]
pub enum DiegeticUiSet {
    /// Proximity-based [`DiegeticFocusState`] updates.
    ///
    /// Runs every `Update` frame.  All downstream rendering systems that
    /// care about focus state must run *after* this set.
    FocusUpdate,
    /// Visibility gating: sets [`Visibility`] on diegetic surface entities
    /// based on [`DiegeticFocusState`].
    ///
    /// Runs after [`FocusUpdate`](DiegeticUiSet::FocusUpdate) so that focus
    /// transitions and visibility are always in sync within the same frame.
    VisibilitySync,
}

// ── Components ───────────────────────────────────────────────────────────────

/// Marker component — every in-world information surface must carry this.
///
/// CI compliance tests query for `Text` nodes that are *not* descended from
/// a [`DiegeticSurface`] entity.  Any screen-space text without this marker
/// is an architectural violation.
///
/// See the module-level documentation for a step-by-step guide on adding
/// new diegetic surfaces.
#[derive(Component, Debug, Clone, Copy)]
pub struct DiegeticSurface;

/// Determines how a diegetic surface is perceived and interacted with.
///
/// This drives the [`update_diegetic_focus`] system's proximity thresholds
/// and is the primary selector for how your rendering code should behave.
///
/// Choose based on the interaction model, not the visual style:
/// - **Readable** — the player approaches and reads static or paged text.
/// - **Instrument** — real-time readouts through visual indicators (no text).
/// - **PhysicalIndicator** — the object *is* the message; no proximity logic.
#[derive(Component, Clone, Debug)]
pub enum DiegeticSurfaceKind {
    /// A surface the player reads (journal datapad, inscriptions, signage).
    ///
    /// Text rendering happens on a world-space quad; the player physically
    /// moves close enough for the text to resolve.
    Readable {
        /// Distance at which the surface starts to be perceivable (fade-in
        /// begins, text is blurry/dim).
        perceivable_range: f32,
        /// Distance at which text becomes fully legible (focus completes).
        ///
        /// Must be ≤ `perceivable_range`.
        legible_range: f32,
    },
    /// An instrument with dynamic readouts (fabricator display, navigation console).
    ///
    /// State is communicated through visual indicators (lights, gauges), not
    /// text.  The single `interaction_range` determines both when the surface
    /// becomes perceivable (2×) and fully interactable (1×).
    Instrument {
        /// Distance at which the player can fully interact with the instrument.
        ///
        /// The perceivable threshold is `interaction_range * 2.0`.
        interaction_range: f32,
    },
    /// A physical indicator — glow, heat shimmer, stress cracks, etc.
    ///
    /// No proximity logic is applied; the object communicates state purely
    /// through its appearance.  [`update_diegetic_focus`] skips these
    /// entities entirely.
    PhysicalIndicator,
}

/// Current focus relationship between a diegetic surface and the player.
///
/// Transitions are driven by [`update_diegetic_focus`] based on player
/// proximity.  The [`Active`](DiegeticFocusState::Active) state is managed
/// exclusively by interaction systems — proximity updates never clear it.
///
/// Rendering systems should gate their work on this component:
/// - [`OutOfRange`](DiegeticFocusState::OutOfRange) → nothing visible.
/// - [`Perceivable`](DiegeticFocusState::Perceivable) → dim/blurry preview.
/// - [`Focused`](DiegeticFocusState::Focused) → fully legible, awaiting activation.
/// - [`Active`](DiegeticFocusState::Active) → player is actively using the surface.
#[derive(Component, Clone, Debug, Default, PartialEq)]
pub enum DiegeticFocusState {
    /// Player is beyond perceivable range — surface produces no output.
    #[default]
    OutOfRange,
    /// Player is within perceivable range but not yet within legible/interaction
    /// range.  `proximity` is `1.0` at the legible boundary and `0.0` at the
    /// perceivable boundary — use it to scale alpha, blur, or LOD.
    Perceivable {
        /// `0.0` = just entered perceivable range.  `1.0` = at legible boundary.
        proximity: f32,
    },
    /// Player is within legible/interaction range.  The surface is fully
    /// readable but the player has not yet activated it.
    Focused,
    /// Player is actively interacting (reading, operating an instrument).
    ///
    /// Set and cleared by interaction systems, *not* by proximity logic.
    /// A surface stays `Active` even if the player steps away, until the
    /// interaction system explicitly transitions it back.
    Active,
}

/// Text content for a [`DiegeticSurfaceKind::Readable`] surface.
///
/// Owning systems (e.g. the journal plugin) write to this component every
/// frame when the surface is [`Active`](DiegeticFocusState::Active).  The
/// rendering system reads it — the two systems are decoupled through this
/// component, which keeps each within the four-parameter limit.
///
/// Content lines should use qualitative language only — no raw numbers, no
/// system identifiers.  The game never shows internal state.
#[derive(Component, Clone, Debug, Default)]
pub struct ReadableSurfaceContent {
    /// Lines of text to display, already formatted for the player.
    ///
    /// Each element is one visible line.  The rendering system displays
    /// `visible_lines` of them starting at `scroll_offset`.
    pub lines: Vec<String>,
    /// Index of the first visible line (for multi-page content).
    pub scroll_offset: usize,
    /// How many lines the surface's physical display area can show at once.
    ///
    /// Set once at spawn based on the surface's physical dimensions.
    /// The rendering system clamps `scroll_offset` to keep the view valid.
    pub visible_lines: usize,
}

// ── Systems ───────────────────────────────────────────────────────────────────

/// Updates [`DiegeticFocusState`] on every diegetic surface based on how far
/// the player is from it.
///
/// # Rules
///
/// - [`DiegeticSurfaceKind::PhysicalIndicator`] surfaces are skipped — they
///   are always visible through their world appearance alone.
/// - [`DiegeticFocusState::Active`] surfaces are never overridden here — that
///   state is managed exclusively by interaction systems.
/// - For all other surfaces the transition ladder is:
///   `OutOfRange → Perceivable { proximity } → Focused`
///
/// Runs in [`DiegeticUiSet::FocusUpdate`].
pub(crate) fn update_diegetic_focus(
    player_query: Query<&Transform, With<Player>>,
    mut surfaces: Query<
        (&Transform, &DiegeticSurfaceKind, &mut DiegeticFocusState),
        With<DiegeticSurface>,
    >,
) {
    let Ok(player_transform) = player_query.single() else {
        return;
    };
    let player_pos = player_transform.translation;

    for (surface_transform, kind, mut focus_state) in surfaces.iter_mut() {
        // Active state is owned by interaction systems — never clobber it.
        if matches!(*focus_state, DiegeticFocusState::Active) {
            continue;
        }

        // Physical indicators have no proximity logic — their appearance is
        // always present in the world.
        let (perceivable_range, legible_range) = match kind {
            DiegeticSurfaceKind::Readable {
                perceivable_range,
                legible_range,
            } => (*perceivable_range, *legible_range),
            DiegeticSurfaceKind::Instrument { interaction_range } => {
                (interaction_range * 2.0, *interaction_range)
            }
            DiegeticSurfaceKind::PhysicalIndicator => continue,
        };

        let distance = player_pos.distance(surface_transform.translation);

        *focus_state = if distance <= legible_range {
            DiegeticFocusState::Focused
        } else if distance <= perceivable_range {
            // Interpolate: 0.0 at outer perceivable edge, 1.0 at legible edge.
            let span = perceivable_range - legible_range;
            let proximity = if span > f32::EPSILON {
                1.0 - (distance - legible_range) / span
            } else {
                // Degenerate: perceivable == legible, jump straight to focused.
                1.0
            };
            DiegeticFocusState::Perceivable { proximity }
        } else {
            DiegeticFocusState::OutOfRange
        };
    }
}

/// Gates [`Visibility`] on every [`DiegeticSurface`] entity based on its
/// current [`DiegeticFocusState`].
///
/// | State | Visibility |
/// |---|---|
/// | [`OutOfRange`](DiegeticFocusState::OutOfRange) | `Hidden` |
/// | [`Perceivable`](DiegeticFocusState::Perceivable) | `Visible` (renderer dims via alpha) |
/// | [`Focused`](DiegeticFocusState::Focused) | `Visible` |
/// | [`Active`](DiegeticFocusState::Active) | `Visible` |
///
/// Downstream renderers are responsible for reading `proximity` from
/// [`Perceivable`](DiegeticFocusState::Perceivable) and adjusting alpha /
/// LOD accordingly — this system only manages show/hide.
///
/// Runs in [`DiegeticUiSet::VisibilitySync`], after [`DiegeticUiSet::FocusUpdate`].
pub(crate) fn sync_readable_surface_visibility(
    mut surfaces: Query<(&DiegeticFocusState, &mut Visibility), With<DiegeticSurface>>,
) {
    for (focus_state, mut visibility) in surfaces.iter_mut() {
        *visibility = match focus_state {
            DiegeticFocusState::OutOfRange => Visibility::Hidden,
            _ => Visibility::Visible,
        };
    }
}

// ── Tests ─────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::app::App;

    // ── Phase 1 tests ────────────────────────────────────────────────────────

    /// Default focus state is OutOfRange.
    #[test]
    fn default_focus_state_is_out_of_range() {
        let state = DiegeticFocusState::default();
        assert_eq!(state, DiegeticFocusState::OutOfRange);
    }

    /// Readable variant is constructible with expected parameters.
    #[test]
    fn readable_kind_constructible() {
        let kind = DiegeticSurfaceKind::Readable {
            perceivable_range: 5.0,
            legible_range: 2.0,
        };
        match kind {
            DiegeticSurfaceKind::Readable {
                perceivable_range,
                legible_range,
            } => {
                assert_eq!(perceivable_range, 5.0);
                assert_eq!(legible_range, 2.0);
            }
            _ => panic!("expected Readable variant"),
        }
    }

    /// Instrument variant is constructible with expected parameters.
    #[test]
    fn instrument_kind_constructible() {
        let kind = DiegeticSurfaceKind::Instrument {
            interaction_range: 3.0,
        };
        match kind {
            DiegeticSurfaceKind::Instrument { interaction_range } => {
                assert_eq!(interaction_range, 3.0);
            }
            _ => panic!("expected Instrument variant"),
        }
    }

    /// PhysicalIndicator variant is constructible.
    #[test]
    fn physical_indicator_kind_constructible() {
        let kind = DiegeticSurfaceKind::PhysicalIndicator;
        assert!(matches!(kind, DiegeticSurfaceKind::PhysicalIndicator));
    }

    // ── Phase 2 tests ────────────────────────────────────────────────────────

    /// Builds a minimal App with just enough to run `update_diegetic_focus`.
    fn focus_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(bevy::app::Update, update_diegetic_focus);
        app
    }

    fn spawn_player_at(app: &mut App, pos: Vec3) -> Entity {
        app.world_mut()
            .spawn((Player, Transform::from_translation(pos)))
            .id()
    }

    fn spawn_surface_at(
        app: &mut App,
        pos: Vec3,
        kind: DiegeticSurfaceKind,
        initial_state: DiegeticFocusState,
    ) -> Entity {
        app.world_mut()
            .spawn((
                DiegeticSurface,
                kind,
                initial_state,
                Transform::from_translation(pos),
            ))
            .id()
    }

    fn focus_state(app: &App, entity: Entity) -> DiegeticFocusState {
        app.world()
            .entity(entity)
            .get::<DiegeticFocusState>()
            .unwrap()
            .clone()
    }

    /// Player beyond perceivable_range → OutOfRange.
    #[test]
    fn player_beyond_perceivable_range_is_out_of_range() {
        let mut app = focus_test_app();
        // Player at origin, surface 10 units away; perceivable_range = 5.
        spawn_player_at(&mut app, Vec3::ZERO);
        let surface = spawn_surface_at(
            &mut app,
            Vec3::new(10.0, 0.0, 0.0),
            DiegeticSurfaceKind::Readable {
                perceivable_range: 5.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        app.update();
        assert_eq!(focus_state(&app, surface), DiegeticFocusState::OutOfRange);
    }

    /// Player between perceivable and legible range → Perceivable with proximity in [0,1).
    #[test]
    fn player_in_perceivable_band_gives_proximity() {
        let mut app = focus_test_app();
        // perceivable = 6.0, legible = 2.0 → band is [2, 6]
        // Player at distance 4.0 → midpoint → proximity = 1 - (4-2)/(6-2) = 0.5
        spawn_player_at(&mut app, Vec3::ZERO);
        let surface = spawn_surface_at(
            &mut app,
            Vec3::new(4.0, 0.0, 0.0),
            DiegeticSurfaceKind::Readable {
                perceivable_range: 6.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        app.update();
        match focus_state(&app, surface) {
            DiegeticFocusState::Perceivable { proximity } => {
                assert!(
                    (proximity - 0.5).abs() < 1e-5,
                    "expected proximity ~0.5, got {proximity}"
                );
            }
            other => panic!("expected Perceivable, got {other:?}"),
        }
    }

    /// Player within legible_range → Focused.
    #[test]
    fn player_within_legible_range_is_focused() {
        let mut app = focus_test_app();
        spawn_player_at(&mut app, Vec3::ZERO);
        let surface = spawn_surface_at(
            &mut app,
            Vec3::new(1.0, 0.0, 0.0),
            DiegeticSurfaceKind::Readable {
                perceivable_range: 5.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        app.update();
        assert_eq!(focus_state(&app, surface), DiegeticFocusState::Focused);
    }

    /// Active state is preserved even when player moves away.
    #[test]
    fn active_state_preserved_when_player_moves_away() {
        let mut app = focus_test_app();
        spawn_player_at(&mut app, Vec3::ZERO);
        let surface = spawn_surface_at(
            &mut app,
            Vec3::new(100.0, 0.0, 0.0), // far away
            DiegeticSurfaceKind::Readable {
                perceivable_range: 5.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::Active, // already active
        );
        app.update();
        // Must remain Active despite distance.
        assert_eq!(focus_state(&app, surface), DiegeticFocusState::Active);
    }

    /// Proximity interpolates correctly: 0.0 at outer edge, 1.0 at legible edge.
    #[test]
    fn proximity_interpolates_between_zero_and_one() {
        let mut app = focus_test_app();
        spawn_player_at(&mut app, Vec3::ZERO);

        // At the perceivable boundary (distance == perceivable_range) → proximity ≈ 0.0
        let outer = spawn_surface_at(
            &mut app,
            Vec3::new(6.0, 0.0, 0.0),
            DiegeticSurfaceKind::Readable {
                perceivable_range: 6.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        // At the legible boundary (distance == legible_range) → Focused (not Perceivable)
        let inner = spawn_surface_at(
            &mut app,
            Vec3::new(2.0, 0.0, 0.0),
            DiegeticSurfaceKind::Readable {
                perceivable_range: 6.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        app.update();

        match focus_state(&app, outer) {
            DiegeticFocusState::Perceivable { proximity } => {
                assert!(
                    proximity < 0.01,
                    "expected proximity ≈ 0.0 at outer edge, got {proximity}"
                );
            }
            other => panic!("expected Perceivable at outer edge, got {other:?}"),
        }
        // At exactly legible_range, the system transitions to Focused.
        assert_eq!(focus_state(&app, inner), DiegeticFocusState::Focused);
    }

    // ── Phase 3 tests ────────────────────────────────────────────────────────

    fn visibility_test_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(
            bevy::app::Update,
            (update_diegetic_focus, sync_readable_surface_visibility).chain(),
        );
        app
    }

    fn spawn_surface_with_visibility(
        app: &mut App,
        pos: Vec3,
        kind: DiegeticSurfaceKind,
        initial_state: DiegeticFocusState,
    ) -> Entity {
        app.world_mut()
            .spawn((
                DiegeticSurface,
                kind,
                initial_state,
                Transform::from_translation(pos),
                Visibility::Hidden,
            ))
            .id()
    }

    fn vis(app: &App, entity: Entity) -> Visibility {
        *app.world().entity(entity).get::<Visibility>().unwrap()
    }

    /// OutOfRange surface stays Hidden after sync.
    #[test]
    fn out_of_range_surface_is_hidden() {
        let mut app = visibility_test_app();
        spawn_player_at(&mut app, Vec3::ZERO);
        let surface = spawn_surface_with_visibility(
            &mut app,
            Vec3::new(100.0, 0.0, 0.0), // far beyond range
            DiegeticSurfaceKind::Readable {
                perceivable_range: 5.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        app.update();
        assert_eq!(vis(&app, surface), Visibility::Hidden);
    }

    /// Focused surface becomes Visible after sync.
    #[test]
    fn focused_surface_is_visible() {
        let mut app = visibility_test_app();
        spawn_player_at(&mut app, Vec3::ZERO);
        let surface = spawn_surface_with_visibility(
            &mut app,
            Vec3::new(1.0, 0.0, 0.0), // within legible_range
            DiegeticSurfaceKind::Readable {
                perceivable_range: 5.0,
                legible_range: 2.0,
            },
            DiegeticFocusState::OutOfRange,
        );
        app.update();
        assert_eq!(vis(&app, surface), Visibility::Visible);
    }
}
