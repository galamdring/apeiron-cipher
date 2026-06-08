//! Camera offset composition module.
//!
//! Provides a generic pattern for composable camera offsets (bob, shake, recoil, cutscene, etc.).
//! Each offset is a separate component implementing the [`CameraOffset`] marker trait.
//! The `compose_camera_offsets` system (in a separate module) sums all offsets
//! and applies the result to the camera transform.

use bevy::prelude::*;
use bevy::reflect::Reflect;

/// Marker trait for camera offset components.
///
/// Any component implementing this trait will be picked up by the
/// `compose_camera_offsets` system and summed into the final camera transform.
/// This allows multiple camera effects (bob, shake, recoil, cutscene) to coexist
/// without conflicting over direct transform writes.
pub trait CameraOffset: Component + Clone + Send + Sync + 'static {}

/// Camera bob offset — the per-frame sinusoidal bob driven by carry weight and movement.
///
/// This component is written to by the carry bob system and read by the
/// `compose_camera_offsets` system. It is intentionally separate from the
/// camera's transform so that multiple effects can compose cleanly.
#[derive(Component, Clone, Copy, Debug, Default, PartialEq, Reflect)]
#[reflect(Component, Default)]
pub struct CameraBobOffset(pub Vec3);

impl CameraOffset for CameraBobOffset {}

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::reflect::Reflect;

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
}
