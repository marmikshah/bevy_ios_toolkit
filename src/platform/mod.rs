//! Apple-platform integrations that need no plugin — plain, fire-and-forget
//! functions, no-ops off iOS. Thin wrappers over the `Platform` Swift shim (one
//! `.swift` file per concern under `Sources/Platform/`).
//!
//! Two groups:
//!
//! - **Input/display niceties** — stateless one-shots: impact/notification/
//!   selection [`haptics`], the four-sided [`safe_area`] insets, opening
//!   outbound [`links`], and the system [`share`] sheet.
//! - **App lifecycle & session** ([`lifecycle`]) — tied to the running app:
//!   the first-frame [`boot_shield`] and owning the [`audio`] session. Re-exported
//!   flat here for convenience.
//!
//! ```no_run
//! use bevy_ios_toolkit::platform::{haptics, safe_area, links, share, boot_shield, audio, Haptic};
//!
//! boot_shield::show(0.03, 0.04, 0.09);
//! haptics::play(Haptic::Light);
//! let inset = safe_area::top(); // or safe_area::insets() for all four edges
//! links::open("https://example.com");
//! share::text("I scored 4200!");
//! audio::configure(true, true); // own the session so ads can't mute the game
//! ```

pub mod haptics;
pub mod lifecycle;
pub mod links;
pub mod safe_area;
pub mod share;

pub use haptics::Haptic;
pub use lifecycle::{audio, boot_shield};
