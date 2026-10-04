//! StoreKit 2 in-app purchases as Bevy resources + messages.
//!
//! Flow:
//! 1. On iOS, send [`RequestAppStoreEnvironment`] when ready for StoreKit's
//!    possible sign-in sheet, then wait for [`AppStoreEnvironment`] to resolve
//!    before choosing service configuration. Reading it does not start work.
//! 2. Insert [`StoreConfig`] with your product ids. The plugin calls into the
//!    backend once, which fetches products and the current entitlements.
//! 3. Read [`StoreProducts`] for prices/titles to render your store UI.
//! 4. Send [`PurchaseRequest`] / [`RestoreRequest`] to act and project
//!    [`StoreActivity`] into visible progress while StoreKit is working.
//! 5. Keep purchase, ad, and tracking decisions closed until
//!    [`Entitlements::is_ready`]. React to [`EntitlementsChanged`] for the
//!    first authoritative snapshot and every later ownership/state change.
//!    [`Entitlements::owns`] then remains runtime truth across fresh purchases,
//!    restores, foreground reconciliation, and already-owned relaunches.

pub use crate::store_environment::{AppStoreEnvironment, RequestAppStoreEnvironment};
pub use crate::store_operation::StoreActivity;
pub use crate::store_state::{
    Entitlements, EntitlementsState, ProductInfo, ProductsState, StoreProducts,
};
use bevy::prelude::*;

#[cfg(target_os = "ios")]
mod native;
#[cfg(target_os = "ios")]
pub use native::StorePlugin;

// ---------- Types ----------

/// Terminal result of a purchase attempt.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PurchaseOutcome {
    Success,
    Failed,
    /// User cancelled the App Store sheet.
    Cancelled,
    /// Deferred — e.g. Ask to Buy. Entitlement may arrive later via updates.
    Pending,
}

/// Terminal result of an explicit App Store synchronization.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum RestoreOutcome {
    Success,
    Failed,
}

// ---------- Resources ----------

/// The product ids to offer. Insert before or after adding the plugin; the
/// store initializes on the first frame it sees a non-empty config.
#[derive(Resource, Clone, Default)]
pub struct StoreConfig {
    pub product_ids: Vec<String>,
}

// ---------- Messages ----------

/// Request a purchase of the given product id.
#[derive(Message, Clone, Debug)]
pub struct PurchaseRequest(pub String);

/// Request restoration of past purchases (`AppStore.sync()`).
#[derive(Message, Clone, Debug)]
pub struct RestoreRequest;

/// Retry the configured product catalogue and verified entitlement read.
/// Unlike restoration, this does not call `AppStore.sync()` or request login.
/// Requests during a purchase/restore are ignored; native loads are coalesced.
#[derive(Message, Clone, Debug)]
pub struct ReloadStoreRequest;

/// Emitted when the catalogue state or contents change.
#[derive(Message, Clone, Debug)]
pub struct ProductsUpdated;

/// Emitted once per purchase attempt when it reaches a terminal state.
#[derive(Message, Clone, Debug)]
pub struct PurchaseCompleted {
    pub product_id: String,
    pub outcome: PurchaseOutcome,
}

/// Emitted once when an explicit restore reaches a terminal result.
#[derive(Message, Clone, Debug)]
pub struct RestoreCompleted {
    pub outcome: RestoreOutcome,
}

/// Emitted when entitlement readiness, failure, or verified ownership changes.
/// The first authoritative empty snapshot emits this message too.
#[derive(Message, Clone, Debug)]
pub struct EntitlementsChanged;
