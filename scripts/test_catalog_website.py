import json
from html.parser import HTMLParser
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class Page(HTMLParser):
    def __init__(self):
        super().__init__()
        self.images = []
        self.stack = []
        self.unbalanced = []

    def handle_starttag(self, tag, attrs):
        if tag == "img":
            self.images.append(dict(attrs))
        if tag in ("div", "ul", "li", "a"):
            self.stack.append(tag)

    def handle_endtag(self, tag):
        if tag in ("div", "ul", "li", "a"):
            if not self.stack or self.stack.pop() != tag:
                self.unbalanced.append(tag)


class CatalogWebsiteTest(unittest.TestCase):
    def test_broadcast_titles_are_escaped_and_station_sections_are_balanced(self):
        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            title = "Don't Stop — 音楽 & Friends"
            rows = [dict(station=s, name=title, url="https://open.spotify.com/playlist/example", track_count=1,
                         preview=[dict(name=title, artists=["Artist & Friends"], image_url="https://example.com/cover.jpg")])
                    for s in ("KALX", "KPOO")]
            data = root / "playlists.jsonl"
            data.write_text("\n".join(json.dumps(row) for row in rows))
            subprocess.run([sys.executable, str(Path(__file__).with_name("generate_static_html.py")), str(data)], cwd=root, check=True)
            html = (root / "docs/index.html").read_text()
            page = Page()
            page.feed(html)
            self.assertEqual([img["alt"] for img in page.images], [title, title])
            self.assertEqual(page.stack, [])
            self.assertEqual(page.unbalanced, [])
            self.assertIn('name="viewport"', html)


if __name__ == "__main__":
    unittest.main()
