# bevy_ios_toolkit

Native iOS integrations for [Bevy](https://bevyengine.org), exposed as ordinary
ECS resources and messages. One crate, one plugin, a **feature per integration**:

| feature | module | what it bridges |
|---------|--------|-----------------|
| `storekit` | `store` | StoreKit app environment + in-app purchases |
| `ads` | `ads` | Google AdMob ads + UMP (GDPR) consent |
| `att` | `att` | App Tracking Transparency prompt |
| `gamekit` | `gamekit` | Game Center auth, leaderboards, achievements |
| `review` | `review` | StoreKit review prompt |
| `platform` | `platform` | haptics, safe-area insets, outbound links, share sheet, thermal/low-power state |

> **Status: experimental (0.4, pre-release).** APIs will move. Live behaviour
> needs a real device, the relevant Apple/Google setup, and the matching Swift
> shim linked from the companion SPM package — see "iOS integration".

## How it works

Every module shares one native contract:

- Each native entry point is `@_cdecl` C-ABI, called **from Rust**.
- The SDKs' async, delegate-driven work surfaces as **polled state** (or a
  drained event queue) read once per frame — *never* callbacks into Rust,
  because re-entrancy against winit's event loop is not safe.
- Each Swift shim (an SPM product, see "iOS integration") sits behind
  `#if canImport(...)` with linking stubs, so the staticlib links on any target.

Off iOS, integrations keep a stateful fake only when there is a meaningful
cross-platform flow to exercise. StoreKit is different: purchases belong to the
platform store, so the `store` module exists only on iOS and has no desktop
backend.

## Features are opt-in for a reason

No feature is enabled by default. A module's `extern "C"` block only exists when
its feature is on, and the matching SPM product must be linked into your app — so
enabling a feature you haven't wired natively fails loudly at link time instead
of misbehaving at runtime.

```toml
[dependencies]
bevy_ios_toolkit = { version = "0.4", features = ["ads", "att"] }

[target.'cfg(target_os = "ios")'.dependencies]
bevy_ios_toolkit = { version = "0.4", features = ["storekit"] }
```

## Quick start

```rust
use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

fn main() {
    let mut app = App::new();
    app.add_plugins((DefaultPlugins, IosPlugin))
        .insert_resource(AdmobConfig::test_ads());         // `ads`

    #[cfg(target_os = "ios")]
    app.insert_resource(StoreConfig {                      // `storekit`
        product_ids: vec!["com.example.app.removeads".into()],
    });

    app.run();
}

// Show an interstitial once it's loaded.
fn show(inv: Res<AdInventory>, mut shows: MessageWriter<ShowAd>) {
    if inv.is_loaded(AdFormat::Interstitial) {
        shows.write(ShowAd(AdFormat::Interstitial));
    }
}

// UMP owns the authoritative readiness decision. Do not infer it from the
// coarse consent status; cached consent may remain usable after an update error.
fn ads_ready(admob: Res<AdmobState>) {
    if admob.can_request_ads { /* ad requests are permitted */ }
}

// Gate features on ownership (covers purchase, restore, relaunch).
#[cfg(target_os = "ios")]
fn gate(entitlements: Res<Entitlements>) {
    if entitlements.owns("com.example.app.removeads") { /* hide ads */ }
}

// Keep progress visible while StoreKit is working. Presentation stays yours.
#[cfg(target_os = "ios")]
fn store_progress(activity: Res<StoreActivity>) {
    match &*activity {
        StoreActivity::Idle => { /* enable store actions */ }
        StoreActivity::Purchasing { product_id } => { /* show purchase progress */ }
        StoreActivity::Restoring => { /* show restore progress */ }
    }
}

// Restore completes only after AppStore.sync() and entitlement refresh finish.
#[cfg(target_os = "ios")]
fn restore_result(mut completed: MessageReader<RestoreCompleted>) {
    for completed in completed.read() {
        match completed.outcome {
            RestoreOutcome::Success => { /* read Entitlements for restored access */ }
            RestoreOutcome::Failed => { /* offer an explicit retry */ }
        }
    }
}

// Wait for StoreKit, then use production service configuration only for the
// production App Store. Xcode, TestFlight/sandbox, and failures stay on test.
#[cfg(target_os = "ios")]
fn choose_service_configuration(environment: Res<AppStoreEnvironment>) {
    if !environment.is_resolved() {
        return; // wait before initializing the SDK
    }
    if environment.is_production() {
        /* insert production config */
    } else {
        /* insert test config */
    }
}

// Keep this action visible only when UMP requires it. Send
// `PresentPrivacyOptions` from the user's tap to reopen their choices.
fn privacy_entry_point(requirement: Res<PrivacyOptionsRequirement>) {
    let visible = *requirement == PrivacyOptionsRequirement::Required;
    /* project `visible` into your UI */
}
```

See [`demo/`](demo/) for the supported fake integrations on desktop and the
complete native surface on iOS.

## iOS integration

The Swift shims ship as a Swift package **in this same repo**, co-versioned with
the crate — one git tag pins both halves, which keeps the `@_cdecl` ↔ `extern "C"`
contract in lockstep. You link only the products for the features you ship; there
are no files to vendor or keep in sync by hand.

1. Add this crate with the features you ship.
2. Add this repo as a Swift package dependency and link the matching products.
   Each product links its own system frameworks; `Ads` brings the Google Mobile
   Ads + UMP SDKs transitively. The symbol prefixes (`store_`, `platform_`, `admob_`,
   `att_`, `gamekit_`, `review_`) won't collide with your own bridge.

   | cargo feature | SPM product |
   |---------------|-------------|
   | `platform` | `Platform` |
   | `storekit` | `Store` |
   | `ads` | `Ads` |
   | `att` | `Att` |
   | `gamekit` | `GameCenter` |
   | `review` | `Review` |

3. Per-feature native setup (stays in your app — the package ships none of it):
   - **ads** — set `GADApplicationIdentifier` in `Info.plist` (use `TEST_APP_ID`
     in dev), add the `SKAdNetworkItems` Google ships, and provide a visible
     action that sends `PresentPrivacyOptions` whenever the
     `PrivacyOptionsRequirement` resource is `Required`.
   - **att** — add `NSUserTrackingUsageDescription` to `Info.plist`.
   - **gamekit** — enable the Game Center capability.
   - **storekit** — define products in App Store Connect (or a StoreKit config).
4. The `demo/ios/` XcodeGen project shows the whole wiring end to end — it
   consumes the package by relative path.

On iOS, `AppStoreEnvironment` resolves independently of `StoreConfig` and the
Swift bridge logs its terminal value once. The resource is not inserted on
non-iOS targets; they do not need an App Store classification. Apple reports
TestFlight as `Sandbox`, but sandbox is not a reliable distinction between
TestFlight and every development install. Use the resource for runtime service
or ad-unit selection, but keep build-time values such as
`GADApplicationIdentifier` in the app target configuration.

`StoreActivity` reports whether a purchase or explicit restore is in flight.
Use it to disable duplicate actions and keep progress visible until
`PurchaseCompleted` or `RestoreCompleted` arrives. Ownership never comes from
the operation result: always read `Entitlements`, including during launch-time
reconciliation when no user-facing success message should be inferred.

## Testing

```bash
cargo test --features all
cargo run --example ads   --features ads
cargo check --target aarch64-apple-ios --features storekit
```

The applicable fakes are env-tunable (force no-fill, show-failures, consent
prompts, privacy-options requirements, ATT outcomes, Game Center sign-out) —
each module documents its knobs. StoreKit has no desktop fake. An explicit
`UmpTestConfig` makes geography testing
deterministic only while `AdmobConfig::use_test_ads` is enabled; production
configurations ignore its geography and reset fields. Simulators are test
devices automatically. For a physical device, include UMP's logged hashed test
device id in `AdmobConfig::test_device_ids`. On-device behaviour must still be
validated in Xcode with the SDKs linked.

## Compatibility

| `bevy_ios_toolkit` | `bevy` | iOS | AdMob SDK |
|--------------------|--------|-----|-----------|
| 0.4                | 0.19   | 16+ | 12.3–12.x (+ UMP 3.x) |

## Authorship

Much of this project — the Rust crate, the Swift bridges, its tests, and these
docs — was written by **Claude Opus 4.8** (Anthropic) under human direction and
review. It ships with a passing test suite and a runnable demo, but it's young
(0.x): read the code, run your own tests, and validate the native iOS paths on a
real device before relying on it in production. Bug reports and PRs welcome.

## License

Released under the [MIT License](LICENSE).
