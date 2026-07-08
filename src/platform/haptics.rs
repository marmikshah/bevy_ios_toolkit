//! iOS haptics through the C-ABI shim. Fire-and-forget; the Swift side owns
//! prepared feedback generators on the main queue. No-op on every other
//! platform.

/// A haptic to fire. The value is the kind index the Swift side maps to a
/// `UIFeedbackGenerator`:
/// - `Light`/`Medium`/`Heavy`/`Rigid` → `UIImpactFeedbackGenerator`
/// - `Success`/`Warning`/`Error` → `UINotificationFeedbackGenerator`
/// - `Selection` → `UISelectionFeedbackGenerator`
#[derive(Clone, Copy, Debug)]
pub enum Haptic {
    /// A soft, light thump.
    Light = 0,
    Medium = 1,
    Heavy = 2,
    /// A sharp tick, distinct from the softer light thump.
    Rigid = 3,
    /// A task succeeded (level clear, purchase complete).
    Success = 4,
    /// A caution (invalid action, low resource).
    Warning = 5,
    /// A failure (death, rejected input).
    Error = 6,
    /// A discrete selection change (picker detent, toggle flip).
    Selection = 7,
}

#[cfg(target_os = "ios")]
pub fn play(kind: Haptic) {
    unsafe extern "C" {
        fn platform_haptic(kind: i32);
    }
    unsafe { platform_haptic(kind as i32) };
}

#[cfg(not(target_os = "ios"))]
pub fn play(_kind: Haptic) {}
