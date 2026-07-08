//! Non-iOS backend: a stateful in-memory fake so purchase flows are fully
//! exercisable on desktop/wasm `cargo run` without a device or sandbox account.
//! Same raw signatures as [`super::backend_ios`], so the safe wrappers in
//! `super` are identical across platforms.
//!
//! Behaviour is env-tunable:
//! - `BEVY_IOS_FAKE_OWNED=id1,id2` — pre-grant entitlements (relaunch /
//!   already-purchased path).
//! - `BEVY_IOS_FAKE_FAIL` — purchases fail.
//! - `BEVY_IOS_FAKE_CANCEL` — purchases report user-cancelled.

use std::collections::BTreeSet;
use std::ffi::{CString, c_char};
use std::sync::{LazyLock, Mutex};

use crate::ffi::read_cstr;

use super::ProductInfo;

#[derive(Default)]
struct Fake {
    product_ids: Vec<String>,
    owned: BTreeSet<String>,
    ent_rev: u64,
    products_state: i32,
    purchase_state: i32,
    purchase_product: String,
    products_json: CString,
    ent_json: CString,
}

impl Fake {
    fn env_owned() -> BTreeSet<String> {
        std::env::var("BEVY_IOS_FAKE_OWNED")
            .unwrap_or_default()
            .split(',')
            .map(str::trim)
            .filter(|s| !s.is_empty())
            .map(String::from)
            .collect()
    }

    fn rebuild_products(&mut self) {
        let items: Vec<ProductInfo> = self
            .product_ids
            .iter()
            .map(|id| ProductInfo {
                id: id.clone(),
                display_name: id.clone(),
                display_price: "$0.99".into(),
                description: "Fake product (desktop build)".into(),
            })
            .collect();
        self.products_json =
            CString::new(serde_json::to_string(&items).unwrap_or_else(|_| "[]".into()))
                .unwrap_or_default();
    }

    fn rebuild_entitlements(&mut self) {
        let ids: Vec<&String> = self.owned.iter().collect();
        self.ent_json = CString::new(serde_json::to_string(&ids).unwrap_or_else(|_| "[]".into()))
            .unwrap_or_default();
    }
}

static FAKE: LazyLock<Mutex<Fake>> = LazyLock::new(|| Mutex::new(Fake::default()));

fn lock() -> std::sync::MutexGuard<'static, Fake> {
    FAKE.lock().unwrap_or_else(|p| p.into_inner())
}

pub unsafe fn store_init(ids: *const c_char) {
    let ids = unsafe { read_cstr(ids) };
    let mut f = lock();
    f.product_ids = ids
        .split(',')
        .map(str::trim)
        .filter(|s| !s.is_empty())
        .map(String::from)
        .collect();
    f.owned = Fake::env_owned();
    f.ent_rev += 1;
    f.products_state = if std::env::var("BEVY_IOS_FAKE_FAIL_PRODUCTS").is_ok() {
        2
    } else {
        1
    };
    f.rebuild_products();
    f.rebuild_entitlements();
}

pub unsafe fn store_products_state() -> i32 {
    lock().products_state
}

pub unsafe fn store_products_json() -> *const c_char {
    lock().products_json.as_ptr()
}

pub unsafe fn store_purchase(id: *const c_char) {
    let id = unsafe { read_cstr(id) };
    let mut f = lock();
    f.purchase_product = id.clone();
    if std::env::var("BEVY_IOS_FAKE_FAIL").is_ok() {
        f.purchase_state = 3;
    } else if std::env::var("BEVY_IOS_FAKE_CANCEL").is_ok() {
        f.purchase_state = 4;
    } else {
        f.purchase_state = 2;
        f.owned.insert(id);
        f.ent_rev += 1;
        f.rebuild_entitlements();
    }
}

pub unsafe fn store_purchase_state() -> i32 {
    lock().purchase_state
}

pub unsafe fn store_purchase_product() -> *const c_char {
    // Park the string in a thread-local so the returned pointer outlives the
    // mutex guard (the poll system reads it on a single thread).
    let product = lock().purchase_product.clone();
    PURCHASE_PRODUCT.with(|buf| {
        *buf.borrow_mut() = CString::new(product).unwrap_or_default();
        buf.borrow().as_ptr()
    })
}

thread_local! {
    static PURCHASE_PRODUCT: std::cell::RefCell<CString> =
        std::cell::RefCell::new(CString::default());
}

pub unsafe fn store_purchase_clear() {
    let mut f = lock();
    f.purchase_state = 0;
    f.purchase_product.clear();
}

pub unsafe fn store_restore() {
    let mut f = lock();
    f.owned = Fake::env_owned();
    f.ent_rev += 1;
    f.rebuild_entitlements();
}

pub unsafe fn store_entitlements_rev() -> u64 {
    lock().ent_rev
}

pub unsafe fn store_entitlements_json() -> *const c_char {
    lock().ent_json.as_ptr()
}
