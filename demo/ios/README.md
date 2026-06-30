# iOS Toolkit Demo — iOS shell

The native wrapper that turns the `bevy_ios_toolkit_demo` Rust staticlib into a
real iOS app, so each toolkit feature can be tried on a device or simulator.

> **⚠️ Two one-time manual steps are required before the iOS app works** — see
> "Manual setup" below. (Desktop — `make run` — needs neither.)

## Manual setup (required)

Two things only you can provide; the demo can't ship them for you.

**1. Set your Apple Developer Team ID.** `DEVELOPMENT_TEAM` in `project.yml` is
empty by default, so the build fails with a code-signing error (*"Signing for
'IosToolkitDemo' requires a development team"*) — including on the Simulator,
because the Game Center entitlement forces signing. Set `DEVELOPMENT_TEAM` to
your team id in `project.yml` and re-run `make xcodeproj` (or pass
`DEVELOPMENT_TEAM=XXXXXXXXXX` to `xcodebuild`). You'll likely also change
`PRODUCT_BUNDLE_IDENTIFIER` to a bundle id your team owns.

**2. Provide the in-app purchase product.** The "Buy: Remove Ads" button requests
`iap.playground.removeads`; StoreKit returns nothing until that id exists. Either:
- add a **StoreKit configuration file** to the scheme (Product → Scheme → Edit
  Scheme → Run → Options → StoreKit Configuration) with a non-consumable product
  `iap.playground.removeads` — works on the Simulator, no account needed; **or**
- create the product in **App Store Connect** and test with a sandbox account on
  a device.

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

From the demo crate root (`bevy_ios_toolkit/demo/`):

```bash
make run         # desktop, against the fakes — no Xcode needed
make xcode       # generate the project and open it in Xcode (pick a device, Run)
make simulator   # build + install + launch on a booted Simulator
```

## What the buttons do

One button per feature; the status line at the top reflects live state
(entitlement owned, ad inventory, banner, consent, ATT status, Game Center auth):

- **Buy: Remove Ads** — StoreKit purchase of `iap.playground.removeads`.
- **Interstitial Ad** / **Rewarded Ad** — load *and* present from one tap.
- **Toggle Banner** — show / hide the banner.
- **Request Ad Consent** — UMP consent form.
- **Request Tracking (ATT)** — the App Tracking Transparency prompt.
- **Haptic Tap** / **Ask for Review** — impact haptic / review prompt.
- **Game Center** — first tap signs in; once signed in, a tap submits a score +
  achievement and opens the dashboard.

For a real (non-test) build, swap the test ad ids for your own and turn off
`AdmobConfig::use_test_ads`.
