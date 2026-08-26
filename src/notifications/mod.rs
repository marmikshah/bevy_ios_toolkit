//! Local user notifications as Bevy resources + messages.
//!
//! Ask for permission with [`RequestNotificationPermission`], read
//! [`NotificationPermission`] to branch, schedule with
//! [`ScheduleNotification`], and find out the app was opened from one by
//! reading [`NotificationOpened`].
//!
//! **Local only.** There is no remote push here: no APNs, no device token, no
//! server, and nothing that counts as data collection on a privacy manifest.
//! Everything is scheduled by the app, on the device, for a time the app
//! already knows about.
//!
//! ```no_run
//! use std::time::Duration;
//!
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::prelude::*;
//!
//! fn ask_once(mut req: MessageWriter<RequestNotificationPermission>, mut done: Local<bool>) {
//!     if !*done {
//!         *done = true;
//!         req.write(RequestNotificationPermission);
//!     }
//! }
//!
//! fn remind_when_the_van_arrives(
//!     permission: Res<NotificationPermission>,
//!     mut schedule: MessageWriter<ScheduleNotification>,
//! ) {
//!     if permission.can_deliver() {
//!         schedule.write(ScheduleNotification {
//!             id: "restock".into(),
//!             title: "THE VAN CAME".into(),
//!             body: "Something new is in the machine.".into(),
//!             after: Duration::from_secs(4 * 60 * 60),
//!         });
//!     }
//! }
//!
//! fn came_from_a_notification(mut opened: MessageReader<NotificationOpened>) {
//!     for NotificationOpened(id) in opened.read() {
//!         let _ = id; // take the player where the notification promised
//!     }
//! }
//! ```
//!
//! # Permission settles a frame or two after launch
//!
//! Asking iOS for the current authorization is asynchronous, so
//! [`NotificationPermission`] reads `NotDetermined` for the first frames of a
//! launch even when the answer has been on file for months. Scheduling is
//! refused while it says so — deliberately, because a notification the system
//! will never deliver is worse than a refusal that says why. Gate on
//! [`NotificationPermission::can_deliver`], as the example above does, and the
//! question never comes up.
//!
//! # Ids are yours, and they replace
//!
//! Scheduling an id that is already pending **replaces** it. That is the point
//! of caller-owned ids: a timer that slips should never leave two
//! notifications queued. It also means rescheduling is idempotent, so a system
//! that runs every frame does no harm.
//!
//! # Catching a cold launch
//!
//! Opens are captured by a `UNUserNotificationCenterDelegate` this module
//! installs on its first update. iOS delivers the response for a notification
//! that *launched* the app very early — possibly before Bevy's first frame —
//! and a response delivered with no delegate set is gone.
//!
//! If catching that matters, call the shim's installer from your own app
//! delegate, before returning from `didFinishLaunchingWithOptions`:
//!
//! ```swift
//! @_silgen_name("notifications_install_delegate")
//! func notificationsInstallDelegate()
//!
//! func application(_: UIApplication, didFinishLaunchingWithOptions _: ...) -> Bool {
//!     notificationsInstallDelegate()
//!     return true
//! }
//! ```
//!
//! It is idempotent, and anything buffered before Bevy starts is drained on the
//! first update.
//!
//! # This is for a real event
//!
//! The same warning [`crate::review::request`] carries. A notification API
//! makes the daily-nag pattern very easy to build. Send one because something
//! the player asked to know about actually happened — never to drag them back.
//! Restraint is the consumer's job, which is exactly what the desktop fake
//! exists to let you test.
//!
//! # Desktop fake (non-iOS)
//!
//! Nothing is ever delivered off iOS; what is modelled is the policy around it.
//! Starts `NotDetermined`; a request resolves it.
//!
//! | variable | effect |
//! |---|---|
//! | `BEVY_IOS_FAKE_NOTIFICATIONS` | `authorized` (default) / `denied` / `provisional` / `notdetermined` — what a request resolves to |
//! | `BEVY_IOS_FAKE_NOTIFICATION_OPENED` | comma-separated ids delivered once as [`NotificationOpened`], as though the app were launched from them |
//! | `BEVY_IOS_FAKE_NOTIFICATIONS_REFUSE` | `1` to make every schedule fail asynchronously, so the failure path is reachable |

use std::ffi::CString;
use std::time::Duration;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

#[cfg(target_os = "ios")]
mod backend_ios;
#[cfg(target_os = "ios")]
use backend_ios as backend;

#[cfg(not(target_os = "ios"))]
mod backend_fake;
#[cfg(not(target_os = "ios"))]
use backend_fake as backend;

/// One thing the native side has to tell us about, drained once per frame.
#[derive(Serialize, Deserialize, Clone, Debug, PartialEq, Eq)]
#[serde(tag = "kind", rename_all = "snake_case")]
enum Event {
    /// The app was opened by tapping this notification.
    Opened { id: String },
    /// The system accepted the request and then refused it.
    ScheduleFailed { id: String, reason: String },
}

/// Whether the app may show notifications.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum NotificationPermission {
    /// Nobody has been asked yet.
    #[default]
    NotDetermined,
    /// Asked and refused. Only Settings can change this.
    Denied,
    /// Granted in full.
    Authorized,
    /// Granted quietly — delivered to the notification centre without a
    /// banner or a sound. Still worth scheduling for.
    Provisional,
}

impl NotificationPermission {
    fn from_i32(value: i32) -> Self {
        match value {
            1 => Self::Denied,
            2 => Self::Authorized,
            3 => Self::Provisional,
            _ => Self::NotDetermined,
        }
    }

    /// Whether a scheduled notification would actually arrive. True for
    /// provisional too: quiet delivery is still delivery.
    pub fn can_deliver(self) -> bool {
        matches!(self, Self::Authorized | Self::Provisional)
    }

    /// Whether the prompt can still be shown. Asking again once the answer is
    /// in shows nothing — send the player to Settings instead.
    pub fn is_determined(self) -> bool {
        !matches!(self, Self::NotDetermined)
    }
}

/// What this app has asked for since it launched and has not cancelled.
///
/// A record of *requests*, not the system's queue: one that has already been
/// delivered, one the player cleared, and anything scheduled before this
/// launch are all absent. It is here so a consumer can answer "have I already
/// asked for tonight's one?" without a round trip, and so scheduling policy is
/// testable off a device. When the two could disagree, believe the system.
#[derive(Resource, Clone, Debug, Default)]
pub struct PendingNotifications(Vec<String>);

impl PendingNotifications {
    pub fn contains(&self, id: &str) -> bool {
        self.0.iter().any(|pending| pending == id)
    }

    pub fn ids(&self) -> impl Iterator<Item = &str> {
        self.0.iter().map(String::as_str)
    }

    pub fn len(&self) -> usize {
        self.0.len()
    }

    pub fn is_empty(&self) -> bool {
        self.0.is_empty()
    }
}

/// Show the system permission prompt. A no-op once the answer is in.
#[derive(Message, Clone, Debug)]
pub struct RequestNotificationPermission;

/// Schedule a notification `after` from now, replacing any pending one with
/// the same `id`.
#[derive(Message, Clone, Debug, PartialEq, Eq)]
pub struct ScheduleNotification {
    /// Yours, and stable: the same id replaces rather than stacks.
    pub id: String,
    pub title: String,
    pub body: String,
    /// How long from now. Must be greater than zero — the system has no way
    /// to schedule something for a moment that has already passed.
    pub after: Duration,
}

/// Drop a pending notification.
#[derive(Message, Clone, Debug)]
pub struct CancelNotification(pub String);

/// Drop every pending notification this app scheduled.
#[derive(Message, Clone, Debug)]
pub struct CancelAllNotifications;

/// Emitted when [`NotificationPermission`] changes.
#[derive(Message, Clone, Debug)]
pub struct NotificationPermissionChanged(pub NotificationPermission);

/// The app was opened by tapping the notification with this id.
#[derive(Message, Clone, Debug)]
pub struct NotificationOpened(pub String);

/// A notification could not be scheduled.
#[derive(Message, Clone, Debug)]
pub struct NotificationScheduleFailed {
    pub id: String,
    pub reason: String,
}

pub struct NotificationsPlugin;

impl Plugin for NotificationsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<NotificationPermission>()
            .init_resource::<PendingNotifications>()
            .add_message::<RequestNotificationPermission>()
            .add_message::<ScheduleNotification>()
            .add_message::<CancelNotification>()
            .add_message::<CancelAllNotifications>()
            .add_message::<NotificationPermissionChanged>()
            .add_message::<NotificationOpened>()
            .add_message::<NotificationScheduleFailed>()
            .add_systems(Startup, install_delegate)
            .add_systems(
                Update,
                (pump_requests, pump_schedules, pump_cancels, poll).chain(),
            );
    }
}

fn install_delegate() {
    unsafe { backend::notifications_install_delegate() };
}

fn pump_requests(mut requests: MessageReader<RequestNotificationPermission>) {
    if requests.read().count() > 0 {
        unsafe { backend::notifications_request() };
    }
}

fn pump_schedules(
    mut requests: MessageReader<ScheduleNotification>,
    mut pending: ResMut<PendingNotifications>,
    mut failed: MessageWriter<NotificationScheduleFailed>,
) {
    for request in requests.read() {
        let after = request.after.as_secs_f64();
        let (Ok(id), Ok(title), Ok(body)) = (
            CString::new(request.id.as_str()),
            CString::new(request.title.as_str()),
            CString::new(request.body.as_str()),
        ) else {
            // An interior NUL cannot cross a C boundary. Say so rather than
            // truncating the text into something the player would read.
            failed.write(NotificationScheduleFailed {
                id: request.id.clone(),
                reason: "text contains a NUL byte".into(),
            });
            continue;
        };
        let code = unsafe {
            backend::notifications_schedule(id.as_ptr(), title.as_ptr(), body.as_ptr(), after)
        };
        if code == 0 {
            if !pending.contains(&request.id) {
                pending.0.push(request.id.clone());
            }
        } else {
            failed.write(NotificationScheduleFailed {
                id: request.id.clone(),
                reason: refusal(code).into(),
            });
        }
    }
}

/// What a non-zero schedule result meant. Kept here rather than in the shim so
/// both backends refuse for the same stated reasons.
fn refusal(code: i32) -> &'static str {
    match code {
        1 => "an id and an interval greater than zero are required",
        2 => "notifications are not authorized",
        _ => "the system refused the request",
    }
}

fn pump_cancels(
    mut one: MessageReader<CancelNotification>,
    mut all: MessageReader<CancelAllNotifications>,
    mut pending: ResMut<PendingNotifications>,
) {
    for CancelNotification(id) in one.read() {
        if let Ok(raw) = CString::new(id.as_str()) {
            unsafe { backend::notifications_cancel(raw.as_ptr()) };
        }
        pending.0.retain(|kept| kept != id);
    }
    if all.read().count() > 0 {
        unsafe { backend::notifications_cancel_all() };
        pending.0.clear();
    }
}

fn poll(
    mut permission: ResMut<NotificationPermission>,
    mut pending: ResMut<PendingNotifications>,
    mut changed: MessageWriter<NotificationPermissionChanged>,
    mut opened: MessageWriter<NotificationOpened>,
    mut failed: MessageWriter<NotificationScheduleFailed>,
) {
    let current = NotificationPermission::from_i32(unsafe { backend::notifications_status() });
    if current != *permission {
        *permission = current;
        changed.write(NotificationPermissionChanged(current));
    }
    for event in drain_events() {
        match event {
            Event::Opened { id } => {
                // It has been delivered, so it is no longer waiting.
                pending.0.retain(|kept| *kept != id);
                opened.write(NotificationOpened(id));
            }
            Event::ScheduleFailed { id, reason } => {
                pending.0.retain(|kept| *kept != id);
                failed.write(NotificationScheduleFailed { id, reason });
            }
        }
    }
}

fn drain_events() -> Vec<Event> {
    let json = unsafe { crate::ffi::read_cstr(backend::notifications_drain_events()) };
    serde_json::from_str(&json).unwrap_or_default()
}

#[cfg(all(test, not(target_os = "ios")))]
mod tests;
