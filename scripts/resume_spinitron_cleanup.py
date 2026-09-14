#!/usr/bin/env python3
"""One paced, resumable pass of the approved legacy playlist cleanup."""
import fcntl
import json
import os
import sys
from pathlib import Path
import shutil
import threading
import time
from datetime import datetime, timezone

import cleanup_spinitron_library as cleanup
from spotify_authorize import SCOPES, authorize

ROOT = cleanup.REPO / "verification/full-cleanup-20260909"
COOLDOWN = ROOT / "rate-limit.json"
INTERVAL = 2.0
MAX_PLAYLISTS = 40


class CooldownActive(RuntimeError):
    pass


class PacedSpotify(cleanup.Spotify):
    def __init__(self, *credentials, cooldown_path=COOLDOWN):
        super().__init__(*credentials)
        self.cooldown_path = cooldown_path
        self.gate = threading.Lock()
        self.next_request = 0.0
        self.cooldown_until = read_cooldown(cooldown_path)

    def before_request(self):
        # A shared gate covers all four metadata-reader threads and all retries.
        # Hold it through pacing so scheduling delays cannot cause a later burst.
        with self.gate:
            if time.time() < self.cooldown_until:
                raise CooldownActive("Spotify cooldown is still active")
            time.sleep(max(0.0, self.next_request - time.monotonic()))
            if time.time() < self.cooldown_until:
                raise CooldownActive("Spotify cooldown began while a request was queued")
            self.next_request = time.monotonic() + INTERVAL

    def rate_limited(self, delay):
        # Set immediately, so queued reads stop even while another thread waits.
        self.cooldown_until = max(self.cooldown_until, time.time() + max(60, delay) + 60)
        with self.gate:
            cleanup.save(self.cooldown_path, {
                "not_before": self.cooldown_until,
                "not_before_utc": datetime.fromtimestamp(self.cooldown_until, timezone.utc).isoformat(),
                "retry_after_seconds": delay,
                "recorded_at": cleanup.now(),
            })
        raise CooldownActive("Spotify returned HTTP 429; cooldown saved for the next scheduled pass")


def read_cooldown(path=COOLDOWN):
    return float(json.loads(path.read_text())["not_before"]) if path.exists() else 0.0


def progress(plan, receipt):
    if receipt["plan_sha256"] != cleanup.digest(plan):
        raise ValueError("Cleanup plan changed")
    planned = {p["playlist_id"] for p in plan["playlists"]}
    if not set(receipt["results"]) <= planned:
        raise ValueError("Receipt contains an unplanned playlist")
    unresolved = [pid for pid, r in receipt["results"].items()
                  if r["status"] not in ("removed", "already_absent")]
    return {"completed": len(receipt["results"]) - len(unresolved),
            "remaining": len(plan["playlists"]) - len(receipt["results"]),
            "needs_reconciliation": unresolved}


def final_audit(spotify, plan, receipt):
    library = {}
    offset = 0
    while True:
        page = spotify.request("GET", f"/me/playlists?limit=50&offset={offset}")
        items = page.get("items")
        if not isinstance(items, list) or "next" not in page:
            raise RuntimeError("Incomplete final library response")
        for item in items:
            if item["id"] in library:
                raise RuntimeError("Library changed during final pagination")
            library[item["id"]] = item
        if page["next"] is None:
            break
        if not items or offset >= 50000:
            raise RuntimeError("Invalid final library pagination")
        offset += len(items)
    original = json.loads((ROOT / "library-before.json").read_text())
    selected = {p["playlist_id"] for p in plan["playlists"]}
    untouched = original.keys() - selected
    report = {
        "plan_sha256": cleanup.digest(plan), "receipt_sha256": cleanup.digest(receipt),
        "verified_at": cleanup.now(), "library_count": len(library),
        "unrelated_missing": sorted(untouched - library.keys()),
        "unrelated_preserved": len(untouched & library.keys()),
        "resaved_completed_playlists": sorted(selected & library.keys()),
    }
    cleanup.save(ROOT / "scheduled-final-audit.json", report)
    if report["unrelated_missing"]:
        raise RuntimeError("Final library audit needs review; unrelated playlists were not all visible")
    return report


def main(verify_final=False):
    # Check the saved cooldown before authentication or any Spotify API call.
    not_before = read_cooldown()
    if time.time() < not_before:
        print(json.dumps({"status": "cooldown", "not_before_utc": datetime.fromtimestamp(not_before, timezone.utc).isoformat()}))
        return
    with (cleanup.REPO / "verification/library-cleanup.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
        plan = json.loads((ROOT / "plan.json").read_text())
        receipt = json.loads((ROOT / "receipt.json").read_text())
        before = progress(plan, receipt)
        if before["needs_reconciliation"]:
            print(json.dumps({"status": "needs_reconciliation", **before}))
            return
        if not before["remaining"]:
            if not verify_final:
                print(json.dumps({"status": "ready_for_final_verification", **before}))
                return
        elif verify_final:
            raise RuntimeError("Cleanup still has pending playlists")
        if shutil.disk_usage(ROOT).free < 100 * 1024 * 1024:
            raise RuntimeError("Not enough free disk space to checkpoint a cleanup pass")
        github = cleanup.PersonalGitHub()
        cleanup.verify_deployment(github, {p["playlist_id"] for p in plan["playlists"]})
        client = os.environ["SPOTIFY_CLIENT_ID"]
        secret = os.environ["SPOTIFY_CLIENT_SECRET"]
        # The task agent completes the existing personal-account browser session.
        # No token is printed or stored; unattended scheduling never embeds secrets.
        refresh = authorize(client, secret, scopes=SCOPES + " user-follow-read")
        spotify = PacedSpotify(client, secret, refresh)
        if verify_final:
            assert spotify.owner() == plan["owner_id"]
            print(json.dumps({"status": "complete", **before, "final_audit": final_audit(spotify, plan, receipt)}))
            return
        catalog = json.loads((cleanup.REPO / "data/catalog.json").read_text())
        cleanup.apply(spotify, catalog, ROOT / "plan.json", github,
                      batch_size=40, max_playlists=MAX_PLAYLISTS)
        receipt = json.loads((ROOT / "receipt.json").read_text())
        after = progress(plan, receipt)
        if not after["remaining"]:
            after["final_audit"] = final_audit(spotify, plan, receipt)
        print(json.dumps({"status": "complete" if not after["remaining"] else "progress", **after}))


if __name__ == "__main__":
    try:
        if sys.argv[1:] not in ([], ["--verify-final"]):
            raise ValueError("Only --verify-final is supported")
        main(verify_final="--verify-final" in sys.argv)
    except (RuntimeError, ValueError, OSError, KeyError) as error:
        raise SystemExit(str(error)) from None
