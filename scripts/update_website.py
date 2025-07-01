#!/usr/bin/env python3
"""
Script to update the GitHub Pages website with fresh playlist data
Used by GitHub Actions and can be run locally for testing
"""

import subprocess
import sys
import os
import json
from datetime import datetime

def run_command(cmd):
    """Run shell command and return output"""
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, check=True)
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
    
    # Check if template exists
    if not os.path.exists("scripts/index.template.md"):
        print("❌ Error: scripts/index.template.md not found")
        sys.exit(1)
    
    # Generate fresh playlist data
    print("📊 Generating fresh playlist data...")
    playlist_output = run_command("./target/release/spinitron-scraper --list-playlists")
    
    # Parse JSONL output
    table_rows = []
    playlist_count = 0
    
    for line in playlist_output.strip().split('\n'):
        if line.strip():
            try:
                playlist = json.loads(line)
                name = playlist['name']
                url = playlist['url']
                track_count = playlist['track_count']
                table_rows.append(f"| [{name}]({url}) | {track_count} |")
                playlist_count += 1
            except (json.JSONDecodeError, KeyError) as e:
                print(f"⚠️  Warning: Failed to parse line: {line}")
                print(f"   Error: {e}")
    
    if not table_rows:
        print("❌ Error: No playlists found in output")
        sys.exit(1)
    
    # Read template
    with open("scripts/index.template.md", "r") as f:
        template = f.read()
    
    # Generate timestamp
    from datetime import timezone
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    
    # Replace template variables
    content = template.replace("{{PLAYLISTS}}", "\n".join(table_rows))
    content = content.replace("{{TIMESTAMP}}", timestamp)
    content = content.replace("{{COUNT}}", str(playlist_count))
    
    # Write the final file
    with open("docs/index.md", "w") as f:
        f.write(content)
    
    print("✅ Website update complete!")
    print(f"📊 Updated with {playlist_count} playlists")
    print("📄 Generated: docs/index.md")

if __name__ == "__main__":
    main()