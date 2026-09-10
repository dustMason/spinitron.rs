import contextlib
from datetime import datetime
from html.parser import HTMLParser
import io
import json
from pathlib import Path
import tempfile
import unittest

from generate_static_html import clock_time, main, normalize, unique_titles
from zoneinfo import ZoneInfo

NOW = datetime.fromisoformat("2026-09-09T02:00:00+00:00")
ZONE = ZoneInfo("America/Los_Angeles")


def playlist(i, **extra):
    return dict(station="KALX", name=f"KALX - Show {i:03}", url=f"https://open.spotify.com/playlist/{i}",
                track_count=15, last_updated="2026-09-08 21:00 UTC",
                preview=[dict(name="Don't Stop — 音楽 <live>", artists=["Artist & Friends"],
                              image_url="https://example.com/art.jpg")], **extra)


class Page(HTMLParser):
    VOID = {"area", "base", "br", "col", "embed", "hr", "img", "input", "link", "meta", "param", "source", "track", "wbr"}

    def __init__(self, html):
        super().__init__()
        self.stack, self.errors, self.rows, self.days, self.text = [], [], [], [], []
        self.feed(html)

    def handle_starttag(self, tag, attrs):
        attrs = dict(attrs)
        if tag not in self.VOID:
            self.stack.append(tag)
        if tag == "article" and attrs.get("class") == "playlist-row":
            self.rows.append(attrs["data-playlist-id"])
        if tag == "section" and attrs.get("class") == "day-section":
            self.days.append(attrs["id"])

    def handle_endtag(self, tag):
        if not self.stack or self.stack.pop() != tag:
            self.errors.append(tag)

    def handle_data(self, text):
        self.text.append(text)


class CatalogWebsiteTest(unittest.TestCase):
    def generate(self, rows, root):
        data = root / "input.jsonl"
        data.write_text("\n".join(json.dumps(r) for r in rows), encoding="utf-8")
        with contextlib.redirect_stdout(io.StringIO()):
            main(data, root / "docs", now=NOW)
        return root / "docs"

    def test_titles_use_broadcast_date_and_disambiguate_same_day_broadcasts(self):
        rows = []
        for i, start in enumerate(["2026-09-01T01:00:00-0700", "2026-09-01T07:00:00-0700",
                                   "2026-08-31T22:00:00-0700"]):
            row = playlist(i, imported_at="2026-09-09T00:00:00Z", broadcast_start=start)
            row["name"] = "KALX - " + datetime.fromisoformat(start).strftime("%Y-%m-%d %H:%M") + " - FREEFORM"
            rows.append(normalize(row, ZONE))
        unique_titles(rows)
        self.assertEqual([r["title"] for r in rows], [
            "FREEFORM - 2026-09-01 1:00am",
            "FREEFORM - 2026-09-01 7:00am",
            "FREEFORM - 2026-08-31",
        ])
        self.assertTrue(all(r["day"] == "2026-09-08" for r in rows))
        self.assertTrue(all("_title_time" not in r for r in rows))

    def test_canonical_names_are_shared_with_spotify_and_suffixes_are_not_doubled(self):
        rows = []
        for i, hour in enumerate([0, 12, 17]):
            start = f"2026-09-09T{hour:02}:00:00-0700"
            self.assertEqual(clock_time(datetime.fromisoformat(start)), ["12:00am", "12:00pm", "5:00pm"][i])
            row = playlist(i, broadcast_start=start)
            row["name"] = "KALX - Radio Dunya - 2026-09-09"
            normalized = normalize(row, ZONE)
            self.assertEqual(normalized["title"], "Radio Dunya - 2026-09-09")
            row["display_name"] = "KALX - FREEFORM - 2026-09-09 " + clock_time(datetime.fromisoformat(start))
            rows.append(normalize(row, ZONE))
        unique_titles(rows)
        self.assertEqual([r["title"] for r in rows], [
            "FREEFORM - 2026-09-09 12:00am", "FREEFORM - 2026-09-09 12:00pm", "FREEFORM - 2026-09-09 5:00pm"])
        self.assertIn("5:00pm", rows[2]["broadcast_label"])
        self.assertNotIn("17:00", rows[2]["broadcast_label"])

    def test_legacy_dates_are_labeled_and_exact_collisions_are_stable(self):
        originals = [playlist(i) for i in range(4)]
        for row in originals:
            row["name"] = "KALX - FREEFORM"
        originals[0]["last_updated"] = "2026-07-18 08:04 UTC"
        originals[1]["last_updated"] = "2026-07-20 09:11 UTC"
        rows = [normalize(r, ZONE) for r in originals]
        unique_titles(rows)
        self.assertEqual(rows[0]["title"], "FREEFORM · updated 2026-07-18")
        self.assertEqual(rows[1]["title"], "FREEFORM · updated 2026-07-20")
        self.assertEqual(len({r["title"] for r in rows}), 4)
        reverse = [normalize(r, ZONE) for r in reversed(originals)]
        unique_titles(reverse)
        self.assertEqual({r["id"]: r["title"] for r in rows}, {r["id"]: r["title"] for r in reverse})
        self.assertTrue(all(r["name"] == "KALX - FREEFORM" for r in rows))

    def test_fall_back_hour_and_missing_dates_have_unique_titles(self):
        originals = [playlist(i) for i in range(4)]
        for row in originals:
            row.update(name="KALX - FREEFORM", last_updated="")
        originals[0]["broadcast_start"] = "2025-11-02T01:00:00-0700"
        originals[1]["broadcast_start"] = "2025-11-02T01:00:00-0800"
        rows = [normalize(r, ZONE) for r in originals]
        unique_titles(rows)
        self.assertEqual(rows[0]["title"], "FREEFORM - 2025-11-02 1:00am -0700")
        self.assertEqual(rows[1]["title"], "FREEFORM - 2025-11-02 1:00am -0800")
        self.assertIn("date unavailable", rows[2]["title"])
        self.assertEqual(len({r["title"] for r in rows}), 4)

    def test_dates_and_titles_are_shared_by_static_pages_and_search_data(self):
        rows = [playlist(i, broadcast_start="2026-09-01T01:00:00-0700") for i in range(26)]
        for row in rows:
            row["name"] = "KALX - 2026-09-01 01:00 - FREEFORM"
        with tempfile.TemporaryDirectory() as tmp:
            output = self.generate(rows, Path(tmp))
            data = json.loads((output / "assets/catalog-data.json").read_text())
            self.assertEqual(len({r["title"] for r in data}), 26)
            pages = "".join("".join(Page(p.read_text()).text) for p in (output / "archive").glob("*.html"))
            for row in data:
                self.assertEqual(pages.count(row["title"]), 1)

    def test_seven_calendar_days_use_import_time_in_pacific_with_legacy_fallback(self):
        rows = [
            playlist(1, imported_at="2026-09-09T00:00:00Z"),
            playlist(2, imported_at="2026-09-02T07:00:00Z"),
            playlist(3, imported_at="2026-09-02T06:59:59Z"),
            playlist(4, imported_at="2026-08-01T00:00:00Z"),
        ]
        # Updated date must not override the original import date.
        rows.append(playlist(5))
        with tempfile.TemporaryDirectory() as tmp:
            output = self.generate(rows, Path(tmp))
            page = Page((output / "index.html").read_text())
            self.assertEqual(page.days, [f"day-2026-09-{d:02}" for d in range(8, 1, -1)])
            expected = {normalize(rows[i], ZONE)["id"] for i in [0, 1, 4]}
            self.assertEqual(set(page.rows), expected)
            self.assertEqual(page.errors, [])
            self.assertEqual(page.stack, [])
            self.assertIn("No imports on this day.", "".join(page.text))

    def test_archive_contains_every_record_once_across_pages_including_empty_undated(self):
        rows = [playlist(i) for i in range(56)]
        rows[0].update(track_count=0, preview=[], last_updated="")
        with tempfile.TemporaryDirectory() as tmp:
            output = self.generate(list(reversed(rows)), Path(tmp))
            ids = []
            for filename, count in [("index.html", 25), ("page-2.html", 25), ("page-3.html", 6)]:
                page = Page((output / "archive" / filename).read_text())
                self.assertEqual(len(page.rows), count)
                self.assertEqual(page.stack, [])
                self.assertEqual(page.errors, [])
                ids.extend(page.rows)
            self.assertEqual(len(ids), len(set(ids)))
            self.assertEqual(set(ids), {normalize(r, ZONE)["id"] for r in rows})
            self.assertEqual(len((output / "playlists.jsonl").read_text().splitlines()), 56)
            self.assertIn("Date unavailable", (output / "archive/page-3.html").read_text())
            self.assertIn('href="page-2.html"', (output / "archive/index.html").read_text())
            self.assertIn('href="../assets/catalog.css', (output / "archive/page-3.html").read_text())

    def test_samples_preserve_unicode_and_quotes_without_broken_markup(self):
        row = playlist(1)
        row["name"] = 'KALX - Don\'t Stop — 音楽 & Friends <live>'
        row["preview"].extend([
            dict(row["preview"][0], artists=["Artist & Friends"]),
            dict(row["preview"][0], artists=["Second artist", "Third artist"]),
            dict(row["preview"][0], artists=["Fourth artist"]),
        ])
        with tempfile.TemporaryDirectory() as tmp:
            output = self.generate([row], Path(tmp))
            html = (output / "index.html").read_text()
            page = Page(html)
            self.assertIn(row["name"].removeprefix("KALX - "), "".join(page.text))
            self.assertIn("Don't Stop — 音楽 <live>", "".join(page.text))
            self.assertNotIn("<live>", html)
            self.assertIn('loading="lazy"', html)
            self.assertEqual(page.errors, [])
            self.assertEqual(page.stack, [])
            self.assertIn('class="sample-strip"', html)
            self.assertIn('aria-label="Expand 4-song sample', html)
            summary = html.split('<summary', 1)[1].split('</summary>', 1)[0].split('>', 1)[1]
            self.assertEqual(summary.count('class="preview-artist"'), 3)
            for artist in ["Artist &amp; Friends", "Second artist", "Third artist"]:
                self.assertIn(artist, summary)
            self.assertNotIn("Fourth artist", summary)
            self.assertNotIn("<img", summary)
            self.assertNotIn('class="song-title"', summary)
            self.assertIn("Fourth artist", html)
            self.assertEqual(html.count('<li class="song">'), 4)

    def test_rebuild_removes_surplus_generated_pages_and_handles_empty_catalog(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = self.generate([playlist(i) for i in range(51)], root)
            self.assertTrue((output / "archive/page-3.html").exists())
            (output / "archive/notes.txt").write_text("keep")
            self.generate([], root)
            self.assertFalse((output / "archive/page-2.html").exists())
            self.assertFalse((output / "archive/page-3.html").exists())
            self.assertTrue((output / "archive/notes.txt").exists())
            self.assertEqual(Page((output / "archive/index.html").read_text()).rows, [])
            self.assertEqual(len(Page((output / "index.html").read_text()).days), 7)

    def test_conflicting_old_counts_are_not_presented_as_zero(self):
        row = normalize(playlist(1), ZONE)
        self.assertEqual(row["count_label"], "15")
        old = playlist(2)
        old["track_count"] = 0
        self.assertEqual(normalize(old, ZONE)["count_label"], "—")


if __name__ == "__main__":
    unittest.main()
