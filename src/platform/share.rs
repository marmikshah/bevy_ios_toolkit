//! Present the system share sheet (`UIActivityViewController`) with text.
//!
//! Fire-and-forget: nothing is reported back. Off iOS the payload is printed.
//!
//! ```no_run
//! use bevy_ios_toolkit::platform::share;
//!
//! share::text("Game — day 214\n7 doors · 142m");
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

/// Desktop fake: print what would be shared (stderr, since the crate has no log
/// facade without Bevy's default features).
#[cfg(not(target_os = "ios"))]
pub fn text(text: &str) {
    eprintln!("[bevy_ios_toolkit fake share] {text}");
}
