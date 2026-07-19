//! `bevy_ios_toolkit` — native iOS integrations for Bevy, as ordinary ECS
//! resources and messages.
//!
//! Each integration is a **cargo feature** and a module:
//!
//! | feature | module | what it bridges |
//! |---------|--------|-----------------|
//! | `storekit` | [`store`] | StoreKit 2 in-app purchases |
//! | `ads` | [`ads`] | Google AdMob ads + UMP consent |
//! | `att` | [`att`] | App Tracking Transparency prompt |
//! | `gamekit` | [`gamekit`] | Game Center auth, leaderboards, achievements |
//! | `review` | [`review`] | StoreKit review prompt |
//! | `platform` | [`platform`] | haptics, safe-area insets, links, share sheet, thermal/low-power state, boot shield, audio session |
//!
//! No feature is on by default. Enable exactly what you ship — a module's
//! `extern "C"` block only exists when its feature is on, and the matching Swift
//! shim must be in your Xcode target or iOS linking fails on undefined symbols.
//!
//! ```toml
//! bevy_ios_toolkit = { version = "0.3", features = ["storekit", "ads", "att"] }
//! ```
//!
//! # The native contract
//!
//! Shared across every module (see [`ffi`]):
//!
//! - Every native entry point is `@_cdecl` C-ABI, called **from Rust**.
//! - Async, delegate-driven SDK work surfaces as **polled state** (or a drained
//!   event queue) read once per frame — *never* callbacks into Rust, because
//!   re-entrancy against winit's event loop is not safe.
//! - Each Swift shim sits behind `#if canImport(...)` with linking stubs, so the
//!   staticlib links on any target.
//!
//! Off iOS every module is a **stateful, env-tunable fake**, so the whole app
//! flow is exercisable on `cargo run` desktop builds with no device.
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

#[cfg(any(feature = "storekit", feature = "ads", feature = "gamekit"))]
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

pub mod prelude {
    pub use crate::IosPlugin;

    #[cfg(feature = "storekit")]
    pub use crate::store::{
        Entitlements, EntitlementsChanged, ProductInfo, ProductsState, ProductsUpdated,
        PurchaseCompleted, PurchaseOutcome, PurchaseRequest, RestoreRequest, StoreConfig,
        StoreProducts,
    };

    #[cfg(feature = "ads")]
    pub use crate::ads::{
        AdClicked, AdDismissed, AdFormat, AdInventory, AdLoadFailed, AdLoadState, AdLoaded,
        AdShowFailed, AdShown, AdmobConfig, AdmobState, BannerPosition, ConsentStatus,
        ConsentUpdated, HideBanner, LoadAd, RequestConsent, RewardEarned, ShowAd, ShowBanner,
        TEST_APP_ID,
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
}

/// Installs every enabled iOS integration. Composition is feature-driven: a
/// module only wires its systems when its feature is on.
pub struct IosPlugin;

impl Plugin for IosPlugin {
    fn build(&self, app: &mut App) {
        #[cfg(feature = "storekit")]
        app.add_plugins(store::StorePlugin);
        #[cfg(feature = "ads")]
        app.add_plugins(ads::AdsPlugin);
        #[cfg(feature = "att")]
        app.add_plugins(att::AttPlugin);
        #[cfg(feature = "gamekit")]
        app.add_plugins(gamekit::GameKitPlugin);
        #[cfg(feature = "platform")]
        app.add_plugins(platform::PlatformPlugin);
        // `review` is a function-only module — nothing to wire.
        let _ = app;
    }
}
