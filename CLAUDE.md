# CLAUDE.md — bevy_ios_toolkit

Agent onboarding. `make` is the entry point; keep this short and current.

## What this is

Native iOS integrations for the Bevy engine, exposed as ECS resources and
messages. One crate, one `IosPlugin`, a cargo feature per integration: `storekit`
(IAP), `ads` (AdMob + UMP consent), `att` (App Tracking Transparency), `gamekit`
(Game Center), `review`, `platform` (haptics / safe-area / links).

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

- `src/<module>/` (or `src/<module>.rs`) per feature; each is `#[cfg(feature = ...)]`.
- The native contract (shared, see `src/ffi.rs`): `@_cdecl` Swift entry points
  called from Rust; results read back as **polled state / a drained event queue**,
  never callbacks into Rust (winit re-entrancy is unsafe).
- Each module has two backends: `backend_ios.rs` (raw `extern "C"`) and a
  `backend_fake.rs` / inline fake for every non-iOS target, so flows are testable
  on desktop. The Swift shims live in `swift/`, behind `#if canImport(...)`.
- `demo/` is a separate crate (the button-per-feature app) with an iOS XcodeGen
  shell in `demo/ios/`; it is excluded from the published library.

## Hard constraints

- A module's `extern "C"` block only exists when its feature is on — enabling a
  feature whose Swift shim isn't in the Xcode target must fail at link time, not
  at runtime. Keep the Rust feature, the Swift file, and the symbol prefix in sync.
- No callbacks from Swift into Rust. New native state is polled or drained.
- Open source: use generic identifiers (`com.example.*`) in examples/demo/docs.

## Dev notes

- The fakes are env-tunable (no-fill, show-failures, consent/ATT outcomes, Game
  Center sign-out) — each module documents its knobs in its doc comment.
- Process-global fakes are singletons; tests that drive them serialize via a guard.
- Verify the iOS link surface with `cargo check --target aarch64-apple-ios --features all`.
