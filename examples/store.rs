//! Desktop walkthrough of the store flow against the built-in fake backend.
//!
//! ```text
//! cargo run --example store
//! BEVY_IOS_FAKE_OWNED=com.example.removeads cargo run --example store
//! BEVY_IOS_FAKE_CANCEL=1 cargo run --example store
//! ```
//!
//! It loads two products, buys one, prints the outcome + entitlements, exits.

use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

fn main() {
    App::new()
        .add_plugins((MinimalPlugins, IosPlugin))
        .insert_resource(StoreConfig {
            product_ids: vec!["com.example.removeads".into(), "com.example.coins".into()],
        })
        .add_systems(
            Update,
            (
                buy_when_ready,
                report_products.run_if(on_message::<ProductsUpdated>),
                report_purchase.run_if(on_message::<PurchaseCompleted>),
                report_entitlements.run_if(on_message::<EntitlementsChanged>),
                exit_after_settling,
            ),
        )
        .run();
}

fn buy_when_ready(
    products: Res<StoreProducts>,
    entitlements: Res<Entitlements>,
    mut requests: MessageWriter<PurchaseRequest>,
    mut done: Local<bool>,
) {
    if *done || products.state != ProductsState::Ready {
        return;
    }
    *done = true;
    if entitlements.owns("com.example.removeads") {
        println!("already owned com.example.removeads (env pre-grant)");
        return;
    }
    println!("buying com.example.removeads");
    requests.write(PurchaseRequest("com.example.removeads".into()));
}

fn report_products(products: Res<StoreProducts>) {
    println!("products [{:?}]:", products.state);
    for p in &products.items {
        println!("  {} — {} ({})", p.id, p.display_price, p.display_name);
    }
}

fn report_purchase(mut completed: MessageReader<PurchaseCompleted>) {
    for c in completed.read() {
        println!("purchase {} -> {:?}", c.product_id, c.outcome);
    }
}

fn report_entitlements(entitlements: Res<Entitlements>) {
    let owned: Vec<&String> = entitlements.iter().collect();
    println!("entitlements now: {:?}", owned);
}

/// Give the flow a handful of frames to settle, then quit.
fn exit_after_settling(mut frames: Local<u32>, mut exit: MessageWriter<AppExit>) {
    *frames += 1;
    if *frames > 8 {
        exit.write(AppExit::Success);
    }
}
