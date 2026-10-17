//! Open an external URL in the system browser. iOS routes through the C-ABI
//! shim. Desktop builds hand the URL to the system opener so the flow is
//! debuggable: `open` on macOS, `explorer` on Windows and `xdg-open` on other
//! Unix systems. Other platforms no-op. Desktop failures are ignored, as a
//! missing opener should not stop the game.

#[cfg(target_os = "ios")]
pub fn open(url: &str) {
    unsafe extern "C" {
        fn platform_open_url(url: *const std::ffi::c_char);
    }
    if let Ok(c) = std::ffi::CString::new(url) {
        unsafe { platform_open_url(c.as_ptr()) };
    }
}

#[cfg(not(target_os = "ios"))]
pub fn open(url: &str) {
    if let Some(opener) = opener() {
        let _ = std::process::Command::new(opener).arg(url).spawn();
    }
}

/// The desktop program that opens a URL in the user's browser.
#[cfg(not(target_os = "ios"))]
fn opener() -> Option<&'static str> {
    if cfg!(target_os = "macos") {
        Some("open")
    } else if cfg!(target_os = "windows") {
        Some("explorer")
    } else if cfg!(all(unix, not(target_os = "android"))) {
        Some("xdg-open")
    } else {
        None
    }
}
