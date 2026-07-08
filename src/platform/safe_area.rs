//! The device's safe-area insets (notch / Dynamic Island, home indicator,
//! rounded corners), in points.
//!
//! Returns the raw insets, or `0.0` when unavailable (off-iOS, or before the key
//! window exists). The caller owns the fallback policy — a layout typically
//! clamps to a notch-class constant when an inset reports `0.0` and adds its own
//! breathing-room margin.

/// The four safe-area insets, in points. A portrait game usually needs only
/// [`top`](Self::top); landscape games and bottom-anchored HUD/toasts want all
/// four (home indicator, rounded corners).
#[derive(Clone, Copy, Debug, Default, PartialEq)]
pub struct Insets {
    /// Notch / Dynamic Island edge.
    pub top: f32,
    /// Home-indicator edge.
    pub bottom: f32,
    /// Left rounded-corner / sensor edge (landscape).
    pub left: f32,
    /// Right rounded-corner / sensor edge (landscape).
    pub right: f32,
}

#[cfg(target_os = "ios")]
pub fn insets() -> Insets {
    unsafe extern "C" {
        fn platform_safe_top() -> f32;
        fn platform_safe_bottom() -> f32;
        fn platform_safe_left() -> f32;
        fn platform_safe_right() -> f32;
    }
    unsafe {
        Insets {
            top: platform_safe_top(),
            bottom: platform_safe_bottom(),
            left: platform_safe_left(),
            right: platform_safe_right(),
        }
    }
}

#[cfg(not(target_os = "ios"))]
pub fn insets() -> Insets {
    Insets::default()
}

/// The top safe-area inset alone — a convenience for the common portrait case.
/// Equivalent to `insets().top`, but a single FFI call.
#[cfg(target_os = "ios")]
pub fn top() -> f32 {
    unsafe extern "C" {
        fn platform_safe_top() -> f32;
    }
    unsafe { platform_safe_top() }
}

#[cfg(not(target_os = "ios"))]
pub fn top() -> f32 {
    0.0
}
