//! UIScene adoption for winit 0.30, including apps built with the iOS 27 SDK.
//!
//! Link the `Platform` SPM product and name `BevyIosToolkitSceneDelegate` in
//! `UIApplicationSceneManifest` (see `demo/ios/project.yml`). Set
//! `UIApplicationSupportsMultipleScenes` to false. The delegate attaches only
//! orphaned `WinitUIWindow` instances when they become visible; it does not
//! replace the app delegate or re-post lifecycle notifications.
//!
//! The workaround replaces `UIWindow.makeKeyAndVisible` once at registration,
//! and calls through for every window. It retains weak references to windows
//! shown before the first scene connection. Scene-aware windows
//! and other window classes retain their normal behavior. Remove this bridge
//! once the supported winit version adopts scenes upstream.

/// Retain/register the toolkit scene delegate before UIKit resolves the class
/// name in `Info.plist`. [`PlatformPlugin`](crate::platform::PlatformPlugin)
/// calls this on the main thread during plugin construction, before
/// `App::run` enters UIKit.
/// Apps using platform functions without the plugin must call this explicitly
/// from their main-thread entry point, before running the event loop. Safe to repeat.
pub fn register() {
    #[cfg(target_os = "ios")]
    unsafe {
        unsafe extern "C" {
            fn platform_register_scene_delegate();
        }
        platform_register_scene_delegate();
    }
}

/// Set the smallest supported iPad window in logical points. Call before run;
/// the toolkit applies it when the scene connects, or to an existing scene.
/// UIKit ignores this on devices without window size restrictions. Invalid
/// dimensions are ignored. Worker calls queue on UIKit without waiting for it.
#[cfg(target_os = "ios")]
pub fn set_minimum_window_size(width: f32, height: f32) {
    if !width.is_finite() || !height.is_finite() || width <= 0. || height <= 0. {
        return;
    }
    unsafe {
        unsafe extern "C" {
            fn platform_set_minimum_window_size(width: f32, height: f32);
        }
        platform_set_minimum_window_size(width, height);
    }
}

#[cfg(not(target_os = "ios"))]
pub fn set_minimum_window_size(_width: f32, _height: f32) {}
