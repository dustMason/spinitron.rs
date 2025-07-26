#!/usr/bin/env python3
"""
Generate a standalone HTML page from the JSON playlist dump.
"""

import json
import sys


def main(infile, outfile):
    data = json.load(open(infile))
    stations = data.get("stations", {})
    ts = data.get("timestamp", "")
    count = data.get("total_playlist_count", 0)

    # Sort playlists within each station by last_updated (newest first)
    for stn, pls in stations.items():
        pls.sort(key=lambda p: p.get("last_updated", ""), reverse=True)

    html = [
        "<!DOCTYPE html>",
        '<html lang="en"><head><meta charset="utf-8">',
        "<title>Radio Station Spotify Playlists</title>",
        "<style>",
        "@import url('https://fonts.googleapis.com/css2?family=Permanent+Marker&display=swap');",
        'body { font-family: "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; max-width: 100vw; margin: 0; padding: 1rem; background: #ffffff; color: #111111; }',
        "a { color: inherit; text-decoration: none; }",
        "h1 { font-size: 1.5rem; margin-bottom: 0.5rem; }",
        "h2 { font-size: 1.25rem; margin-bottom: 0.5rem; }",
        "h3 { font-family: 'Permanent Marker', cursive; font-size: 1.5rem; margin: 0 0 0.5rem; }",
        ".station { margin-bottom: 2rem; }",
        ".playlist-list { list-style: none; margin: 0; padding: 0; }",
        ".playlist-list .card { display: block; max-width: 800px; margin: 0 auto 1rem; background: #fafafa; border-radius: 8px; padding: 1rem; text-decoration: none; color: inherit; transition: background-color 0.2s ease; }",
        ".card:hover { background-color: #eaeaea; }",
        ".card h3 { font-size: 2rem; margin: 0 0 0.5rem; }",
        ".meta { font-size: 0.9rem; color: #555555; margin: 0 0 0.5rem; }",
        ".media-block { display: grid; grid-template-columns: repeat(5, 1fr); gap: 0.25rem; margin-bottom: 0.5rem; }",
        ".artists-list { grid-column: 1; list-style: none; margin: 0; padding: 0; }",
        ".artists-list li { margin-bottom: 0.5rem; }",
        ".preview-grid { grid-column: 2 / span 4; display: grid; grid-template-columns: repeat(4, 1fr); gap: 0.25rem; }",
        ".preview-grid img { width: 100%; height: auto; object-fit: cover; border-radius: 4px; }",
        "</style></head><body>",
        f"<h1>Radio Station Spotify Playlists</h1>",
        f"<p>Updated: {ts} · Total Playlists: {count}</p>",
    ]

    for station in sorted(stations):
        html.append(f'<div class="station"><h2>{station}</h2>')
        html.append('<ul class="playlist-list">')
        # ensure playlists sorted by last_updated descending
        for p in sorted(
            stations[station], key=lambda p: p.get("last_updated", ""), reverse=True
        ):
            html.append(f"<li><a class='card' href='{p['url']}'>")
            html.append(f"<h3>{p['name']}</h3>")
            html.append(
                f"<p class='meta'>{p.get('track_count', 0)} songs · {p.get('last_updated', '')}</p>"
            )
            # Build side-by-side artist list + preview grid
            seen = set()
            html.append('<div class="media-block">')
            html.append('<ul class="artists-list">')
            for t in p.get("preview", [])[:12]:
                for artist in t.get("artists", []):
                    if artist not in seen:
                        seen.add(artist)
                        html.append(f"<li>{artist}</li>")
            html.append("</ul>")
            html.append('<div class="preview-grid">')
            for t in p.get("preview", [])[:12]:
                img = t.get("image_url")
                name = t.get("name", "")
                if img:
                    html.append(f"<img src='{img}' alt='{name}'/>")
            html.append("</div>")
            html.append("</div>")
            html.append("</a></li>")
        html.append("</ul></div>")

    html.append("</body></html>")
    with open(outfile, "w") as f:
        f.write("\n".join(html))


if __name__ == "__main__":
    in_json = sys.argv[1] if len(sys.argv) > 1 else "docs/playlists.json"
    out_html = sys.argv[2] if len(sys.argv) > 2 else "docs/index.html"
    main(in_json, out_html)
