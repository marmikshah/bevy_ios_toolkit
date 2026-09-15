//! Window or startup screen dimensions in UIKit points.

use bevy::prelude::Vec2;

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
