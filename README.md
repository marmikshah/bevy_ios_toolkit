# bevy_ios_toolkit

Native iOS integrations for [Bevy](https://bevyengine.org), exposed as ordinary
ECS resources and messages. One crate, one plugin, a **feature per integration**:

| feature | module | what it bridges |
|---------|--------|-----------------|
| `storekit` | `store` | StoreKit 2 in-app purchases |
| `ads` | `ads` | Google AdMob ads + UMP (GDPR) consent |
| `att` | `att` | App Tracking Transparency prompt |
| `gamekit` | `gamekit` | Game Center auth, leaderboards, achievements |
| `review` | `review` | StoreKit review prompt |
| `platform` | `platform` | haptics, safe-area insets, outbound links, share sheet, thermal/low-power state |

> **Status: experimental (0.1, pre-release).** APIs will move. Live behaviour
> needs a real device, the relevant Apple/Google setup, and the matching Swift
> shim linked from the companion SPM package — see "iOS integration". Everything
> is fully exercisable on desktop first via the built-in fakes.

## How it works

Every module shares one native contract:

- Each native entry point is `@_cdecl` C-ABI, called **from Rust**.
- The SDKs' async, delegate-driven work surfaces as **polled state** (or a
  drained event queue) read once per frame — *never* callbacks into Rust,
  because re-entrancy against winit's event loop is not safe.
- Each Swift shim (an SPM product, see "iOS integration") sits behind
  `#if canImport(...)` with linking stubs, so the staticlib links on any target.

Off iOS every module is a **stateful, env-tunable fake**, so the whole app flow —
purchases, ads, rewards, consent, ATT, Game Center — runs on `cargo run` desktop
builds with no device.

## Features are opt-in for a reason

No feature is enabled by default. A module's `extern "C"` block only exists when
its feature is on, and the matching SPM product must be linked into your app — so
enabling a feature you haven't wired natively fails loudly at link time instead
of misbehaving at runtime.

```toml
bevy_ios_toolkit = { version = "0.2", features = ["storekit", "ads", "att"] }
```

## Quick start

```rust
use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

fn main() {
    App::new()
        .add_plugins((DefaultPlugins, IosPlugin))
        .insert_resource(AdmobConfig::test_ads())          // `ads`
        .insert_resource(StoreConfig {                     // `storekit`
            product_ids: vec!["com.example.app.removeads".into()],
        })
        .run();
}

// Show an interstitial once it's loaded.
fn show(inv: Res<AdInventory>, mut shows: MessageWriter<ShowAd>) {
    if inv.is_loaded(AdFormat::Interstitial) {
        shows.write(ShowAd(AdFormat::Interstitial));
    }
}

// Gate features on ownership (covers purchase, restore, relaunch).
fn gate(entitlements: Res<Entitlements>) {
    if entitlements.owns("com.example.app.removeads") { /* hide ads */ }
}
```

See [`demo/`](demo/) for a button-per-feature app that runs on desktop and iOS.

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
     in dev) and add the `SKAdNetworkItems` Google ships.
   - **att** — add `NSUserTrackingUsageDescription` to `Info.plist`.
   - **gamekit** — enable the Game Center capability.
   - **storekit** — define products in App Store Connect (or a StoreKit config).
4. The `demo/ios/` XcodeGen project shows the whole wiring end to end — it
   consumes the package by relative path.

## Testing

```bash
cargo test --features all
cargo run --example store --features storekit
cargo run --example ads   --features ads
```

The fakes are env-tunable (force no-fill, show-failures, consent prompts, ATT
outcomes, Game Center sign-out) — each module documents its knobs. On-device
behaviour must be validated in Xcode with the SDKs linked; this crate can't
simulate Apple's or Google's servers.

## Compatibility

| `bevy_ios_toolkit` | `bevy` | iOS | AdMob SDK |
|--------------------|--------|-----|-----------|
| 0.2                | 0.19   | 16+ | 12.x (+ UMP 2.x) |

## Authorship

Much of this project — the Rust crate, the Swift bridges, its tests, and these
docs — was written by **Claude Opus 4.8** (Anthropic) under human direction and
review. It ships with a passing test suite and a runnable demo, but it's young
(0.x): read the code, run your own tests, and validate the native iOS paths on a
real device before relying on it in production. Bug reports and PRs welcome.

## License

Released under the [MIT License](LICENSE).
