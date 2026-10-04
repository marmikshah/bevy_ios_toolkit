#!/usr/bin/env bash
# Exercise native ATT/UMP coordination without permission state or SDK networks.
set -euo pipefail
if [[ "${1:-}" == --help ]]; then
    echo "Usage: tests/att/run.sh"
    echo "Requires Swift 6; tests foreground waits, retries and concurrent requests."
    exit 0
fi
cd "$(dirname "$0")/../.."
mkdir -p target/att-tests
swiftc -swift-version 6 -parse-as-library \
    Sources/Platform/NativePromptCoordinator.swift tests/att/NativePromptTests.swift \
    Sources/Att/TrackingRequestCoordinator.swift tests/att/TrackingRequestTests.swift \
    Sources/Ads/ConsentRequestCoordinator.swift tests/att/ConsentRequestTests.swift \
    -o target/att-tests/TrackingRequestTests
target/att-tests/TrackingRequestTests
