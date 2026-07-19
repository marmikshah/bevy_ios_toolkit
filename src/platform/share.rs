//! Present the system share sheet (`UIActivityViewController`) with a block of
//! text — the organic-virality primitive: a player taps "share" on a result card
//! and the OS owns the rest.
//!
//! Fire-and-forget, like every other `platform` function: iOS routes through the
//! C-ABI shim, other platforms log the payload so the flow is traceable on a
//! desktop run. Nothing is reported back — which activity the user picked, or
//! whether they cancelled, is not observable here.
//!
//! ```no_run
//! use bevy_ios_toolkit::platform::share;
//!
//! share::text("undelivered — day 214\n7 doors · 142m");
//! ```

/// Present the share sheet with `text`. No-op if a sheet is already up, or if
/// `text` contains an interior NUL.
#[cfg(target_os = "ios")]
pub fn text(text: &str) {
    unsafe extern "C" {
        fn platform_share_text(text: *const std::ffi::c_char);
    }
    if let Ok(c) = std::ffi::CString::new(text) {
        unsafe { platform_share_text(c.as_ptr()) };
    }
}

/// Desktop fake: print what would be shared. There is no cross-platform share
/// sheet worth emulating and nothing observes the outcome, so the payload on
/// stderr is the whole observable effect. Goes direct rather than through a log
/// facade because the crate depends on bevy with no default features.
#[cfg(not(target_os = "ios"))]
pub fn text(text: &str) {
    eprintln!("[bevy_ios_toolkit fake share] {text}");
}
