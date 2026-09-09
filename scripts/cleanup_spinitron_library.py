#!/usr/bin/env python3
"""Plan, then explicitly apply, removal of legacy Spinitron playlists from Your Library.

Playlist contents and the catalog are never edited. Uses Spotify's library API:
https://developer.spotify.com/documentation/web-api/reference/remove-library-items
"""
import argparse
import base64
from concurrent.futures import ThreadPoolExecutor
from datetime import datetime, timezone
import fcntl
import getpass
import hashlib
import json
import os
from pathlib import Path
import re
import subprocess
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

from spotify_authorize import authorize

REPO = Path(__file__).resolve().parents[1]
GITHUB_REPO = "dustMason/spinitron.rs"
GITHUB_USER = "dustMason"
SPOTIFY_USER = "dustmason"


def now():
    return datetime.now(timezone.utc).isoformat()


def digest(value):
    return hashlib.sha256(json.dumps(value, sort_keys=True).encode()).hexdigest()


def save(path, value):
    path = Path(path)
    with tempfile.NamedTemporaryFile(mode="w", dir=path.parent, delete=False) as file:
        temp = Path(file.name)
        try:
            json.dump(value, file, indent=2, ensure_ascii=False)
            file.write("\n")
            file.flush()
            os.fsync(file.fileno())
        except BaseException:
            temp.unlink(missing_ok=True)
            raise
    os.replace(temp, path)


class Spotify:
    def __init__(self, client_id, client_secret, refresh_token):
        self.credentials = (client_id, client_secret, refresh_token)
        self.token = None
        self.expires = 0

    def access_token(self):
        if time.monotonic() >= self.expires:
            client_id, secret, refresh = self.credentials
            request = urllib.request.Request(
                "https://accounts.spotify.com/api/token",
                data=urllib.parse.urlencode({"grant_type": "refresh_token", "refresh_token": refresh}).encode(),
                headers={"Authorization": "Basic " + base64.b64encode(f"{client_id}:{secret}".encode()).decode()},
            )
            try:
                with urllib.request.urlopen(request, timeout=30) as response:
                    payload = json.load(response)
            except urllib.error.HTTPError as error:
                raise RuntimeError(f"Spotify authentication failed (HTTP {error.code}); try --reauthorize") from None
            self.token = payload.get("access_token")
            if not self.token:
                raise RuntimeError("Spotify did not return an access token")
            self.expires = time.monotonic() + max(1, payload.get("expires_in", 3600) - 60)
        return self.token

    def request(self, method, path):
        # Enforce the script's only permitted mutation at the transport boundary.
        parsed = urllib.parse.urlsplit(path)
        if parsed.scheme or parsed.netloc or not path.startswith("/"):
            raise ValueError("Expected a relative Spotify API path")
        if method != "GET":
            params = urllib.parse.parse_qs(parsed.query)
            uri = params.get("uris", [""])
            values = uri[0].split(",")
            if (method != "DELETE" or parsed.path != "/me/library" or len(uri) != 1
                    or not 1 <= len(values) <= 40 or len(set(values)) != len(values)
                    or any(not re.fullmatch(r"spotify:playlist:[A-Za-z0-9]+", value) for value in values)):
                raise ValueError("Only removing up to 40 unique playlists' library membership is supported")
        for attempt in range(3):
            request = urllib.request.Request("https://api.spotify.com/v1" + path, method=method,
                                             headers={"Authorization": "Bearer " + self.access_token()})
            try:
                with urllib.request.urlopen(request, timeout=30) as response:
                    body = response.read()
                    return json.loads(body) if body else None
            except urllib.error.HTTPError as error:
                retry = error.headers.get("Retry-After", str(2 ** attempt))
                delay = int(retry) if retry.isdigit() else 2 ** attempt
                if method == "GET" and attempt < 2 and (error.code == 429 or error.code >= 500) and delay <= 30:
                    time.sleep(delay)
                    continue
                raise RuntimeError(f"Spotify {method} {parsed.path} failed (HTTP {error.code})") from None
            except (urllib.error.URLError, TimeoutError):
                if method == "GET" and attempt < 2:
                    time.sleep(2 ** attempt)
                    continue
                raise RuntimeError(f"Spotify {method} {parsed.path} failed; response uncertain") from None

    def owner(self):
        owner = self.request("GET", "/me").get("id")
        if owner != SPOTIFY_USER:
            raise RuntimeError(f"Expected Spotify account {SPOTIFY_USER}; refusing a different account")
        return owner

    def metadata(self, playlist_id):
        return self.request("GET", f"/playlists/{playlist_id}?fields=id,name,description,owner(id),snapshot_id,public,tracks(total)")

    def saved(self, playlist_id):
        query = urllib.parse.urlencode({"uris": f"spotify:playlist:{playlist_id}"})
        value = self.request("GET", "/me/library/contains?" + query)
        if not isinstance(value, list) or len(value) != 1 or type(value[0]) is not bool:
            raise RuntimeError("Invalid Spotify library-membership response")
        return value[0]

    def remove(self, playlist_id):
        self.request("DELETE", "/me/library?" + urllib.parse.urlencode({"uris": f"spotify:playlist:{playlist_id}"}))

    def saved_many(self, playlist_ids):
        query = urllib.parse.urlencode({"uris": ",".join(f"spotify:playlist:{pid}" for pid in playlist_ids)})
        value = self.request("GET", "/me/library/contains?" + query)
        if not isinstance(value, list) or len(value) != len(playlist_ids) or any(type(v) is not bool for v in value):
            raise RuntimeError("Invalid Spotify library-membership response")
        return value

    def remove_many(self, playlist_ids):
        self.request("DELETE", "/me/library?" + urllib.parse.urlencode({
            "uris": ",".join(f"spotify:playlist:{pid}" for pid in playlist_ids)}))


def legacy_ids(catalog):
    if catalog.get("version") != 1 or not isinstance(catalog.get("entries"), dict):
        raise ValueError("Unsupported or invalid catalog")
    ids = [e["playlist_id"] for e in catalog["entries"].values() if e["state"] == "legacy"]
    if len(ids) != len(set(ids)) or any(not re.fullmatch(r"[A-Za-z0-9]+", i) for i in ids):
        raise ValueError("Invalid or duplicate catalog playlist IDs")
    return set(ids)


def metadata_record(item, owner):
    playlist_id = item["id"]
    if not re.fullmatch(r"[A-Za-z0-9]+", playlist_id) or item["owner"]["id"] != owner:
        raise ValueError("Playlist owner or ID does not match")
    if not item.get("snapshot_id") or type(item["tracks"]["total"]) is not int:
        raise ValueError("Playlist version or track count is unavailable")
    return {"playlist_id": playlist_id, "name": item["name"], "owner_id": owner,
            "snapshot_id": item["snapshot_id"], "track_count": item["tracks"]["total"],
            "public": item["public"], "url": f"https://open.spotify.com/playlist/{playlist_id}"}


def prepare(spotify, catalog, output):
    owner = spotify.owner()
    eligible = legacy_ids(catalog)
    cataloged = {e.get("playlist_id") for e in catalog["entries"].values()}
    selected, unlisted, seen = [], [], set()
    offset = 0
    while True:
        page = spotify.request("GET", f"/me/playlists?limit=50&offset={offset}")
        items = page.get("items")
        if not isinstance(items, list) or "next" not in page:
            raise ValueError("Incomplete Spotify library response")
        for item in items:
            playlist_id = item["id"]
            if playlist_id in seen:
                raise ValueError("Library changed during pagination; prepare the plan again")
            seen.add(playlist_id)
            if item["owner"]["id"] != owner:
                continue
            if playlist_id in eligible:
                selected.append(metadata_record(item, owner))
            elif playlist_id not in cataloged and "Generated from Spinitron playlists" in (item.get("description") or ""):
                unlisted.append({"playlist_id": playlist_id, "name": item["name"]})
        print(f"Read {len(seen)} library playlists; {len(selected)} legacy catalog matches", flush=True)
        if page["next"] is None:
            break
        if not items or not isinstance(page["next"], str) or offset >= 50000:
            raise ValueError("Invalid Spotify pagination")
        offset += len(items)
    plan = {"version": 1, "action": "remove_legacy_library_membership", "created_at": now(),
            "owner_id": owner, "catalog_sha256": digest(catalog),
            "playlists": sorted(selected, key=lambda p: (p["name"], p["playlist_id"])),
            "unlisted_generated_playlists": unlisted}
    output.mkdir(parents=True, exist_ok=False)
    save(output / "catalog-backup.json", catalog)
    save(output / "plan.json", plan)
    print(f"Read-only plan: {output / 'plan.json'} ({len(selected)} playlists). Nothing removed.")
    if unlisted:
        print(f"{len(unlisted)} other generated playlists need cataloging before they can be removed.")
    return plan


class PersonalGitHub:
    def __init__(self):
        token = subprocess.run(["gh", "auth", "token", "--hostname", "github.com", "--user", GITHUB_USER],
                               capture_output=True, text=True, check=True).stdout.strip()
        if not token:
            raise RuntimeError("Personal GitHub authentication is unavailable")
        self.env = dict(os.environ, GH_TOKEN=token, GH_HOST="github.com")
        if self.get("user").get("login", "").casefold() != GITHUB_USER.casefold():
            raise RuntimeError("GitHub credential is not for dustMason")

    def get(self, path):
        result = subprocess.run(["gh", "api", path], env=self.env, capture_output=True, text=True, check=True)
        return json.loads(result.stdout)

    def file(self, path, ref):
        value = self.get(f"repos/{GITHUB_REPO}/contents/{path}?ref={ref}")
        if value.get("encoding") != "base64":
            # The JSON catalog can exceed the contents API's inline size limit.
            value = self.get(f"repos/{GITHUB_REPO}/git/blobs/{value['sha']}")
        return base64.b64decode(value["content"]).decode()


def verify_deployment(github, selected):
    workflow = github.file(".github/workflows/daily-playlist-update.yml", "main")
    if not re.search(r"run:.*spinitron-scraper --archive\s*$", workflow, re.MULTILINE) or re.search(r"run:.* --spotify\b", workflow):
        raise RuntimeError("Deploy the broadcast archive workflow before removing legacy library entries")
    runs = github.get(f"repos/{GITHUB_REPO}/actions/workflows/daily-playlist-update.yml/runs?per_page=100")
    if any(run["status"] != "completed" for run in runs["workflow_runs"]):
        raise RuntimeError("Wait for the active daily workflow to finish before library cleanup")
    build = github.get(f"repos/{GITHUB_REPO}/pages/builds/latest")
    if build.get("status") != "built" or not re.fullmatch(r"[0-9a-f]{40}", build.get("commit", "")):
        raise RuntimeError("Wait for a successful GitHub Pages deployment")
    ref = build["commit"]
    catalog = json.loads(github.file("data/catalog.json", ref))
    published = {r["url"].rsplit("/", 1)[-1] for r in json.loads(github.file("docs/assets/catalog-data.json", ref))}
    if not selected <= legacy_ids(catalog) or not selected <= published:
        raise RuntimeError("Some selected playlists are missing from the deployed catalog or website")
    return {"pages_commit": ref, "verified_at": now()}


def apply_batches(spotify, plan, receipt, receipt_path, batch_size):
    """Serial writes, bounded parallel reads, durable intent for every selected ID."""
    prior_uncertain = [pid for pid, result in receipt["results"].items()
                       if result["status"] not in ("removed", "already_absent")]
    if prior_uncertain:
        raise RuntimeError(f"{len(prior_uncertain)} uncertain previous attempts need review; they were not retried")
    pending = [item for item in plan["playlists"] if item["playlist_id"] not in receipt["results"]]
    def read_metadata(item):
        return metadata_record(spotify.metadata(item["playlist_id"]), plan["owner_id"])
    with ThreadPoolExecutor(max_workers=4) as pool:
        for start in range(0, len(pending), batch_size):
            batch = pending[start:start + batch_size]
            before = list(pool.map(read_metadata, batch))
            if before != batch:
                raise RuntimeError("A playlist changed since planning; review it before cleanup")
            saved = spotify.saved_many([item["playlist_id"] for item in batch])
            selected = []
            for item, present in zip(batch, saved):
                pid = item["playlist_id"]
                if present:
                    selected.append(item)
                else:
                    receipt["results"][pid] = {"status": "already_absent", "at": now()}
            if not selected:
                save(receipt_path, receipt)
                continue
            ids = [item["playlist_id"] for item in selected]
            for pid in ids:
                receipt["results"][pid] = {"status": "attempted", "at": now()}
            save(receipt_path, receipt)  # Persist the whole batch before its only write.
            try:
                spotify.remove_many(ids)
                after = list(pool.map(read_metadata, selected))
                still_saved = spotify.saved_many(ids)
                for item, preserved, present in zip(selected, after, still_saved):
                    if item != preserved or present:
                        raise RuntimeError("Library removal or playlist preservation could not be verified")
            except Exception as error:
                for pid in ids:
                    receipt["results"][pid].update(status="uncertain", error=str(error))
                save(receipt_path, receipt)
                raise RuntimeError("Stopped after an uncertain batch; inspect receipt.json before further action") from error
            for item in after:
                receipt["results"][item["playlist_id"]] = {"status": "removed", "at": now(), "preserved": item}
            save(receipt_path, receipt)
            print(f"{len(receipt['results'])}/{len(plan['playlists'])} checked; last {len(ids)} removed and preserved", flush=True)


def apply(spotify, catalog, plan_path, github, batch_size=1):
    if not 1 <= batch_size <= 40:
        raise ValueError("Batch size must be between 1 and 40")
    plan = json.loads(plan_path.read_text())
    backup = json.loads(plan_path.with_name("catalog-backup.json").read_text())
    if plan.get("version") != 1 or plan.get("action") != "remove_legacy_library_membership" or plan.get("catalog_sha256") != digest(backup):
        raise ValueError("Invalid cleanup plan or catalog backup")
    ids = [p["playlist_id"] for p in plan["playlists"]]
    selected = set(ids)
    if len(ids) != len(selected) or not selected <= legacy_ids(catalog) or not selected <= legacy_ids(backup):
        raise ValueError("Cleanup plan contains duplicate or uncataloged IDs")
    if plan.get("owner_id") != spotify.owner():
        raise ValueError("Cleanup plan belongs to a different Spotify account")
    deployment = verify_deployment(github, selected)
    receipt_path = plan_path.with_name("receipt.json")
    receipt = json.loads(receipt_path.read_text()) if receipt_path.exists() else {
        "plan_sha256": digest(plan), "deployment": deployment, "results": {}}
    if receipt.get("plan_sha256") != digest(plan):
        raise ValueError("Plan changed after cleanup started; preserve the original plan")
    if batch_size > 1:
        apply_batches(spotify, plan, receipt, receipt_path, batch_size)
        print(f"Cleanup complete. Playlist links and verification results: {receipt_path}")
        return
    unresolved = []
    for index, item in enumerate(plan["playlists"], 1):
        playlist_id = item["playlist_id"]
        prior = receipt["results"].get(playlist_id)
        if prior:
            if prior["status"] not in ("removed", "already_absent"):
                unresolved.append(playlist_id)
            continue  # Never remove a playlist again if the user re-saves it.
        before = metadata_record(spotify.metadata(playlist_id), plan["owner_id"])
        if before != item:
            raise RuntimeError(f"Playlist {playlist_id} changed since planning; review it before cleanup")
        if not spotify.saved(playlist_id):
            receipt["results"][playlist_id] = {"status": "already_absent", "at": now()}
            save(receipt_path, receipt)
            continue
        receipt["results"][playlist_id] = {"status": "attempted", "at": now()}
        save(receipt_path, receipt)  # Durable intent before the one library DELETE.
        try:
            spotify.remove(playlist_id)
            after = metadata_record(spotify.metadata(playlist_id), plan["owner_id"])
            if spotify.saved(playlist_id) or after != before:
                raise RuntimeError("Library removal or playlist preservation could not be verified")
        except Exception as error:
            receipt["results"][playlist_id].update(status="uncertain", error=str(error))
            save(receipt_path, receipt)
            raise RuntimeError(f"Stopped at {playlist_id}; inspect receipt.json before any further action") from error
        receipt["results"][playlist_id] = {"status": "removed", "at": now(), "preserved": after}
        save(receipt_path, receipt)
        print(f"{index}/{len(ids)} removed from library; playlist preserved: {item['name']}", flush=True)
    if unresolved:
        raise RuntimeError(f"{len(unresolved)} uncertain previous attempts need review; they were not retried")
    print(f"Cleanup complete. Playlist links and verification results: {receipt_path}")


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, default=REPO / "data/catalog.json")
    parser.add_argument("--output-dir", type=Path, help="New folder for the read-only plan and catalog backup")
    parser.add_argument("--apply", type=Path, metavar="PLAN.json", help="Explicitly apply a previously reviewed plan after deployment")
    parser.add_argument("--batch-size", type=int, default=1, help="Playlists per removal request (1–40; default 1)")
    auth = parser.add_mutually_exclusive_group()
    auth.add_argument("--prompt", action="store_true", help="Prompt once for all three Spotify credentials")
    auth.add_argument("--reauthorize", action="store_true", help="Reuse app credentials and capture a token via Spotify sign-in")
    args = parser.parse_args()
    if not 1 <= args.batch_size <= 40:
        parser.error("--batch-size must be between 1 and 40")
    if args.apply and args.output_dir:
        parser.error("--output-dir only applies when preparing a plan")
    catalog = json.loads(args.catalog.read_text())
    legacy_ids(catalog)
    github = PersonalGitHub() if args.apply else None
    # Refuse an undeployed cleanup before asking for Spotify credentials.
    if args.apply:
        planned = json.loads(args.apply.read_text())
        verify_deployment(github, {p["playlist_id"] for p in planned["playlists"]})
    credentials = {}
    names = ["SPOTIFY_CLIENT_ID", "SPOTIFY_CLIENT_SECRET"]
    if not args.reauthorize:
        names.append("SPOTIFY_REFRESH_TOKEN")
    for name in names:
        credentials[name] = getpass.getpass(name + ": ").strip() if args.prompt or not os.environ.get(name) else os.environ[name]
        if not credentials[name]:
            parser.error(name + " must not be empty")
    if args.reauthorize:
        credentials["SPOTIFY_REFRESH_TOKEN"] = authorize(credentials["SPOTIFY_CLIENT_ID"], credentials["SPOTIFY_CLIENT_SECRET"])
    spotify = Spotify(*(credentials[n] for n in ("SPOTIFY_CLIENT_ID", "SPOTIFY_CLIENT_SECRET", "SPOTIFY_REFRESH_TOKEN")))
    if args.apply:
        lock_path = REPO / "verification/library-cleanup.lock"
        lock_path.parent.mkdir(parents=True, exist_ok=True)
        with lock_path.open("a") as lock:
            fcntl.flock(lock, fcntl.LOCK_EX | fcntl.LOCK_NB)
            apply(spotify, catalog, args.apply, github, args.batch_size)
    else:
        output = args.output_dir or REPO / "verification" / datetime.now(timezone.utc).strftime("library-cleanup-%Y%m%dT%H%M%SZ")
        prepare(spotify, catalog, output.resolve())


if __name__ == "__main__":
    try:
        main()
    except (RuntimeError, ValueError, OSError, KeyError, subprocess.CalledProcessError) as error:
        raise SystemExit(str(error)) from None
