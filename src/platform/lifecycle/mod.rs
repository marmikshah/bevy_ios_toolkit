//! App lifecycle & session, re-exported flat under [`platform`](crate::platform).
//!
//! - [`boot_shield`] — cover the first-frame `CAMetalLayer` flash on cold launch.
//! - [`audio`] — own the audio session so ads (or the OS default) can't leave
//!   the game muted.
//! - [`scene`] — scene-based launch and the minimum window size.

pub mod audio;
pub mod boot_shield;
pub mod scene;
