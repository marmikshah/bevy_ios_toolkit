//! iOS backend: the raw C-ABI surface implemented by `AdMobBridge.swift`.
//! All async, delegate-driven AdMob work happens Swift-side; Rust only issues
//! commands and drains cached events (no callbacks into Rust — winit
//! re-entrancy is unsafe).
//!
//! The string returned by `admob_drain_events` points at a Swift-owned buffer
//! valid only until the next call that regenerates it; the safe wrapper in
//! `super` copies immediately.

use std::ffi::c_char;

unsafe extern "C" {
    /// Start the Mobile Ads SDK. `test_devices` is a comma-separated list of
    /// test device ids; `use_test_ads` is a bool (0/1) the app may use to log
    /// or branch. Also kicks off a UMP consent-info refresh.
    pub fn admob_init(test_devices: *const c_char, use_test_ads: i32);
    /// Begin loading a full-screen ad of `format` from `unit_id` (async; result
    /// surfaces as a `loaded` / `load_failed` event).
    pub fn admob_load(format: i32, unit_id: *const c_char);
    /// Present the loaded full-screen ad of `format`.
    pub fn admob_show(format: i32);
    /// Create/attach the banner for `unit_id` at `position` (0 top, 1 bottom).
    pub fn admob_banner_show(unit_id: *const c_char, position: i32);
    /// Remove the banner from the view hierarchy.
    pub fn admob_banner_hide();
    /// Load and present the UMP consent form if one is required/available.
    pub fn admob_request_consent();
    /// 0 unknown, 1 required, 2 not-required, 3 obtained.
    pub fn admob_consent_status() -> i32;
    /// JSON array of pending events, draining the queue. Each element is
    /// `{format, kind, error?, reward_amount?, reward_type?}` where `kind` is
    /// one of `loaded | load_failed | shown | dismissed | show_failed | reward
    /// | clicked`.
    pub fn admob_drain_events() -> *const c_char;
}
