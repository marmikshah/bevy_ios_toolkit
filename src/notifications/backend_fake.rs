//! Non-iOS backend: a deterministic in-memory fake, so scheduling *policy* is
//! exercisable on desktop without a device.
//!
//! Nothing is ever delivered here. A fake that popped a real desktop
//! notification would be inventing a platform capability to make native state
//! runnable, which this repository's rules refuse. What it does model is
//! everything a consumer's own code has to get right: whether permission was
//! granted, whether a schedule was accepted, that an id replaces rather than
//! stacks, and what happens when the app is opened from a notification.
//!
//! Tunable through the environment — see the module docs.

use std::collections::VecDeque;
use std::ffi::{CStr, CString, c_char};
use std::sync::{LazyLock, Mutex};

use super::Event;

#[derive(Default)]
struct Fake {
    status: i32,
    scheduled: Vec<String>,
    events: VecDeque<Event>,
    drained_env_opens: bool,
    events_json: Option<CString>,
}

static FAKE: LazyLock<Mutex<Fake>> = LazyLock::new(|| Mutex::new(Fake::default()));

fn lock() -> std::sync::MutexGuard<'static, Fake> {
    FAKE.lock().unwrap_or_else(|poisoned| poisoned.into_inner())
}

/// What a request resolves to. Authorized unless told otherwise, because the
/// interesting desktop flow is the one where notifications work.
fn env_status() -> i32 {
    match std::env::var("BEVY_IOS_FAKE_NOTIFICATIONS")
        .unwrap_or_default()
        .to_ascii_lowercase()
        .as_str()
    {
        "denied" => 1,
        "provisional" => 3,
        "notdetermined" => 0,
        _ => 2,
    }
}

fn env_flag(key: &str) -> bool {
    matches!(
        std::env::var(key)
            .unwrap_or_default()
            .to_ascii_lowercase()
            .as_str(),
        "1" | "true" | "yes"
    )
}

/// Ids the app should behave as though it was opened from, delivered once.
fn env_opens() -> Vec<String> {
    std::env::var("BEVY_IOS_FAKE_NOTIFICATION_OPENED")
        .unwrap_or_default()
        .split(',')
        .map(str::trim)
        .filter(|id| !id.is_empty())
        .map(str::to_string)
        .collect()
}

unsafe fn text(ptr: *const c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned()
}

pub unsafe fn notifications_install_delegate() {}

pub unsafe fn notifications_request() {
    lock().status = env_status();
}

pub unsafe fn notifications_status() -> i32 {
    lock().status
}

pub unsafe fn notifications_schedule(
    id: *const c_char,
    _title: *const c_char,
    _body: *const c_char,
    after_secs: f64,
) -> i32 {
    let id = unsafe { text(id) };
    let mut fake = lock();
    // The same refusals the real trigger makes, so a consumer meets them here
    // rather than for the first time on a device.
    // `is_finite` first so a NaN interval is refused rather than compared.
    if id.is_empty() || !after_secs.is_finite() || after_secs <= 0.0 {
        return 1;
    }
    if fake.status != 2 && fake.status != 3 {
        return 2;
    }
    if env_flag("BEVY_IOS_FAKE_NOTIFICATIONS_REFUSE") {
        fake.events.push_back(Event::ScheduleFailed {
            id,
            reason: "the fake was told to refuse".into(),
        });
        return 0;
    }
    // An id replaces rather than stacks — the whole reason ids are caller-owned.
    fake.scheduled.retain(|pending| *pending != id);
    fake.scheduled.push(id);
    0
}

pub unsafe fn notifications_cancel(id: *const c_char) {
    let id = unsafe { text(id) };
    lock().scheduled.retain(|pending| *pending != id);
}

pub unsafe fn notifications_cancel_all() {
    lock().scheduled.clear();
}

pub unsafe fn notifications_drain_events() -> *const c_char {
    let mut fake = lock();
    if !fake.drained_env_opens {
        fake.drained_env_opens = true;
        for id in env_opens() {
            fake.events.push_back(Event::Opened { id });
        }
    }
    let events: Vec<Event> = fake.events.drain(..).collect();
    let json = serde_json::to_string(&events).unwrap_or_else(|_| "[]".into());
    let owned = CString::new(json).unwrap_or_default();
    let ptr = owned.as_ptr();
    // Held until the next drain, matching the Swift buffer's lifetime exactly
    // so the safe wrapper above is identical on both platforms.
    fake.events_json = Some(owned);
    ptr
}

/// What the fake currently believes is pending. Test-only: the real backend
/// has no synchronous equivalent, and inventing one would be a lie.
#[cfg(test)]
pub fn scheduled() -> Vec<String> {
    lock().scheduled.clone()
}

/// Deliver an open for `id` on the next drain, as though the player had
/// tapped it. Test-only: the environment knob fires once at startup, which
/// cannot exercise an open arriving *after* something was scheduled.
#[cfg(test)]
pub fn open(id: &str) {
    lock().events.push_back(Event::Opened { id: id.into() });
}

/// Put the fake back to how it starts. Test-only.
#[cfg(test)]
pub fn reset() {
    let mut fake = lock();
    *fake = Fake::default();
}
