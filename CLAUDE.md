# CLAUDE.md — bevy_ios_toolkit

Agent onboarding. `make` is the entry point; keep this short and current.

## What this is

Native iOS integrations for the Bevy engine, exposed as ECS resources and
messages. One crate, one `IosPlugin`, a cargo feature per integration: `storekit`
(IAP), `ads` (AdMob + UMP consent), `att` (App Tracking Transparency), `gamekit`
(Game Center), `review`, `platform` (haptics / safe-area / links / boot-shield /
audio-session).

## Entry point

**Everything is a `make` target — never run ad-hoc scripts.** `make help` lists them.

| target | use |
|--------|-----|
| `make run` | run the desktop demo app (against the fakes) |
| `make test` | test suite (`--all-features`) |
| `make pre-commit-checks` | `cargo fmt --check` + clippy `-D warnings` (what the hooks run) |
| `make release` | tag a clean `master` → CI publishes to crates.io |
| `make clean` | wipe build artifacts |

## Architecture

- One module per feature, each `#[cfg(feature = ...)]`.
- The native contract (shared, see `src/ffi.rs`): `@_cdecl` Swift entry points
  called from Rust; results read back as **polled state / a drained event queue**,
  never callbacks into Rust (winit re-entrancy is unsafe). `ffi::read_cstr` is the
  one shared marshalling helper (used by the string-passing modules — store, ads,
  gamekit — and their fakes); don't re-roll it per module.
- **Module shape follows complexity — pick the lightest that fits, don't mix:**
  - *Heavy* (pollable state + FFI string marshalling): a **directory** —
    `mod.rs` + `backend_ios.rs` (raw `extern "C"`) + `backend_fake.rs` (stateful,
    env-tunable desktop fake). Used by `store/`, `ads/`.
  - *Simple state* (an i32 status to poll, no string buffers back): a **single
    file** with inline `#[cfg] mod backend` twice — iOS externs + inline fake.
    Used by `gamekit.rs`, `att.rs`.
  - *Fire-and-forget* (no state, no fake worth writing): a **single file** (or a
    submodule under a grouped dir) with bare `#[cfg] fn` pairs — the iOS extern
    call and a non-iOS no-op. Used by `platform/` (`haptics`, `safe_area`,
    `links`) and its `platform/lifecycle/` group (`boot_shield`, `audio`), and
    `review.rs`.
- Swift shims live in `Sources/<Product>/`, behind `#if canImport(...)`, and ship
  as an SPM package (`Package.swift`) — one library product per feature
  (`Platform`/`Store`/`Ads`/`Att`/`GameCenter`/`Review`). Mirror the Rust split:
  one `.swift` file per concern (see `Sources/Platform/` — `Haptics.swift`,
  `SafeArea.swift`, …), not one monolith.
- `demo/` is a separate crate (the button-per-feature app) with an iOS XcodeGen
  shell in `demo/ios/`; it is excluded from the published library.

## Hard constraints

- A module's `extern "C"` block only exists when its feature is on — enabling a
  feature whose SPM product isn't linked must fail at link time, not at runtime.
  Keep the Rust feature, the Swift product, and the symbol prefix in sync.
- No callbacks from Swift into Rust. New native state is polled or drained.
- Open source: use generic identifiers (`com.example.*`) in examples/demo/docs.

## Dev notes

- The fakes are env-tunable (no-fill, show-failures, consent/ATT outcomes, Game
  Center sign-out) — each module documents its knobs in its doc comment.
- Process-global fakes are singletons; tests that drive them serialize via a guard.
- Verify the iOS link surface with `cargo check --target aarch64-apple-ios --features all`.
