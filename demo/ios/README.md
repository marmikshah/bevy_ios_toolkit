# iOS Toolkit Demo — iOS shell

The native wrapper that turns the `bevy_ios_toolkit_demo` Rust staticlib into a
real iOS app, so each toolkit feature can be tried on a device or simulator.

> **⚠️ Two one-time manual steps are required before the iOS app works** — see
> "Manual setup" below. The desktop demo omits StoreKit and needs neither.

## Manual setup (required)

Two things only you can provide; the demo can't ship them for you.

**1. Set your Apple Developer Team ID.** `DEVELOPMENT_TEAM` in `project.yml` is
empty by default, so the build fails with a code-signing error. Pass your team
to `xcodebuild` or select it in Xcode. The committed bundle identifier belongs
to the published playground app and must remain unchanged for live StoreKit.

**2. Provide the in-app purchase product.** The "Buy: Remove Ads" button requests
`iap.playground.removeads`, which is configured for the published playground
app. Run the signed app on a physical device with a sandbox account. Do not add
a StoreKit configuration file: this app exists to verify the real App Store
catalogue, localized price, purchase, relaunch, foreground, and restore flows.

Everything else runs without setup: ads use Google's **test** ids (they fill
without risking a policy strike), and ATT / haptics / review need nothing. Game
Center *sign-in* works as-is; submitting to `lb.demo.highscore` /
`ach.demo.first_tap` additionally needs those created in App Store Connect.

## Layout

| file | role |
|------|------|
| `project.yml` | XcodeGen project: target, signing, Info.plist, and the toolkit SPM package (consumed from `../..` by relative path) |
| `build_rust.sh` | Xcode pre-build phase — `cargo build` the staticlib, stage it into `rustlib/$(PLATFORM_NAME)` |
| `main.m` | C entry point; calls the Rust `main_rs` symbol |
| `IosToolkitDemo.entitlements` | Game Center capability |
| `PrivacyInfo.xcprivacy` | privacy manifest (advertising id + required-reason APIs) |

## Prerequisites

- Xcode + an Apple Developer team (StoreKit, Game Center and ads need a signed
  app — see "Manual setup" above).
- `xcodegen` and the iOS Rust targets:
  ```bash
  brew install xcodegen
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim
  ```
- The AdMob + UMP SDKs resolve automatically via Swift Package Manager on first
  build (declared in `project.yml`).

## Run

From the repository root:

```bash
cargo run --manifest-path demo/Cargo.toml --bin demo
cd demo/ios
xcodegen generate
open IosToolkitDemo.xcodeproj
```

Pick a device or Simulator in Xcode and run the generated `IosToolkitDemo`
scheme. For an unsigned command-line Simulator build, use:

```bash
xcodebuild -project IosToolkitDemo.xcodeproj \
  -scheme IosToolkitDemo \
  -configuration Debug \
  -destination 'generic/platform=iOS Simulator' \
  CODE_SIGNING_ALLOWED=NO \
  build
```

## What the buttons do

One button per feature; the status line at the top reflects the live App Store
environment, localized price, entitlement readiness and ownership, StoreKit
activity and terminal result, plus the other toolkit integrations:

- **Buy: Remove Ads** — StoreKit purchase of `iap.playground.removeads`.
- **Restore Purchases** — explicit App Store synchronization with visible
  in-flight state.
- **Check App Store Environment** — explicitly resolve the app transaction;
  this can ask for an Apple Account. Ownership still reconciles at launch.
- **Interstitial Ad** / **Rewarded Ad** — load *and* present from one tap.
- **Toggle Banner** — show / hide the banner.
- **Request Ad Consent** — UMP consent form.
- **Privacy Options** — reopens UMP choices when the SDK requires the entry point.
- **Request Tracking (ATT)** — the App Tracking Transparency prompt.
- **Haptic Tap** / **Ask for Review** — impact haptic / review prompt.
- **Game Center** — first tap signs in; once signed in, a tap submits a score +
  achievement and opens the dashboard.

For a real (non-test) build, swap the test ad ids for your own and turn off
`AdmobConfig::use_test_ads`.

## Simulator regression checks

After building and installing the demo, run `tests/ios/demo.sh SIMULATOR_UDID`
from the repository root. It checks rendered controls, deferred environment
resolution, mounted banner height, touch input, and background/foreground
recovery. Screenshots and XCTest results go in `target/`. Run on both iOS 26.x
and iOS 27; the same SDK 27 build must work on both. `tests/ios/run.sh` separately
checks scene attachment and startup/window measurements without the ad SDK.
StoreKit uses Marmik's Playground (`com.marmikshah.playground`) and its live
`iap.playground.removeads` product. Consent follows the device's saved choices
and Google's response for its location. Authenticated purchase and restore
checks use this same App Store Connect listing on a signed device build.
