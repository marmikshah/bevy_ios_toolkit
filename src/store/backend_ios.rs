//! iOS backend: the raw C-ABI surface implemented by `StoreKitBridge.swift`.
//! All async StoreKit work happens Swift-side; Rust only issues commands and
//! polls cached state (no callbacks into Rust — winit re-entrancy is unsafe).
//!
//! Strings returned by the `*_json` getters point at Swift-owned buffers valid
//! only until the next call that regenerates them; the safe wrappers in `super`
//! copy immediately.

use std::ffi::c_char;

unsafe extern "C" {
    /// Begin: fetch products for the comma-separated ids and start the
    /// `Transaction.updates` listener + an entitlements refresh.
    pub fn cupertino_store_init(ids: *const c_char);
    /// 0 = loading, 1 = ready, 2 = failed.
    pub fn cupertino_store_products_state() -> i32;
    /// JSON `[{id, display_name, display_price, description}]`.
    pub fn cupertino_store_products_json() -> *const c_char;
    /// Begin a purchase for `id` (async, surfaces via purchase_state).
    pub fn cupertino_store_purchase(id: *const c_char);
    /// 0 idle, 1 purchasing, 2 success, 3 failed, 4 cancelled, 5 pending.
    pub fn cupertino_store_purchase_state() -> i32;
    /// The product id the current purchase_state refers to ("" if idle).
    pub fn cupertino_store_purchase_product() -> *const c_char;
    /// Ack a terminal purchase result; resets purchase_state to idle.
    pub fn cupertino_store_purchase_clear();
    /// Restore purchases (`AppStore.sync()` + entitlements refresh).
    pub fn cupertino_store_restore();
    /// Bumped whenever the entitlement set changes; poll cheaply, parse only on change.
    pub fn cupertino_store_entitlements_rev() -> u64;
    /// JSON `["id", ...]` of currently-entitled product ids.
    pub fn cupertino_store_entitlements_json() -> *const c_char;
}
