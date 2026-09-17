#!/usr/bin/env bash
# Exercise the native ATT coordinator without permission state or SDK networks.
set -euo pipefail
if [[ "${1:-}" == --help ]]; then
    echo "Usage: tests/att/run.sh"
    echo "Requires Swift 6; tests foreground waits, retries and concurrent requests."
    exit 0
fi
cd "$(dirname "$0")/../.."
mkdir -p target/att-tests
swiftc -swift-version 6 -parse-as-library \
    Sources/Att/TrackingRequestCoordinator.swift tests/att/TrackingRequestTests.swift \
    -o target/att-tests/TrackingRequestTests
target/att-tests/TrackingRequestTests
