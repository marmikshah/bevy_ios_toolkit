//! The device's power signals — thermal pressure and Low Power Mode — as a
//! polled Bevy resource plus a change message.
//!
//! Apple's recommended adaptive-quality pattern: when the device reports
//! [`ThermalState::Serious`] the system is already throttling, so a game should
//! shed work (cap the frame rate, cheapen post-processing) rather than be
//! throttled into a stutter. Low Power Mode is the user asking for the same
//! thing explicitly.
//!
//! Read [`PowerState`] for the current values, or react once per transition with
//! [`PowerStateChanged`] — the resource is polled every frame but the message is
//! only written when something actually moves.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::prelude::*;
//!
//! fn adapt(mut changes: MessageReader<PowerStateChanged>) {
//!     for PowerStateChanged(power) in changes.read() {
//!         if power.is_constrained() { /* drop to the low quality tier */ }
//!     }
//! }
//! ```
//!
//! # Desktop fake (non-iOS)
//!
//! Reports `Nominal` / not-low-power. Override with
//! `BEVY_IOS_FAKE_THERMAL=nominal|fair|serious|critical` and
//! `BEVY_IOS_FAKE_LOW_POWER=1`. Both are re-read every poll, so flipping one at
//! runtime exercises the transition path without cooking a phone.

use bevy::prelude::*;

#[cfg(target_os = "ios")]
mod backend {
    unsafe extern "C" {
        /// 0 nominal, 1 fair, 2 serious, 3 critical.
        pub fn platform_thermal_state() -> i32;
        /// 1 when Low Power Mode is on, 0 otherwise.
        pub fn platform_low_power_mode() -> i32;
    }
}

#[cfg(not(target_os = "ios"))]
mod backend {
    pub unsafe fn platform_thermal_state() -> i32 {
        match std::env::var("BEVY_IOS_FAKE_THERMAL")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "fair" => 1,
            "serious" => 2,
            "critical" => 3,
            _ => 0,
        }
    }

    pub unsafe fn platform_low_power_mode() -> i32 {
        match std::env::var("BEVY_IOS_FAKE_LOW_POWER")
            .unwrap_or_default()
            .as_str()
        {
            "1" | "true" => 1,
            _ => 0,
        }
    }
}

/// Thermal pressure, mirroring `ProcessInfo.ThermalState`.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ThermalState {
    /// No corrective action needed.
    #[default]
    Nominal,
    /// Mildly elevated — shed discretionary work (background downloads, etc).
    Fair,
    /// The system is throttling. Reduce the frame rate and effect cost now.
    Serious,
    /// Throttling hard; the device may shut down peripherals. Do the minimum.
    Critical,
}

impl ThermalState {
    fn from_i32(v: i32) -> ThermalState {
        match v {
            1 => ThermalState::Fair,
            2 => ThermalState::Serious,
            3 => ThermalState::Critical,
            _ => ThermalState::Nominal,
        }
    }
}

/// The device's current power signals. Updated every frame by
/// [`PlatformPlugin`](super::PlatformPlugin); stays at its defaults off iOS
/// unless the fake's env knobs are set.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct PowerState {
    pub thermal: ThermalState,
    pub low_power_mode: bool,
}

impl PowerState {
    /// Whether the game should be on a reduced quality tier — the device is
    /// throttling, or the user asked for longer battery life. The two are
    /// separate signals but call for the same response, so most games branch
    /// on this rather than on either field.
    pub fn is_constrained(self) -> bool {
        self.low_power_mode
            || matches!(self.thermal, ThermalState::Serious | ThermalState::Critical)
    }
}

/// Emitted when [`PowerState`] changes — the transition, not the level, so a
/// game can retune once instead of diffing every frame.
#[derive(Message, Clone, Copy, Debug)]
pub struct PowerStateChanged(pub PowerState);

pub(super) fn poll_power(
    mut power: ResMut<PowerState>,
    mut changed: MessageWriter<PowerStateChanged>,
) {
    let current = PowerState {
        thermal: ThermalState::from_i32(unsafe { backend::platform_thermal_state() }),
        low_power_mode: unsafe { backend::platform_low_power_mode() } != 0,
    };
    if current != *power {
        *power = current;
        changed.write(PowerStateChanged(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::platform::PlatformPlugin;

    #[test]
    fn thermal_change_updates_resource_and_emits_once() {
        // SAFETY: single-threaded test; only this test reads these two vars.
        unsafe { std::env::remove_var("BEVY_IOS_FAKE_THERMAL") };

        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(PlatformPlugin);

        app.update();
        assert_eq!(
            app.world().resource::<PowerState>().thermal,
            ThermalState::Nominal
        );
        assert!(!app.world().resource::<PowerState>().is_constrained());

        unsafe { std::env::set_var("BEVY_IOS_FAKE_THERMAL", "serious") };
        app.update();

        let power = *app.world().resource::<PowerState>();
        assert_eq!(power.thermal, ThermalState::Serious);
        assert!(power.is_constrained());

        // The transition wrote exactly one message...
        let written: Vec<_> = app
            .world_mut()
            .resource_mut::<Messages<PowerStateChanged>>()
            .drain()
            .collect();
        assert_eq!(written.len(), 1);
        assert_eq!(written[0].0.thermal, ThermalState::Serious);

        // ...and holding that level writes no more.
        app.update();
        assert!(
            app.world_mut()
                .resource_mut::<Messages<PowerStateChanged>>()
                .drain()
                .next()
                .is_none()
        );

        unsafe { std::env::remove_var("BEVY_IOS_FAKE_THERMAL") };
    }
}
