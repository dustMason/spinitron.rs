#!/usr/bin/env python3
"""
Generate a standalone HTML page from the JSON playlist dump.
"""

import json
import sys


def main(infile, outfile):
    data = json.load(open(infile))
    stations = data.get('stations', {})
    ts = data.get('timestamp', '')
    count = data.get('total_playlist_count', 0)

    # Sort playlists within each station by last_updated (newest first)
    for stn, pls in stations.items():
        pls.sort(key=lambda p: p.get('last_updated', ''), reverse=True)

    html = [
        '<!DOCTYPE html>',
        '<html lang="en"><head><meta charset="utf-8">',
        '<title>Radio Station Spotify Playlists</title>',
        '<style>',
        'body{font-family:"Segoe UI",Roboto,"Helvetica Neue",Arial,sans-serif;max-width:100vw;margin:0;padding:1rem;background:#121212;color:#e0e0e0}',
        'a{color:#bb86fc;text-decoration:none}',
        'a:hover{text-decoration:underline}',
        'h1{font-size:1.5rem;margin-bottom:0.5rem}',
        'h2{font-size:1.25rem;margin-bottom:0.5rem}',
        'table{width:100%;border-collapse:collapse;margin-bottom:2rem}',
        'th,td{border:1px solid #333;padding:0.5rem;vertical-align:top}',
        'th{background:#1f1f1f}',
        '.preview-item{display:inline-block;margin:0.25rem;vertical-align:top;text-align:left}',
        '.preview-item img{width:200px;height:200px;object-fit:cover}',
        '.preview-item span{display:block;width:200px;margin-top:0.5rem;font-size:0.9rem}',
        '</style></head><body>',
        f'<h1>Radio Station Spotify Playlists</h1>',
        f'<p>Updated: {ts} · Total Playlists: {count}</p>',
    ]

    for station in sorted(stations):
        html.append(f'<h2>{station}</h2>')
        html.append('<table><thead><tr><th>Show</th><th>Tracks</th><th>Updated</th><th>Preview</th></tr></thead><tbody>')
        for p in stations[station]:
            html.append('<tr>')
            html.append(f"<td><a href='{p['url']}'>{p['name']}</a></td>")
            html.append(f"<td>{p['track_count']}</td>")
            html.append(f"<td>{p.get('last_updated','')}</td>")
            # previews
            previews = []
            for t in p.get('preview', []):
                artists = ', '.join(t.get('artists', []))
                name = t.get('name', '')
                img = t.get('image_url')
                if img:
                    previews.append(
                        f"<div class='preview-item'><img src='{img}' alt='{name}'><span>{artists} – {name}</span></div>"
                    )
                else:
                    previews.append(f"<div class='preview-item'><span>{artists} – {name}</span></div>")
            html.append(f"<td>{''.join(previews)}</td>")
            html.append('</tr>')
        html.append('</tbody></table>')

    html.append('</body></html>')
    with open(outfile, 'w') as f:
        f.write('\n'.join(html))


if __name__ == '__main__':
    in_json = sys.argv[1] if len(sys.argv) > 1 else 'docs/playlists.json'
    out_html = sys.argv[2] if len(sys.argv) > 2 else 'docs/index.html'
    main(in_json, out_html)
