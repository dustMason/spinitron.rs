#!/usr/bin/env python3
"""
Script to update the GitHub Pages website with fresh playlist data
Used by GitHub Actions and can be run locally for testing
"""

import subprocess
import sys
import os
from datetime import datetime
import re

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
    if not os.path.exists("docs/index.template.md"):
        print("❌ Error: docs/index.template.md not found")
        sys.exit(1)
    
    # Generate fresh playlist data
    print("📊 Generating fresh playlist data...")
    playlist_output = run_command("./target/release/spinitron-scraper --list-playlists --spotify")
    
    # Extract playlist lines (lines that start with "- [KALX -")
    playlist_lines = []
    for line in playlist_output.split('\n'):
        if line.strip().startswith('- [KALX -'):
            playlist_lines.append(line.strip())
    
    if not playlist_lines:
        print("❌ Error: No playlists found in output")
        sys.exit(1)
    
    # Read template
    with open("docs/index.template.md", "r") as f:
        template = f.read()
    
    # Generate timestamp
    from datetime import timezone
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    
    # Replace template variables
    content = template.replace("{{PLAYLISTS}}", "\n".join(playlist_lines))
    content = content.replace("{{TIMESTAMP}}", timestamp)
    content = content.replace("{{COUNT}}", str(len(playlist_lines)))
    
    # Write the final file
    with open("docs/index.md", "w") as f:
        f.write(content)
    
    print("✅ Website update complete!")
    print(f"📊 Updated with {len(playlist_lines)} playlists")
    print("📄 Generated: docs/index.md")

if __name__ == "__main__":
    main()