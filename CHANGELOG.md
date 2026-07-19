# Changelog

Notable changes to `bevy_ios_toolkit`, newest first. Follows [SemVer](https://semver.org);
format loosely [Keep a Changelog](https://keepachangelog.com). Entries begin
from the point this file was added — earlier releases live in the crates.io
version history and the git log.

## Unreleased

### Added
- `platform::share::text(&str)` (Platform product): present the system share
  sheet (`UIActivityViewController`) with a block of text. Presents from the
  topmost view controller so it still appears with a modal up, and anchors the
  iPad popover (a nil source view traps). Fire-and-forget — the chosen activity
  and the cancel path are not reported back.

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
