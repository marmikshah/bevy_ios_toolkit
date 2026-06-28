# iOS Toolkit Demo — iOS shell

The native wrapper that turns the `bevy_ios_toolkit_demo` Rust staticlib into a
real iOS app, so each toolkit feature can be tried on a device or simulator.

## Layout

| file | role |
|------|------|
| `project.yml` | XcodeGen project: target, signing, Info.plist, SPM packages, Swift sources |
| `build_rust.sh` | Xcode pre-build phase — `cargo build` the staticlib, stage it into `rustlib/$(PLATFORM_NAME)` |
| `main.m` | C entry point; calls the Rust `main_rs` symbol |
| `*.swift` | the six toolkit bridges, vendored from `../../swift/` |
| `IosToolkitDemo.entitlements` | Game Center capability |
| `PrivacyInfo.xcprivacy` | privacy manifest (advertising id + required-reason APIs) |

## Prerequisites

- Xcode + an Apple Developer team (StoreKit, Game Center and ads all need a
  signed app — set `DEVELOPMENT_TEAM` in `project.yml` or pass it to xcodebuild).
- `xcodegen` and the iOS Rust targets:
  ```bash
  brew install xcodegen
  rustup target add aarch64-apple-ios aarch64-apple-ios-sim
  ```
- The AdMob + UMP SDKs resolve automatically via Swift Package Manager on first
  build (declared in `project.yml`).

## Run

From the demo crate root (`bevy_ios_toolkit/demo/`):

```bash
make run         # desktop, against the fakes — no Xcode needed
make xcode       # generate the project and open it in Xcode (pick a device, Run)
make simulator   # build + install + launch on a booted Simulator
```

## What the buttons do

Each row maps to one toolkit message/call: buy / restore (StoreKit), load &
show interstitial / rewarded, toggle banner, request consent (AdMob + UMP),
haptic tap, request tracking (ATT), ask for review, Game Center sign-in / submit
score / show dashboard. The status line at the top reflects live state
(entitlement owned, ad inventory, consent, ATT status, Game Center auth).

On device the bridges are real; the `GADApplicationIdentifier` and ad units in
`project.yml` / the app are Google's **test** ids, so ads fill without risking a
policy strike. Swap in your own ids (and turn off `AdmobConfig::use_test_ads`)
for a real build. StoreKit needs products in App Store Connect (or a local
StoreKit configuration); Game Center needs leaderboards/achievements created
with the ids the demo uses (`lb.demo.highscore`, `ach.demo.first_tap`).
