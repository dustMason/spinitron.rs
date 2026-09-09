import contextlib
import copy
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch
import urllib.error
import urllib.parse

import cleanup_spinitron_library as cleanup


def item(playlist_id, owner="dustmason", count=3):
    return {"id": playlist_id, "name": f"KALX - {playlist_id}", "description": "Generated from Spinitron playlists",
            "owner": {"id": owner}, "public": True, "snapshot_id": "original", "tracks": {"total": count}}


def catalog():
    return {"version": 1, "entries": {i: {"state": state, "playlist_id": i} for i, state in [
        ("a", "legacy"), ("b", "legacy"), ("otherowner", "legacy"), ("draft", "filling"), ("new", "released")]}}


class FakeSpotify:
    def __init__(self):
        self.playlists = {i: item(i, owner="someoneelse" if i == "otherowner" else "dustmason", count=0 if i == "b" else 3)
                          for i in ["a", "b", "otherowner", "draft", "new", "unlisted"]}
        self.library = set(self.playlists)
        self.removed = []
        self.fail_remove = False

    def owner(self):
        return "dustmason"

    def request(self, method, path):
        assert method == "GET"
        offset = int(urllib.parse.parse_qs(urllib.parse.urlsplit(path).query)["offset"][0])
        values = list(self.playlists.values())
        return {"items": copy.deepcopy(values[offset:offset + 3]), "next": "next-page" if offset + 3 < len(values) else None}

    def metadata(self, playlist_id):
        return copy.deepcopy(self.playlists[playlist_id])

    def saved(self, playlist_id):
        return playlist_id in self.library

    def remove(self, playlist_id):
        self.removed.append(playlist_id)
        self.library.discard(playlist_id)
        if self.fail_remove:
            raise RuntimeError("Connection lost after write")

    def saved_many(self, playlist_ids):
        return [self.saved(pid) for pid in playlist_ids]

    def remove_many(self, playlist_ids):
        for pid in playlist_ids:
            self.remove(pid)


class FakeGitHub:
    def __init__(self):
        self.workflow = "run: ./target/release/spinitron-scraper --archive\n"
        self.runs = []
        self.published = ["a", "b", "otherowner"]

    def get(self, path):
        if "workflow" in path:
            return {"workflow_runs": self.runs}
        return {"status": "built", "commit": "a" * 40}

    def file(self, path, ref):
        if path.endswith(".yml"):
            return self.workflow
        if path == "data/catalog.json":
            return json.dumps(catalog())
        return json.dumps([{"url": "https://open.spotify.com/playlist/" + i} for i in self.published])


class CleanupTest(unittest.TestCase):
    def setUp(self):
        redirect = contextlib.redirect_stdout(io.StringIO())
        redirect.__enter__()
        self.addCleanup(redirect.__exit__, None, None, None)
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.output = Path(self.temp.name) / "cleanup"
        self.spotify = FakeSpotify()
        self.github = FakeGitHub()
        self.plan_path = self.output / "plan.json"

    def prepare(self):
        return cleanup.prepare(self.spotify, catalog(), self.output)

    def apply(self):
        cleanup.apply(self.spotify, catalog(), self.plan_path, self.github)

    def test_plan_is_read_only_and_retains_backup_and_uncataloged_report(self):
        before = copy.deepcopy(self.spotify.playlists)
        plan = self.prepare()
        self.assertEqual([p["playlist_id"] for p in plan["playlists"]], ["a", "b"])
        self.assertEqual(plan["playlists"][1]["track_count"], 0)
        self.assertEqual([p["playlist_id"] for p in plan["unlisted_generated_playlists"]], ["unlisted"])
        self.assertEqual(json.loads((self.output / "catalog-backup.json").read_text()), catalog())
        self.assertEqual(self.spotify.playlists, before)
        self.assertEqual(self.spotify.removed, [])

    def test_apply_preserves_playlist_contents_and_saved_again_survives_resume(self):
        self.prepare()
        before = copy.deepcopy(self.spotify.playlists)
        self.apply()
        self.assertEqual(self.spotify.removed, ["a", "b"])
        self.assertEqual(self.spotify.playlists, before)
        self.assertTrue({"draft", "new", "otherowner", "unlisted"} <= self.spotify.library)
        self.spotify.library.add("a")
        self.apply()
        self.assertIn("a", self.spotify.library)
        self.assertEqual(self.spotify.removed, ["a", "b"])
        receipt = json.loads((self.output / "receipt.json").read_text())
        self.assertEqual(receipt["results"]["a"]["preserved"]["snapshot_id"], "original")

    def test_old_workflow_running_job_or_unpublished_link_prevents_all_removals(self):
        self.prepare()
        for condition in ["old", "running", "missing"]:
            with self.subTest(condition=condition):
                self.github = FakeGitHub()
                if condition == "old":
                    self.github.workflow = "run: ./target/release/spinitron-scraper --spotify\n"
                elif condition == "running":
                    self.github.runs = [{"status": "in_progress"}]
                else:
                    self.github.published = ["a"]
                with self.assertRaises(RuntimeError):
                    self.apply()
                self.assertEqual(self.spotify.removed, [])

    def test_changed_playlist_wrong_account_or_added_id_prevents_removal(self):
        plan = self.prepare()
        self.spotify.playlists["a"]["snapshot_id"] = "edited"
        with self.assertRaises(RuntimeError):
            self.apply()
        self.spotify.playlists["a"]["snapshot_id"] = "original"
        for change in ["owner", "extra"]:
            altered = copy.deepcopy(plan)
            if change == "owner":
                altered["owner_id"] = "someoneelse"
            else:
                altered["playlists"].append(cleanup.metadata_record(item("unlisted"), "dustmason"))
            cleanup.save(self.plan_path, altered)
            with self.assertRaises(ValueError):
                self.apply()
        self.assertEqual(self.spotify.removed, [])

    def test_uncertain_write_is_checkpointed_and_never_retried(self):
        self.prepare()
        self.spotify.fail_remove = True
        with self.assertRaises(RuntimeError):
            self.apply()
        self.assertEqual(json.loads((self.output / "receipt.json").read_text())["results"]["a"]["status"], "uncertain")
        self.spotify.fail_remove = False
        self.spotify.library.add("a")
        with self.assertRaisesRegex(RuntimeError, "uncertain previous"):
            self.apply()
        self.assertIn("a", self.spotify.library)
        self.assertEqual(self.spotify.removed, ["a", "b"])

    def test_already_absent_is_not_removed_and_failed_preservation_stops(self):
        self.prepare()
        self.spotify.library.discard("a")
        original = self.spotify.remove
        def remove_and_change(playlist_id):
            original(playlist_id)
            self.spotify.playlists[playlist_id]["tracks"]["total"] += 1
        self.spotify.remove = remove_and_change
        with self.assertRaises(RuntimeError):
            self.apply()
        self.assertEqual(self.spotify.removed, ["b"])
        receipt = json.loads((self.output / "receipt.json").read_text())
        self.assertEqual(receipt["results"]["a"]["status"], "already_absent")
        self.assertEqual(receipt["results"]["b"]["status"], "uncertain")

    def test_transport_only_allows_library_removal_and_does_not_retry_writes(self):
        client = cleanup.Spotify("id", "secret", "refresh")
        client.token, client.expires = "test", float("inf")
        with patch.object(cleanup.urllib.request, "urlopen", return_value=io.BytesIO(b"")) as request:
            client.remove("abc123")
            actual = request.call_args.args[0]
            self.assertEqual(actual.method, "DELETE")
            self.assertEqual(urllib.parse.urlsplit(actual.full_url).path, "/v1/me/library")
            self.assertEqual(urllib.parse.parse_qs(urllib.parse.urlsplit(actual.full_url).query), {"uris": ["spotify:playlist:abc123"]})
        with patch.object(cleanup.urllib.request, "urlopen") as request:
            for method, path in [("DELETE", "/playlists/a/tracks"), ("PUT", "/playlists/a"), ("DELETE", "/me/library?uris=spotify:track:a")]:
                with self.assertRaises(ValueError):
                    client.request(method, path)
            request.assert_not_called()
        failure = urllib.error.HTTPError("https://api.spotify.com/v1/me/library", 503, "failed", {}, None)
        with patch.object(cleanup.urllib.request, "urlopen", side_effect=failure) as request:
            with self.assertRaises(RuntimeError):
                client.remove("a")
            self.assertEqual(request.call_count, 1)

    def test_github_uses_personal_token_and_rejects_wrong_identity(self):
        def result(text):
            return cleanup.subprocess.CompletedProcess([], 0, stdout=text)
        with patch.object(cleanup.subprocess, "run", side_effect=[result("personal-test"), result('{"login":"work-user"}')]) as run:
            with self.assertRaises(RuntimeError):
                cleanup.PersonalGitHub()
            self.assertEqual(run.call_args_list[0].args[0][-2:], ["--user", "dustMason"])
            self.assertEqual(run.call_args_list[1].kwargs["env"]["GH_TOKEN"], "personal-test")

    def test_batch_preserves_playlists_and_saved_again_is_not_removed_on_resume(self):
        self.prepare()
        before = copy.deepcopy(self.spotify.playlists)
        cleanup.apply(self.spotify, catalog(), self.plan_path, self.github, batch_size=40)
        self.assertEqual(self.spotify.removed, ["a", "b"])
        self.assertEqual(before, self.spotify.playlists)
        self.assertTrue({"draft", "new", "otherowner", "unlisted"} <= self.spotify.library)
        self.spotify.library.add("a")
        cleanup.apply(self.spotify, catalog(), self.plan_path, self.github, batch_size=40)
        self.assertIn("a", self.spotify.library)
        self.assertEqual(self.spotify.removed, ["a", "b"])

    def test_batch_changed_metadata_aborts_before_any_mutation(self):
        self.prepare()
        self.spotify.playlists["b"]["snapshot_id"] = "changed"
        with self.assertRaisesRegex(RuntimeError, "changed since planning"):
            cleanup.apply(self.spotify, catalog(), self.plan_path, self.github, batch_size=40)
        self.assertEqual(self.spotify.removed, [])

    def test_batch_uncertain_write_is_fully_checkpointed_and_never_retried(self):
        self.prepare()
        self.spotify.fail_remove = True
        with self.assertRaisesRegex(RuntimeError, "uncertain batch"):
            cleanup.apply(self.spotify, catalog(), self.plan_path, self.github, batch_size=40)
        receipt = json.loads((self.output / "receipt.json").read_text())
        self.assertEqual({r["status"] for r in receipt["results"].values()}, {"uncertain"})
        self.assertEqual(set(receipt["results"]), {"a", "b"})
        self.spotify.fail_remove = False
        self.spotify.library.add("a")
        before = list(self.spotify.removed)
        with self.assertRaisesRegex(RuntimeError, "uncertain previous"):
            cleanup.apply(self.spotify, catalog(), self.plan_path, self.github, batch_size=40)
        self.assertEqual(self.spotify.removed, before)
        self.assertIn("a", self.spotify.library)

    def test_batch_absent_items_are_skipped_and_failed_preservation_stops(self):
        self.prepare()
        self.spotify.library.discard("a")
        original = self.spotify.remove
        def remove_and_change(pid):
            original(pid)
            self.spotify.playlists[pid]["tracks"]["total"] += 1
        self.spotify.remove = remove_and_change
        with self.assertRaisesRegex(RuntimeError, "uncertain batch"):
            cleanup.apply(self.spotify, catalog(), self.plan_path, self.github, batch_size=40)
        receipt = json.loads((self.output / "receipt.json").read_text())
        self.assertEqual(receipt["results"]["a"]["status"], "already_absent")
        self.assertEqual(receipt["results"]["b"]["status"], "uncertain")
        self.assertEqual(self.spotify.removed, ["b"])

    def test_batch_transport_rejects_duplicate_mixed_and_oversized_requests(self):
        client = cleanup.Spotify("id", "secret", "refresh")
        client.token, client.expires = "test", float("inf")
        with patch.object(cleanup.urllib.request, "urlopen", return_value=io.BytesIO(b"")) as request:
            client.remove_many(["a", "b"])
            actual = request.call_args.args[0]
            self.assertEqual(actual.method, "DELETE")
            self.assertEqual(urllib.parse.parse_qs(urllib.parse.urlsplit(actual.full_url).query),
                             {"uris": ["spotify:playlist:a,spotify:playlist:b"]})
        with patch.object(cleanup.urllib.request, "urlopen") as request:
            for ids in [[], ["a", "a"], [str(i) for i in range(41)], ["a", "spotify:track:b"]]:
                with self.assertRaises(ValueError):
                    client.remove_many(ids)
            request.assert_not_called()


if __name__ == "__main__":
    unittest.main()
