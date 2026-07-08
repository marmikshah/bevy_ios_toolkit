//! Apple-platform niceties: impact [`haptics`], the [`safe_area`] insets,
//! opening outbound [`links`], and a first-frame [`boot_shield`]. Thin,
//! fire-and-forget wrappers over `Sources/Platform/PlatformBridge.swift`;
//! no-ops off iOS (links shell out to `open` on macOS so the flow is debuggable
//! on desktop).
//!
//! These are plain functions — call them from any system. There's no plugin and
//! no state to wire.
//!
//! ```no_run
//! use bevy_ios_toolkit::platform::{haptics, safe_area, links, boot_shield, Haptic};
//!
//! boot_shield::show(0.03, 0.04, 0.09);
//! haptics::play(Haptic::Light);
//! let inset = safe_area::top(); // or safe_area::insets() for all four edges
//! links::open("https://example.com");
//! ```

pub mod boot_shield;
pub mod haptics;
pub mod links;
pub mod safe_area;

pub use haptics::Haptic;
