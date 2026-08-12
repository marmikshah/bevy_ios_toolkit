//! iOS backend: the raw C-ABI surface implemented by `StoreKitBridge.swift`.
//! All async StoreKit work happens Swift-side; Rust only issues commands and
//! polls cached state (no callbacks into Rust — winit re-entrancy is unsafe).
//!
//! String getters transfer ownership of an independent allocation to Rust.
//! The safe wrappers in `super` copy it and call `store_string_free`.

use std::ffi::c_char;

unsafe extern "C" {
    /// Begin resolving `AppTransaction.shared.environment`.
    pub fn store_environment_init();
    /// 0 pending, 1 Xcode, 2 sandbox, 3 production, 4 unavailable, 5 unknown.
    pub fn store_environment_state() -> i32;
    /// Begin: fetch products for the comma-separated ids and start the
    /// `Transaction.updates` listener + an entitlements refresh.
    pub fn store_init(ids: *const c_char);
    /// 0 = loading, 1 = ready, 2 = failed.
    pub fn store_products_state() -> i32;
    /// JSON `[{id, display_name, display_price, description}]`.
    pub fn store_products_json() -> *mut c_char;
    /// Begin a purchase for `id` (async, surfaces via purchase_state).
    pub fn store_purchase(id: *const c_char);
    /// 0 idle, 1 purchasing, 2 success, 3 failed, 4 cancelled, 5 pending.
    pub fn store_purchase_state() -> i32;
    /// The product id the current purchase_state refers to ("" if idle).
    pub fn store_purchase_product() -> *mut c_char;
    /// Ack a terminal purchase result; resets purchase_state to idle.
    pub fn store_purchase_clear();
    /// Restore purchases (`AppStore.sync()` + entitlements refresh).
    pub fn store_restore();
    /// 0 idle, 1 restoring, 2 success, 3 failed.
    pub fn store_restore_state() -> i32;
    /// Ack a terminal restore result; resets restore_state to idle.
    pub fn store_restore_clear();
    /// Bumped when entitlement readiness, failure, or verified ownership changes.
    pub fn store_entitlements_rev() -> u64;
    /// JSON `{state, product_ids}` for one atomic entitlement snapshot.
    pub fn store_entitlements_json() -> *mut c_char;
    /// Release any string returned by this Store bridge.
    pub fn store_string_free(value: *mut c_char);
}
