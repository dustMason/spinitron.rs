#!/usr/bin/env python3
"""File saved catalog playlists using Spotify's private rootlist API.

Dry-run by default. SPOTIFY_SP_DC is a sensitive Spotify web login cookie.
No public Web API credentials are used; this script only changes folder order.
"""

import argparse
from datetime import datetime, timezone
from email.utils import parsedate_to_datetime
import hashlib
import hmac
import json
import math
import os
from pathlib import Path
import re
import struct
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request

ACCOUNT = "dustmason"
FOLDER_ID = "90c0bb37f5ab64a7"
ROOT_URL = f"https://spclient.wg.spotify.com/playlist/v2/user/{ACCOUNT}/rootlist"
USER_AGENT = "Mozilla/5.0 (Macintosh; Intel Mac OS X 10_15_7) AppleWebKit/537.36 (KHTML, like Gecko) Chrome/134.0.0.0 Safari/537.36"
# Public protocol data, not account credentials. Pin reviewed parameters rather
# than downloading executable code or sending login cookies to a third party.
# https://github.com/xyloflake/spot-secrets-go/blob/main/secrets/secretDict.json
# https://github.com/mirrorfm/spotify-webplayer-token/blob/main/app/app.go
TOTP_VERSION = 61
TOTP_CIPHER = [44, 55, 47, 42, 70, 40, 34, 114, 76, 74, 50, 111, 120, 97, 75, 76, 94, 102, 43, 69, 49, 120, 118, 80, 64, 78]
COMPLETED_STATES = {"legacy", "ready", "release_attempted", "released"}


class FilingError(Exception):
    """Only messages safe to print, never response bodies or request headers."""


class NoRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise FilingError("Spotify redirected a request; credentials were not forwarded.")


def save_json(path, data):
    """Atomic, owner-only receipts and rate-limit state; never store auth here."""
    path = Path(path)
    path.parent.mkdir(parents=True, exist_ok=True)
    fd, name = tempfile.mkstemp(dir=path.parent, prefix=".filing-")
    try:
        with os.fdopen(fd, "w") as out:
            json.dump(data, out, indent=2)
            out.write("\n")
        os.replace(name, path)
    finally:
        if os.path.exists(name):
            os.unlink(name)


def retry_seconds(header, now):
    if header and re.fullmatch(r"[0-9]{1,9}", header.strip()):
        return max(1, int(header))
    try:
        date = parsedate_to_datetime(header)
        if date.tzinfo is not None:
            return max(1, math.ceil(date.timestamp() - now))
    except (ValueError, TypeError, OverflowError):
        pass
    # A local backoff, not a claimed server reset time.
    return 300


class Spotify:
    def __init__(self, cooldown):
        self.cooldown = Path(cooldown)
        self.opener = urllib.request.build_opener(NoRedirect())
        self.headers = {"Accept": "application/json", "User-Agent": USER_AGENT,
                        "App-Platform": "WebPlayer"}

    def request(self, url, *, headers=None, payload=None):
        # All callers use fixed Spotify endpoints. Refuse credentials elsewhere.
        parsed = urllib.parse.urlsplit(url)
        allowed = (
            parsed.scheme == "https" and not parsed.fragment
            and ((parsed.netloc == "open.spotify.com" and parsed.path == "/api/token")
                 or (parsed.netloc == "spclient.wg.spotify.com"
                     and parsed.path in (urllib.parse.urlsplit(ROOT_URL).path,
                                         urllib.parse.urlsplit(ROOT_URL).path + "/changes")))
        )
        if not allowed:
            raise FilingError("Refusing an unexpected API destination.")
        if self.cooldown.exists():
            try:
                until = json.loads(self.cooldown.read_text())["retry_at"]
                if not isinstance(until, (float, int)) or not math.isfinite(until):
                    raise ValueError
            except (KeyError, TypeError, ValueError):
                raise FilingError("Unreadable saved cooldown; no request sent.") from None
            if until > time.time():
                raise FilingError(f"Spotify cooldown active for {math.ceil(until - time.time())} more seconds; no request sent.")
        method = "POST" if payload is not None else "GET"
        request_headers = dict(headers if headers is not None else self.headers)
        if payload is not None:
            request_headers["Content-Type"] = "application/json;charset=UTF-8"
        request = urllib.request.Request(url, method=method, headers=request_headers,
                                         data=json.dumps(payload).encode() if payload is not None else None)
        try:
            with self.opener.open(request, timeout=30) as response:
                raw = response.read(8_000_001)
        except urllib.error.HTTPError as error:
            if error.code == 429:
                now = time.time()
                delay = retry_seconds(error.headers.get("Retry-After"), now)
                save_json(self.cooldown, {"retry_at": now + delay})
                raise FilingError(f"Spotify HTTP 429; wait at least {delay} seconds. No automatic retry.") from None
            if error.code in (401, 403):
                raise FilingError(f"Spotify HTTP {error.code}; check the personal web session and current web-token protocol. No automatic retry.") from None
            if error.code == 409:
                raise FilingError("Spotify library revision changed. No retry; the next run will read and plan again.") from None
            raise FilingError(f"Spotify HTTP {error.code}; response body withheld. No automatic retry.") from None
        except (urllib.error.URLError, TimeoutError, OSError):
            raise FilingError("Spotify request failed. A write may have succeeded; no automatic retry. Inspect the receipt before rerunning.") from None
        if len(raw) > 8_000_000:
            raise FilingError("Spotify response exceeds the size limit.")
        if method == "POST":
            # Some versions return an empty or protobuf body. Readback, not this
            # acknowledgement, is the source of truth about whether a move worked.
            return
        try:
            return json.loads(raw)
        except ValueError:
            raise FilingError("Spotify returned invalid JSON; response withheld.") from None

    def authenticate(self, cookie):
        if not isinstance(cookie, str) or not re.fullmatch(r"[A-Za-z0-9._~%+/=-]{20,8192}", cookie):
            raise FilingError("SPOTIFY_SP_DC must contain only the sp_dc cookie value.")
        key = "".join(str(x ^ ((i % 33) + 9)) for i, x in enumerate(TOTP_CIPHER)).encode()
        digest = hmac.new(key, struct.pack(">Q", int(time.time()) // 30), hashlib.sha1).digest()
        offset = digest[-1] & 15
        code = f'{(struct.unpack(">I", digest[offset:offset + 4])[0] & 0x7fffffff) % 1000000:06d}'
        query = urllib.parse.urlencode(dict(reason="transport", productType="web-player",
                                            totp=code, totpServer=code, totpVer=TOTP_VERSION))
        result = self.request("https://open.spotify.com/api/token?" + query, headers={
            **self.headers, "Cookie": "sp_dc=" + cookie, "Referer": "https://open.spotify.com/",
        })
        if (not isinstance(result, dict) or result.get("isAnonymous") is not False
                or not isinstance(result.get("accessToken"), str) or not result["accessToken"]):
            raise FilingError("Spotify did not return a signed-in web session; renew SPOTIFY_SP_DC.")
        expiry = result.get("accessTokenExpirationTimestampMs")
        if not isinstance(expiry, (int, float)) or not math.isfinite(expiry) or expiry < (time.time() + 30) * 1000:
            raise FilingError("Spotify returned an expired web session.")
        self.headers["Authorization"] = "Bearer " + result["accessToken"]

    def rootlist(self):
        items, metadata = [], []
        revision, length = None, None
        while True:
            query = urllib.parse.urlencode({"decorate": "revision,length,attributes,timestamp,owner",
                                           "from": len(items), "length": 200})
            page = self.request(ROOT_URL + "?" + query)
            if not isinstance(page, dict) or not isinstance(page.get("revision"), str) or not page["revision"]:
                raise FilingError("Expected a revisioned rootlist.")
            if revision is None:
                revision, length = page["revision"], page.get("length")
                if type(length) is not int or not 0 <= length <= 20_000:
                    raise FilingError("Invalid or excessive library length.")
            if page["revision"] != revision or page.get("length") != length:
                raise FilingError("Library changed during pagination; no moves planned.")
            contents = page.get("contents", {})
            chunk, meta = contents.get("items"), contents.get("metaItems")
            if (contents.get("pos") != len(items) or not isinstance(chunk, list)
                    or not isinstance(meta, list) or len(chunk) != len(meta)
                    or any(not isinstance(m, dict) for m in meta)):
                raise FilingError("Incomplete or misaligned rootlist metadata.")
            if not chunk and len(items) != length:
                raise FilingError("Pagination made no progress.")
            items.extend(chunk)
            metadata.extend(meta)
            if len(items) > length:
                raise FilingError("Rootlist exceeds its declared length.")
            if len(items) == length:
                if contents.get("truncated"):
                    raise FilingError("Spotify marked the complete rootlist as truncated.")
                return {"revision": revision, "items": items, "metadata": metadata}

    def move(self, revision, source, destination):
        # MOV destination refers to the list *before* removing the source.
        self.request(ROOT_URL + "/changes", payload={"baseRevision": revision,
            "deltas": [{"ops": [{"kind": "MOV", "mov": {
                "fromIndex": source, "length": 1, "toIndex": destination,
            }}]}]})


def layout(root):
    items, metadata = root["items"], root["metadata"]
    if len(items) != len(metadata):
        raise FilingError("Rootlist metadata is not aligned.")
    stack, folders, membership, uris = [], {}, {}, []
    for item in items:
        uri = item.get("uri") if isinstance(item, dict) else None
        if not isinstance(uri, str) or not uri or uri in membership:
            raise FilingError("Invalid or duplicate library entry.")
        uris.append(uri)
        membership[uri] = tuple(stack)
        if uri.startswith("spotify:start-group:"):
            parts = uri.split(":", 3)
            if len(parts) != 4 or not parts[2] or parts[2] in folders:
                raise FilingError("Invalid or duplicate folder marker.")
            folders[parts[2]] = urllib.parse.unquote(parts[3])
            stack.append(parts[2])
        elif uri.startswith("spotify:end-group:"):
            if not stack or stack.pop() != uri.split(":", 2)[2]:
                raise FilingError("Folder boundaries do not match.")
    if stack:
        raise FilingError("Unclosed folder boundary.")
    if folders.get(FOLDER_ID) != "KALX" or list(folders.values()).count("KALX") != 1:
        raise FilingError("Expected exactly the existing KALX folder; no folder was created.")
    target = next(uri for uri in uris if uri.startswith("spotify:start-group:" + FOLDER_ID + ":"))
    if membership[target]:
        raise FilingError("The destination KALX folder is no longer at the library root.")
    return uris, membership


def catalog_ids(catalog):
    if not isinstance(catalog, dict) or catalog.get("version") != 1 or not isinstance(catalog.get("entries"), dict):
        raise FilingError("Unsupported catalog format.")
    ids = set()
    for entry in catalog["entries"].values():
        if (entry.get("station") not in {"KALX", "KPOO"}
                or entry.get("state") not in COMPLETED_STATES
                or entry.get("owner_id") not in (None, ACCOUNT)):
            continue
        playlist_id = entry.get("playlist_id")
        # Legacy catalog records may only have a public listing URL.
        url = entry.get("listing", {}).get("url", "")
        match = re.fullmatch(r"https://open\.spotify\.com/playlist/([A-Za-z0-9]{22})", url)
        if playlist_id is None and match:
            playlist_id = match[1]
        if playlist_id is None:
            continue
        if not isinstance(playlist_id, str) or not re.fullmatch(r"[A-Za-z0-9]{22}", playlist_id):
            raise FilingError("Catalog contains an invalid playlist ID.")
        if match and match[1] != playlist_id:
            raise FilingError("Catalog playlist ID and URL disagree.")
        ids.add("spotify:playlist:" + playlist_id)
    return ids


def candidates(root, catalog):
    uris, membership = layout(root)
    return [{"uri": uri, "title": meta.get("attributes", {}).get("name", uri)}
            for uri, meta in zip(uris, root["metadata"])
            if uri in catalog and membership[uri] == () and meta.get("ownerUsername") == ACCOUNT]


def stable_metadata(root):
    # Spotify recommendation decorations (DJ, Summer Rewind) vary per read.
    # Owned playlist content revisions, lengths and attributes must not change.
    return {item["uri"]: meta for item, meta in zip(root["items"], root["metadata"])
            if item["uri"].startswith("spotify:playlist:") and meta.get("ownerUsername") == ACCOUNT}


def verify_move(before, after, uri):
    original, _ = layout(before)
    actual, membership = layout(after)
    source = original.index(uri)
    destination = original.index("spotify:end-group:" + FOLDER_ID)
    expected = original[:]
    value = expected.pop(source)
    expected.insert(destination - (source < destination), value)
    if actual != expected or membership[uri] != (FOLDER_ID,):
        raise FilingError("Library order did not match the planned move; stopped for review.")
    if stable_metadata(before) != stable_metadata(after):
        raise FilingError("Owned playlist metadata changed during filing; stopped for review.")


def run(client, catalog, receipt_path, *, apply=False, max_moves=50, interval=1.0):
    root = client.rootlist()
    planned = candidates(root, catalog)
    receipt = {"account": ACCOUNT, "folder_id": FOLDER_ID,
               "started_at": datetime.now(timezone.utc).isoformat(),
               "status": "planned" if apply else "dry_run", "planned": planned,
               "completed": [], "pending": None}
    save_json(receipt_path, receipt)
    if not apply:
        return receipt
    # Keep each write small and independently verified. Fresh revisions from the
    # readback protect against concurrent UI moves. No blind write retries.
    for entry in planned[:max_moves]:
        current = {item["uri"] for item in candidates(root, catalog)}
        uri = entry["uri"]
        if uri not in current:
            raise FilingError("A planned playlist no longer qualifies; stopped for review.")
        uris, _ = layout(root)
        receipt.update(status="write_pending", pending={**entry, "base_revision": root["revision"]})
        save_json(receipt_path, receipt)
        try:
            client.move(root["revision"], uris.index(uri), uris.index("spotify:end-group:" + FOLDER_ID))
            after = client.rootlist()
            verify_move(root, after, uri)
        except FilingError:
            # Keep pending: the request may have worked. A later fresh run reads
            # actual folder membership and never replays this receipt as commands.
            receipt["status"] = "needs_review"
            save_json(receipt_path, receipt)
            raise
        root = after
        receipt["completed"].append(entry)
        receipt.update(status="in_progress", pending=None)
        save_json(receipt_path, receipt)
        time.sleep(interval)
    remaining = candidates(root, catalog)
    receipt.update(status="completed" if not remaining else "batch_limit_reached",
                   remaining=len(remaining), completed_at=datetime.now(timezone.utc).isoformat())
    save_json(receipt_path, receipt)
    return receipt


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--catalog", type=Path, default=Path("data/catalog.json"))
    parser.add_argument("--receipt", type=Path, default=Path("verification/spotify-folder-filing.json"))
    parser.add_argument("--cooldown", type=Path, default=Path("spotify_cache/folder-cooldown.json"))
    parser.add_argument("--max-moves", type=int, default=50)
    parser.add_argument("--apply", action="store_true", help="Move only the verified root-level candidates")
    args = parser.parse_args()
    if not 1 <= args.max_moves <= 200:
        raise FilingError("--max-moves must be between 1 and 200.")
    catalog = catalog_ids(json.loads(args.catalog.read_text()))
    cookie = os.environ.get("SPOTIFY_SP_DC", "")
    if not cookie:
        raise FilingError("SPOTIFY_SP_DC is missing. Configure the personal Spotify web session first.")
    client = Spotify(args.cooldown)
    client.authenticate(cookie)
    receipt = run(client, catalog, args.receipt, apply=args.apply, max_moves=args.max_moves)
    if not args.apply:
        print(f"Dry run: {len(receipt['planned'])} owned catalog playlists at the library root. Receipt: {args.receipt}")
    else:
        print(f"Filed {len(receipt['completed'])} playlists in KALX; {receipt['remaining']} remaining. Receipt: {args.receipt}")


if __name__ == "__main__":
    try:
        main()
    except (FilingError, OSError, ValueError, KeyError, TypeError, AttributeError):
        error = sys.exc_info()[1]
        print(str(error) if isinstance(error, FilingError) else "Filing could not complete; private details withheld.", file=sys.stderr)
        sys.exit(1)
