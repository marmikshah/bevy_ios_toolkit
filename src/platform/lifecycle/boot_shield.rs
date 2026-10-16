//! First-frame boot shield — cover the magenta `CAMetalLayer` flash on cold
//! launch.
//!
//! Between the launch screen and the first presented drawable, the
//! uninitialized `CAMetalLayer` shows through (the classic magenta/black flash).
//! [`show`] raises an opaque, launch-coloured window over the winit view at
//! startup; [`first_frame_presented`] tears it down (fading) once the game has
//! actually drawn a frame — so shells stop hand-rolling a frame-counter FFI to
//! do the same thing.
//!
//! ```no_run
//! use bevy_ios_toolkit::platform::boot_shield;
//!
//! // At startup, before the first frame:
//! boot_shield::show(0.03, 0.04, 0.09); // your launch background colour
//!
//! // From the system that runs after the first rendered frame:
//! boot_shield::first_frame_presented();
//! ```
//!
//! Both are fire-and-forget and no-ops off iOS.
//!
//! # `BootShieldPlugin` (`boot-shield` feature)
//!
//! Counting app frames is not proof of visible pixels: cold pipeline
//! compilation can run several updates before the first real present.
//! [`BootShieldPlugin`] adds two render-world systems on iOS. The first paints
//! each window's drawable with `ClearColor` for the first
//! [`LAUNCH_PAINT_FRAMES`] frames and while any pipeline is still compiling, so
//! an unwritten Metal drawable never shows. The second calls
//! [`first_frame_presented`] after [`WARM_PRESENTS`] window presents with every
//! pipeline ready, or after [`MAX_PRESENTS`] regardless. The game still calls
//! [`show`] before building its app. [`BootShieldReady`] reports the dismissal
//! to the main world.
//!
//! ```no_run
//! # #[cfg(feature = "boot-shield")] {
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::platform::boot_shield::{self, BootShieldPlugin};
//!
//! boot_shield::show(0.03, 0.04, 0.09);
//! App::new()
//!     .add_plugins((DefaultPlugins, BootShieldPlugin))
//!     .insert_resource(ClearColor(Color::srgb(0.03, 0.04, 0.09)))
//!     .run();
//! # }
//! ```

/// Raise the boot shield: an opaque window in `(r, g, b)` (each `0.0..=1.0`)
/// covering everything until [`first_frame_presented`] dismisses it. Call once
/// at startup, as early as possible. A second call while the shield is up is
/// ignored.
#[cfg(target_os = "ios")]
pub fn show(r: f32, g: f32, b: f32) {
    unsafe extern "C" {
        fn platform_boot_shield_show(r: f32, g: f32, b: f32);
    }
    unsafe { platform_boot_shield_show(r, g, b) };
}

#[cfg(not(target_os = "ios"))]
pub fn show(_r: f32, _g: f32, _b: f32) {}

/// Dismiss the boot shield (a short fade), revealing the game's first real
/// frame. Call once the renderer has presented — idempotent and a no-op if no
/// shield is up.
#[cfg(target_os = "ios")]
pub fn first_frame_presented() {
    unsafe extern "C" {
        fn platform_boot_shield_dismiss();
    }
    unsafe { platform_boot_shield_dismiss() };
}

#[cfg(not(target_os = "ios"))]
pub fn first_frame_presented() {}

#[cfg(feature = "boot-shield")]
pub use plugin::{
    BootShieldPlugin, BootShieldReady, LAUNCH_PAINT_FRAMES, MAX_PRESENTS, WARM_PRESENTS,
};

#[cfg(feature = "boot-shield")]
mod plugin;

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn desktop_calls_are_noops() {
        // Off iOS the shield is inert — the flow stays callable on desktop.
        show(0.03, 0.04, 0.09);
        first_frame_presented();
    }
}
