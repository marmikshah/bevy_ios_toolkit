//! Shared FFI plumbing for the C-ABI modules that marshal C strings across the
//! bridge (store, ads, gamekit).
//!
//! Every native module follows the same contract: `@_cdecl` Swift entry points
//! called from Rust, results read back as polled state or a drained event queue
//! — never callbacks into Rust, because re-entrancy against winit's event loop
//! is not safe. The string getters return pointers to Swift-owned buffers valid
//! only until the next regenerating call, so callers copy immediately via
//! [`read_cstr`].

use std::ffi::{CStr, c_char};

/// Copy a backend-owned C string into an owned `String`. Null → empty.
// Used by StoreKit on iOS and by ads on every target; genuinely unused only in
// a gamekit-only build.
#[allow(dead_code)]
pub(crate) unsafe fn read_cstr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
