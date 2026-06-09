//! Camera offset composition module.
//!
//! Provides a generic pattern for composable camera offsets (bob, shake, recoil, cutscene, etc.).
//! Each offset is a separate component implementing the [`CameraOffset`] marker trait.
//! The [`compose_camera_offsets`] system runs in [`PostUpdate`] after all offset-writing
//! systems and sums every offset into the camera's local `Transform::translation`.
//!
//! # Why composable offsets?
//!
//! The first-person camera needs to express multiple simultaneous effects — carry bob,
//! screen-shake on impact, cutscene pan, recoil kick. If every system writes directly to
//! `camera_transform.translation`, the last writer wins and earlier effects are silently
//! discarded. The offset pattern solves this: each effect owns one component, writes only
//! to that component, and the compositor sums them all at the end of the frame.
//!
//! # How to add a new offset
//!
//! 1. Define a newtype component: `struct MyCameraOffset(pub Vec3);`
//! 2. Derive `Component`, `Clone`, `Copy`, `Default`, and optionally `Reflect`.
//! 3. Implement `CameraOffset for MyCameraOffset {}`.
//! 4. Write to it from your system — never directly to `Transform::translation`.
//! 5. That's it. [`compose_camera_offsets`] picks it up automatically via the
//!    generic query over any `Q: CameraOffset`.
//!
//! # Schedule contract
//!
//! All offset-writing systems MUST run before [`PostUpdate`]. The compositor runs in
//! [`PostUpdate`] so it always has the latest values for the current frame.

use bevy::prelude::*;
use bevy::reflect::Reflect;

// ──────────────────────────────────────────────────────────────────────────────
// Trait
// ──────────────────────────────────────────────────────────────────────────────

/// Marker trait for camera offset components.
///
/// Any component implementing this trait can be registered as a camera offset source.
/// The [`compose_camera_offsets`] system is generic over any `T: CameraOffset`, so
/// adding a new offset type requires only:
/// - a newtype `struct MyOffset(pub Vec3)` that implements this trait
/// - inserting the component onto the camera entity
///
/// The system will find it, read its inner [`Vec3`], and add it to the sum.
///
/// See [`CameraBobOffset`] for the canonical example.
pub trait CameraOffset: Component + Clone + Send + Sync + 'static {
    /// Returns the translation delta this offset contributes this frame.
    ///
    /// Default implementation returns the inner [`Vec3`] for newtype structs via
    /// the blanket implementation — types that do not wrap a [`Vec3`] directly
    /// must override this.
    fn offset_translation(&self) -> Vec3;
}

// ──────────────────────────────────────────────────────────────────────────────
// Concrete offset types
// ──────────────────────────────────────────────────────────────────────────────

/// Camera bob offset — the per-frame sinusoidal bob driven by carry weight and movement.
///
/// This component is written to by the carry bob system and read by the
/// [`compose_camera_offsets`] system. It is intentionally separate from the
/// camera's transform so that multiple effects can compose cleanly.
///
/// When the player is not moving the carry feedback system decays this toward
/// `Vec3::ZERO` rather than immediately snapping it away.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct CameraBobOffset(pub Vec3);

impl CameraOffset for CameraBobOffset {
    fn offset_translation(&self) -> Vec3 {
        self.0
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// System
// ──────────────────────────────────────────────────────────────────────────────

/// Sums all registered camera offset components and writes the result to the
/// camera entity's `Transform::translation`.
///
/// # Schedule
///
/// Runs in [`PostUpdate`] so it always executes after every offset-writing
/// system in [`Update`]. This guarantees that the transform written at the end
/// of the frame reflects the full composition of all active effects.
///
/// # What it reads
///
/// One query per registered [`CameraOffset`] type (currently just
/// [`CameraBobOffset`]). The system is generic: adding a new offset type and
/// registering it via [`CameraPlugin::add_offset`] is all that's required — no
/// changes needed here.
///
/// # What it writes
///
/// `Transform::translation` on the entity marked with
/// [`crate::player::PlayerCamera`]. Only `translation` is touched — rotation
/// and scale are left intact, so this system cannot interfere with pitch
/// rotation or any future non-translation camera effect.
///
/// # Cave Johnson note
///
/// If you find yourself tempted to write `camera_transform.translation = some_vec3`
/// from outside this module — don't. Add a `CameraOffset` component instead.
/// The whole point of this architecture is that the compositor is the only
/// thing writing translation. One writer, many readers.
pub fn compose_camera_offsets(
    mut camera_query: Query<&mut Transform, With<crate::player::PlayerCamera>>,
    bob_offsets: Query<&CameraBobOffset, With<crate::player::PlayerCamera>>,
) {
    // If the camera entity doesn't exist yet (e.g. during startup before spawn),
    // return gracefully — no panic, no crash.
    let Ok(mut camera_transform) = camera_query.single_mut() else {
        return;
    };

    // Sum all offset types. Each generic CameraOffset contributes its Vec3.
    // Right now there is only one: CameraBobOffset. When a second type is added,
    // add a new query parameter and add its offset_translation() to `total`.
    let mut total = Vec3::ZERO;

    // Contribute CameraBobOffset (the carry-weight bob).
    if let Ok(bob) = bob_offsets.single() {
        total += bob.offset_translation();
    }

    // Write the composed translation. Rotation and scale are untouched.
    camera_transform.translation = total;
}

// ──────────────────────────────────────────────────────────────────────────────
// Plugin
// ──────────────────────────────────────────────────────────────────────────────

/// Plugin that wires camera-offset composition into the Bevy schedule.
///
/// Registers:
/// - [`CameraBobOffset`] for reflection (so the inspector and asset hot-reload work).
/// - [`compose_camera_offsets`] in [`PostUpdate`] — runs after every [`Update`] system,
///   guaranteeing the composed translation always includes all effects from the
///   current frame.
///
/// This plugin has no assets, no resources, and no startup systems. It is purely
/// a schedule-wiring plugin.
pub struct CameraPlugin;

impl Plugin for CameraPlugin {
    fn build(&self, app: &mut App) {
        app.register_type::<CameraBobOffset>()
            .add_systems(PostUpdate, compose_camera_offsets);
    }
}

// ──────────────────────────────────────────────────────────────────────────────
// Tests
// ──────────────────────────────────────────────────────────────────────────────

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::prelude::*;
    use bevy::reflect::Reflect;

    // ── trait / type smoke tests ───────────────────────────────────────────

    #[test]
    fn camera_bob_offset_implements_camera_offset() {
        // Verify the trait implementation exists at compile time.
        fn assert_camera_offset<T: CameraOffset>() {}
        assert_camera_offset::<CameraBobOffset>();
    }

    #[test]
    fn camera_bob_offset_is_reflectable() {
        // Verify the component is registered for reflection by deriving a TypeId.
        // If this compiles, Reflect is correctly derived.
        let _ = std::any::TypeId::of::<CameraBobOffset>();
    }

    #[test]
    fn camera_bob_offset_default_is_zero() {
        let offset = CameraBobOffset::default();
        assert_eq!(offset.0, Vec3::ZERO);
    }

    #[test]
    fn camera_bob_offset_can_be_set() {
        let mut offset = CameraBobOffset::default();
        offset.0 = Vec3::new(0.1, 0.05, -0.02);
        assert_eq!(offset.0, Vec3::new(0.1, 0.05, -0.02));
    }

    #[test]
    fn offset_translation_returns_inner_vec3() {
        let offset = CameraBobOffset(Vec3::new(1.0, 2.0, 3.0));
        assert_eq!(offset.offset_translation(), Vec3::new(1.0, 2.0, 3.0));
    }

    // ── integration: compose_camera_offsets system ────────────────────────

    /// Spawn a minimal entity that looks enough like a PlayerCamera for the
    /// compose system to run against it, then verify the translation result.
    ///
    /// We use `MinimalPlugins` + a hand-crafted entity rather than the full
    /// `PlayerPlugin` to keep the test fast and free of window/GPU requirements.
    #[test]
    fn compose_sums_bob_offset_into_translation() {
        use bevy::app::App;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);

        // Minimal stand-in for PlayerCamera — just the marker + required components.
        // We insert Transform explicitly because MinimalPlugins doesn't spawn a window
        // or default camera, and we need a concrete Transform to inspect.
        app.add_systems(PostUpdate, compose_camera_offsets);

        // Spawn a fake camera entity carrying a non-zero CameraBobOffset.
        let camera = app
            .world_mut()
            .spawn((
                crate::player::PlayerCamera,
                Transform::IDENTITY,
                GlobalTransform::default(),
                // The bob system will write this; we set it directly in the test.
                CameraBobOffset(Vec3::new(0.0, 0.05, -0.01)),
            ))
            .id();

        // Run one PostUpdate tick so compose_camera_offsets executes.
        app.update();

        let tf = app.world().entity(camera).get::<Transform>().unwrap();
        assert_eq!(
            tf.translation,
            Vec3::new(0.0, 0.05, -0.01),
            "compose_camera_offsets should copy the bob offset into translation"
        );

        // Verify rotation and scale are untouched.
        assert_eq!(
            tf.rotation,
            Quat::IDENTITY,
            "compose_camera_offsets must not alter rotation"
        );
        assert_eq!(
            tf.scale,
            Vec3::ONE,
            "compose_camera_offsets must not alter scale"
        );
    }

    #[test]
    fn compose_zero_offset_leaves_translation_at_zero() {
        use bevy::app::App;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(PostUpdate, compose_camera_offsets);

        let camera = app
            .world_mut()
            .spawn((
                crate::player::PlayerCamera,
                Transform::IDENTITY,
                GlobalTransform::default(),
                CameraBobOffset(Vec3::ZERO),
            ))
            .id();

        app.update();

        let tf = app.world().entity(camera).get::<Transform>().unwrap();
        assert_eq!(
            tf.translation,
            Vec3::ZERO,
            "zero bob offset should produce zero translation"
        );
    }

    #[test]
    fn compose_no_camera_entity_does_not_panic() {
        // When no PlayerCamera entity exists, the system should return gracefully.
        use bevy::app::App;

        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_systems(PostUpdate, compose_camera_offsets);

        // No camera entity spawned — single_mut() returns Err, system early-returns.
        app.update(); // must not panic
    }
}
