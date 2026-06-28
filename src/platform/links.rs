//! Open an external URL in the system browser. iOS routes through the C-ABI
//! shim; macOS shells out to `open` so the flow is debuggable on desktop; other
//! platforms no-op.

#[cfg(target_os = "ios")]
pub fn open(url: &str) {
    unsafe extern "C" {
        fn cupertino_open_url(url: *const std::ffi::c_char);
    }
    if let Ok(c) = std::ffi::CString::new(url) {
        unsafe { cupertino_open_url(c.as_ptr()) };
    }
}

#[cfg(target_os = "macos")]
pub fn open(url: &str) {
    let _ = std::process::Command::new("open").arg(url).spawn();
}

#[cfg(not(any(target_os = "ios", target_os = "macos")))]
pub fn open(_url: &str) {}
