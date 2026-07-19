# Changelog

Notable changes to `bevy_ios_toolkit`, newest first. Follows [SemVer](https://semver.org);
format loosely [Keep a Changelog](https://keepachangelog.com). Entries begin
from the point this file was added — earlier releases live in the crates.io
version history and the git log.

## Unreleased

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
