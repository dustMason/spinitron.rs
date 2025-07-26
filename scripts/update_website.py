#!/usr/bin/env python3
"""
Script to update the GitHub Pages website with fresh playlist data
Used by GitHub Actions and can be run locally for testing
"""

import subprocess
import sys
import os


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

    if not os.path.exists("./target/release/spinitron-scraper"):
        print("❌ Error: ./target/release/spinitron-scraper not found")
        print("   Run 'cargo build --release' first")
        sys.exit(1)

    print("📊 Generating fresh playlist data...")
    raw = run_command("./target/release/spinitron-scraper --list-playlists")
    os.makedirs("docs", exist_ok=True)
    with open("docs/playlists.jsonl", "w") as jf:
        jf.write(raw)

    print("📄 Generating static HTML...")
    run_command(
        "python3 scripts/generate_static_html.py docs/playlists.jsonl"
    )

    print("✅ Website update complete!")
    print("📄 Generated: docs/index.html and docs/playlists.json")


if __name__ == "__main__":
    main()
