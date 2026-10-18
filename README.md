# bevy_ios_toolkit

Native iOS integrations for [Bevy](https://bevyengine.org), exposed as ordinary
ECS resources and messages. One crate, one plugin, a **feature per integration**:

| feature | module | what it bridges |
|---------|--------|-----------------|
| `game` | `game` | default iOS game startup: StoreKit, AdMob, permissions, notifications |
| `storekit` | `store` | StoreKit app environment + in-app purchases |
| `ads` | `ads` | Google AdMob ads + automatic ATT + UMP consent |
| `att` | `att` | App Tracking Transparency prompt |
| `gamekit` | `gamekit` | Game Center auth, leaderboards, achievements |
| `review` | `review` | StoreKit review prompt |
| `platform` | `platform` | haptics, safe-area insets, outbound links, share sheet, thermal/low-power state |
| `notifications` | `notifications` | local user notifications (no APNs) |
| `boot-shield` | `platform::boot_shield` | `BootShieldPlugin`: lower the launch shield after real presents |
| `capture` | `capture` | desktop offscreen rendering for screenshots and recordings |

> **Status: experimental (0.9, pre-v1).** APIs will move. Live behaviour
> needs a real device, the relevant Apple/Google setup, and the matching Swift
> shim linked from the companion SPM package — see "iOS integration".

How each integration behaves (timing, retries, fakes, invariants) is documented
on its module at [docs.rs](https://docs.rs/bevy_ios_toolkit). The crate-level
page explains the native contract every module shares.

## Install

No feature is on by default. A module's `extern "C"` block exists only when its
feature is on, and the matching SPM product must be linked into the app, so a
feature you haven't wired natively fails at link time rather than at runtime.

```toml
[dependencies]
bevy_ios_toolkit = { version = "0.9", features = ["game"] }
```

## Quick start

```rust
use bevy::prelude::*;
use bevy_ios_toolkit::prelude::*;

fn main() {
    let ads = AdmobConfig::for_build()
        .with_unit(AdFormat::Banner, "YOUR_BANNER_UNIT_ID")
        .with_unit(AdFormat::Interstitial, "YOUR_INTERSTITIAL_UNIT_ID");
    App::new()
        .add_plugins((
            DefaultPlugins,
            IosGamePlugin(IosGameConfig::new(["com.example.app.removeads"], ads)),
        ))
        .run();
}

// Send once at a natural break after applying the game's ownership policy.
fn show(mut shows: MessageWriter<TryShowAd>) {
    shows.write(TryShowAd(AdFormat::Interstitial)); // unavailable inventory is skipped
}
```

Use real production ad unit IDs; the toolkit selects Google's sample IDs for
debug and simulator builds. Use `AdmobState::can_request_ads` for ad readiness
and verified `Entitlements` for ownership. Neither is a gameplay readiness gate.
The `game` module docs describe the launch sequence.

## iOS integration

The Swift shims ship as a Swift package in this repository. One Git tag pins
both halves, so pin the Rust crate and the package to the same revision.

1. Add this crate with the features you ship.
2. Add this repository as a Swift package dependency and link the matching
   products. Each product links its own system frameworks; `Ads` brings the
   Google Mobile Ads and UMP SDKs and the `Att` product. `Package.swift` owns
   the exact SDK versions: don't redeclare or override them.

   | cargo feature | SPM product |
   |---------------|-------------|
   | `platform` | `Platform` |
   | `storekit` | `Store` |
   | `ads` | `Ads` |
   | `att` | `Att` |
   | `gamekit` | `GameCenter` |
   | `review` | `Review` |
   | `notifications` | `Notifications` |

3. Per-feature setup in your app (the package ships none of it):
   - **platform**: for SDK 27 builds, copy the `UIApplicationSceneManifest`
     from `demo/ios/project.yml` into your Info.plist. Without scene adoption
     UIKit refuses to launch.
   - **ads**: set `GADApplicationIdentifier` (use `TEST_APP_ID` in
     development), `NSUserTrackingUsageDescription` and Google's
     `SKAdNetworkItems` in Info.plist, and provide an action that sends
     `PresentPrivacyOptions` while `PrivacyOptionsRequirement::is_required()`.
   - **att**: add `NSUserTrackingUsageDescription`.
   - **gamekit**: enable the Game Center capability.
   - **storekit**: define products in App Store Connect.
   - **notifications**: nothing. They are local, not push.
4. `demo/ios/` is an XcodeGen project that shows the whole wiring end to end;
   see [demo/ios/README.md](demo/ios/README.md).

## Shared game helpers

`save/` is `game_save`, a standard-library-only crate for atomic save files that
never autosave over unreadable progress. It is not on crates.io; pin it to the
same revision as the toolkit:

```toml
[dependencies]
game_save = { git = "https://github.com/marmikshah/bevy_ios_toolkit", rev = "…" }
```

The toolkit itself also carries helpers games share: `ads::UmpQa` (UMP test
geographies from launch variables), `environment::dev_var` / `dev_flag`
(development-only switches), `platform::SafeArea`, `platform::initial_resolution`,
`BootShieldPlugin` and the desktop `capture` module.

## Layout

| path | what it is |
|------|------------|
| `src/` | the crate. `lib.rs` wires `IosPlugin` and the per-feature modules; each module has an iOS backend (`extern "C"`) and, where useful, a desktop fake |
| `Sources/` | the Swift package: one target of `@_cdecl` shims per SPM product |
| `save/` | the `game_save` crate (workspace member, not published) |
| `examples/ads.rs` | desktop walkthrough of the ads fake |
| `demo/` | a button-per-feature app (its own crate and lockfile); `demo/ios/` is its Xcode shell |
| `tests/att/` | Swift tests for the ATT, UMP and prompt coordinators |
| `tests/ios/` | UIKit scene smoke test and StoreKitTest scenarios |
| `bin/` | `check.sh`, the portable checks CI runs |

A frame on iOS runs `IosSystems::Poll` (read native state), then
`IosSystems::Intents` (game policy), then `IosSystems::Dispatch` (send commands).
Native work never calls back into Rust.

## Develop

```bash
bin/check.sh                       # fmt, clippy -D warnings, tests: library and demo
bin/check.sh --help                # narrower scopes
cargo run --example ads --features ads
cargo run --manifest-path demo/Cargo.toml   # desktop demo with fakes
cargo clippy --target aarch64-apple-ios --features all,game,boot-shield --all-targets
```

CI runs `bin/check.sh` plus the native jobs in `.github/workflows/ci.yml` when a
pull request is marked ready. CI builds only `--all-features`, so after touching
`cfg` gates also run clippy with the single features you changed, and the iOS
target above.

Native checks need Xcode on a Mac:

```bash
tests/att/run.sh                                 # ATT, UMP and prompt coordination
tests/ios/run.sh BOOTED_SIMULATOR_UDID           # UIKit checks; run on iOS 26.x and 27
tests/ios/storekit.sh BOOTED_SIMULATOR_UDID      # Store bridge against StoreKitTest
```

The desktop fakes are tuned through environment variables; each module lists
its own. StoreKit has no desktop fake. On-device behaviour still needs a device
with the SDKs linked.

## Releases

Pushing a `v*` tag from `master` runs `.github/workflows/release.yml`, which
checks the tag against `Cargo.toml`, publishes to crates.io and creates the
GitHub Release from the version's `## X.Y.Z` section in
[CHANGELOG.md](CHANGELOG.md); a tag without that section fails before
publishing. Swift consumers pick up the same tag through SPM.

## Compatibility

| `bevy_ios_toolkit` | `bevy` | iOS | AdMob SDK |
|--------------------|--------|-----|-----------|
| 0.9                | 0.19   | 26.0+ | 12.14.0 (+ UMP 3.1.0), exact package pins |

## Before you rely on it

About 99% of this toolkit is AI-generated. I use it in my own projects, and if
it helps yours, great! Until v1, please treat it as experimental: read the code,
test what you use, and expect bugs and breaking changes. Contributions welcome.

## License

Released under the [MIT License](LICENSE).
