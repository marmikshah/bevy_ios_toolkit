//! Native StoreKit transport and ECS polling.
use super::*;
use crate::store_environment::{environment_from_raw, take_environment_request};
use crate::store_operation::{
    begin_catalog_reload, finish_activity, start_purchase, start_restore,
};
use crate::store_state::decode_entitlement_snapshot;
use std::ffi::{CStr, CString, c_char};
#[path = "backend_ios.rs"]
mod backend;

// ---------- Safe backend wrappers ----------

fn init_environment() {
    unsafe { backend::store_environment_init() };
}

fn environment() -> AppStoreEnvironment {
    environment_from_raw(unsafe { backend::store_environment_state() })
}

fn init(ids: &[String]) {
    let Ok(joined) = CString::new(ids.join(",")) else {
        return;
    };
    unsafe { backend::store_init(joined.as_ptr()) };
}

fn products_state() -> ProductsState {
    match unsafe { backend::store_products_state() } {
        1 => ProductsState::Ready,
        2 => ProductsState::Failed,
        _ => ProductsState::Loading,
    }
}

fn products() -> Result<Vec<ProductInfo>, serde_json::Error> {
    let json = unsafe { take_string(backend::store_products_json()) };
    serde_json::from_str(&json)
}

fn purchase(id: &str) -> bool {
    let Ok(id) = CString::new(id) else {
        return false;
    };
    unsafe { backend::store_purchase(id.as_ptr()) };
    true
}

/// Returns the terminal outcome (if any) and the product it refers to.
fn purchase_result() -> Option<(PurchaseOutcome, String)> {
    let outcome = match unsafe { backend::store_purchase_state() } {
        2 => PurchaseOutcome::Success,
        3 => PurchaseOutcome::Failed,
        4 => PurchaseOutcome::Cancelled,
        5 => PurchaseOutcome::Pending,
        _ => return None,
    };
    let product = unsafe { take_string(backend::store_purchase_product()) };
    Some((outcome, product))
}

fn purchase_clear() {
    unsafe { backend::store_purchase_clear() };
}

fn restore() {
    unsafe { backend::store_restore() };
}

fn restore_result() -> Option<RestoreOutcome> {
    match unsafe { backend::store_restore_state() } {
        2 => Some(RestoreOutcome::Success),
        3 => Some(RestoreOutcome::Failed),
        _ => None,
    }
}

fn restore_clear() {
    unsafe { backend::store_restore_clear() };
}

fn entitlements_rev() -> u64 {
    unsafe { backend::store_entitlements_rev() }
}

fn fetch_entitlements() -> Result<crate::store_state::EntitlementSnapshot, serde_json::Error> {
    let json = unsafe { take_string(backend::store_entitlements_json()) };
    decode_entitlement_snapshot(&json)
}

/// Copy one caller-owned native string and release the bridge allocation.
unsafe fn take_string(ptr: *mut c_char) -> String {
    if ptr.is_null() {
        return String::new();
    }
    let value = unsafe { CStr::from_ptr(ptr) }
        .to_string_lossy()
        .into_owned();
    unsafe { backend::store_string_free(ptr) };
    value
}

// ---------- Plugin ----------

#[derive(Resource)]
struct StorePoll {
    inited: bool,
    last_products: ProductsState,
    ent_rev: u64,
}

impl Default for StorePoll {
    fn default() -> Self {
        Self {
            inited: false,
            last_products: ProductsState::Loading,
            ent_rev: 0,
        }
    }
}

pub struct StorePlugin;

impl Plugin for StorePlugin {
    fn build(&self, app: &mut App) {
        crate::configure_systems(app);
        app.init_resource::<StoreProducts>()
            .init_resource::<Entitlements>()
            .init_resource::<StoreActivity>()
            .init_resource::<StorePoll>()
            .add_message::<PurchaseRequest>()
            .add_message::<RestoreRequest>()
            .add_message::<ReloadStoreRequest>()
            .add_message::<ProductsUpdated>()
            .add_message::<PurchaseCompleted>()
            .add_message::<RestoreCompleted>()
            .add_message::<EntitlementsChanged>()
            .add_systems(
                Update,
                (init_once, poll_products, poll_entitlements, poll_operations)
                    .chain()
                    .in_set(crate::IosSystems::Poll),
            )
            .add_systems(Update, pump_requests.in_set(crate::IosSystems::Dispatch));

        app.init_resource::<AppStoreEnvironment>()
            .add_message::<RequestAppStoreEnvironment>()
            .add_systems(
                Update,
                take_environment_request
                    .pipe(init_environment_once)
                    .in_set(crate::IosSystems::Dispatch),
            )
            .add_systems(Update, poll_environment.in_set(crate::IosSystems::Poll));
    }
}

/// Start only after an explicit request, independently of configured products.
fn init_environment_once(In(requested): In<bool>) {
    if requested {
        init_environment();
    }
}

/// Publish the immutable terminal environment exactly once.
fn poll_environment(mut current: ResMut<AppStoreEnvironment>) {
    if current.is_resolved() {
        return;
    }
    let resolved = environment();
    if !resolved.is_resolved() {
        return;
    }

    *current = resolved;
}

/// Initialize the backend the first frame a non-empty [`StoreConfig`] exists.
/// Tolerant of insertion order — the config can land any time.
fn init_once(config: Option<Res<StoreConfig>>, mut poll: ResMut<StorePoll>) {
    if poll.inited {
        return;
    }
    if let Some(config) = config
        && !config.product_ids.is_empty()
    {
        init(&config.product_ids);
        poll.inited = true;
    }
}

/// Forward consumer requests to the backend.
#[allow(clippy::too_many_arguments)]
fn pump_requests(
    mut poll: ResMut<StorePoll>,
    mut products: ResMut<StoreProducts>,
    entitlements: Res<Entitlements>,
    mut activity: ResMut<StoreActivity>,
    mut buys: MessageReader<PurchaseRequest>,
    mut restores: MessageReader<RestoreRequest>,
    mut reloads: MessageReader<ReloadStoreRequest>,
    mut products_updated: MessageWriter<ProductsUpdated>,
    mut purchase_completed: MessageWriter<PurchaseCompleted>,
) {
    if !poll.inited {
        return;
    }
    for buy in buys.read() {
        if !activity.is_idle() {
            continue;
        }
        let is_available = products.state == ProductsState::Ready && products.get(&buy.0).is_some();
        let can_purchase = entitlements.is_ready() && !entitlements.owns(&buy.0);
        if !is_available || !can_purchase {
            purchase_completed.write(PurchaseCompleted {
                product_id: buy.0.clone(),
                outcome: PurchaseOutcome::Failed,
            });
            continue;
        }
        if !start_purchase(&mut activity, &buy.0) {
            continue;
        }
        if !purchase(&buy.0) {
            finish_activity(&mut activity);
            purchase_completed.write(PurchaseCompleted {
                product_id: buy.0.clone(),
                outcome: PurchaseOutcome::Failed,
            });
        }
    }
    for _ in restores.read() {
        if !activity.is_idle() {
            continue;
        }
        if !start_restore(&mut activity) {
            continue;
        }
        restore();
    }
    if reloads.read().count() > 0 && begin_catalog_reload(&mut products, &activity) {
        // Set the Rust state as well as the backend: even a load completing
        // between frames must be read again when it returns to Ready.
        poll.last_products = ProductsState::Loading;
        products_updated.write(ProductsUpdated);
        unsafe { backend::store_reload() };
    }
}

/// Project the native product catalogue into its resource and update message.
fn poll_products(
    mut poll: ResMut<StorePoll>,
    mut store_products: ResMut<StoreProducts>,
    mut products_updated: MessageWriter<ProductsUpdated>,
) {
    if !poll.inited {
        return;
    }

    let state = products_state();
    if state != poll.last_products {
        poll.last_products = state;
        match state {
            ProductsState::Ready => match products() {
                Ok(items) => {
                    store_products.items = items;
                    store_products.state = ProductsState::Ready;
                }
                Err(_) => {
                    store_products.items.clear();
                    store_products.state = ProductsState::Failed;
                }
            },
            ProductsState::Loading | ProductsState::Failed => {
                store_products.items.clear();
                store_products.state = state;
            }
        }
        products_updated.write(ProductsUpdated);
    }
}

/// Project native entitlement truth before operation completions are emitted.
fn poll_entitlements(
    mut poll: ResMut<StorePoll>,
    mut entitlements: ResMut<Entitlements>,
    mut entitlements_changed: MessageWriter<EntitlementsChanged>,
) {
    if !poll.inited {
        return;
    }
    let rev = entitlements_rev();
    if rev != poll.ent_rev {
        poll.ent_rev = rev;
        let changed = match fetch_entitlements() {
            Ok(snapshot) => entitlements.apply(snapshot),
            Err(_) => entitlements.fail(),
        };
        if changed {
            entitlements_changed.write(EntitlementsChanged);
        }
    }
}

/// Publish terminal purchase and restore operations after entitlement truth.
fn poll_operations(
    poll: Res<StorePoll>,
    mut activity: ResMut<StoreActivity>,
    mut purchase_completed: MessageWriter<PurchaseCompleted>,
    mut restore_completed: MessageWriter<RestoreCompleted>,
) {
    if !poll.inited {
        return;
    }
    if let Some((outcome, product_id)) = purchase_result() {
        finish_activity(&mut activity);
        if !product_id.is_empty() {
            purchase_completed.write(PurchaseCompleted {
                product_id,
                outcome,
            });
        }
        purchase_clear();
    }

    if let Some(outcome) = restore_result() {
        finish_activity(&mut activity);
        restore_completed.write(RestoreCompleted { outcome });
        restore_clear();
    }
}
