#!/usr/bin/env bash
# Build and test the demo: rendering, deferred StoreKit, banners, resume.
set -euo pipefail
if [[ "${1:-}" == --help || $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: tests/ios/demo.sh BOOTED_SIMULATOR_UDID [--build-only]"
    echo "Requires Xcode, Rust iOS simulator target and xcodegen. Results go in target/."
    exit 0
fi
action='test'
if [[ "${2:-}" == --build-only ]]; then
    action=build-for-testing
elif [[ $# -eq 2 ]]; then
    echo "Unknown option: $2" >&2
    exit 2
fi
cd "$(dirname "$0")/../.."
output="$PWD/target/demo-smoke-$1"
mkdir -p "$output"
xcodegen generate --spec demo/ios/project.yml --project demo/ios
xcodebuild -project demo/ios/IosToolkitDemo.xcodeproj -scheme IosToolkitDemo \
    -destination "platform=iOS Simulator,id=$1" -parallel-testing-enabled NO \
    -derivedDataPath "$output/build" -resultBundlePath "$output/results-$(date +%s).xcresult" \
    CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Automatic \
    DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= "$action"
