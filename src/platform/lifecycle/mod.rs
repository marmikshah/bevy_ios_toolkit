//! App lifecycle & session — the platform integrations tied to the app's launch
//! and audio session rather than one-off input/display niceties.
//!
//! - [`boot_shield`] — cover the first-frame `CAMetalLayer` flash on cold launch.
//! - [`audio`] — own the audio session so ads (or the OS default) can't leave
//!   the game muted.
//!
//! Re-exported flat under [`platform`](crate::platform) (`platform::boot_shield`,
//! `platform::audio`); grouped here because they share a concern — the running
//! app's state — distinct from the stateless niceties beside them.

pub mod audio;
pub mod boot_shield;
