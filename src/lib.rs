//! `bevy_ios_toolkit` — native iOS integrations for Bevy, as ordinary ECS
//! resources and messages.
//!
//! Each integration is a **cargo feature** and a module:
//!
//! | feature | module | what it bridges |
//! |---------|--------|-----------------|
//! | `storekit` | [`store`] | StoreKit environment + in-app purchases (iOS only) |
//! | `ads` | [`ads`] | Google AdMob ads + UMP consent |
//! | `att` | [`att`] | App Tracking Transparency prompt |
//! | `gamekit` | [`gamekit`] | Game Center auth, leaderboards, achievements |
//! | `review` | [`review`] | StoreKit review prompt |
//! | `notifications` | [`notifications`] | local user notifications |
//! | `platform` | [`platform`] | haptics, safe-area insets, links, share sheet, thermal/low-power state, boot shield, audio session |
//!
//! No feature is on by default. Enable exactly what you ship — a module's
//! `extern "C"` block only exists when its feature is on, and the matching Swift
//! shim must be in your Xcode target or iOS linking fails on undefined symbols.
//!
//! ```toml
//! [dependencies]
//! bevy_ios_toolkit = { version = "0.7", features = ["ads"] }
//!
//! [target.'cfg(target_os = "ios")'.dependencies]
//! bevy_ios_toolkit = { version = "0.7", features = ["storekit"] }
//! ```
//!
//! # The native contract
//!
//! Shared across every native module:
//!
//! - Every native entry point is `@_cdecl` C-ABI, called **from Rust**.
//! - Async, delegate-driven SDK work surfaces as **polled state** (or a drained
//!   event queue) read once per frame — *never* callbacks into Rust, because
//!   re-entrancy against winit's event loop is not safe.
//! - Each Swift shim sits behind `#if canImport(...)` with linking stubs, so the
//!   staticlib links on any target.
//!
//! Off iOS, integrations keep a fake only where a real cross-platform flow
//! benefits from one. StoreKit purchases are platform-owned: [`store`] is iOS
//! only and has no desktop backend.
//!
//! ```no_run
//! use bevy::prelude::*;
//! use bevy_ios_toolkit::prelude::*;
//!
//! fn main() {
//!     App::new()
//!         .add_plugins((MinimalPlugins, IosPlugin))
//!         .run();
//! }
//! ```

use bevy::prelude::*;

#[cfg(all(feature = "storekit", any(target_os = "ios", doc, test)))]
#[path = "store/environment.rs"]
mod store_environment;

#[cfg(all(feature = "storekit", any(target_os = "ios", doc, test)))]
#[path = "store/operation.rs"]
mod store_operation;

#[cfg(all(feature = "storekit", any(target_os = "ios", doc, test)))]
#[path = "store/state.rs"]
#[cfg_attr(test, allow(dead_code))]
mod store_state;

#[cfg(any(
    all(feature = "storekit", any(target_os = "ios", doc)),
    feature = "ads",
    feature = "gamekit",
    feature = "notifications"
))]
mod ffi;

#[cfg(all(feature = "storekit", any(target_os = "ios", doc)))]
pub mod store;

#[cfg(feature = "ads")]
pub mod ads;

#[cfg(feature = "platform")]
pub mod platform;

#[cfg(feature = "att")]
pub mod att;

#[cfg(feature = "gamekit")]
pub mod gamekit;

#[cfg(feature = "review")]
pub mod review;

#[cfg(feature = "notifications")]
pub mod notifications;

pub mod prelude {
    pub use crate::IosPlugin;

    #[cfg(all(feature = "storekit", any(target_os = "ios", doc)))]
    pub use crate::store::{
        AppStoreEnvironment, Entitlements, EntitlementsChanged, EntitlementsState, ProductInfo,
        ProductsState, ProductsUpdated, PurchaseCompleted, PurchaseOutcome, PurchaseRequest,
        RequestAppStoreEnvironment, RestoreCompleted, RestoreOutcome, RestoreRequest,
        StoreActivity, StoreConfig, StoreProducts,
    };

    #[cfg(feature = "ads")]
    pub use crate::ads::{
        AdClicked, AdDismissed, AdFormat, AdInventory, AdLoadFailed, AdLoadState, AdLoaded,
        AdShowFailed, AdShown, AdTrackingPrompt, AdmobConfig, AdmobState, BannerPosition,
        ConsentInfoUpdateFailed, ConsentStatus, ConsentUpdated, HideBanner, LoadAd,
        PresentPrivacyOptions, PrivacyOptionsRequirement, RequestConsent, RewardEarned, ShowAd,
        ShowBanner, TEST_APP_ID, UmpDebugGeography, UmpTestConfig,
    };

    #[cfg(feature = "platform")]
    pub use crate::platform::{self, Haptic, PowerState, PowerStateChanged, ThermalState};

    #[cfg(feature = "att")]
    pub use crate::att::{RequestTracking, TrackingStatus, TrackingStatusChanged};

    #[cfg(feature = "gamekit")]
    pub use crate::gamekit::{
        AuthenticateGameCenter, GameCenter, GameCenterAuthChanged, GameCenterAuthState,
        ReportAchievement, ShowGameCenter, SubmitScore,
    };

    #[cfg(feature = "review")]
    pub use crate::review;

    #[cfg(feature = "notifications")]
    pub use crate::notifications::{
        CancelAllNotifications, CancelNotification, NotificationOpened, NotificationPermission,
        NotificationPermissionChanged, NotificationScheduleFailed, PendingNotifications,
        RequestNotificationPermission, ScheduleNotification,
    };
}

/// Installs every enabled iOS integration. Composition is feature-driven: a
/// module only wires its systems when its feature is on.
pub struct IosPlugin;

impl Plugin for IosPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(all(feature = "storekit", any(target_os = "ios", doc)))]
        app.add_plugins(store::StorePlugin);
        #[cfg(feature = "att")]
        if !app.is_plugin_added::<att::AttPlugin>() {
            app.add_plugins(att::AttPlugin);
        }
        #[cfg(feature = "ads")]
        app.add_plugins(ads::AdsPlugin);
        #[cfg(feature = "gamekit")]
        app.add_plugins(gamekit::GameKitPlugin);
        #[cfg(feature = "platform")]
        app.add_plugins(platform::PlatformPlugin);
        #[cfg(feature = "notifications")]
        app.add_plugins(notifications::NotificationsPlugin);
        // `review` is a function-only module — nothing to wire.
        let _ = app;
    }
}
