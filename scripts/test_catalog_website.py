import contextlib
from datetime import datetime
from html.parser import HTMLParser
import io
import json
from pathlib import Path
import tempfile
import unittest

from generate_static_html import main, normalize
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
            self.assertIn('aria-label="Expand 1-song sample', html)

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
