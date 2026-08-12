//! StoreKit 2 in-app purchases as Bevy resources + messages.
//!
//! Flow:
//! 1. On iOS, read [`AppStoreEnvironment`] to choose non-production service
//!    configuration for Xcode, sandbox, or unavailable execution. Wait while
//!    it is [`AppStoreEnvironment::Pending`]. The resource is absent off iOS.
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

use std::ffi::{CStr, CString, c_char};

pub use crate::store_environment::AppStoreEnvironment;
use crate::store_environment::environment_from_raw;
use bevy::prelude::*;

#[cfg(not(test))]
#[path = "backend_ios.rs"]
mod backend;
#[cfg(test)]
#[path = "backend_test.rs"]
mod backend;
mod operation;
mod state;

pub use operation::StoreActivity;
use operation::{finish_activity, start_purchase, start_restore};
use state::decode_entitlement_snapshot;
pub use state::{Entitlements, EntitlementsState, ProductInfo, ProductsState, StoreProducts};

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

fn fetch_entitlements() -> Result<state::EntitlementSnapshot, serde_json::Error> {
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
        app.init_resource::<StoreProducts>()
            .init_resource::<Entitlements>()
            .init_resource::<StoreActivity>()
            .init_resource::<StorePoll>()
            .add_message::<PurchaseRequest>()
            .add_message::<RestoreRequest>()
            .add_message::<ProductsUpdated>()
            .add_message::<PurchaseCompleted>()
            .add_message::<RestoreCompleted>()
            .add_message::<EntitlementsChanged>()
            .add_systems(
                Update,
                (
                    init_once,
                    poll_products,
                    poll_entitlements,
                    pump_requests,
                    poll_operations,
                )
                    .chain(),
            );

        app.init_resource::<AppStoreEnvironment>()
            .add_systems(Update, (init_environment_once, poll_environment).chain());
    }
}

/// Start environment resolution even when the consumer has no products.
fn init_environment_once(mut initialized: Local<bool>) {
    if *initialized {
        return;
    }
    init_environment();
    *initialized = true;
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
fn pump_requests(
    poll: Res<StorePoll>,
    products: Res<StoreProducts>,
    entitlements: Res<Entitlements>,
    mut activity: ResMut<StoreActivity>,
    mut buys: MessageReader<PurchaseRequest>,
    mut restores: MessageReader<RestoreRequest>,
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

#[cfg(test)]
mod tests {
    use super::*;
    use bevy::ecs::message::Messages;

    fn app_with_config() -> App {
        backend::reset();
        let mut app = App::new();
        app.insert_resource(StoreConfig {
            product_ids: vec!["com.example.supporter".into()],
        });
        app.add_plugins(StorePlugin);
        app
    }

    #[test]
    fn first_empty_snapshot_resolves_and_emits() {
        let mut app = app_with_config();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<EntitlementsChanged>>()
            .clear();

        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        app.update();

        let entitlements = app.world().resource::<Entitlements>();
        assert!(entitlements.is_ready());
        assert!(!entitlements.owns("com.example.supporter"));
        assert_eq!(
            app.world()
                .resource::<Messages<EntitlementsChanged>>()
                .len(),
            1
        );
    }

    #[test]
    fn owned_snapshot_then_revocation_are_distinct_events() {
        let mut app = app_with_config();
        app.update();
        app.world_mut()
            .resource_mut::<Messages<EntitlementsChanged>>()
            .clear();

        backend::publish_entitlements(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        );
        app.update();
        assert!(
            app.world()
                .resource::<Entitlements>()
                .owns("com.example.supporter")
        );

        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        app.update();
        assert!(
            !app.world()
                .resource::<Entitlements>()
                .owns("com.example.supporter")
        );
        assert_eq!(
            app.world()
                .resource::<Messages<EntitlementsChanged>>()
                .len(),
            2
        );
    }

    #[test]
    fn malformed_payload_fails_without_revoking_verified_ownership() {
        let mut app = app_with_config();
        app.update();
        backend::publish_entitlements(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        );
        app.update();

        backend::publish_entitlements("not-json");
        app.update();

        let entitlements = app.world().resource::<Entitlements>();
        assert_eq!(entitlements.state(), EntitlementsState::Failed);
        assert!(entitlements.owns("com.example.supporter"));
    }

    #[test]
    fn failed_snapshot_carries_last_verified_ownership_across_skipped_revisions() {
        let mut app = app_with_config();
        app.update();

        backend::publish_entitlements(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        );
        backend::publish_entitlements(
            r#"{"state":"failed","product_ids":["com.example.supporter"]}"#,
        );
        app.update();

        let entitlements = app.world().resource::<Entitlements>();
        assert_eq!(entitlements.state(), EntitlementsState::Failed);
        assert!(entitlements.owns("com.example.supporter"));
    }

    #[test]
    fn failed_initial_read_can_recover_to_authoritative_empty() {
        let mut app = app_with_config();
        app.update();

        backend::publish_entitlements(r#"{"state":"failed","product_ids":[]}"#);
        app.update();
        assert_eq!(
            app.world().resource::<Entitlements>().state(),
            EntitlementsState::Failed
        );

        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        app.update();

        let entitlements = app.world().resource::<Entitlements>();
        assert!(entitlements.is_ready());
        assert!(!entitlements.owns("com.example.supporter"));
    }

    #[test]
    fn purchase_is_rejected_until_store_truth_is_ready() {
        let mut app = app_with_config();
        app.update();
        app.world_mut()
            .write_message(PurchaseRequest("com.example.supporter".into()));
        app.update();

        assert_eq!(backend::purchase_calls(), 0);
        assert_eq!(
            app.world().resource::<Messages<PurchaseCompleted>>().len(),
            1
        );
        let completed = app
            .world_mut()
            .resource_mut::<Messages<PurchaseCompleted>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(completed.outcome, PurchaseOutcome::Failed);
    }

    #[test]
    fn malformed_product_payload_fails_instead_of_becoming_ready_empty() {
        let mut app = app_with_config();
        app.update();

        backend::publish_products("not-json");
        app.update();

        let products = app.world().resource::<StoreProducts>();
        assert_eq!(products.state, ProductsState::Failed);
        assert!(products.items.is_empty());
    }

    #[test]
    fn purchase_requires_localized_product_and_confirmed_unowned_snapshot() {
        let mut app = app_with_config();
        app.update();
        backend::publish_products(
            r#"[{"id":"com.example.supporter","display_name":"Supporter","display_price":"HK$ 18.00","description":"Thank you"}]"#,
        );
        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        app.update();

        assert_eq!(
            app.world()
                .resource::<StoreProducts>()
                .get("com.example.supporter")
                .unwrap()
                .display_price,
            "HK$ 18.00"
        );
        app.world_mut()
            .write_message(PurchaseRequest("com.example.supporter".into()));
        app.update();
        assert_eq!(backend::purchase_calls(), 1);
    }

    #[test]
    fn purchase_is_rejected_when_product_is_already_owned() {
        let mut app = app_with_config();
        app.update();
        backend::publish_products(
            r#"[{"id":"com.example.supporter","display_name":"Supporter","display_price":"HK$ 18.00","description":"Thank you"}]"#,
        );
        backend::publish_entitlements(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        );
        app.update();

        app.world_mut()
            .write_message(PurchaseRequest("com.example.supporter".into()));
        app.update();

        assert_eq!(backend::purchase_calls(), 0);
        let completed = app
            .world_mut()
            .resource_mut::<Messages<PurchaseCompleted>>()
            .drain()
            .next()
            .unwrap();
        assert_eq!(completed.outcome, PurchaseOutcome::Failed);
    }

    #[test]
    fn duplicate_purchase_while_busy_does_not_emit_a_false_failure() {
        let mut app = app_with_config();
        app.update();
        backend::publish_products(
            r#"[{"id":"com.example.supporter","display_name":"Supporter","display_price":"HK$ 18.00","description":"Thank you"}]"#,
        );
        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        app.update();

        app.world_mut()
            .write_message(PurchaseRequest("com.example.supporter".into()));
        app.update();
        assert_eq!(backend::purchase_calls(), 1);

        app.world_mut()
            .write_message(PurchaseRequest("com.example.supporter".into()));
        app.update();

        assert_eq!(backend::purchase_calls(), 1);
        assert!(
            app.world()
                .resource::<Messages<PurchaseCompleted>>()
                .is_empty()
        );
        assert!(!app.world().resource::<StoreActivity>().is_idle());
    }

    #[test]
    fn purchase_completion_observes_entitlement_from_the_same_poll() {
        let mut app = app_with_config();
        app.update();
        backend::publish_products(
            r#"[{"id":"com.example.supporter","display_name":"Supporter","display_price":"€1.99","description":"Thank you"}]"#,
        );
        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        app.update();
        app.world_mut()
            .write_message(PurchaseRequest("com.example.supporter".into()));
        app.update();
        backend::publish_entitlements(
            r#"{"state":"ready","product_ids":["com.example.supporter"]}"#,
        );
        backend::publish_purchase_success("com.example.supporter");
        app.update();

        assert!(
            app.world()
                .resource::<Entitlements>()
                .owns("com.example.supporter")
        );
        assert_eq!(
            app.world().resource::<Messages<PurchaseCompleted>>().len(),
            1
        );
        assert!(app.world().resource::<StoreActivity>().is_idle());
    }

    #[test]
    fn restore_empty_resolves_before_completion() {
        let mut app = app_with_config();
        app.update();
        app.world_mut().write_message(RestoreRequest);
        app.update();
        backend::publish_entitlements(r#"{"state":"ready","product_ids":[]}"#);
        backend::publish_restore_success();
        app.update();

        assert!(app.world().resource::<Entitlements>().is_ready());
        assert_eq!(
            app.world().resource::<Messages<RestoreCompleted>>().len(),
            1
        );
        assert!(app.world().resource::<StoreActivity>().is_idle());
    }
}
