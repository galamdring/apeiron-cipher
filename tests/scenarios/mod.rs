//! Scenario-based integration test harness for Apeiron Cipher.
//!
//! Runs table-driven multi-frame playtest sequences against a real Bevy
//! [`App`] with no window, no GPU, and no asset I/O.  Each scenario
//! defines setup, timed actions, and timed assertions that execute
//! across an `app.update()` loop.

use bevy::prelude::*;
use std::panic::{AssertUnwindSafe, catch_unwind};

pub mod helpers;

/// One row in a table test.
pub struct Scenario {
    /// Human-readable label — printed on failure.
    pub name: &'static str,
    /// Per-scenario setup applied *after* the shared setup.
    pub setup: Box<dyn Fn(&mut App)>,
    /// Ordered timeline of actions and assertions.
    pub steps: Vec<Step>,
    /// Hard ceiling — the scenario fails if a `WaitUntil` hasn't
    /// resolved by this frame.
    pub max_frames: u32,
}

/// A single point on the scenario timeline.
pub enum Step {
    /// Fire a world mutation at a specific frame.
    Act(u32, Box<dyn Fn(&mut World)>),
    /// Assert a condition at a specific frame.  The `&'static str` is a
    /// label printed on failure.
    Assert(u32, &'static str, Box<dyn Fn(&mut World)>),
    /// Poll every frame until the predicate returns `true`.  Fails if
    /// still `false` after `timeout_frames` frames have elapsed since
    /// frame 0.
    WaitUntil(u32, &'static str, Box<dyn Fn(&mut World) -> bool>),
}

/// Run a batch of [`Scenario`]s that share the same base wiring.
///
/// `shared_setup` is called once per scenario *before* the scenario's
/// own `setup` closure.  This is where you register shared systems,
/// resources, and events.
pub fn run_scenarios(shared_setup: impl Fn(&mut App), scenarios: Vec<Scenario>) {
    let mut failures: Vec<String> = Vec::new();

    for scenario in &scenarios {
        let name = scenario.name;

        let result = catch_unwind(AssertUnwindSafe(|| {
            let mut app = App::new();
            shared_setup(&mut app);
            (scenario.setup)(&mut app);

            // Track which WaitUntil steps have resolved.
            let mut resolved: Vec<bool> = scenario
                .steps
                .iter()
                .map(|s| !matches!(s, Step::WaitUntil(..)))
                .collect();

            for frame in 0..scenario.max_frames {
                // --- actions ---
                for step in &scenario.steps {
                    if let Step::Act(f, action) = step {
                        if *f == frame {
                            action(app.world_mut());
                        }
                    }
                }

                app.update();

                // --- assertions ---
                for (i, step) in scenario.steps.iter().enumerate() {
                    match step {
                        Step::Assert(f, _label, check) => {
                            if *f == frame {
                                check(app.world_mut());
                                resolved[i] = true;
                            }
                        }
                        Step::WaitUntil(timeout, label, check) => {
                            if !resolved[i] && check(app.world_mut()) {
                                resolved[i] = true;
                            }
                            if !resolved[i] && frame >= *timeout {
                                panic!(
                                    "WaitUntil '{label}' not satisfied \
                                     after {timeout} frames"
                                );
                            }
                        }
                        _ => {}
                    }
                }
            }

            // Anything still unresolved is a bug in the scenario
            // definition (e.g. Assert frame > max_frames).
            for (i, step) in scenario.steps.iter().enumerate() {
                if !resolved[i] {
                    if let Step::Assert(f, label, _) = step {
                        panic!(
                            "Assert '{label}' at frame {f} never ran \
                             (max_frames = {})",
                            scenario.max_frames,
                        );
                    }
                }
            }
        }));

        if let Err(e) = result {
            let msg = if let Some(s) = e.downcast_ref::<String>() {
                s.clone()
            } else if let Some(s) = e.downcast_ref::<&str>() {
                (*s).to_string()
            } else {
                "unknown panic".to_string()
            };
            failures.push(format!("Scenario '{name}': {msg}"));
        }
    }

    if !failures.is_empty() {
        panic!(
            "\n{} scenario(s) failed:\n  - {}\n",
            failures.len(),
            failures.join("\n  - "),
        );
    }
}
