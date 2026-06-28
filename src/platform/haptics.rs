//! iOS impact haptics through the C-ABI shim. Fire-and-forget; the Swift side
//! owns prepared `UIImpactFeedbackGenerator`s on the main queue. No-op on every
//! other platform.

/// Impact strength. Values are the generator index the Swift side maps to
/// `UIImpactFeedbackStyle` (`.light/.medium/.heavy/.rigid`).
#[derive(Clone, Copy, Debug)]
pub enum Haptic {
    Light = 0,
    Medium = 1,
    Heavy = 2,
    /// A sharp tick, distinct from the softer light thump.
    Rigid = 3,
}

#[cfg(target_os = "ios")]
pub fn play(kind: Haptic) {
    unsafe extern "C" {
        fn cupertino_haptic(kind: i32);
    }
    unsafe { cupertino_haptic(kind as i32) };
}

#[cfg(not(target_os = "ios"))]
pub fn play(_kind: Haptic) {}
