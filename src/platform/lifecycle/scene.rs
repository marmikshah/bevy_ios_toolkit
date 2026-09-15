//! UIScene adoption for winit 0.30, including apps built with the iOS 27 SDK.
//!
//! Link the `Platform` SPM product and name `BevyIosToolkitSceneDelegate` in
//! `UIApplicationSceneManifest` (see `demo/ios/project.yml`). Set
//! `UIApplicationSupportsMultipleScenes` to false. The delegate attaches only
//! orphaned `WinitUIWindow` instances when they become visible; it does not
//! replace the app delegate or re-post lifecycle notifications.
//!
//! The workaround replaces `UIWindow.makeKeyAndVisible` once, on the first
//! scene connection, and calls through for every window. Scene-aware windows
//! and other window classes retain their normal behavior. Remove this bridge
//! once the supported winit version adopts scenes upstream.

/// Retain/register the toolkit scene delegate before UIKit resolves the class
/// name in `Info.plist`. [`PlatformPlugin`](crate::platform::PlatformPlugin)
/// calls this during plugin construction, before `App::run` enters UIKit.
/// Apps using platform functions without the plugin must call this explicitly
/// from their entry point, before running the event loop. Safe to repeat.
pub fn register() {
    #[cfg(target_os = "ios")]
    unsafe {
        unsafe extern "C" {
            fn platform_register_scene_delegate();
        }
        platform_register_scene_delegate();
    }
}
