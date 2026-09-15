#!/usr/bin/env bash
# Compile and run the UIKit scene/screen smoke test on a booted simulator.
set -euo pipefail
if [[ "${1:-}" == --help || $# -ne 1 ]]; then
    echo "Usage: tests/ios/run.sh BOOTED_SIMULATOR_UDID"
    echo "Requires Xcode; use an iOS 27 runtime to verify SDK 27 scene adoption."
    exit 0
fi
cd "$(dirname "$0")/../.."
output="$PWD/target/ios-tests"
app="$output/SceneSmoke.app"
mkdir -p "$app"
python3 - "$app/Info.plist" <<'PY'
import plistlib
import sys
with open(sys.argv[1], "wb") as output:
    plistlib.dump({
        "CFBundleIdentifier": "com.example.bevi.scene-smoke",
        "CFBundleExecutable": "SceneSmoke",
        "CFBundleName": "SceneSmoke",
        "CFBundlePackageType": "APPL",
        "CFBundleVersion": "1",
        "CFBundleShortVersionString": "1.0",
        "MinimumOSVersion": "16.0",
        "UILaunchScreen": {},
        "UIApplicationSceneManifest": {
            "UIApplicationSupportsMultipleScenes": False,
            "UISceneConfigurations": {"UIWindowSceneSessionRoleApplication": [{
                "UISceneConfigurationName": "Bevy",
                "UISceneDelegateClassName": "BevyIosToolkitSceneDelegate",
            }]},
        },
    }, output)
PY
xcrun swiftc -parse-as-library -target arm64-apple-ios16.0-simulator \
    -sdk "$(xcrun --sdk iphonesimulator --show-sdk-path)" \
    Sources/Platform/Scene.swift Sources/Platform/Screen.swift \
    tests/ios/SceneSmoke.swift -o "$app/SceneSmoke"
xcrun simctl install "$1" "$app"
xcrun simctl launch --terminate-running-process --console "$1" \
    com.example.bevi.scene-smoke > "$output/result.log" 2>&1
cat "$output/result.log"
grep -q SCENE_TESTS_PASSED "$output/result.log"
