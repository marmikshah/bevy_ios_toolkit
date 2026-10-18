//! Apple-platform integrations — thin wrappers over the `Platform` Swift shim
//! (one `.swift` file per concern under `Sources/Platform/`), no-ops off iOS.
//!
//! - **Input/display** — impact/notification/selection [`haptics`], the
//!   four-sided [`safe_area`] insets, the screen size ([`initial_resolution`],
//!   [`screen_size`]), outbound [`links`], and the system [`share`] sheet.
//! - **App lifecycle & session** ([`lifecycle`]) — the first-frame
//!   [`boot_shield`], the [`audio`] session and [`scene`] setup.
//! - **Device state** ([`power`]) — thermal pressure and Low Power Mode, polled
//!   into a resource.
//!
//! [`PlatformPlugin`] drives [`power`], [`SafeArea`] and scene registration;
//! everything else is a plain function you call directly.
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

use bevy::prelude::*;

pub mod haptics;
pub mod lifecycle;
pub mod links;
pub mod power;
pub mod safe_area;
mod screen;
pub mod share;

pub use haptics::Haptic;
pub use lifecycle::{audio, boot_shield, scene};
pub use power::{PowerState, PowerStateChanged, ThermalState};
pub use safe_area::{SafeArea, SafeAreaPlugin};
pub use screen::{initial_resolution, screen_size};

/// Wires the polled parts of this module. Installed by
/// [`IosPlugin`](crate::IosPlugin); the fire-and-forget functions need none of
/// it and work without the plugin.
pub struct PlatformPlugin;

impl Plugin for PlatformPlugin {
    fn build(&self, app: &mut App) {
        scene::register();
        app.init_resource::<PowerState>()
            .add_message::<PowerStateChanged>()
            .add_systems(Update, power::poll_power)
            .add_plugins(SafeAreaPlugin);
    }
}
