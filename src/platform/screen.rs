//! Window or startup screen dimensions in UIKit points.

use bevy::{prelude::Vec2, window::WindowResolution};

/// The key window's size in points, falling back to the foreground scene and
/// then the main screen before UIKit creates a window or scene.
///
/// Call on the main thread before building `WindowPlugin` to size the initial
/// iOS window. These are points, not physical pixels: at startup winit treats
/// the requested dimensions as points, before it learns the display scale.
/// Once a key window exists this reports its bounds, including iPad resizing.
/// Worker-thread calls return the latest cached measurement and schedule a
/// refresh without blocking; the cache starts at zero. Returns zero off iOS.
#[cfg(target_os = "ios")]
pub fn screen_size() -> Vec2 {
    unsafe extern "C" {
        fn platform_screen_size(out: *mut f32);
    }
    let mut size = [0.0; 2];
    unsafe { platform_screen_size(size.as_mut_ptr()) };
    Vec2::from_array(size)
}

#[cfg(not(target_os = "ios"))]
pub fn screen_size() -> Vec2 {
    Vec2::ZERO
}

/// The primary window's initial resolution: [`screen_size`] in points when it
/// is a usable measurement (finite and over one point on both axes), otherwise
/// `fallback`. Fractional points are truncated.
///
/// Off iOS the screen size is zero, so desktop builds get `fallback`, such as
/// a phone-shaped preview window. Pass `WindowResolution::default()` to keep
/// Bevy's default when UIKit has not measured yet.
///
/// ```no_run
/// use bevy::prelude::*;
/// use bevy_ios_toolkit::platform;
///
/// let window = Window {
///     resolution: platform::initial_resolution((390, 844)),
///     ..default()
/// };
/// ```
pub fn initial_resolution(fallback: impl Into<WindowResolution>) -> WindowResolution {
    resolution_for(screen_size(), fallback.into())
}

fn resolution_for(size: Vec2, fallback: WindowResolution) -> WindowResolution {
    if size.is_finite() && size.x > 1.0 && size.y > 1.0 {
        WindowResolution::new(size.x as u32, size.y as u32)
    } else {
        fallback
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn measured_screens_size_the_window_and_unmeasured_ones_fall_back() {
        let fallback = || WindowResolution::new(390, 844);
        let size = |resolution: WindowResolution| {
            (resolution.physical_width(), resolution.physical_height())
        };
        assert_eq!(
            size(resolution_for(Vec2::new(402.0, 874.5), fallback())),
            (402, 874)
        );
        for unmeasured in [
            Vec2::ZERO,
            Vec2::new(1.0, 844.0),
            Vec2::new(f32::NAN, 844.0),
        ] {
            assert_eq!(size(resolution_for(unmeasured, fallback())), (390, 844));
        }
        // Desktop has no screen measurement.
        #[cfg(not(target_os = "ios"))]
        assert_eq!(size(initial_resolution((390, 844))), (390, 844));
    }
}
