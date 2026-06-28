//! The device's top safe-area inset (notch / Dynamic Island), in points.
//!
//! Returns the raw inset, or `0.0` when unavailable (off-iOS, or before the key
//! window exists). The caller owns the fallback policy — a layout typically
//! clamps to a notch-class constant when this reports `0.0` and adds its own
//! breathing-room margin.

#[cfg(target_os = "ios")]
pub fn top() -> f32 {
    unsafe extern "C" {
        fn cupertino_safe_top() -> f32;
    }
    unsafe { cupertino_safe_top() }
}

#[cfg(not(target_os = "ios"))]
pub fn top() -> f32 {
    0.0
}
