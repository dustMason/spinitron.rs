#!/usr/bin/env python3
"""Verify the Spotify archive flow and prepare an existing-library cleanup report."""
import argparse
import getpass
import os
from pathlib import Path
import subprocess
from spotify_authorize import authorize


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    modes = parser.add_mutually_exclusive_group()
    modes.add_argument("--prompt", action="store_true", help="Prompt once for all three credentials")
    modes.add_argument("--reauthorize", action="store_true", help="Reuse the app credentials and capture a new token through Spotify sign-in")
    parser.add_argument("--output-dir", type=Path, default=Path("verification"))
    args = parser.parse_args()
    repo = Path(__file__).resolve().parents[1]
    output = args.output_dir.resolve()
    output.mkdir(parents=True, exist_ok=True)
    env = os.environ.copy()
    print("This verifies one temporary playlist and writes a read-only cleanup proposal.")
    print("Existing playlists will not be changed. Secret inputs are hidden.")
    names = ["SPOTIFY_CLIENT_ID", "SPOTIFY_CLIENT_SECRET"]
    if not args.reauthorize:
        names.append("SPOTIFY_REFRESH_TOKEN")
    for name in names:
        if args.prompt or not env.get(name):
            env[name] = getpass.getpass(name + ": ").strip()
        if not env[name]:
            parser.error(name + " must not be empty")
    if args.reauthorize:
        env["SPOTIFY_REFRESH_TOKEN"] = authorize(env["SPOTIFY_CLIENT_ID"], env["SPOTIFY_CLIENT_SECRET"])
    # Keep the previous recovery binary intact, and reuse builds across invocations.
    target = Path.home() / ".cache" / "spinitron-catalog-target"
    env["CARGO_TARGET_DIR"] = str(target)
    subprocess.run(["cargo", "build", "--locked"], cwd=repo, env=env, check=True)
    binary = target / "debug" / "spinitron-scraper"
    receipt = output / "spotify-archive-verification.json"
    # A failed verification must not leave an old receipt looking current.
    if receipt.exists():
        receipt.rename(output / "spotify-archive-verification.previous.json")
    subprocess.run([str(binary), "--verify-archive-flow", str(receipt)], cwd=repo, env=env, check=True)
    proposal = output / "spotify-library-cleanup-plan.json"
    subprocess.run([str(binary), "--plan-library-cleanup", str(proposal)], cwd=repo, env=env, check=True)
    print("Verification receipt: " + str(receipt))
    print("Read-only cleanup proposal: " + str(proposal))


if __name__ == "__main__":
    try:
        main()
    except subprocess.CalledProcessError as error:
        raise SystemExit(error.returncode)
