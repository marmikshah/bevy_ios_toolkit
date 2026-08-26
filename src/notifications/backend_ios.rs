//! iOS backend: raw `extern "C"` declarations resolved by the `Notifications`
//! Swift product's `@_cdecl` shims.
//!
//! Same contract as every other native module: Rust calls in, results come
//! back as polled state or a drained event queue — never callbacks into Rust,
//! because re-entrancy against winit's event loop is not safe.
//!
//! The string returned by `notifications_drain_events` points at a
//! Swift-owned buffer valid only until the next call, so callers copy it
//! immediately.

use std::ffi::c_char;

unsafe extern "C" {
    /// Install the `UNUserNotificationCenterDelegate` that buffers opens.
    /// Idempotent. See the module docs for why an app that wants to catch a
    /// cold launch calls this from its own app delegate instead.
    pub fn notifications_install_delegate();

    /// Ask for permission. Asynchronous; the resolved status surfaces through
    /// `notifications_status`.
    pub fn notifications_request();

    /// 0 not-determined, 1 denied, 2 authorized, 3 provisional.
    ///
    /// Reads a cached value and kicks off a refresh — never blocks. Asking the
    /// system synchronously would mean blocking a Bevy worker thread on the
    /// main thread, which deadlocks.
    pub fn notifications_status() -> i32;

    /// Schedule `id` to fire `after_secs` from now, replacing any pending
    /// request with the same id. Returns 0 on success and non-zero when the
    /// request was refused outright; a failure that only shows up later
    /// arrives as a drained event.
    pub fn notifications_schedule(
        id: *const c_char,
        title: *const c_char,
        body: *const c_char,
        after_secs: f64,
    ) -> i32;

    pub fn notifications_cancel(id: *const c_char);
    pub fn notifications_cancel_all();

    /// JSON array of pending events, draining the queue.
    pub fn notifications_drain_events() -> *const c_char;
}
