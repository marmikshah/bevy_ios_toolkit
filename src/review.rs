//! Ask for an App Store rating via `SKStoreReviewController` / `AppStore`.
//!
//! Fire-and-forget: iOS decides whether to actually show the prompt (it is
//! heavily rate-limited — at most a few times a year, never guaranteed). Call
//! [`request`] at a genuine moment of delight; never gate anything on it.
//!
//! ```no_run
//! use bevy_ios_toolkit::review;
//!
//! fn on_win() {
//!     review::request();
//! }
//! ```
//!
//! No-op on every non-iOS platform.

/// Request the system review prompt. The OS may or may not present it.
#[cfg(target_os = "ios")]
pub fn request() {
    unsafe extern "C" {
        fn review_request();
    }
    unsafe { review_request() };
}

#[cfg(not(target_os = "ios"))]
pub fn request() {}
