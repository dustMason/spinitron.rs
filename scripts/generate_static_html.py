#!/usr/bin/env python3
"""
Generate a standalone HTML page from the JSON playlist dump.
"""

import json
import sys
import os
from datetime import datetime, timezone


# Main processing: group JSONL (playlists.jsonl) into a JSON dump and produce HTML
def main(infile):
    stations = {}
    total_playlist_count = 0
    with open(infile) as f:
        for line in f:
            if not line.strip():
                continue
            try:
                playlist = json.loads(line)
                station = playlist.get("station")
                if not station:
                    continue
                entry = {
                    "name": playlist.get("name"),
                    "url": playlist.get("url"),
                    "track_count": playlist.get("track_count"),
                    "last_updated": playlist.get("last_updated", ""),
                    "preview": playlist.get("preview", []),
                }
                stations.setdefault(station, []).append(entry)
                total_playlist_count += 1
            except (json.JSONDecodeError, KeyError) as e:
                print(f"⚠️  Warning: Failed to parse line: {line.rstrip()}", file=sys.stderr)
                print(f"   Error: {e}", file=sys.stderr)
    if not stations:
        print("❌ Error: No playlists found in input", file=sys.stderr)
        sys.exit(1)

    # Build JSON data with timestamp, station grouping, and total count
    data = {
        "stations": stations,
        "timestamp": datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC"),
        "total_playlist_count": total_playlist_count,
    }

    # Prepare for HTML rendering
    stations = data["stations"]
    ts = data["timestamp"]
    count = data["total_playlist_count"]

    # Sort playlists within each station by last_updated (newest first)
    for stn, pls in stations.items():
        pls.sort(key=lambda p: p.get("last_updated", ""), reverse=True)

    html = [
        "<!DOCTYPE html>",
        '<html lang="en"><head><meta charset="utf-8">',
        "<style>",
        "@import url('https://fonts.googleapis.com/css2?family=Permanent+Marker&display=swap');",
        "@import url('https://fonts.googleapis.com/css2?family=Special+Gothic+Expanded+One:wght@400&display=swap');",
        'body { font-family: "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; max-width: 100vw; margin: 0; padding: 1rem; background-color: #ffffff; }',
        "a { color: inherit; text-decoration: none; }",
        "h2 { font-size: 1.25rem; margin-bottom: 0.5rem; }",
        "h3 { font-family: 'Permanent Marker', cursive; font-size: 1.5rem; margin: 0 0 0.5rem; }",
        ".station { margin-bottom: 2rem; }",
        ".playlist-list { list-style: none; margin: 0; padding: 0; }",
        ".playlist-list .card { display: block; max-width: 800px; margin: 0 auto 1rem; background: #fafafa; border: 1px solid #ddd; border-radius: 8px; padding: 0.5rem; text-decoration: none; color: inherit; transition: background-color 0.2s ease; }",
        ".card:hover { background-color: #eaeaea; }",
        ".card h3 { font-size: 2rem; margin: 0 0 0.5rem; text-align: center; }",
        ".meta { font-size: 0.9rem; color: #555555; margin: 0 0 0.5rem; text-align: center; }",
        ".media-block { position: relative; display: block; margin-bottom: 0.5rem; }",
        ".preview-grid { display: grid; grid-template-columns: repeat(4, 1fr); }",
        ".preview-grid img { width: 100%; height: auto; object-fit: cover; }",
        ".overlay-all { position: absolute; inset: 0; background: rgba(0,0,0,0.1); color: #fff; display: flex; flex-wrap: wrap; justify-content: center; padding: 1rem; font-family: 'Special Gothic Expanded One', sans-serif; font-weight: 400; font-size: 3rem; text-transform: uppercase; overflow: hidden }",
        ".toc { position: fixed; top: 1rem; left: 1rem; max-width: 200px; }",
        ".toc strong { display: block; margin-bottom: 0.5rem; }",
        ".toc ul { list-style: none; padding: 0; margin: 0; }",
        ".toc li { margin-bottom: 0.5rem; }",
        ".main { margin-left: 220px; }",
        ".station h2 { margin-bottom: 0.25rem; }",
        ".station h2 + hr { margin: 0 auto 1rem; border: none; border-top: 1px solid #ccc; }",
        ".footer { text-align: center; margin-top: 2rem; font-size: 0.9rem; color: #555555; }",
        "</style></head><body>",
        "<div class='toc'><strong>Stations</strong><ul>",
    ]
    for station in sorted(stations):
        html.append(f"<li><a href='#{station}'>{station}</a></li>")
    html.append("</ul></div>")
    html.append("<div class='main'>")

    for station in sorted(stations):
        html.append(f'<div class="station" id="{station}"><h2>{station}</h2><hr/>')
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
            html.append('<div class="preview-grid">')
            # render album-art tiles
            artist_set = []
            for t in p.get("preview", [])[:12]:
                img = t.get("image_url")
                for art in t.get("artists", []):
                    if art not in artist_set:
                        artist_set.append(art)
                if img:
                    html.append(f"<img src='{img}' alt='{t.get('name','')}'/>")
            html.append("</div>")
            # overlay artist names across the entire grid
            html.append(f"<div class='overlay-all'>{' '.join(artist_set)}</div>")
            html.append("</div>")
            html.append("</a></li>")
    html.append("</ul></div>")

    # close main content and add overall timestamp footer
    html.append("</div>")
    html.append(f"<div class='footer'>Updated: {ts} · Total Playlists: {count}</div>")
    html.append("</body></html>")
    os.makedirs(os.path.dirname("docs/index.html"), exist_ok=True)
    with open("docs/index.html", "w") as f:
        f.write("\n".join(html))


if __name__ == "__main__":
    if len(sys.argv) != 2:
        print(f"Usage: {sys.argv[0]} <playlists.jsonl>", file=sys.stderr)
        sys.exit(1)
    main(sys.argv[1])
