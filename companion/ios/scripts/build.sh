#!/bin/bash
set -euo pipefail
cd "$(dirname "$0")/.."
# A personal signing team is only required when installing on an actual iPhone.
xcodebuild -project Spinitron.xcodeproj -scheme Spinitron \
  -configuration Debug -destination 'generic/platform=iOS Simulator' \
  -derivedDataPath build CODE_SIGNING_ALLOWED=NO build
