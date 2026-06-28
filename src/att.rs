//! App Tracking Transparency (ATT) as Bevy resources + messages.
//!
//! iOS requires the ATT prompt before an app accesses the IDFA for cross-app
//! tracking — which AdMob uses to serve personalized ads. Send
//! [`RequestTracking`] (typically once, after the first frame or after your own
//! pre-prompt) and read [`TrackingStatus`] to branch.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::prelude::*;
//!
//! fn ask_once(mut req: MessageWriter<RequestTracking>, mut done: Local<bool>) {
//!     if !*done {
//!         *done = true;
//!         req.write(RequestTracking);
//!     }
//! }
//!
//! fn react(status: Res<TrackingStatus>) {
//!     if status.is_authorized() { /* request personalized ads */ }
//! }
//! ```
//!
//! # Desktop fake (non-iOS)
//!
//! Starts `NotDetermined`; a [`RequestTracking`] resolves it to `Authorized`.
//! Override with `BEVY_IOS_FAKE_ATT=authorized|denied|restricted|notdetermined`
//! (the value a request resolves to).

use bevy::prelude::*;

#[cfg(target_os = "ios")]
mod backend {
    unsafe extern "C" {
        /// Present the ATT prompt if status is not-determined (async; the
        /// resolved status surfaces via `att_status`).
        pub fn att_request();
        /// 0 not-determined, 1 restricted, 2 denied, 3 authorized.
        pub fn att_status() -> i32;
    }
}

#[cfg(not(target_os = "ios"))]
mod backend {
    use std::sync::atomic::{AtomicI32, Ordering};

    static STATUS: AtomicI32 = AtomicI32::new(0);

    fn env_status() -> i32 {
        match std::env::var("BEVY_IOS_FAKE_ATT")
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str()
        {
            "restricted" => 1,
            "denied" => 2,
            "authorized" => 3,
            "notdetermined" => 0,
            _ => 3, // default: a request grants authorization
        }
    }

    pub unsafe fn att_request() {
        STATUS.store(env_status(), Ordering::SeqCst);
    }

    pub unsafe fn att_status() -> i32 {
        STATUS.load(Ordering::SeqCst)
    }
}

/// ATT authorization status for IDFA access.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum TrackingStatus {
    /// The user has not yet been prompted.
    #[default]
    NotDetermined,
    /// Tracking is restricted (e.g. parental controls); cannot be changed.
    Restricted,
    /// The user denied tracking.
    Denied,
    /// The user authorized tracking — the IDFA is available.
    Authorized,
}

impl TrackingStatus {
    fn from_i32(v: i32) -> TrackingStatus {
        match v {
            1 => TrackingStatus::Restricted,
            2 => TrackingStatus::Denied,
            3 => TrackingStatus::Authorized,
            _ => TrackingStatus::NotDetermined,
        }
    }

    pub fn is_authorized(self) -> bool {
        matches!(self, TrackingStatus::Authorized)
    }

    /// Whether the prompt can still be shown (only from `NotDetermined`).
    pub fn is_determined(self) -> bool {
        !matches!(self, TrackingStatus::NotDetermined)
    }
}

/// Present the ATT prompt (no-op if already determined).
#[derive(Message, Clone, Debug)]
pub struct RequestTracking;

/// Emitted when [`TrackingStatus`] changes.
#[derive(Message, Clone, Debug)]
pub struct TrackingStatusChanged(pub TrackingStatus);

pub struct AttPlugin;

impl Plugin for AttPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<TrackingStatus>()
            .add_message::<RequestTracking>()
            .add_message::<TrackingStatusChanged>()
            .add_systems(Update, (pump_requests, poll_status).chain());
    }
}

fn pump_requests(mut requests: MessageReader<RequestTracking>) {
    let mut wanted = false;
    for _ in requests.read() {
        wanted = true;
    }
    if wanted {
        unsafe { backend::att_request() };
    }
}

fn poll_status(
    mut status: ResMut<TrackingStatus>,
    mut changed: MessageWriter<TrackingStatusChanged>,
) {
    let current = TrackingStatus::from_i32(unsafe { backend::att_status() });
    if current != *status {
        *status = current;
        changed.write(TrackingStatusChanged(current));
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn request_resolves_status_and_emits_change() {
        // SAFETY: single-threaded test; the fake reads this env during this app.
        unsafe { std::env::set_var("BEVY_IOS_FAKE_ATT", "denied") };

        let mut app = App::new();
        app.add_plugins(MinimalPlugins).add_plugins(AttPlugin);

        app.update();
        assert_eq!(
            *app.world().resource::<TrackingStatus>(),
            TrackingStatus::NotDetermined
        );

        app.world_mut()
            .resource_mut::<Messages<RequestTracking>>()
            .write(RequestTracking);
        app.update(); // pump request
        app.update(); // poll resolves

        assert_eq!(
            *app.world().resource::<TrackingStatus>(),
            TrackingStatus::Denied
        );
        assert!(TrackingStatus::Authorized.is_authorized());
        assert!(!TrackingStatus::NotDetermined.is_determined());

        unsafe { std::env::remove_var("BEVY_IOS_FAKE_ATT") };
    }
}
