#!/usr/bin/env bash
# Run the production Store bridge against local StoreKit transactions.
set -euo pipefail
if [[ "${1:-}" == --help || $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: tests/ios/storekit.sh BOOTED_SIMULATOR_UDID [--build-only]"
    echo "Requires Xcode, xcodegen and an iOS 26.1 StoreKitTest runtime. Results go in target/."
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
output="$PWD/target/storekit-$1"
mkdir -p "$output" target/native-storekit-project
xcodegen generate --spec tests/ios/project.yml --project target/native-storekit-project --project-root tests/ios
xcodebuild -project target/native-storekit-project/ToolkitStoreTests.xcodeproj -scheme ToolkitStoreTests \
    -destination "platform=iOS Simulator,id=$1" -parallel-testing-enabled NO \
    -derivedDataPath "$output/build" -resultBundlePath "$output/results-$(date +%s).xcresult" \
    CODE_SIGNING_ALLOWED=YES CODE_SIGN_IDENTITY=- CODE_SIGN_STYLE=Automatic \
    DEVELOPMENT_TEAM= PROVISIONING_PROFILE_SPECIFIER= "$action"
