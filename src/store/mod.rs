//! StoreKit 2 in-app purchases as Bevy resources + messages.
//!
//! Flow:
//! 1. Insert [`StoreConfig`] with your product ids. The plugin calls into the
//!    backend once, which fetches products and the current entitlements.
//! 2. Read [`StoreProducts`] for prices/titles to render your store UI.
//! 3. Send [`PurchaseRequest`] / [`RestoreRequest`] to act.
//! 4. React to [`PurchaseCompleted`] / [`EntitlementsChanged`], or just read
//!    the [`Entitlements`] resource — `owns(id)` is the source of truth and
//!    covers fresh purchases, restores, and already-owned-on-relaunch alike.

use std::collections::HashSet;
use std::ffi::CString;

use bevy::prelude::*;
use serde::{Deserialize, Serialize};

use crate::ffi::read_cstr;

#[cfg(target_os = "ios")]
#[path = "backend_ios.rs"]
mod backend;

#[cfg(not(target_os = "ios"))]
#[path = "backend_fake.rs"]
mod backend;

// ---------- Types ----------

/// One purchasable product, as reported by StoreKit (localized price/title).
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct ProductInfo {
    pub id: String,
    pub display_name: String,
    /// Localized, currency-formatted price string ("$0.99", "₹89").
    pub display_price: String,
    pub description: String,
}

/// Loading state of the product catalogue.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum ProductsState {
    #[default]
    Loading,
    Ready,
    Failed,
}

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

// ---------- Resources ----------

/// The product ids to offer. Insert before or after adding the plugin; the
/// store initializes on the first frame it sees a non-empty config.
#[derive(Resource, Clone, Default)]
pub struct StoreConfig {
    pub product_ids: Vec<String>,
}

/// The fetched catalogue. Mirrors the backend; read-only to consumers.
#[derive(Resource, Default)]
pub struct StoreProducts {
    pub state: ProductsState,
    pub items: Vec<ProductInfo>,
}

impl StoreProducts {
    pub fn get(&self, id: &str) -> Option<&ProductInfo> {
        self.items.iter().find(|p| p.id == id)
    }
}

/// The set of currently-entitled product ids (non-consumables + active subs).
/// `owns(id)` is the gate to use everywhere — it stays correct across purchase,
/// restore, and relaunch.
#[derive(Resource, Default)]
pub struct Entitlements {
    owned: HashSet<String>,
}

impl Entitlements {
    pub fn owns(&self, id: &str) -> bool {
        self.owned.contains(id)
    }

    pub fn iter(&self) -> impl Iterator<Item = &String> {
        self.owned.iter()
    }
}

// ---------- Messages ----------

/// Request a purchase of the given product id.
#[derive(Message, Clone, Debug)]
pub struct PurchaseRequest(pub String);

/// Request restoration of past purchases (`AppStore.sync()`).
#[derive(Message, Clone, Debug)]
pub struct RestoreRequest;

/// Emitted when the catalogue state or contents change.
#[derive(Message, Clone, Debug)]
pub struct ProductsUpdated;

/// Emitted once per purchase attempt when it reaches a terminal state.
#[derive(Message, Clone, Debug)]
pub struct PurchaseCompleted {
    pub product_id: String,
    pub outcome: PurchaseOutcome,
}

/// Emitted when the entitlement set changes. Read [`Entitlements`] for the new
/// state.
#[derive(Message, Clone, Debug)]
pub struct EntitlementsChanged;

// ---------- Safe backend wrappers ----------

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

fn products() -> Vec<ProductInfo> {
    let json = unsafe { read_cstr(backend::store_products_json()) };
    serde_json::from_str(&json).unwrap_or_default()
}

fn purchase(id: &str) {
    let Ok(id) = CString::new(id) else {
        return;
    };
    unsafe { backend::store_purchase(id.as_ptr()) };
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
    let product = unsafe { read_cstr(backend::store_purchase_product()) };
    Some((outcome, product))
}

fn purchase_clear() {
    unsafe { backend::store_purchase_clear() };
}

fn restore() {
    unsafe { backend::store_restore() };
}

fn entitlements_rev() -> u64 {
    unsafe { backend::store_entitlements_rev() }
}

fn fetch_entitlements() -> Vec<String> {
    let json = unsafe { read_cstr(backend::store_entitlements_json()) };
    serde_json::from_str(&json).unwrap_or_default()
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
        app.init_resource::<StoreProducts>()
            .init_resource::<Entitlements>()
            .init_resource::<StorePoll>()
            .add_message::<PurchaseRequest>()
            .add_message::<RestoreRequest>()
            .add_message::<ProductsUpdated>()
            .add_message::<PurchaseCompleted>()
            .add_message::<EntitlementsChanged>()
            .add_systems(Update, (init_once, pump_requests, poll_store).chain());
    }
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
fn pump_requests(
    poll: Res<StorePoll>,
    mut buys: MessageReader<PurchaseRequest>,
    mut restores: MessageReader<RestoreRequest>,
) {
    if !poll.inited {
        return;
    }
    for buy in buys.read() {
        purchase(&buy.0);
    }
    for _ in restores.read() {
        restore();
    }
}

/// Drain the polled backend state into resources + messages.
fn poll_store(
    mut poll: ResMut<StorePoll>,
    mut store_products: ResMut<StoreProducts>,
    mut entitlements: ResMut<Entitlements>,
    mut products_updated: MessageWriter<ProductsUpdated>,
    mut purchase_completed: MessageWriter<PurchaseCompleted>,
    mut entitlements_changed: MessageWriter<EntitlementsChanged>,
) {
    if !poll.inited {
        return;
    }

    let state = products_state();
    if state != poll.last_products {
        poll.last_products = state;
        store_products.state = state;
        if state == ProductsState::Ready {
            store_products.items = products();
        }
        products_updated.write(ProductsUpdated);
    }

    if let Some((outcome, product_id)) = purchase_result() {
        if !product_id.is_empty() {
            purchase_completed.write(PurchaseCompleted {
                product_id,
                outcome,
            });
        }
        purchase_clear();
    }

    let rev = entitlements_rev();
    if rev != poll.ent_rev {
        poll.ent_rev = rev;
        entitlements.owned = fetch_entitlements().into_iter().collect();
        entitlements_changed.write(EntitlementsChanged);
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn buy_once(mut requests: MessageWriter<PurchaseRequest>, mut fired: Local<bool>) {
        if !*fired {
            *fired = true;
            requests.write(PurchaseRequest("com.test.removeads".into()));
        }
    }

    /// Drives the whole plugin against the fake backend: config -> products
    /// load -> purchase request -> entitlement granted.
    #[test]
    fn fake_purchase_flow_grants_entitlement() {
        let mut app = App::new();
        app.add_plugins(MinimalPlugins);
        app.add_plugins(StorePlugin);
        app.insert_resource(StoreConfig {
            product_ids: vec!["com.test.removeads".into()],
        });
        app.add_systems(Update, buy_once);

        // A few ticks: init, fire request, pump, poll the result.
        for _ in 0..5 {
            app.update();
        }

        let products = app.world().resource::<StoreProducts>();
        assert_eq!(products.state, ProductsState::Ready);
        assert!(products.get("com.test.removeads").is_some());

        let entitlements = app.world().resource::<Entitlements>();
        assert!(
            entitlements.owns("com.test.removeads"),
            "purchase should have granted the entitlement"
        );
    }
}
