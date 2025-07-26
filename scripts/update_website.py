#!/usr/bin/env python3
"""
Script to update the GitHub Pages website with fresh playlist data
Used by GitHub Actions and can be run locally for testing
"""

import subprocess
import sys
import os
import json
from datetime import datetime, timezone


def run_command(cmd):
    """Run shell command and return output"""
    try:
        result = subprocess.run(
            cmd, shell=True, capture_output=True, text=True, check=True
        )
        return result.stdout
    except subprocess.CalledProcessError as e:
        print(f"❌ Error running command: {cmd}")
        print(f"   {e.stderr}")
        sys.exit(1)


def main():
    print("🌐 Updating website with latest playlists...")

    # Check if we have the required binary
    if not os.path.exists("./target/release/spinitron-scraper"):
        print("❌ Error: ./target/release/spinitron-scraper not found")
        print("   Run 'cargo build --release' first")
        sys.exit(1)

    # Generate fresh playlist data
    print("📊 Generating fresh playlist data...")
    playlist_output = run_command("./target/release/spinitron-scraper --list-playlists")

    # Parse JSONL output and group by station
    stations = {}
    total_playlist_count = 0

    for line in playlist_output.strip().split("\n"):
        if line.strip():
            try:
                playlist = json.loads(line)
                station = playlist["station"]
                name = playlist["name"]
                url = playlist["url"]
                track_count = playlist["track_count"]
                last_updated = playlist.get("last_updated", "")
                preview = playlist.get("preview", [])

                if station not in stations:
                    stations[station] = []

                stations[station].append(
                    {
                        "name": name,
                        "url": url,
                        "track_count": track_count,
                        "last_updated": last_updated,
                        "preview": preview,
                    }
                )
                total_playlist_count += 1
            except (json.JSONDecodeError, KeyError) as e:
                print(f"⚠️  Warning: Failed to parse line: {line}")
                print(f"   Error: {e}")

    if not stations:
        print("❌ Error: No playlists found in output")
        sys.exit(1)

    json_data = {
        "stations": stations,
        "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC"),
        "total_playlist_count": total_playlist_count,
    }
    with open("docs/playlists.json", "w") as jf:
        json.dump(json_data, jf, indent=2)

    print("📄 Generating static HTML...")
    run_command(
        "python3 scripts/generate_static_html.py docs/playlists.json docs/index.html"
    )

    print("✅ Website update complete!")
    print(
        f"📊 Updated with {total_playlist_count} playlists across {len(stations)} stations"
    )
    print("📄 Generated: docs/index.html and docs/playlists.json")


if __name__ == "__main__":
    main()
