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
| `notifications` | `notifications` | local user notifications (no APNs) |

> **Status: experimental (0.5, pre-release).** APIs will move. Live behaviour
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
bevy_ios_toolkit = { version = "0.5", features = ["ads", "att"] }

[target.'cfg(target_os = "ios")'.dependencies]
bevy_ios_toolkit = { version = "0.5", features = ["storekit"] }
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

// Ask once, then schedule a real event. `notifications`
fn remind(
    permission: Res<NotificationPermission>,
    mut ask: MessageWriter<RequestNotificationPermission>,
    mut notify: MessageWriter<ScheduleNotification>,
) {
    if !permission.is_determined() {
        ask.write(RequestNotificationPermission);
    } else if permission.can_deliver() {
        // The same id replaces rather than stacks, so this is idempotent.
        notify.write(ScheduleNotification {
            id: "restock".into(),
            title: "THE VAN CAME".into(),
            body: "Something new is in the machine.".into(),
            after: std::time::Duration::from_secs(4 * 60 * 60),
        });
    }
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

// StoreKit first resolves Checking -> Ready or Failed. Keep purchases, ads,
// and tracking closed until Ready; only then is an absent id confirmed unowned.
#[cfg(target_os = "ios")]
fn gate(entitlements: Res<Entitlements>) {
    match entitlements.state() {
        EntitlementsState::Checking | EntitlementsState::Failed => {
            /* keep purchase, ads, and tracking closed */
        }
        EntitlementsState::Ready
            if entitlements.owns("com.example.app.removeads") => { /* purchased */ }
        EntitlementsState::Ready => { /* localized offer may be shown */ }
    }
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

See [`demo/`](demo/) for desktop fakes and the complete native iOS surface.

## iOS integration

The Swift shims ship as a Swift package **in this same repo**, co-versioned with
the crate — one git tag pins both halves, which keeps the `@_cdecl` ↔ `extern "C"`
contract in lockstep. You link only the products for the features you ship; there
are no files to vendor or keep in sync by hand.

1. Add this crate with the features you ship.
2. Add this repo as a Swift package dependency and link the matching products.
   For SDK 27 builds, enable `platform`, link `Platform`, and copy the
   `UIApplicationSceneManifest` from `demo/ios/project.yml` into your app's
   Info.plist configuration. It names `BevyIosToolkitSceneDelegate`; without
   scene adoption UIKit refuses to launch. `IosPlugin` registers it before run.
   Each product links its own system frameworks; `Ads` brings the Google Mobile
   Ads + UMP SDKs transitively. The symbol prefixes (`store_`, `platform_`, `admob_`,
   `att_`, `gamekit_`, `review_`, `notifications_`) won't collide with your own
   bridge.

   | cargo feature | SPM product |
   |---------------|-------------|
   | `platform` | `Platform` |
   | `storekit` | `Store` |
   | `ads` | `Ads` |
   | `att` | `Att` |
   | `gamekit` | `GameCenter` |
   | `review` | `Review` |
   | `notifications` | `Notifications` |

3. Per-feature native setup (stays in your app — the package ships none of it):
   - **ads** — set `GADApplicationIdentifier` in `Info.plist` (use `TEST_APP_ID`
     in dev), add the `SKAdNetworkItems` Google ships, and provide a visible
     action that sends `PresentPrivacyOptions` whenever the
     `PrivacyOptionsRequirement` resource is `Required`.
   - **att** — add `NSUserTrackingUsageDescription` to `Info.plist`.
   - **gamekit** — enable the Game Center capability.
   - **storekit** — define products in App Store Connect (or a StoreKit config).
   - **notifications** — nothing. No `Info.plist` key, no entitlement, no
     capability: these are local notifications, not push. To catch one that
     *launched* the app, call `notifications_install_delegate()` from your app
     delegate — see below.
4. The `demo/ios/` XcodeGen project shows the whole wiring end to end — it
   consumes the package by relative path.

With `platform`, call `platform::screen_size()` on the main thread before
building `WindowPlugin` to size its initial window in **points**. After launch,
it returns the key window's bounds, including resized iPad windows.

On iOS, send `RequestAppStoreEnvironment` when ready for a possible Apple Account
sign-in sheet. Reading `AppStoreEnvironment` leaves it `Pending` until requested;
`StoreConfig` still starts product and entitlement loading independently.
The Swift bridge logs its terminal value once. The resource is not inserted on
non-iOS targets; they do not need an App Store classification. Apple reports
TestFlight as `Sandbox`, but sandbox is not a reliable distinction between
TestFlight and every development install. Use the resource for runtime service
or ad-unit selection, but keep build-time values such as
`GADApplicationIdentifier` in the app target configuration.

`StoreProducts::get(id).display_price` is StoreKit's localized
`Product.displayPrice`; never replace it with a hard-coded production price.
`Entitlements` begins in `Checking` and publishes `EntitlementsChanged` for its
first verified snapshot even when the result is empty. Keep purchase, ads, and
tracking closed until `EntitlementsState::Ready`; `Failed` retains the last
verified ownership set but is not permission to infer that an absent id is
unowned. Consumables do not appear in `Entitlements`. Do not persist or migrate
ownership in a game save. The toolkit
reconciles at launch, foreground activation, purchase, restore, and verified
transaction updates.

`StoreActivity` reports whether a purchase or explicit restore is in flight.
Use it to disable duplicate actions and keep progress visible until
`PurchaseCompleted` or `RestoreCompleted` arrives. Ownership never comes from
the operation result: always read `Entitlements`, including during launch-time
reconciliation when no user-facing success message should be inferred.

### Notifications are local, and for a real event

There is no remote push here: no APNs, no device token, no server, and nothing
that counts as data collection on a privacy manifest.

Ids are yours and **replace** rather than stack — a timer that slips must never
leave two notifications queued, which also makes rescheduling idempotent.
`PendingNotifications` records what this app has asked for *since it launched*;
it is not the system's queue, so when the two could disagree, believe the system.

Opens are captured by a `UNUserNotificationCenterDelegate` installed on the
first update. iOS delivers the response for a notification that *launched* the
app very early — possibly before Bevy's first frame — and a response delivered
with no delegate set is gone. If catching that matters, call the shim's
installer from your own app delegate first; it is idempotent, and anything
buffered beforehand is drained on the first update:

```swift
@_silgen_name("notifications_install_delegate")
func notificationsInstallDelegate()

func application(_: UIApplication, didFinishLaunchingWithOptions _: ...) -> Bool {
    notificationsInstallDelegate()
    return true
}
```

A notification API makes the daily-nag pattern very easy to build. Send one
because something the player asked to know about actually happened — never to
drag them back. Restraint is yours to enforce, which is what the desktop fake
exists to let you test.

## Testing

```bash
cargo test --features all
cargo run --example ads   --features ads
cargo check --target aarch64-apple-ios --features storekit
tests/ios/run.sh BOOTED_SIMULATOR_UDID # UIKit scene/screen checks; use iOS 27
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
| 0.5                | 0.19   | 16+ | 12.3–12.x (+ UMP 3.x) |

## Authorship

Much of this project was written by **Claude Opus 4.8** (Anthropic) under human
direction and review. It is young (0.x): run the tests and validate native iOS
paths on a real device before relying on it in production. Bug reports and PRs welcome.

## License

Released under the [MIT License](LICENSE).
