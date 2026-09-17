# Changelog

Notable changes to `bevy_ios_toolkit`, newest first. Follows [SemVer](https://semver.org);
format loosely [Keep a Changelog](https://keepachangelog.com). Entries begin
from the point this file was added — earlier releases live in the crates.io
version history and the git log.

## 0.7.0 — Unreleased

### Changed
- `ads` includes ATT and automatically requests permission when ad configuration
  is inserted, waiting for an active foreground window. Existing decisions skip
  the prompt; interrupted requests retry and concurrent calls share one request.
- Ad startup waits for ATT; Mobile Ads initialization and ad requests also wait
  for UMP readiness. Early ad commands report failures rather than reaching the
  SDK. Early consent requests survive the ATT wait.
- `AdmobState::initialized` means the consent bridge is configured after ATT;
  Mobile Ads itself starts only when UMP permits ads.

### Upgrading from 0.6
- Use matching 0.7 Rust and Swift versions. The Swift `Ads` product includes `Att`.
- Add `NSUserTrackingUsageDescription` for any app using ads and remove local
  automatic ATT loops. `AdTrackingPrompt::Manual` permits custom prompt timing
  but still requires a resolved ATT decision before ad startup.
- Omit `AdmobConfig` for captures without prompts or ads; wait for
  `AdmobState::can_request_ads` before sending ad commands.

## 0.6.0

### Added
- Expose `AdmobState::banner_height` as the mounted banner's height in UIKit
  points, including reserved space before an ad fills.
- Add `platform::screen_size()` for startup screen and current window bounds
  in points, with cached, non-blocking reads from worker threads.
- Provide `BevyIosToolkitSceneDelegate` and attach winit windows to UIKit scenes,
  including windows created before the scene connects.

### Changed
- Require an explicit `RequestAppStoreEnvironment` message to query the App
  Store environment. Reading the resource leaves it pending; product loading
  and entitlement reconciliation still start independently at launch.
- Set the minimum supported iOS version to 26.0. Native integration tests pass
  on iOS 26.2 iPhone and iPad, and iOS 27 iPhone.
- Use repository-owned CI workflows and checked-in dependency lockfiles. Run the
  portable checks with `bin/check.sh`.

### Fixed
- Size banners adaptively and derive visibility from native state, so a failed
  mount does not reserve space.
- Preserve full-window rendering and input after background/foreground with
  SDK 27 scene lifecycle support.

### Upgrading from 0.5
- Send `RequestAppStoreEnvironment` when ready for a possible sign-in prompt.
- For SDK 27 builds, enable `platform`, link the Swift `Platform` product, and
  copy the demo's `UIApplicationSceneManifest` into the app's configuration.
- Use matching 0.6 versions of the Rust crate and Swift package.

## 0.5.0 — 2026-08-26

### Added
- A `notifications` feature bridging local user notifications: ask for
  permission, schedule against a caller-owned id that replaces rather than
  stacks, cancel one or all, and read `NotificationOpened` when the app is
  opened from one. Local only — no APNs, no device token, no server, and no
  `Info.plist` key required.
- A desktop fake for the whole flow, so scheduling policy is testable off a
  device: `BEVY_IOS_FAKE_NOTIFICATIONS`, `BEVY_IOS_FAKE_NOTIFICATION_OPENED`,
  and `BEVY_IOS_FAKE_NOTIFICATIONS_REFUSE`.

## 0.4.2 — 2026-08-12

### Added
- Expose entitlement readiness as `Checking`, `Ready`, or `Failed`, and emit
  `EntitlementsChanged` for the first authoritative StoreKit snapshot even when
  the account owns no products.
- Reconcile verified entitlements on launch, foreground activation, purchase,
  restore, and transaction updates through one serialized native owner.
- Show the real localized catalogue price, entitlement readiness and ownership,
  operation state, and terminal result in the signed playground app.

### Fixed
- Prevent a stale persisted ownership decision from surviving forever because
  StoreKit's initial empty entitlement result produced no event.
- Retain the last verified ownership set when reconciliation fails, reject
  purchases until product and entitlement truth are ready, and stop malformed
  bridge payloads from silently becoming empty state.
- Return caller-owned native strings across the C boundary so a concurrent
  Swift update cannot invalidate data while Rust copies it.

## 0.4.1 — 2026-08-10

### Added
- Expose the current StoreKit purchase or restore operation through the typed
  `StoreActivity` resource so consumers can render native progress.
- Publish `RestoreCompleted` with an explicit success or failure outcome after
  the entitlement refresh finishes.

### Fixed
- Report `AppStore.sync()` failures instead of silently discarding them.
- Suppress overlapping purchase and restore requests while StoreKit is busy.

## 0.4.0 — 2026-08-09

### Added
- Expose the verified StoreKit app environment as a typed Bevy resource,
  resolved independently of purchase configuration and logged once on iOS.
- Treat unavailable, unverified, and unknown iOS environments explicitly so
  consumers can reserve production service configuration for the production
  App Store; non-iOS targets receive no App Store environment resource.
- Provide a fail-closed `is_production()` predicate for service configuration.

### Changed
- Make the `storekit` Rust surface iOS-only and remove its desktop purchase
  backend and example. Other platforms own their purchase integrations.

## 0.3.3 — 2026-08-01

### Added
- Expose UMP's authoritative `canRequestAds` result as
  `AdmobState::can_request_ads` and surface consent-info update failures as
  `ConsentInfoUpdateFailed` messages.

### Fixed
- Preserve ad readiness from cached consent when the current UMP consent-info
  refresh fails or its coarse consent status remains unknown.

## 0.3.2 — 2026-07-31

### Added
- Expose UMP's privacy-options requirement as a polled Bevy resource and present
  the privacy-options form through an explicit message from visible UI.
- Add test-ad-gated UMP geography and consent-reset configuration, with
  independently controllable desktop consent and privacy-options fakes.

### Fixed
- Treat unknown UMP consent as unresolved when deciding whether ads may load.

## 0.3.1 — 2026-07-28

### Fixed
- Give the boot-shield window a root view controller before making it visible,
  preventing the iOS 26 launch assertion while preserving the cover over an
  uninitialized Metal surface.

## 0.3.0 — 2026-07-19

### Added
- `platform::share::text(&str)` (Platform product): present the system share
  sheet (`UIActivityViewController`) with a block of text. Presents from the
  topmost view controller so it still appears with a modal up, and anchors the
  iPad popover (a nil source view traps). Fire-and-forget — the chosen activity
  and the cancel path are not reported back.
- `platform::power` (Platform product): `PowerState` — `ProcessInfo`'s thermal
  state and Low Power Mode — as a polled resource, plus a `PowerStateChanged`
  message on each transition, for Apple's adaptive-quality pattern. Swift-side
  values are cached and refreshed from the two change notifications, so a
  per-frame poll costs an atomic read. Env-tunable off iOS
  (`BEVY_IOS_FAKE_THERMAL`, `BEVY_IOS_FAKE_LOW_POWER`).
- `PlatformPlugin`, installed by `IosPlugin` when the `platform` feature is on.
  The module's fire-and-forget functions still need no plugin.

## 0.2.2 — 2026-07-11

### Added
- `platform::audio::configure(playback, mix_with_others)` (Platform product):
  own the app's `AVAudioSession`, re-asserted on every foreground return, so an
  ad SDK reconfiguring the session can't leave the game muted. The category
  choice stays the app's.
- First-frame `platform::boot_shield` — an opaque window over the winit view
  from launch until the first real frame, covering the uninitialized
  `CAMetalLayer` flash.
- Notification and selection `platform::haptics`, and four-sided
  `platform::safe_area` insets.
