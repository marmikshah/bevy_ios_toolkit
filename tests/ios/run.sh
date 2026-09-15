#!/usr/bin/env bash
# Compile and run the UIKit scene/screen smoke test on a booted simulator.
set -euo pipefail
if [[ "${1:-}" == --help || $# -ne 1 ]]; then
    echo "Usage: tests/ios/run.sh BOOTED_SIMULATOR_UDID"
    echo "Requires Xcode; run on iOS 26.0 and iOS 27 to check compatibility."
    exit 0
fi
cd "$(dirname "$0")/../.."
output="$PWD/target/ios-tests-$1"
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
        "MinimumOSVersion": "26.0",
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
xcrun --sdk iphonesimulator swiftc -parse-as-library -target arm64-apple-ios26.0-simulator \
    -sdk "$(xcrun --sdk iphonesimulator --show-sdk-path)" \
    Sources/Platform/Scene.swift Sources/Platform/Screen.swift \
    tests/ios/SceneSmoke.swift -o "$app/SceneSmoke"
xcrun simctl install "$1" "$app"
container="$(xcrun simctl get_app_container "$1" com.example.bevi.scene-smoke data)"
result="$container/Documents/scene-smoke-result.txt"
rm -f "$result"
# Xcode 27 can delay streamed stdout from simultaneous simulator processes.
# An app-owned completion file also proves the assertions actually finished.
xcrun simctl launch --terminate-running-process "$1" com.example.bevi.scene-smoke
for _ in {1..60}; do
    if [[ -f "$result" ]]; then
        cp "$result" "$output/result.log"
        cat "$output/result.log"
        exit 0
    fi
    sleep 1
done
echo "SceneSmoke did not complete; inspect the simulator's crash log." >&2
exit 1
