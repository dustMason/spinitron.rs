#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
if [ -z "${IOS_SIMULATOR_ID:-}" ]; then
  IOS_SIMULATOR_ID=$(xcrun simctl list devices available -j | python3 -c '
import json, sys
devices = [d for values in json.load(sys.stdin)["devices"].values() for d in values
           if d.get("isAvailable") and "iPhone" in d["name"]]
devices.sort(key=lambda d: d["state"] != "Booted")
if not devices:
    sys.exit("No iPhone simulator available. Install an iOS runtime in Xcode Settings > Components.")
print(devices[0]["udid"])
')
fi
xcodebuild -project Spinitron.xcodeproj -scheme Spinitron \
  -destination "platform=iOS Simulator,id=$IOS_SIMULATOR_ID" \
  -derivedDataPath build CODE_SIGNING_ALLOWED=NO test
