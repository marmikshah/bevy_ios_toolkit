//! Google AdMob ads as Bevy resources + messages.
//!
//! Flow:
//! 1. Insert [`AdmobConfig`] with your per-format ad unit ids (or
//!    [`AdmobConfig::test_ads`] to use Google's official sample units). The
//!    plugin calls into the backend once to start the Mobile Ads SDK.
//! 2. Send [`LoadAd`] to preload a full-screen format; read [`AdInventory`] —
//!    `is_loaded(format)` — to know when it's ready to present.
//! 3. Send [`ShowAd`] to present it, or [`ShowBanner`] / [`HideBanner`] for the
//!    banner.
//! 4. React to [`AdLoaded`] / [`AdLoadFailed`] / [`AdShown`] / [`AdDismissed`] /
//!    [`AdShowFailed`] / [`RewardEarned`] / [`AdClicked`].
//!
//! Consent: read [`AdmobState::consent`]; send [`RequestConsent`] to present the
//! UMP form when required. AdMob requires a valid consent state before serving
//! personalized ads in the EEA/UK.
//!
//! # Desktop fake (non-iOS)
//!
//! On `cargo run` off-device the bridge is a deterministic in-memory fake so the
//! full ad UX is exercisable with no SDK. It's env-tunable:
//! - `BEVY_ADMOB_FAKE_NO_FILL=interstitial,rewarded` — those formats fail to load.
//! - `BEVY_ADMOB_FAKE_SHOW_FAIL=interstitial` — those formats fail to present.
//! - `BEVY_ADMOB_FAKE_REWARD_AMOUNT=10` / `BEVY_ADMOB_FAKE_REWARD_TYPE=coins` —
//!   reward granted by rewarded formats (default `1` / `Reward`).
//! - `BEVY_ADMOB_FAKE_CONSENT=required` — consent starts `Required` instead of
//!   `Obtained`; a [`RequestConsent`] then resolves it to `Obtained`.

use std::collections::HashMap;
use std::ffi::CString;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ffi::read_cstr;

#[cfg(target_os = "ios")]
#[path = "backend_ios.rs"]
mod backend;

#[cfg(not(target_os = "ios"))]
#[path = "backend_fake.rs"]
mod backend;

// ---------- Test ad units ----------

/// Google's official iOS **test** app id (set as `GADApplicationIdentifier` in
/// `Info.plist` while developing). Real ad ids only serve in production.
pub const TEST_APP_ID: &str = "ca-app-pub-3940256099942544~1458002511";

/// Google's official iOS **test** ad unit id for `format`. Always safe to
/// request; returns fillable test creatives without risking policy strikes.
/// <https://developers.google.com/admob/ios/test-ads>
pub fn test_unit_id(format: AdFormat) -> &'static str {
    match format {
        AdFormat::Banner => "ca-app-pub-3940256099942544/2934735716",
        AdFormat::Interstitial => "ca-app-pub-3940256099942544/4411468910",
        AdFormat::Rewarded => "ca-app-pub-3940256099942544/1712485313",
        AdFormat::RewardedInterstitial => "ca-app-pub-3940256099942544/6978759866",
        AdFormat::AppOpen => "ca-app-pub-3940256099942544/5575463023",
    }
}

// ---------- Types ----------

/// An AdMob ad format. The discriminants are the stable wire values shared with
/// the Swift bridge; do not reorder.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Hash, Serialize, Deserialize)]
pub enum AdFormat {
    Banner,
    Interstitial,
    Rewarded,
    RewardedInterstitial,
    AppOpen,
}

impl AdFormat {
    /// Every format, for iteration (inventory init, diagnostics).
    pub const ALL: [AdFormat; 5] = [
        AdFormat::Banner,
        AdFormat::Interstitial,
        AdFormat::Rewarded,
        AdFormat::RewardedInterstitial,
        AdFormat::AppOpen,
    ];

    /// True for the full-screen formats whose loaded creative is consumed by a
    /// single presentation (everything except the persistent banner).
    pub fn is_full_screen(self) -> bool {
        !matches!(self, AdFormat::Banner)
    }

    pub fn as_i32(self) -> i32 {
        match self {
            AdFormat::Banner => 0,
            AdFormat::Interstitial => 1,
            AdFormat::Rewarded => 2,
            AdFormat::RewardedInterstitial => 3,
            AdFormat::AppOpen => 4,
        }
    }

    pub fn from_i32(v: i32) -> Option<AdFormat> {
        Some(match v {
            0 => AdFormat::Banner,
            1 => AdFormat::Interstitial,
            2 => AdFormat::Rewarded,
            3 => AdFormat::RewardedInterstitial,
            4 => AdFormat::AppOpen,
            _ => return None,
        })
    }

    /// Parse the lowercase token used by the env knobs (`interstitial`, etc.).
    /// Only the desktop fake consumes this.
    #[cfg_attr(target_os = "ios", allow(dead_code))]
    fn from_token(token: &str) -> Option<AdFormat> {
        Some(match token.trim().to_ascii_lowercase().as_str() {
            "banner" => AdFormat::Banner,
            "interstitial" => AdFormat::Interstitial,
            "rewarded" => AdFormat::Rewarded,
            "rewardedinterstitial" | "rewarded_interstitial" => AdFormat::RewardedInterstitial,
            "appopen" | "app_open" => AdFormat::AppOpen,
            _ => return None,
        })
    }
}

/// Per-format load state, owned by [`AdInventory`] and advanced by events.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum AdLoadState {
    /// Nothing loaded; the slot is empty or the loaded ad was consumed.
    #[default]
    Idle,
    /// A load is in flight.
    Loading,
    /// A creative is loaded and ready to present.
    Loaded,
    /// The last load failed (no fill / network / config). Send [`LoadAd`] again.
    Failed,
}

/// Where a banner is pinned on screen.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum BannerPosition {
    Top,
    #[default]
    Bottom,
}

impl BannerPosition {
    fn as_i32(self) -> i32 {
        match self {
            BannerPosition::Top => 0,
            BannerPosition::Bottom => 1,
        }
    }
}

/// UMP (User Messaging Platform) consent state for personalized ads.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ConsentStatus {
    /// Not yet determined (SDK still resolving, or pre-init).
    #[default]
    Unknown,
    /// Consent is required and not yet obtained — present the form via
    /// [`RequestConsent`] before requesting ads.
    Required,
    /// Consent is not required in the user's region.
    NotRequired,
    /// Consent has been gathered (or is not required and already resolved).
    Obtained,
}

impl ConsentStatus {
    fn from_i32(v: i32) -> ConsentStatus {
        match v {
            1 => ConsentStatus::Required,
            2 => ConsentStatus::NotRequired,
            3 => ConsentStatus::Obtained,
            _ => ConsentStatus::Unknown,
        }
    }

    /// Whether it's safe to request ads (anything but an outstanding requirement).
    pub fn can_request_ads(self) -> bool {
        !matches!(self, ConsentStatus::Required)
    }
}

// ---------- Resources ----------

/// Ad configuration. Insert before or after adding the plugin; the SDK starts
/// on the first frame this resource exists.
#[derive(Resource, Clone, Default)]
pub struct AdmobConfig {
    /// Ad unit id per format. A format with no entry here falls back to the
    /// Google test unit (so an unconfigured format is never a hard error in
    /// development). In production, set every format you use.
    pub unit_ids: HashMap<AdFormat, String>,
    /// Device ids to treat as test devices (so real ad units serve test
    /// creatives). The SDK logs the id of each device on first request.
    pub test_device_ids: Vec<String>,
    /// Force Google's official test unit ids for *every* format, ignoring
    /// `unit_ids`. Keep this on in dev; turn it off for release builds.
    pub use_test_ads: bool,
}

impl AdmobConfig {
    /// A config that serves Google's official test ads for every format — the
    /// zero-setup default for development.
    pub fn test_ads() -> Self {
        Self {
            use_test_ads: true,
            ..Default::default()
        }
    }

    /// Set the ad unit id for one format (builder-style).
    pub fn with_unit(mut self, format: AdFormat, unit_id: impl Into<String>) -> Self {
        self.unit_ids.insert(format, unit_id.into());
        self
    }

    /// The effective unit id to request for `format`: the configured id, or the
    /// Google test unit when `use_test_ads` is set or the format is unconfigured.
    pub fn resolve_unit(&self, format: AdFormat) -> String {
        if self.use_test_ads {
            return test_unit_id(format).to_string();
        }
        match self.unit_ids.get(&format) {
            Some(id) if !id.is_empty() => id.clone(),
            _ => test_unit_id(format).to_string(),
        }
    }
}

/// Per-format readiness. Read `is_loaded(format)` before sending [`ShowAd`].
#[derive(Resource, Default)]
pub struct AdInventory {
    states: HashMap<AdFormat, AdLoadState>,
}

impl AdInventory {
    pub fn state(&self, format: AdFormat) -> AdLoadState {
        self.states.get(&format).copied().unwrap_or_default()
    }

    pub fn is_loaded(&self, format: AdFormat) -> bool {
        self.state(format) == AdLoadState::Loaded
    }

    pub fn is_loading(&self, format: AdFormat) -> bool {
        self.state(format) == AdLoadState::Loading
    }

    fn set(&mut self, format: AdFormat, state: AdLoadState) {
        self.states.insert(format, state);
    }
}

/// Coarse SDK + consent + banner state, for UI that needs the big picture.
#[derive(Resource, Default)]
pub struct AdmobState {
    /// The Mobile Ads SDK has been started.
    pub initialized: bool,
    /// Current UMP consent state.
    pub consent: ConsentStatus,
    /// Whether a banner is currently on screen.
    pub banner_visible: bool,
}

// ---------- Messages (in) ----------

/// Preload a full-screen ad of `format`. No-op for [`AdFormat::Banner`] (use
/// [`ShowBanner`]).
#[derive(Message, Clone, Debug)]
pub struct LoadAd(pub AdFormat);

/// Present a previously-loaded full-screen ad. If it isn't loaded an
/// [`AdShowFailed`] is emitted.
#[derive(Message, Clone, Debug)]
pub struct ShowAd(pub AdFormat);

/// Show (or move) the banner at `position`. Loads and displays in one step.
#[derive(Message, Clone, Debug, Default)]
pub struct ShowBanner {
    pub position: BannerPosition,
}

/// Hide and release the banner.
#[derive(Message, Clone, Debug)]
pub struct HideBanner;

/// Present the UMP consent form if one is required/available.
#[derive(Message, Clone, Debug)]
pub struct RequestConsent;

// ---------- Messages (out) ----------

/// A load completed and a creative is ready to present.
#[derive(Message, Clone, Debug)]
pub struct AdLoaded(pub AdFormat);

/// A load failed (no fill, network, or configuration).
#[derive(Message, Clone, Debug)]
pub struct AdLoadFailed {
    pub format: AdFormat,
    pub error: String,
}

/// A full-screen ad began presenting (good moment to pause gameplay/audio).
#[derive(Message, Clone, Debug)]
pub struct AdShown(pub AdFormat);

/// A full-screen ad was dismissed and control returned to the app.
#[derive(Message, Clone, Debug)]
pub struct AdDismissed(pub AdFormat);

/// Presentation failed (e.g. nothing loaded, or the OS refused).
#[derive(Message, Clone, Debug)]
pub struct AdShowFailed {
    pub format: AdFormat,
    pub error: String,
}

/// The user earned a reward from a rewarded / rewarded-interstitial ad. Grant
/// it idempotently — this fires once per completed view.
#[derive(Message, Clone, Debug)]
pub struct RewardEarned {
    pub format: AdFormat,
    pub amount: i64,
    pub reward_type: String,
}

/// The user tapped the ad.
#[derive(Message, Clone, Debug)]
pub struct AdClicked(pub AdFormat);

/// The consent state changed; read [`AdmobState::consent`] for the new value.
#[derive(Message, Clone, Debug)]
pub struct ConsentUpdated(pub ConsentStatus);

// ---------- Wire event (bridge -> Rust) ----------

/// One event drained from the backend's queue. Mirrors the JSON the Swift shim
/// (and the fake) emit; field names are the wire contract.
#[derive(Serialize, Deserialize, Clone, Debug)]
pub(crate) struct AdEvent {
    pub format: i32,
    pub kind: String,
    #[serde(default)]
    pub error: String,
    #[serde(default)]
    pub reward_amount: i64,
    #[serde(default)]
    pub reward_type: String,
}

// ---------- Safe backend wrappers ----------

fn init(test_devices: &[String], use_test_ads: bool) {
    let Ok(joined) = CString::new(test_devices.join(",")) else {
        return;
    };
    unsafe { backend::admob_init(joined.as_ptr(), use_test_ads as i32) };
}

fn load(format: AdFormat, unit_id: &str) {
    let Ok(unit) = CString::new(unit_id) else {
        return;
    };
    unsafe { backend::admob_load(format.as_i32(), unit.as_ptr()) };
}

fn show(format: AdFormat) {
    unsafe { backend::admob_show(format.as_i32()) };
}

fn banner_show(unit_id: &str, position: BannerPosition) {
    let Ok(unit) = CString::new(unit_id) else {
        return;
    };
    unsafe { backend::admob_banner_show(unit.as_ptr(), position.as_i32()) };
}

fn banner_hide() {
    unsafe { backend::admob_banner_hide() };
}

fn request_consent() {
    unsafe { backend::admob_request_consent() };
}

fn consent_status() -> ConsentStatus {
    ConsentStatus::from_i32(unsafe { backend::admob_consent_status() })
}

fn drain_events() -> Vec<AdEvent> {
    let json = unsafe { read_cstr(backend::admob_drain_events()) };
    serde_json::from_str(&json).unwrap_or_default()
}

// ---------- Plugin ----------

#[derive(Resource, Default)]
struct AdsPoll {
    inited: bool,
    consent: ConsentStatus,
}

pub struct AdsPlugin;

impl Plugin for AdsPlugin {
    fn build(&self, app: &mut App) {
        app.init_resource::<AdInventory>()
            .init_resource::<AdmobState>()
            .init_resource::<AdsPoll>()
            .add_message::<LoadAd>()
            .add_message::<ShowAd>()
            .add_message::<ShowBanner>()
            .add_message::<HideBanner>()
            .add_message::<RequestConsent>()
            .add_message::<AdLoaded>()
            .add_message::<AdLoadFailed>()
            .add_message::<AdShown>()
            .add_message::<AdDismissed>()
            .add_message::<AdShowFailed>()
            .add_message::<RewardEarned>()
            .add_message::<AdClicked>()
            .add_message::<ConsentUpdated>()
            .add_systems(Update, (init_once, pump_requests, poll_backend).chain());
    }
}

/// Start the SDK the first frame a [`AdmobConfig`] exists. Insertion-order
/// tolerant — the config can land any time.
fn init_once(
    config: Option<Res<AdmobConfig>>,
    mut poll: ResMut<AdsPoll>,
    mut state: ResMut<AdmobState>,
) {
    if poll.inited {
        return;
    }
    if let Some(config) = config {
        init(&config.test_device_ids, config.use_test_ads);
        poll.inited = true;
        state.initialized = true;
    }
}

/// Forward consumer requests to the backend, resolving unit ids from config.
#[allow(clippy::too_many_arguments)]
fn pump_requests(
    poll: Res<AdsPoll>,
    config: Option<Res<AdmobConfig>>,
    mut inventory: ResMut<AdInventory>,
    mut state: ResMut<AdmobState>,
    mut loads: MessageReader<LoadAd>,
    mut shows: MessageReader<ShowAd>,
    mut banner_shows: MessageReader<ShowBanner>,
    mut banner_hides: MessageReader<HideBanner>,
    mut consents: MessageReader<RequestConsent>,
) {
    if !poll.inited {
        return;
    }
    let resolve = |format: AdFormat| {
        config
            .as_ref()
            .map(|c| c.resolve_unit(format))
            .unwrap_or_else(|| test_unit_id(format).to_string())
    };

    for LoadAd(format) in loads.read() {
        if format.is_full_screen() {
            inventory.set(*format, AdLoadState::Loading);
            load(*format, &resolve(*format));
        }
    }
    for ShowAd(format) in shows.read() {
        show(*format);
    }
    for ShowBanner { position } in banner_shows.read() {
        banner_show(&resolve(AdFormat::Banner), *position);
        state.banner_visible = true;
    }
    for _ in banner_hides.read() {
        banner_hide();
        state.banner_visible = false;
    }
    for _ in consents.read() {
        request_consent();
    }
}

/// Drain the backend's polled state into resources + messages.
#[allow(clippy::too_many_arguments)]
fn poll_backend(
    mut poll: ResMut<AdsPoll>,
    mut inventory: ResMut<AdInventory>,
    mut state: ResMut<AdmobState>,
    mut loaded: MessageWriter<AdLoaded>,
    mut load_failed: MessageWriter<AdLoadFailed>,
    mut shown: MessageWriter<AdShown>,
    mut dismissed: MessageWriter<AdDismissed>,
    mut show_failed: MessageWriter<AdShowFailed>,
    mut reward: MessageWriter<RewardEarned>,
    mut clicked: MessageWriter<AdClicked>,
    mut consent_updated: MessageWriter<ConsentUpdated>,
) {
    if !poll.inited {
        return;
    }

    let consent = consent_status();
    if consent != poll.consent {
        poll.consent = consent;
        state.consent = consent;
        consent_updated.write(ConsentUpdated(consent));
    }

    for ev in drain_events() {
        let Some(format) = AdFormat::from_i32(ev.format) else {
            continue;
        };
        match ev.kind.as_str() {
            "loaded" => {
                inventory.set(format, AdLoadState::Loaded);
                loaded.write(AdLoaded(format));
            }
            "load_failed" => {
                inventory.set(format, AdLoadState::Failed);
                load_failed.write(AdLoadFailed {
                    format,
                    error: ev.error,
                });
            }
            "shown" => {
                // A presented full-screen creative is consumed; require a fresh load.
                if format.is_full_screen() {
                    inventory.set(format, AdLoadState::Idle);
                }
                shown.write(AdShown(format));
            }
            "dismissed" => {
                dismissed.write(AdDismissed(format));
            }
            "show_failed" => {
                if format.is_full_screen() {
                    inventory.set(format, AdLoadState::Idle);
                }
                show_failed.write(AdShowFailed {
                    format,
                    error: ev.error,
                });
            }
            "reward" => {
                reward.write(RewardEarned {
                    format,
                    amount: ev.reward_amount,
                    reward_type: ev.reward_type,
                });
            }
            "clicked" => {
                clicked.write(AdClicked(format));
            }
            _ => {}
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// One-shot helper systems keyed off a `Local` latch.
    fn load_interstitial_once(mut loads: MessageWriter<LoadAd>, mut fired: Local<bool>) {
        if !*fired {
            *fired = true;
            loads.write(LoadAd(AdFormat::Interstitial));
        }
    }

    fn show_when_loaded(
        inventory: Res<AdInventory>,
        mut shows: MessageWriter<ShowAd>,
        mut fired: Local<bool>,
    ) {
        if !*fired && inventory.is_loaded(AdFormat::Interstitial) {
            *fired = true;
            shows.write(ShowAd(AdFormat::Interstitial));
        }
    }

    fn build_app() -> App {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(AdsPlugin);
        app
    }

    /// The fake backend is a process-global singleton, so tests that drive it
    /// must not run concurrently. Holding this guard serializes them and resets
    /// the fake (and clears env knobs) to a clean slate.
    static FAKE_GUARD: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn guarded() -> std::sync::MutexGuard<'static, ()> {
        let g = FAKE_GUARD.lock().unwrap_or_else(|p| p.into_inner());
        backend::reset();
        for key in [
            "BEVY_ADMOB_FAKE_NO_FILL",
            "BEVY_ADMOB_FAKE_SHOW_FAIL",
            "BEVY_ADMOB_FAKE_REWARD_AMOUNT",
            "BEVY_ADMOB_FAKE_REWARD_TYPE",
            "BEVY_ADMOB_FAKE_CONSENT",
        ] {
            unsafe { std::env::remove_var(key) };
        }
        g
    }

    #[test]
    fn format_i32_round_trips() {
        for f in AdFormat::ALL {
            assert_eq!(AdFormat::from_i32(f.as_i32()), Some(f));
        }
        assert_eq!(AdFormat::from_i32(99), None);
    }

    #[test]
    fn resolve_unit_prefers_config_then_falls_back_to_test() {
        let cfg = AdmobConfig::default().with_unit(AdFormat::Interstitial, "ca-app-pub-x/y");
        assert_eq!(cfg.resolve_unit(AdFormat::Interstitial), "ca-app-pub-x/y");
        // Unconfigured format falls back to the Google test unit.
        assert_eq!(
            cfg.resolve_unit(AdFormat::Rewarded),
            test_unit_id(AdFormat::Rewarded)
        );
        // use_test_ads overrides everything.
        let test_cfg = AdmobConfig::test_ads().with_unit(AdFormat::Interstitial, "ca-app-pub-x/y");
        assert_eq!(
            test_cfg.resolve_unit(AdFormat::Interstitial),
            test_unit_id(AdFormat::Interstitial)
        );
    }

    /// Accumulates terminal outcomes across frames so assertions don't race the
    /// double-buffered message swap (a real game would react frame-by-frame).
    #[derive(Resource, Default)]
    struct Recorder {
        loaded: u32,
        dismissed: u32,
        rewards: Vec<RewardEarned>,
    }

    fn record(
        mut rec: ResMut<Recorder>,
        mut loaded: MessageReader<AdLoaded>,
        mut dismissed: MessageReader<AdDismissed>,
        mut rewards: MessageReader<RewardEarned>,
    ) {
        rec.loaded += loaded.read().count() as u32;
        rec.dismissed += dismissed.read().count() as u32;
        for r in rewards.read() {
            rec.rewards.push(r.clone());
        }
    }

    fn show_rewarded_once(
        inv: Res<AdInventory>,
        mut shows: MessageWriter<ShowAd>,
        mut fired: Local<bool>,
    ) {
        if !*fired && inv.is_loaded(AdFormat::Rewarded) {
            *fired = true;
            shows.write(ShowAd(AdFormat::Rewarded));
        }
    }

    /// End-to-end against the fake: config → load → loaded → show → dismissed.
    #[test]
    fn interstitial_load_show_dismiss_flow() {
        let _guard = guarded();
        let mut app = build_app();
        app.init_resource::<Recorder>();
        app.insert_resource(AdmobConfig::test_ads());
        app.add_systems(Update, (load_interstitial_once, show_when_loaded, record));

        for _ in 0..10 {
            app.update();
        }

        let rec = app.world().resource::<Recorder>();
        assert_eq!(
            rec.loaded, 1,
            "interstitial should have reported loaded once"
        );
        assert_eq!(rec.dismissed, 1, "interstitial should have dismissed once");
        // Consumed after showing.
        assert_eq!(
            app.world()
                .resource::<AdInventory>()
                .state(AdFormat::Interstitial),
            AdLoadState::Idle
        );
    }

    /// A rewarded ad grants a reward when shown.
    #[test]
    fn rewarded_grants_reward() {
        let _guard = guarded();
        // SAFETY: `guarded()` serializes fake-driving tests, so this env state is
        // not observed by any concurrent test.
        unsafe {
            std::env::set_var("BEVY_ADMOB_FAKE_REWARD_AMOUNT", "7");
            std::env::set_var("BEVY_ADMOB_FAKE_REWARD_TYPE", "gems");
        }

        let mut app = build_app();
        app.init_resource::<Recorder>();
        app.insert_resource(AdmobConfig::test_ads());
        app.add_systems(
            Update,
            (
                |mut loads: MessageWriter<LoadAd>, mut fired: Local<bool>| {
                    if !*fired {
                        *fired = true;
                        loads.write(LoadAd(AdFormat::Rewarded));
                    }
                },
                show_rewarded_once,
                record,
            ),
        );

        for _ in 0..10 {
            app.update();
        }

        let rec = app.world().resource::<Recorder>();
        assert_eq!(
            rec.rewards.len(),
            1,
            "rewarded ad should grant exactly one reward"
        );
        assert_eq!(rec.rewards[0].amount, 7);
        assert_eq!(rec.rewards[0].reward_type, "gems");
        assert_eq!(rec.dismissed, 1, "rewarded ad should also dismiss");
    }

    /// A no-fill load surfaces as `Failed`, not `Loaded`.
    #[test]
    fn no_fill_reports_failure() {
        let _guard = guarded();
        unsafe { std::env::set_var("BEVY_ADMOB_FAKE_NO_FILL", "appopen") }

        let mut app = build_app();
        app.insert_resource(AdmobConfig::test_ads());
        app.add_systems(
            Update,
            |mut loads: MessageWriter<LoadAd>, mut fired: Local<bool>| {
                if !*fired {
                    *fired = true;
                    loads.write(LoadAd(AdFormat::AppOpen));
                }
            },
        );

        for _ in 0..6 {
            app.update();
        }

        assert_eq!(
            app.world()
                .resource::<AdInventory>()
                .state(AdFormat::AppOpen),
            AdLoadState::Failed
        );
    }

    /// Consent starts `Required`, then `RequestConsent` resolves it to
    /// `Obtained` and emits a `ConsentUpdated`.
    #[test]
    fn consent_required_then_obtained() {
        let _guard = guarded();
        unsafe { std::env::set_var("BEVY_ADMOB_FAKE_CONSENT", "required") }

        let mut app = build_app();
        app.insert_resource(AdmobConfig::test_ads());

        app.update(); // init + first consent poll
        assert_eq!(
            app.world().resource::<AdmobState>().consent,
            ConsentStatus::Required
        );

        app.world_mut()
            .resource_mut::<Messages<RequestConsent>>()
            .write(RequestConsent);
        app.update(); // pump request
        app.update(); // poll the new status

        assert_eq!(
            app.world().resource::<AdmobState>().consent,
            ConsentStatus::Obtained
        );
        assert!(ConsentStatus::Obtained.can_request_ads());
        assert!(!ConsentStatus::Required.can_request_ads());
    }

    #[test]
    fn banner_show_hide_toggles_state() {
        let _guard = guarded();
        let mut app = build_app();
        app.insert_resource(AdmobConfig::test_ads());

        app.update(); // init
        app.world_mut()
            .resource_mut::<Messages<ShowBanner>>()
            .write(ShowBanner::default());
        app.update();
        assert!(app.world().resource::<AdmobState>().banner_visible);

        app.world_mut()
            .resource_mut::<Messages<HideBanner>>()
            .write(HideBanner);
        app.update();
        assert!(!app.world().resource::<AdmobState>().banner_visible);
    }
}
