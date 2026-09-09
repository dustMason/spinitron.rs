#!/usr/bin/env python3
"""Export the durable catalog and generate the static website without Spotify."""

import argparse
from pathlib import Path
import subprocess
import sys

from generate_static_html import main as generate


def main():
    repo = Path(__file__).resolve().parents[1]
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--binary", type=Path, default=repo / "target/release/spinitron-scraper")
    parser.add_argument("--output-dir", type=Path, default=repo / "docs")
    args = parser.parse_args()
    binary = args.binary.resolve()
    if not binary.is_file():
        parser.error(f"{binary} not found. Build with cargo build --release first, or pass --binary.")
    raw = subprocess.run([str(binary), "--list-catalog"], cwd=repo, capture_output=True, text=True, check=True).stdout
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    data = output / "playlists.jsonl"
    data.write_text(raw, encoding="utf-8")
    generate(data, output)
    print(f"Website ready: {output / 'index.html'}")


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        print(error.stderr, file=sys.stderr)
        raise SystemExit(error.returncode)
