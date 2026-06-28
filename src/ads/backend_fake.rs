//! Non-iOS backend: a deterministic in-memory fake so ad flows are fully
//! exercisable on desktop/wasm `cargo run` without the Mobile Ads SDK or a
//! device. Same raw signatures as [`super::backend_ios`], so the safe wrappers
//! in `super` are identical across platforms.
//!
//! Loads resolve synchronously (queued as events drained next frame), shows
//! emit `shown` then `dismissed`, and rewarded formats emit a `reward` before
//! dismissing. Behaviour is env-tunable — see the `ads` module docs.

use std::collections::{HashSet, VecDeque};
use std::ffi::{CString, c_char};
use std::sync::{LazyLock, Mutex};

use super::{AdEvent, AdFormat};

#[derive(Default)]
struct Fake {
    use_test_ads: bool,
    consent: i32,
    loaded: HashSet<i32>,
    events: VecDeque<AdEvent>,
}

impl Fake {
    /// Lowercase format tokens from an env var (`a,b,c`).
    fn env_formats(key: &str) -> HashSet<i32> {
        std::env::var(key)
            .unwrap_or_default()
            .split(',')
            .filter_map(AdFormat::from_token)
            .map(AdFormat::as_i32)
            .collect()
    }

    fn push(&mut self, ev: AdEvent) {
        self.events.push_back(ev);
    }
}

static FAKE: LazyLock<Mutex<Fake>> = LazyLock::new(|| Mutex::new(Fake::default()));

fn lock() -> std::sync::MutexGuard<'static, Fake> {
    FAKE.lock().unwrap_or_else(|p| p.into_inner())
}

fn event(format: i32, kind: &str) -> AdEvent {
    AdEvent {
        format,
        kind: kind.to_string(),
        error: String::new(),
        reward_amount: 0,
        reward_type: String::new(),
    }
}

pub unsafe fn admob_init(_test_devices: *const c_char, use_test_ads: i32) {
    let mut f = lock();
    f.use_test_ads = use_test_ads != 0;
    f.consent = if std::env::var("BEVY_ADMOB_FAKE_CONSENT")
        .map(|v| v.eq_ignore_ascii_case("required"))
        .unwrap_or(false)
    {
        1 // required
    } else {
        3 // obtained
    };
}

pub unsafe fn admob_load(format: i32, _unit_id: *const c_char) {
    let no_fill = Fake::env_formats("BEVY_ADMOB_FAKE_NO_FILL");
    let mut f = lock();
    if no_fill.contains(&format) {
        f.loaded.remove(&format);
        let mut ev = event(format, "load_failed");
        ev.error = "fake no fill".into();
        f.push(ev);
    } else {
        f.loaded.insert(format);
        f.push(event(format, "loaded"));
    }
}

pub unsafe fn admob_show(format: i32) {
    let show_fail = Fake::env_formats("BEVY_ADMOB_FAKE_SHOW_FAIL");
    let reward_amount: i64 = std::env::var("BEVY_ADMOB_FAKE_REWARD_AMOUNT")
        .ok()
        .and_then(|v| v.parse().ok())
        .unwrap_or(1);
    let reward_type =
        std::env::var("BEVY_ADMOB_FAKE_REWARD_TYPE").unwrap_or_else(|_| "Reward".into());

    let mut f = lock();
    if show_fail.contains(&format) || !f.loaded.contains(&format) {
        f.loaded.remove(&format);
        let mut ev = event(format, "show_failed");
        ev.error = if show_fail.contains(&format) {
            "fake show failure".into()
        } else {
            "ad not loaded".into()
        };
        f.push(ev);
        return;
    }

    f.loaded.remove(&format);
    f.push(event(format, "shown"));
    // Rewarded formats grant the reward mid-presentation, before dismissal.
    if matches!(
        AdFormat::from_i32(format),
        Some(AdFormat::Rewarded | AdFormat::RewardedInterstitial)
    ) {
        let mut ev = event(format, "reward");
        ev.reward_amount = reward_amount;
        ev.reward_type = reward_type;
        f.push(ev);
    }
    f.push(event(format, "dismissed"));
}

pub unsafe fn admob_banner_show(_unit_id: *const c_char, _position: i32) {
    lock().push(event(AdFormat::Banner.as_i32(), "shown"));
}

pub unsafe fn admob_banner_hide() {
    lock().push(event(AdFormat::Banner.as_i32(), "dismissed"));
}

pub unsafe fn admob_request_consent() {
    let mut f = lock();
    // Presenting the form resolves any outstanding requirement.
    f.consent = 3;
}

pub unsafe fn admob_consent_status() -> i32 {
    lock().consent
}

pub unsafe fn admob_drain_events() -> *const c_char {
    let events: Vec<AdEvent> = {
        let mut f = lock();
        f.events.drain(..).collect()
    };
    let json = serde_json::to_string(&events).unwrap_or_else(|_| "[]".into());
    // Park the buffer in a thread-local so the pointer outlives the lock guard
    // (the poll system reads it on a single thread, copying immediately).
    EVENTS_BUF.with(|buf| {
        *buf.borrow_mut() = CString::new(json).unwrap_or_default();
        buf.borrow().as_ptr()
    })
}

thread_local! {
    static EVENTS_BUF: std::cell::RefCell<CString> = std::cell::RefCell::new(CString::default());
}

/// Clear the process-global fake state. The fake mirrors the real SDK's
/// singleton, so tests that drive it must serialize and reset between runs.
#[cfg(test)]
pub fn reset() {
    *lock() = Fake::default();
}
