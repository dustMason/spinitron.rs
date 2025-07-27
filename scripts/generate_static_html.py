#!/usr/bin/env python3
"""
Generate a standalone HTML page from the JSON playlist dump.
"""

import json
import sys
import os
from datetime import datetime, timezone
import random


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
                # skip empty playlists
                track_count = playlist.get("track_count", 0) or 0
                if track_count == 0:
                    continue
                entry = {
                    "name": playlist.get("name"),
                    "url": playlist.get("url"),
                    "track_count": track_count,
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
        "@import url('https://fonts.googleapis.com/css2?family=Libre+Bodoni:ital@1&display=swap');",
        'body { font-family: "Segoe UI", Roboto, "Helvetica Neue", Arial, sans-serif; max-width: 100vw; margin: 0; padding: 1rem; background-color: #ffffff; background-image: url(bg.jpg); background-attachment: fixed; background-size: cover }',
        "a { color: inherit; text-decoration: none; }",
        "h2 { font-size: 1.25rem; margin-bottom: 0.5rem; }",
        "h3 { font-family: 'Permanent Marker', cursive; font-size: 1.5rem; margin: 0 0 0.5rem; }",
        ".station { margin-bottom: 2rem; }",
        ".playlist-list { list-style: none; margin: 0; padding: 0; }",
        ".playlist-list .card { display: block; max-width: 800px; text-decoration: none; color: inherit; transition: background-color 0.5s ease; }",
        ".card:hover { background-color: #000; }",
        ".card h3 { font-size: 2rem; margin: 0 0 0.5rem; text-align: center; }",
        ".meta { font-size: 0.9rem; color: #555555; margin: 0 0 0.5rem; text-align: center; }",
        ".media-block { position: relative; margin-bottom: 2rem; }",
        ".preview-grid { display: grid; grid-template-columns: repeat(4, 1fr); }",
        ".preview-grid img { width: 100%; height: auto; object-fit: cover; }",
        ".overlay-all { position: relative; overflow: hidden; }",
        ".playlist-name { position: absolute; z-index: 5; text-align: left; }",
        ".playlist-name > span { position: absolute; z-index: 10; background: #000; color: #fff; word-break: break-word; }",
        ".overlay-all .mask-text { position: absolute; inset: 0; padding: 0.5rem; font-family: 'Special Gothic Expanded One', sans-serif; font-weight: 400; font-size: 4.12rem; color: #000; text-transform: uppercase; text-align: justify; line-height: 0.9; word-break: break-all; transition: opacity 0.5s ease; }",
        ".card:hover .overlay-all .mask-text { opacity: 0 }",
        ".header-bar { position: relative; padding: 2rem; color: #fff; display: flex; flex-direction: column; justify-content: center; }",
        ".header-bar .title { font-family: 'Libre Bodoni', serif; font-style: italic; font-size: 28px; margin: 0; }",
        ".header-bar .timestamp { font-family: 'Libre Bodoni', serif; font-style: italic; font-size: 16px; margin: 0; }",
        ".media-block img { mix-blend-mode: lighten; }",
        ".badge { position: absolute; top: 4.5rem; right: -1.5rem; z-index: 10; background: #e63946; color: #fff; border-radius: 50%; width: 5rem; height: 5rem; display: flex; align-items: center; justify-content: center; font-size: 1.8rem; font-weight: bold; }",
        ".toc { position: fixed; top: 1rem; left: 1rem; max-width: 200px; }",
        ".toc strong { display: block; margin-bottom: 0.5rem; }",
        ".toc ul { list-style: none; padding: 0; margin: 0; }",
        ".toc li { margin-bottom: 0.5rem; }",
        ".main { margin-left: 220px; }",
        "@media (max-width: 768px) {",
        "  .toc { position: static; margin: 0 0 1rem; max-width: none; }",
        "  .main { margin-left: 0; }",
        "  .overlay-all .mask-text { font-size: 2rem; }",
        "  .badge { position: relative; top: auto; right: auto; margin-left: 0.5rem; }",
        "}",
        "@media (max-width: 768px) {",
        "  .toc { position: static; margin-bottom: 1rem; max-width: none; }",
        "  .main { margin-left: 0; }",
        "  .overlay-all .mask-text { font-size: 2.5rem; }",
        "  .badge { position: relative; top: auto; right: auto; margin-left: 0.5rem; }",
        "}" ,
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
            # Build side-by-side artist list + preview grid
            html.append('<div class="media-block">')
            # wrap both images and text in a dark blending container
            artist_set = []
            # collect unique artist names first
            for t in p.get("preview", [])[:12]:
                for art in t.get("artists", []):
                    if art not in artist_set:
                        artist_set.append(art)
            # join artists with a random symbol between each name
            symbols = [
                '◆', '◇', '•', '×', '/', '\\', '✦', '✧', '✵', '✶', '✹', '✺',
                '✿', '❖', '❂', '❄', '❈', '❉', '❋', '◈', '▫', '▱',
                '✢', '✣', '✤', '✥', '✦', '✧', '★', '☆', '☉', '☾', '☽'
            ]
            txt = ''
            if artist_set:
                txt = artist_set[0]
                for art in artist_set[1:]:
                    sep = random.choice(symbols)
                    txt += f" {sep} {art}"
            # choose a theme color from a fixed palette based on playlist name
            palette = [
                '#896241ff',  # raw-umber
                '#422A19ff',  # bistre
                '#88B1D4ff',  # carolina-blue
                '#A9C8D8ff',  # columbia-blue
                '#5A7ACFff',  # glaucous
            ]
            # stable selection via MD5 of playlist name
            import hashlib
            name_hash = hashlib.md5(p['name'].encode('utf-8')).hexdigest()
            idx = int(name_hash[:8], 16) % len(palette)
            color = palette[idx]
            # header bar for title + timestamp (above masked grid)
            last_up = p.get('last_updated', '')
            html.append(f"<div class='header-bar' style='background:{color}'>")
            html.append(f"<div class='badge'>{p.get('track_count',0)}</div>")
            html.append(f"<div class='title'>{p['name']}</div>")
            html.append(f"<div class='timestamp'>Last Updated {last_up}</div>")
            html.append("</div>")
            html.append("<div class='overlay-all'>")
            # overlay with artists only (title removed)
            html.append(f"<div class='mask-text'>{txt}</div>")
            html.append('<div class="preview-grid">')
            for t in p.get("preview", [])[:12]:
                img = t.get("image_url")
                if img:
                    html.append(f"<img src='{img}' alt='{t.get('name','')}'/>")
            html.append("</div>")
            # sticker showing track count
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
