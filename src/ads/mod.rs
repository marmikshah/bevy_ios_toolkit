//! Google AdMob ads as Bevy resources + messages.
//!
//! Flow:
//! 1. Insert [`AdmobConfig`]: [`AdmobConfig::for_build`] with your per-format
//!    ad unit ids serves test creatives in development and your units in
//!    release. The plugin automatically requests ATT and waits for a resolved
//!    decision. The native bridge verifies UMP and presents any required form
//!    at startup, respecting saved approval and rejection. Mobile Ads starts
//!    only when UMP permits ads. `ads` includes the Rust `att` feature and
//!    Swift `Att` product.
//! 2. Wait until [`AdmobState::can_request_ads`] is true; gameplay never waits.
//! 3. Send [`LoadAd`] to preload a full-screen format; read [`AdInventory`] —
//!    `is_loaded(format)` — to know when it's ready to present.
//! 4. Send [`TryShowAd`] at a natural break (or [`ShowAd`] to require it).
//!    For banners, add [`BannerPlugin`] and set [`BannerIntent`].
//! 5. React to [`AdLoaded`] / [`AdLoadFailed`] / [`AdShown`] / [`AdDismissed`] /
//!    [`AdShowFailed`] / [`RewardEarned`] / [`AdClicked`].
//!
//! Consent verification, required forms and bounded retries belong to the
//! toolkit. Games need no launch/death consent trigger. [`RequestConsent`]
//! remains available for an explicit retry. If the [`PrivacyOptionsRequirement`]
//! resource is [`PrivacyOptionsRequirement::Required`], keep a visible action
//! that sends [`PresentPrivacyOptions`] so the user can revisit their choices.
//! Early consent requests survive the ATT wait. Early ad loads/presentations
//! emit failures instead of reaching the SDK; retry when readiness is true.
//! For custom ATT timing, set [`AdmobConfig::tracking_prompt`] to
//! [`AdTrackingPrompt::Manual`] and send [`RequestTracking`] yourself. Omitting
//! the ad config entirely leaves both automatic ATT and ad services idle.
//!
//! # Desktop fake (non-iOS)
//!
//! On `cargo run` off-device the bridge is a deterministic in-memory fake so the
//! full ad UX is exercisable with no SDK. It's env-tunable:
//! - `BEVY_ADMOB_FAKE_NO_FILL=interstitial,rewarded` — those formats fail to load.
//! - `BEVY_ADMOB_FAKE_SHOW_FAIL=interstitial` — those formats fail to present.
//! - `BEVY_ADMOB_FAKE_REWARD_AMOUNT=10` / `BEVY_ADMOB_FAKE_REWARD_TYPE=coins` —
//!   reward granted by rewarded formats (default `1` / `Reward`).
//! - `BEVY_ADMOB_FAKE_CONSENT=required` — startup collects a required decision;
//!   `approved` and `rejected` both represent an already gathered decision.
//! - `BEVY_ADMOB_FAKE_CONSENT_PENDING=1` — leave that startup form pending.
//! - `BEVY_ADMOB_FAKE_CAN_REQUEST_ADS=true` — override authoritative UMP ad
//!   readiness independently of its coarse consent status.
//! - `BEVY_ADMOB_FAKE_CONSENT_UPDATE=failed|fail_once` — surface a persistent
//!   or one-shot [`ConsentInfoUpdateFailed`].
//! - `BEVY_ADMOB_FAKE_PRIVACY_OPTIONS=required` — the privacy-options entry
//!   point is required for the session.
//!
//! # UMP QA launches
//!
//! [`UmpQa::from_env`] reads a game's `<PREFIX>UMP_QA` (`eea`, `us`, `other`)
//! and `<PREFIX>UMP_TEST_DEVICE_ID` in development builds, producing the
//! [`UmpTestConfig`] and test device ids for a regional consent test.

use std::collections::HashMap;
use std::ffi::CString;

use crate::att::{AttPlugin, AttSystems, RequestTracking, TrackingStatus};
use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ffi::read_cstr;

mod banner;
pub use banner::{BannerIntent, BannerPlugin};
mod ump_qa;
pub use ump_qa::UmpQa;

#[cfg(target_os = "ios")]
#[path = "backend_ios.rs"]
mod backend;

#[cfg(not(target_os = "ios"))]
#[path = "backend_fake.rs"]
mod backend;

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

/// UMP (User Messaging Platform) consent collection state.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ConsentStatus {
    /// Not yet determined (SDK still resolving, or pre-init).
    #[default]
    Unknown,
    /// A decision is required; the toolkit presents the form automatically.
    Required,
    /// Consent is not required in the user's region.
    NotRequired,
    /// A decision has been gathered, including rejection. Ad readiness still
    /// comes exclusively from [`AdmobState::can_request_ads`].
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

    /// A coarse fallback retained for source compatibility.
    ///
    /// UMP's authoritative readiness is [`AdmobState::can_request_ads`]. The
    /// native SDK can permit ads from cached consent even when this status is
    /// `Unknown`, including after an update error.
    #[deprecated(since = "0.3.3", note = "use AdmobState::can_request_ads")]
    pub fn can_request_ads(self) -> bool {
        matches!(self, Self::NotRequired | Self::Obtained)
    }
}

/// Whether UMP requires a visible privacy-options entry point.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum PrivacyOptionsRequirement {
    /// The consent-info refresh has not completed.
    #[default]
    Unknown,
    /// Keep a visible action that sends [`PresentPrivacyOptions`].
    Required,
    /// No privacy-options entry point is required for this user.
    NotRequired,
}

impl PrivacyOptionsRequirement {
    /// Whether to show the privacy-options action now: only
    /// [`Required`](Self::Required). `Unknown` reads as not required until the
    /// consent-info refresh completes.
    pub const fn is_required(self) -> bool {
        matches!(self, Self::Required)
    }

    fn from_i32(v: i32) -> Self {
        match v {
            1 => Self::Required,
            2 => Self::NotRequired,
            _ => Self::Unknown,
        }
    }
}

/// Test-only geography override for UMP consent flows.
///
/// The backend ignores every value unless [`AdmobConfig::use_test_ads`] is true.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum UmpDebugGeography {
    /// Simulate a user in the European Economic Area.
    Eea,
    /// Simulate a user in a regulated US state.
    RegulatedUsState,
    /// Simulate a geography where no regulation is in force.
    Other,
}

impl UmpDebugGeography {
    fn as_i32(self) -> i32 {
        match self {
            Self::Eea => 1,
            Self::RegulatedUsState => 3,
            Self::Other => 4,
        }
    }
}

/// Who requests ATT when ads are configured. Both modes keep ad startup
/// blocked until ATT is authorized, denied or restricted.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub enum AdTrackingPrompt {
    /// Request automatically once a foreground window can present the sheet.
    #[default]
    Automatic,
    /// The game sends [`RequestTracking`] at its chosen time. For captures,
    /// leave it unsent; an undetermined status keeps ads closed.
    Manual,
}

/// Ad configuration. Insert before or after adding the plugin. By default,
/// ATT is requested first, then the native consent bridge is configured.
/// The Mobile Ads SDK itself starts only when UMP permits ad requests.
#[derive(Resource, Clone, Default)]
pub struct AdmobConfig {
    /// Automatic by default; manual timing never bypasses authorization.
    pub tracking_prompt: AdTrackingPrompt,
    /// Ad unit id per format. A format with no entry here falls back to the
    /// Google test unit (so an unconfigured format is never a hard error in
    /// development). In production, set every format you use.
    pub unit_ids: HashMap<AdFormat, String>,
    /// Device ids to treat as test devices (so real ad units serve test
    /// creatives). The SDK logs the id of each device on first request.
    pub test_device_ids: Vec<String>,
    /// Force Google's official test unit ids for *every* format, ignoring
    /// `unit_ids`. [`AdmobConfig::for_build`] sets it per build.
    pub use_test_ads: bool,
}

impl AdmobConfig {
    /// Debug and simulator builds use test creatives. Release device builds,
    /// including TestFlight, use the configured production placements.
    pub fn for_build() -> Self {
        Self {
            use_test_ads: crate::environment::AppEnvironment::current().is_development(),
            ..Default::default()
        }
    }

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

/// Explicit UMP test overrides. Insert before [`AdsPlugin`] initializes.
///
/// Every override is ignored unless [`AdmobConfig::use_test_ads`] is true.
#[derive(Resource, Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct UmpTestConfig {
    /// Optional geography presented to UMP's debug settings.
    pub geography: Option<UmpDebugGeography>,
    /// Clear UMP consent state before the first consent-info refresh.
    pub reset_consent_on_start: bool,
}

impl UmpTestConfig {
    fn wire_values(self, use_test_ads: bool) -> (i32, i32) {
        if use_test_ads {
            (
                self.geography.map(UmpDebugGeography::as_i32).unwrap_or(0),
                self.reset_consent_on_start as i32,
            )
        } else {
            (0, 0)
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
    /// The native consent bridge has been configured after ATT resolved.
    /// Mobile Ads startup still waits for UMP readiness.
    pub initialized: bool,
    /// Current UMP consent state.
    pub consent: ConsentStatus,
    /// UMP's authoritative answer for whether ads may be requested.
    ///
    /// This is intentionally not inferred from [`Self::consent`]. Google may
    /// preserve usable consent from a previous session when the current
    /// consent-info update fails.
    pub can_request_ads: bool,
    /// The launch check/form finished or failed; gameplay never waits for it.
    pub consent_check_complete: bool,
    /// Whether a banner is currently on screen.
    pub banner_visible: bool,
    /// Visible, filled banner height in UIKit points. Zero while loading,
    /// on no-fill, or when hidden or detached. Excludes safe-area
    /// insets and caller padding; the desktop fake reserves 50 points. Native
    /// changes can take one main-loop turn to appear when polled by a worker.
    pub banner_height: f32,
}

/// Preload a full-screen ad of `format` after
/// [`AdmobState::can_request_ads`] is true. No-op for [`AdFormat::Banner`]
/// (use [`ShowBanner`]).
#[derive(Message, Clone, Debug)]
pub struct LoadAd(pub AdFormat);

/// Present a previously-loaded full-screen ad. If it isn't loaded an
/// [`AdShowFailed`] is emitted.
#[derive(Message, Clone, Debug)]
pub struct ShowAd(pub AdFormat);

/// Try once at a natural break. An unavailable creative is skipped immediately;
/// it is never queued to interrupt gameplay when a later load completes.
#[derive(Message, Clone, Debug)]
pub struct TryShowAd(pub AdFormat);

/// Show (or move) the banner after [`AdmobState::can_request_ads`] is true.
/// Loads asynchronously, remaining hidden with zero height until filled.
/// Prefer [`BannerPlugin`] and [`BannerIntent`] for managed placements.
#[derive(Message, Clone, Debug, Default)]
pub struct ShowBanner {
    pub position: BannerPosition,
}

/// Hide and release the banner.
#[derive(Message, Clone, Debug)]
pub struct HideBanner;

/// Retry UMP verification and present a required form, sharing any pending
/// startup operation. The toolkit already does this automatically at launch.
#[derive(Message, Clone, Debug)]
pub struct RequestConsent;

/// Present the UMP privacy-options form in response to a visible user action.
///
/// Only send this while the [`PrivacyOptionsRequirement`] resource is
/// [`PrivacyOptionsRequirement::Required`]
/// ([`PrivacyOptionsRequirement::is_required`]).
#[derive(Message, Clone, Debug)]
pub struct PresentPrivacyOptions;

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

/// The latest UMP consent-info update failed.
///
/// Cached consent may still permit ads, so read
/// [`AdmobState::can_request_ads`]. [`RequestConsent`] retries.
#[derive(Message, Clone, Debug)]
pub struct ConsentInfoUpdateFailed {
    pub error: String,
}

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

fn init(config: &AdmobConfig, ump_test: UmpTestConfig) {
    let Ok(joined) = CString::new(config.test_device_ids.join(",")) else {
        return;
    };
    let (debug_geography, reset_consent) = ump_test.wire_values(config.use_test_ads);
    unsafe {
        backend::admob_init_with_ump_test(
            joined.as_ptr(),
            config.use_test_ads as i32,
            debug_geography,
            reset_consent,
        )
    };
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

fn present_privacy_options() {
    unsafe { backend::admob_present_privacy_options() };
}

fn consent_status() -> ConsentStatus {
    ConsentStatus::from_i32(unsafe { backend::admob_consent_status() })
}

fn can_request_ads() -> bool {
    unsafe { backend::admob_can_request_ads() != 0 }
}

fn privacy_options_requirement() -> PrivacyOptionsRequirement {
    PrivacyOptionsRequirement::from_i32(unsafe {
        backend::admob_privacy_options_requirement_status()
    })
}

fn drain_events() -> Vec<AdEvent> {
    let json = unsafe { read_cstr(backend::admob_drain_events()) };
    serde_json::from_str(&json).unwrap_or_default()
}

#[derive(Resource, Default)]
struct AdsPoll {
    inited: bool,
    tracking_requested: bool,
    consent_requested: bool,
    consent: ConsentStatus,
    presenting: Option<AdFormat>,
}

pub struct AdsPlugin;

impl Plugin for AdsPlugin {
    fn build(&self, app: &mut App) {
        crate::configure_systems(app);
        if !app.is_plugin_added::<AttPlugin>() {
            app.add_plugins(AttPlugin);
        }
        app.init_resource::<AdInventory>()
            .init_resource::<AdmobState>()
            .init_resource::<PrivacyOptionsRequirement>()
            .init_resource::<UmpTestConfig>()
            .init_resource::<AdsPoll>()
            .add_message::<LoadAd>()
            .add_message::<ShowAd>()
            .add_message::<TryShowAd>()
            .add_message::<ShowBanner>()
            .add_message::<HideBanner>()
            .add_message::<RequestConsent>()
            .add_message::<PresentPrivacyOptions>()
            .add_message::<AdLoaded>()
            .add_message::<AdLoadFailed>()
            .add_message::<AdShown>()
            .add_message::<AdDismissed>()
            .add_message::<AdShowFailed>()
            .add_message::<RewardEarned>()
            .add_message::<AdClicked>()
            .add_message::<ConsentUpdated>()
            .add_message::<ConsentInfoUpdateFailed>()
            .add_systems(
                Update,
                (init_once, poll_backend)
                    .chain()
                    .after(AttSystems)
                    .in_set(crate::IosSystems::Poll),
            )
            .add_systems(Update, pump_requests.in_set(crate::IosSystems::Dispatch));
    }
}

/// Request ATT once, then initialize the consent bridge after it resolves.
/// The native request owns foreground waits and retries; no per-frame prompts.
fn init_once(
    config: Option<Res<AdmobConfig>>,
    ump_test: Res<UmpTestConfig>,
    tracking: Res<TrackingStatus>,
    mut requests: MessageWriter<RequestTracking>,
    mut poll: ResMut<AdsPoll>,
    mut state: ResMut<AdmobState>,
) {
    if poll.inited {
        return;
    }
    if let Some(config) = config {
        if !tracking.is_determined() {
            if config.tracking_prompt == AdTrackingPrompt::Automatic && !poll.tracking_requested {
                poll.tracking_requested = true;
                requests.write(RequestTracking);
            }
            return;
        }
        init(&config, *ump_test);
        poll.inited = true;
        state.initialized = true;
    }
}

/// Forward consumer requests to the backend, resolving unit ids from config.
#[allow(clippy::too_many_arguments)]
fn pump_requests(
    mut poll: ResMut<AdsPoll>,
    tracking: Res<TrackingStatus>,
    config: Option<Res<AdmobConfig>>,
    mut inventory: ResMut<AdInventory>,
    mut loads: MessageReader<LoadAd>,
    mut shows: MessageReader<ShowAd>,
    mut tries: MessageReader<TryShowAd>,
    mut banner_shows: MessageReader<ShowBanner>,
    mut banner_hides: MessageReader<HideBanner>,
    mut consents: MessageReader<RequestConsent>,
    mut privacy_options: MessageReader<PresentPrivacyOptions>,
    mut load_failed: MessageWriter<AdLoadFailed>,
    mut show_failed: MessageWriter<AdShowFailed>,
) {
    // A launch-time consent request must survive the asynchronous ATT wait.
    if consents.read().count() > 0 {
        poll.consent_requested = true;
    }
    if poll.inited && poll.consent_requested {
        poll.consent_requested = false;
        request_consent();
    }
    let ready = poll.inited && tracking.is_determined() && can_request_ads();
    let resolve = |format: AdFormat| {
        config
            .as_ref()
            .map(|c| c.resolve_unit(format))
            .unwrap_or_else(|| test_unit_id(format).to_string())
    };

    for LoadAd(format) in loads.read() {
        if format.is_full_screen() {
            if !ready {
                inventory.set(*format, AdLoadState::Failed);
                load_failed.write(AdLoadFailed {
                    format: *format,
                    error: "ad request blocked: ATT or UMP is not ready".into(),
                });
                continue;
            }
            if inventory.is_loading(*format) || inventory.is_loaded(*format) {
                continue;
            }
            inventory.set(*format, AdLoadState::Loading);
            load(*format, &resolve(*format));
        }
    }
    for ShowAd(format) in shows.read() {
        if ready
            && format.is_full_screen()
            && inventory.is_loaded(*format)
            && poll.presenting.is_none()
        {
            inventory.set(*format, AdLoadState::Idle);
            poll.presenting = Some(*format);
            show(*format);
        } else {
            show_failed.write(AdShowFailed {
                format: *format,
                error: "ad presentation blocked: ATT or UMP is not ready".into(),
            });
        }
    }
    for TryShowAd(format) in tries.read() {
        if ready
            && format.is_full_screen()
            && inventory.is_loaded(*format)
            && poll.presenting.is_none()
        {
            inventory.set(*format, AdLoadState::Idle);
            poll.presenting = Some(*format);
            show(*format);
        }
    }
    for ShowBanner { position } in banner_shows.read() {
        if ready {
            inventory.set(AdFormat::Banner, AdLoadState::Loading);
            banner_show(&resolve(AdFormat::Banner), *position);
        } else {
            show_failed.write(AdShowFailed {
                format: AdFormat::Banner,
                error: "banner blocked: ATT or UMP is not ready".into(),
            });
        }
    }
    for _ in banner_hides.read() {
        if poll.inited {
            inventory.set(AdFormat::Banner, AdLoadState::Idle);
            banner_hide();
        }
    }
    for _ in privacy_options.read() {
        if poll.inited {
            present_privacy_options();
        }
    }
}

/// Drain the backend's polled state into resources + messages.
#[allow(clippy::too_many_arguments)]
fn poll_backend(
    mut poll: ResMut<AdsPoll>,
    mut inventory: ResMut<AdInventory>,
    mut state: ResMut<AdmobState>,
    mut privacy_options_state: ResMut<PrivacyOptionsRequirement>,
    mut loaded: MessageWriter<AdLoaded>,
    mut load_failed: MessageWriter<AdLoadFailed>,
    mut shown: MessageWriter<AdShown>,
    mut dismissed: MessageWriter<AdDismissed>,
    mut show_failed: MessageWriter<AdShowFailed>,
    mut reward: MessageWriter<RewardEarned>,
    mut clicked: MessageWriter<AdClicked>,
    mut consent_updated: MessageWriter<ConsentUpdated>,
    mut consent_update_failed: MessageWriter<ConsentInfoUpdateFailed>,
) {
    if !poll.inited {
        return;
    }

    let consent = consent_status();
    state.banner_height = unsafe { backend::admob_banner_height() };
    state.banner_visible = state.banner_height > 0.0;
    if consent != poll.consent {
        poll.consent = consent;
        state.consent = consent;
        consent_updated.write(ConsentUpdated(consent));
    }
    let complete = unsafe { backend::admob_consent_check_complete() != 0 };
    if state.consent_check_complete != complete {
        state.consent_check_complete = complete;
    }
    let ready = can_request_ads();
    if state.can_request_ads != ready {
        state.can_request_ads = ready;
    }
    let privacy_options = privacy_options_requirement();
    if privacy_options != *privacy_options_state {
        *privacy_options_state = privacy_options;
    }

    for ev in drain_events() {
        if ev.kind == "consent_update_failed" {
            consent_update_failed.write(ConsentInfoUpdateFailed { error: ev.error });
            continue;
        }
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
                if poll.presenting == Some(format) {
                    poll.presenting = None;
                }
                dismissed.write(AdDismissed(format));
            }
            "show_failed" => {
                if poll.presenting == Some(format) {
                    poll.presenting = None;
                }
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

// These tests drive the desktop fake; native behavior is covered by the demo.
#[cfg(all(test, not(target_os = "ios")))]
mod tests;

#[cfg(all(test, feature = "game", not(target_os = "ios")))]
pub(crate) fn reset_test_backend() {
    backend::reset();
}
