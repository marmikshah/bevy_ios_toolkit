use std::cell::RefCell;
use std::ffi::{CString, c_char};

#[derive(Default)]
struct FakeStore {
    products_state: i32,
    products_json: String,
    purchase_state: i32,
    purchase_product: String,
    restore_state: i32,
    entitlement_revision: u64,
    entitlements_json: String,
    purchase_calls: usize,
}

thread_local! {
    static STORE: RefCell<FakeStore> = RefCell::new(FakeStore::default());
}

pub(super) fn reset() {
    STORE.with(|store| *store.borrow_mut() = FakeStore::default());
}

pub(super) fn publish_products(json: &str) {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.products_json = json.into();
        store.products_state = 1;
    });
}

pub(super) fn publish_entitlements(json: &str) {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.entitlements_json = json.into();
        store.entitlement_revision += 1;
    });
}

pub(super) fn publish_purchase_success(product_id: &str) {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.purchase_product = product_id.into();
        store.purchase_state = 2;
    });
}

pub(super) fn publish_restore_success() {
    STORE.with(|store| store.borrow_mut().restore_state = 2);
}

pub(super) fn purchase_calls() -> usize {
    STORE.with(|store| store.borrow().purchase_calls)
}

pub unsafe fn store_environment_init() {}
pub unsafe fn store_environment_state() -> i32 {
    4
}
pub unsafe fn store_init(_ids: *const c_char) {}
pub unsafe fn store_products_state() -> i32 {
    STORE.with(|store| store.borrow().products_state)
}
pub unsafe fn store_products_json() -> *mut c_char {
    STORE.with(|store| owned_string(&store.borrow().products_json))
}
pub unsafe fn store_purchase(_id: *const c_char) {
    STORE.with(|store| store.borrow_mut().purchase_calls += 1);
}
pub unsafe fn store_purchase_state() -> i32 {
    STORE.with(|store| store.borrow().purchase_state)
}
pub unsafe fn store_purchase_product() -> *mut c_char {
    STORE.with(|store| owned_string(&store.borrow().purchase_product))
}
pub unsafe fn store_purchase_clear() {
    STORE.with(|store| {
        let mut store = store.borrow_mut();
        store.purchase_state = 0;
        store.purchase_product.clear();
    });
}
pub unsafe fn store_restore() {}
pub unsafe fn store_restore_state() -> i32 {
    STORE.with(|store| store.borrow().restore_state)
}
pub unsafe fn store_restore_clear() {
    STORE.with(|store| store.borrow_mut().restore_state = 0);
}
pub unsafe fn store_entitlements_rev() -> u64 {
    STORE.with(|store| store.borrow().entitlement_revision)
}
pub unsafe fn store_entitlements_json() -> *mut c_char {
    STORE.with(|store| owned_string(&store.borrow().entitlements_json))
}
pub unsafe fn store_string_free(value: *mut c_char) {
    if !value.is_null() {
        drop(unsafe { CString::from_raw(value) });
    }
}

fn owned_string(value: &str) -> *mut c_char {
    CString::new(value).unwrap().into_raw()
}
