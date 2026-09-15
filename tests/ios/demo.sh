#!/usr/bin/env bash
# Test an already installed demo: rendering, deferred StoreKit, banners, resume.
set -euo pipefail
if [[ "${1:-}" == --help || $# -lt 1 || $# -gt 2 ]]; then
    echo "Usage: tests/ios/demo.sh BOOTED_SIMULATOR_UDID [--build-only]"
    echo "Install the demo first. Requires Xcode and xcodegen. Results go in target/."
    exit 0
fi
action=test
if [[ "${2:-}" == --build-only ]]; then
    action=build-for-testing
elif [[ $# -eq 2 ]]; then
    echo "Unknown option: $2" >&2
    exit 2
fi
cd "$(dirname "$0")/../.."
output="$PWD/target/demo-smoke-$1"
mkdir -p "$output"
cat > "$output/project.yml" <<YAML
name: DemoSmoke
options:
  deploymentTarget:
    iOS: "26.0"
targets:
  DemoSmoke:
    type: bundle.ui-testing
    platform: iOS
    sources:
      - path: $PWD/tests/ios/DemoSmoke.swift
    settings:
      base:
        GENERATE_INFOPLIST_FILE: YES
        PRODUCT_BUNDLE_IDENTIFIER: com.example.bevy.demo-smoke
        SWIFT_VERSION: "5.0"
schemes:
  DemoSmoke:
    build:
      targets:
        DemoSmoke: all
    test:
      targets:
        - DemoSmoke
YAML
xcodegen generate --spec "$output/project.yml" --project "$output"
xcodebuild -project "$output/DemoSmoke.xcodeproj" -scheme DemoSmoke \
    -destination "platform=iOS Simulator,id=$1" -parallel-testing-enabled NO \
    -derivedDataPath "$output/build" -resultBundlePath "$output/results-$(date +%s).xcresult" \
    CODE_SIGNING_ALLOWED=NO "$action"
