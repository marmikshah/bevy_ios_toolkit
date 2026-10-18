//! C-string plumbing shared by the native modules (see the crate's native
//! contract). String getters return Swift-owned buffers valid only until the
//! next regenerating call, so callers copy immediately via [`read_cstr`].

use std::ffi::{CStr, c_char};

/// Copy a backend-owned C string into an owned `String`. Null → empty.
pub(crate) unsafe fn read_cstr(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}
