#!/usr/bin/env bash
# Xcode pre-build phase: compile the bevy_ios_toolkit_demo static library for the
# platform being built, then stage it where the linker can find it (-lbevy_ios_toolkit_demo).
set -euo pipefail

# Xcode strips PATH; make cargo + the linker tools reachable.
export PATH="$HOME/.cargo/bin:/opt/homebrew/bin:/usr/local/bin:$PATH"

# Build the staticlib for the same minimum iOS as the app (Xcode sets this in a
# build phase; default to the project's floor otherwise). Without it rustc uses
# the SDK's newer minimum and the linker warns the object is "built for newer iOS".
export IPHONEOS_DEPLOYMENT_TARGET="${IPHONEOS_DEPLOYMENT_TARGET:-16.0}"

# The demo crate root is the parent of this ios/ directory.
DEMO="$(cd "$(dirname "$0")/.." && pwd)"
cd "$DEMO"

# Map the Xcode platform/configuration onto a Rust target + profile.
if [ "${PLATFORM_NAME:-iphonesimulator}" = "iphonesimulator" ]; then
  RUST_TARGET="aarch64-apple-ios-sim"
else
  RUST_TARGET="aarch64-apple-ios"
fi

if [ "${CONFIGURATION:-Debug}" = "Release" ]; then
  PROFILE="release"
  cargo build --lib --target "$RUST_TARGET" --release
else
  PROFILE="debug"
  cargo build --lib --target "$RUST_TARGET"
fi

DEST="$DEMO/ios/rustlib/${PLATFORM_NAME:-iphonesimulator}"
mkdir -p "$DEST"
cp -f "$DEMO/target/$RUST_TARGET/$PROFILE/libbevy_ios_toolkit_demo.a" "$DEST/libbevy_ios_toolkit_demo.a"
echo "build_rust.sh: staged $DEST/libbevy_ios_toolkit_demo.a"
