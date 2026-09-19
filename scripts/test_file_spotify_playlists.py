import copy
import io
import json
from pathlib import Path
import tempfile
import time
import unittest
from unittest.mock import Mock, patch
import urllib.error

import file_spotify_playlists as filing


def playlist(letter):
    return "spotify:playlist:" + letter * 22


START = "spotify:start-group:" + filing.FOLDER_ID + ":KALX"
END = "spotify:end-group:" + filing.FOLDER_ID
OTHER_START = "spotify:start-group:other:Other"
OTHER_END = "spotify:end-group:other"


def root(uris, owner=filing.ACCOUNT):
    return {"revision": "revision-1", "items": [{"uri": uri} for uri in uris],
            "metadata": [{"ownerUsername": owner, "attributes": {"name": uri},
                          "length": 3, "revision": "tracks-1"}
                         if uri.startswith("spotify:playlist:") else {} for uri in uris]}


class FakeSpotify:
    def __init__(self, snapshot):
        self.snapshot = copy.deepcopy(snapshot)
        self.moves = []
        self.error_after_write = False
        self.concurrent_change = False

    def rootlist(self):
        result = copy.deepcopy(self.snapshot)
        if self.moves and self.concurrent_change:
            for key in ("items", "metadata"):
                result[key][0], result[key][1] = result[key][1], result[key][0]
        return result

    def move(self, revision, source, destination):
        assert revision == self.snapshot["revision"]
        self.moves.append((source, destination))
        for key in ("items", "metadata"):
            value = self.snapshot[key].pop(source)
            self.snapshot[key].insert(destination - (source < destination), value)
        self.snapshot["revision"] += "-next"
        if self.error_after_write:
            raise filing.FilingError("Uncertain write response")


class FilingTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.receipt = Path(self.temp.name) / "receipt.json"

    def test_selects_only_owned_catalog_root_playlists(self):
        snapshot = root([playlist("a"), START, playlist("b"), END,
                         OTHER_START, playlist("c"), OTHER_END, playlist("d"), playlist("e")])
        snapshot["metadata"][-2]["ownerUsername"] = "someone-else"
        actual = filing.candidates(snapshot, {playlist(c) for c in "abcd"})
        self.assertEqual([row["uri"] for row in actual], [playlist("a")])

    def test_catalog_accepts_completed_kalx_kpoo_and_legacy_ids_only(self):
        def entry(c, **kw):
            return {"station": "KALX", "state": "ready", "owner_id": filing.ACCOUNT,
                    "playlist_id": c * 22, **kw}
        catalog = {"version": 1, "entries": {
            "a": entry("a"), "b": entry("b", station="KPOO"),
            "c": entry("c", state="filling"), "d": entry("d", station="KPFA"),
            "e": entry("e", owner_id="other"),
            "f": {"station": "KALX", "state": "legacy", "listing": {
                "url": "https://open.spotify.com/playlist/" + "f" * 22}},
        }}
        self.assertEqual(filing.catalog_ids(catalog), {playlist(c) for c in "abf"})
        catalog["entries"]["a"]["listing"] = {"url": "https://open.spotify.com/playlist/" + "z" * 22}
        with self.assertRaises(filing.FilingError):
            filing.catalog_ids(catalog)

    def test_rejects_incomplete_ambiguous_duplicate_and_nested_destination(self):
        for uris in ([START, playlist("a")], [END, START], [playlist("a")],
                     [START, END, playlist("a"), playlist("a")],
                     [START, END, "spotify:start-group:duplicate:KALX", "spotify:end-group:duplicate"],
                     [OTHER_START, START, END, OTHER_END]):
            with self.subTest(uris=uris), self.assertRaises(filing.FilingError):
                filing.layout(root(uris))

    def test_files_above_and_below_folder_with_fresh_indices_and_revisions(self):
        initial = root([playlist("a"), START, playlist("b"), END, playlist("c"),
                        OTHER_START, playlist("d"), OTHER_END, playlist("e")])
        client = FakeSpotify(initial)
        receipt = filing.run(client, {playlist(c) for c in "abcd"}, self.receipt, apply=True, interval=0)
        self.assertEqual(receipt["status"], "completed")
        self.assertEqual([row["uri"] for row in receipt["completed"]], [playlist("a"), playlist("c")])
        self.assertEqual(filing.layout(client.snapshot)[0], [START, playlist("b"), playlist("a"),
            playlist("c"), END, OTHER_START, playlist("d"), OTHER_END, playlist("e")])
        self.assertEqual(client.moves, [(0, 3), (4, 3)])
        # Second run is a genuine no-op: unsaved catalog entries are not restored.
        filing.run(client, {playlist(c) for c in "abcdz"}, self.receipt, apply=True, interval=0)
        self.assertEqual(len(client.moves), 2)

    def test_dry_run_never_writes(self):
        client = FakeSpotify(root([START, END, playlist("a")]))
        result = filing.run(client, {playlist("a")}, self.receipt)
        self.assertEqual(len(result["planned"]), 1)
        self.assertEqual(client.moves, [])
        self.assertEqual(self.receipt.stat().st_mode & 0o777, 0o600)

    def test_uncertain_write_is_not_retried_and_next_run_uses_membership(self):
        client = FakeSpotify(root([START, END, playlist("a"), playlist("b")]))
        client.error_after_write = True
        with self.assertRaises(filing.FilingError):
            filing.run(client, {playlist("a"), playlist("b")}, self.receipt, apply=True, interval=0)
        self.assertEqual(len(client.moves), 1)
        receipt = json.loads(self.receipt.read_text())
        self.assertEqual(receipt["status"], "needs_review")
        self.assertEqual(receipt["pending"]["uri"], playlist("a"))
        client.error_after_write = False
        result = filing.run(client, {playlist("a"), playlist("b")}, self.receipt, apply=True, interval=0)
        self.assertEqual([x["uri"] for x in result["completed"]], [playlist("b")])

    def test_moved_playlist_track_changes_fail_verification(self):
        before = root([START, END, playlist("a")])
        client = FakeSpotify(before)
        client.move(before["revision"], 2, 1)
        after = client.rootlist()
        after["metadata"][1]["revision"] = "different-tracks"
        with self.assertRaises(filing.FilingError):
            filing.verify_move(before, after, playlist("a"))

    def test_unrelated_recommendation_decorations_can_change(self):
        before = root([START, END, playlist("a"), playlist("z")])
        before["metadata"][-1]["ownerUsername"] = "spotify"
        client = FakeSpotify(before)
        client.move(before["revision"], 2, 1)
        after = client.rootlist()
        after["metadata"][-1]["recommendationRequestId"] = "dynamic"
        filing.verify_move(before, after, playlist("a"))

    def test_unrelated_library_reorder_stops_subsequent_moves(self):
        before = root([playlist("z"), START, END, playlist("a"), playlist("b")])
        client = FakeSpotify(before)
        client.concurrent_change = True
        with self.assertRaises(filing.FilingError):
            filing.run(client, {playlist("a"), playlist("b")}, self.receipt, apply=True, interval=0)
        self.assertEqual(len(client.moves), 1)

    def test_batch_limit_leaves_remaining_for_next_run(self):
        client = FakeSpotify(root([START, END, playlist("a"), playlist("b")]))
        result = filing.run(client, {playlist("a"), playlist("b")}, self.receipt,
                            apply=True, max_moves=1, interval=0)
        self.assertEqual((result["status"], result["remaining"]), ("batch_limit_reached", 1))


class TransportTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.client = filing.Spotify(Path(self.temp.name) / "cooldown.json")

    def test_auth_cookie_only_goes_to_spotify_token_endpoint(self):
        self.client.request = Mock(return_value={"isAnonymous": False, "accessToken": "token-value",
            "accessTokenExpirationTimestampMs": (time.time() + 3600) * 1000})
        self.client.authenticate("a" * 40)
        args = self.client.request.call_args
        self.assertTrue(args.args[0].startswith("https://open.spotify.com/api/token?"))
        self.assertEqual(args.kwargs["headers"]["Cookie"], "sp_dc=" + "a" * 40)
        self.assertNotIn("Cookie", self.client.headers)
        self.assertNotIn("client-token", self.client.headers)

    def test_anonymous_session_and_cookie_header_injection_rejected(self):
        self.client.request = Mock(return_value={"isAnonymous": True, "accessToken": "secret"})
        with self.assertRaises(filing.FilingError):
            self.client.authenticate("a" * 40)
        self.client.request.reset_mock()
        with self.assertRaises(filing.FilingError):
            self.client.authenticate("a" * 40 + "; extra=bad")
        self.client.request.assert_not_called()

    def test_rejects_unexpected_hosts_and_rootlist_accounts(self):
        self.client.opener.open = Mock()
        for url in (filing.ROOT_URL.replace("dustmason", "other"), "https://example.com/api/token",
                    filing.ROOT_URL.replace("https://", "http://")):
            with self.subTest(url=url), self.assertRaises(filing.FilingError):
                self.client.request(url)
        self.client.opener.open.assert_not_called()

    def test_redirect_does_not_forward_credentials(self):
        with self.assertRaises(filing.FilingError):
            filing.NoRedirect().redirect_request(None, None, 302, None, {}, "https://example.com")

    def test_rate_limit_persists_and_blocks_followups_without_leaking_body(self):
        error = urllib.error.HTTPError(filing.ROOT_URL, 429, "secret-error", {"Retry-After": "120"}, io.BytesIO(b"secret-body"))
        self.client.opener.open = Mock(side_effect=error)
        with self.assertRaises(filing.FilingError) as raised:
            self.client.request(filing.ROOT_URL)
        self.assertNotIn("secret", str(raised.exception))
        self.assertGreater(json.loads(self.client.cooldown.read_text())["retry_at"], time.time() + 100)
        with self.assertRaises(filing.FilingError):
            self.client.request(filing.ROOT_URL)
        self.assertEqual(self.client.opener.open.call_count, 1)

    def test_retry_after_http_date_and_missing_header(self):
        self.assertEqual(filing.retry_seconds("Thu, 01 Jan 1970 00:02:00 GMT", 60), 60)
        self.assertEqual(filing.retry_seconds(None, 60), 300)

    def test_pagination_handles_folder_crossing_page_boundary(self):
        snapshot = root([START, playlist("a"), END, playlist("b")])
        def page(pos, end):
            return {"revision": "same", "length": 4, "contents": {
                "pos": pos, "truncated": end < 4, "items": snapshot["items"][pos:end],
                "metaItems": snapshot["metadata"][pos:end]}}
        self.client.request = Mock(side_effect=[page(0, 2), page(2, 4)])
        complete = self.client.rootlist()
        self.assertEqual(filing.layout(complete)[1][playlist("b")], ())
        self.assertIn("from=2", self.client.request.call_args.args[0])

    def test_pagination_rejects_revision_change_and_missing_metadata(self):
        def page(revision, pos):
            return {"revision": revision, "length": 2, "contents": {
                "pos": pos, "truncated": pos == 0,
                "items": [{"uri": START if pos == 0 else END}], "metaItems": [{}]}}
        self.client.request = Mock(side_effect=[page("first", 0), page("second", 1)])
        with self.assertRaises(filing.FilingError):
            self.client.rootlist()
        broken = page("first", 0)
        broken["contents"]["metaItems"] = []
        self.client.request = Mock(return_value=broken)
        with self.assertRaises(filing.FilingError):
            self.client.rootlist()


if __name__ == "__main__":
    unittest.main()
