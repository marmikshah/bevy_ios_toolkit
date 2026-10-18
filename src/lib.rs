//! `bevy_ios_toolkit` — native iOS integrations for Bevy, as ordinary ECS
//! resources and messages.
//!
//! Each integration is a **cargo feature** and a module:
//!
//! | feature | module | what it bridges |
//! |---------|--------|-----------------|
//! | `game` | [`game`](game/index.html) | default iOS game startup: StoreKit, AdMob, permissions, notifications |
//! | `storekit` | [`store`](store/index.html) | StoreKit environment + in-app purchases |
//! | `ads` | [`ads`](ads/index.html) | Google AdMob ads + UMP consent |
//! | `att` | [`att`](att/index.html) | App Tracking Transparency prompt |
//! | `gamekit` | [`gamekit`](gamekit/index.html) | Game Center auth, leaderboards, achievements |
//! | `review` | [`review`](review/index.html) | StoreKit review prompt |
//! | `notifications` | [`notifications`](notifications/index.html) | local user notifications |
//! | `platform` | [`platform`](platform/index.html) | haptics, safe-area insets, links, share sheet, thermal/low-power state, boot shield, audio session |
//! | `boot-shield` | [`platform::boot_shield`](platform/lifecycle/boot_shield/index.html) | `BootShieldPlugin`, lowering the boot shield after real presents |
//! | `capture` | [`capture`](capture/index.html) | desktop offscreen rendering for screenshots and recordings |
//!
//! No feature is on by default. Enable exactly what you ship — a module's
//! `extern "C"` block only exists when its feature is on, and the matching SPM
//! product must be linked into the app or iOS linking fails on undefined symbols.
//!
//! ```toml
//! [dependencies]
//! bevy_ios_toolkit = { version = "0.9", features = ["game"] }
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
//! benefits from one. StoreKit purchases are platform-owned: [`store`](store/index.html) exposes
//! portable policy types, and the native backend exists only on iOS.
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

pub mod environment;

/// Order native polling, game policy, and command dispatch explicitly.
#[derive(SystemSet, Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum IosSystems {
    Poll,
    Intents,
    Dispatch,
}

pub(crate) fn configure_systems(app: &mut App) {
    app.configure_sets(
        Update,
        (IosSystems::Poll, IosSystems::Intents, IosSystems::Dispatch).chain(),
    );
}

#[cfg(feature = "game")]
pub mod game;

#[cfg(any(
    feature = "ads",
    feature = "notifications",
    all(feature = "gamekit", not(target_os = "ios"))
))]
mod ffi;

#[cfg(feature = "storekit")]
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

#[cfg(all(
    feature = "capture",
    any(target_os = "macos", target_os = "windows", target_os = "linux")
))]
pub mod capture;

pub mod prelude {
    pub use crate::environment::AppEnvironment;
    #[cfg(feature = "game")]
    pub use crate::game::{IosGameConfig, IosGamePlugin};
    pub use crate::{IosPlugin, IosSystems};

    #[cfg(feature = "storekit")]
    pub use crate::store::{
        AppStoreEnvironment, Entitlements, EntitlementsChanged, EntitlementsState, ProductInfo,
        ProductsState, ProductsUpdated, PurchaseCompleted, PurchaseOutcome, PurchaseRequest,
        ReloadStoreRequest, RequestAppStoreEnvironment, RestoreCompleted, RestoreOutcome,
        RestoreRequest, StoreActivity, StoreConfig, StoreProducts,
    };

    #[cfg(feature = "ads")]
    pub use crate::ads::{
        AdClicked, AdDismissed, AdFormat, AdInventory, AdLoadFailed, AdLoadState, AdLoaded,
        AdShowFailed, AdShown, AdTrackingPrompt, AdmobConfig, AdmobState, BannerIntent,
        BannerPlugin, BannerPosition, ConsentInfoUpdateFailed, ConsentStatus, ConsentUpdated,
        HideBanner, LoadAd, PresentPrivacyOptions, PrivacyOptionsRequirement, RequestConsent,
        RewardEarned, ShowAd, ShowBanner, TEST_APP_ID, TryShowAd, UmpDebugGeography, UmpQa,
        UmpTestConfig,
    };

    #[cfg(feature = "platform")]
    pub use crate::platform::{
        self, Haptic, PowerState, PowerStateChanged, SafeArea, ThermalState,
    };

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
        NotificationPermissionChanged, NotificationPermissionFailed, NotificationScheduleFailed,
        PendingNotifications, RequestNotificationPermission, ScheduleNotification,
    };
}

/// Installs every enabled iOS integration. Composition is feature-driven: a
/// module only wires its systems when its feature is on.
pub struct IosPlugin;

impl Plugin for IosPlugin {
    fn build(&self, app: &mut App) {
        configure_systems(app);
        #[cfg(all(feature = "storekit", target_os = "ios"))]
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
