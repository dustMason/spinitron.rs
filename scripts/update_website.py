#!/usr/bin/env python3
"""
Script to update the GitHub Pages website with fresh playlist data
Used by GitHub Actions and can be run locally for testing
"""

import subprocess
import sys
import os
import json
from datetime import datetime

def run_command(cmd):
    """Run shell command and return output"""
    try:
        result = subprocess.run(cmd, shell=True, capture_output=True, text=True, check=True)
        return result.stdout
    except subprocess.CalledProcessError as e:
        print(f"❌ Error running command: {cmd}")
        print(f"   {e.stderr}")
        sys.exit(1)


def main():
    print("🌐 Updating website with latest playlists...")
    
    # Check if we have the required binary
    if not os.path.exists("./target/release/spinitron-scraper"):
        print("❌ Error: ./target/release/spinitron-scraper not found")
        print("   Run 'cargo build --release' first")
        sys.exit(1)
    
    # Check if template exists
    if not os.path.exists("scripts/index.template.md"):
        print("❌ Error: scripts/index.template.md not found")
        sys.exit(1)
    
    # Generate fresh playlist data
    print("📊 Generating fresh playlist data...")
    playlist_output = run_command("./target/release/spinitron-scraper --list-playlists")
    
    # Parse JSONL output and group by station
    stations = {}
    total_playlist_count = 0
    
    for line in playlist_output.strip().split('\n'):
        if line.strip():
            try:
                playlist = json.loads(line)
                station = playlist['station']
                name = playlist['name']
                url = playlist['url']
                track_count = playlist['track_count']
                last_updated = playlist.get('last_updated', '')
                preview = playlist.get('preview', [])

                if station not in stations:
                    stations[station] = []

                stations[station].append({
                    'name': name,
                    'url': url,
                    'track_count': track_count,
                    'last_updated': last_updated,
                    'preview': preview
                })
                total_playlist_count += 1
            except (json.JSONDecodeError, KeyError) as e:
                print(f"⚠️  Warning: Failed to parse line: {line}")
                print(f"   Error: {e}")
    
    if not stations:
        print("❌ Error: No playlists found in output")
        sys.exit(1)
    
    # Generate HTML table sections for each station (raw HTML avoids Markdown table limitations)
    sections = []
    for station in sorted(stations.keys()):
        playlists = stations[station]
        section_lines = [f"<h2>{station}</h2>",
                         "<table>",
                         "<thead><tr><th>Show</th><th>Tracks</th><th>Updated</th><th>Preview</th></tr></thead>",
                         "<tbody>"]

        for playlist in playlists:
            # Build HTML preview of up to 4 tracks as a horizontal grid with larger covers
            items = []
            for track in playlist.get('preview', []):
                name = track.get('name', '')
                artists = ', '.join(track.get('artists', []))
                img = track.get('image_url')
                if img:
                    items.append(
                        f'<div class="preview-item">'
                        f'<img src="{img}" alt="{name}"/>'
                        f'<br><span>{artists} – {name}</span>'
                        f'</div>'
                    )
                else:
                    items.append(
                        f'<div class="preview-item">'
                        f'<span>{artists} – {name}</span>'
                        f'</div>'
                    )
            preview_html = f'<div class="preview-row">{"".join(items)}</div>'

            section_lines.extend([
                "<tr>",
                f"<td><a href=\"{playlist['url']}\">{playlist['name']}</a></td>",
                f"<td>{playlist['track_count']}</td>",
                f"<td>{playlist.get('last_updated','')}</td>",
                f"<td>{preview_html}</td>",
                "</tr>"
            ])

        section_lines.extend(["</tbody>", "</table>"])
        sections.append("\n".join(section_lines))
    
    playlist_sections = "\n\n".join(sections)
    
    # Read template
    with open("scripts/index.template.md", "r") as f:
        template = f.read()
    
    # Generate timestamp
    from datetime import timezone
    timestamp = datetime.now(timezone.utc).strftime("%Y-%m-%d %H:%M UTC")
    
    # Replace template variables
    content = template.replace("{{PLAYLISTS}}", playlist_sections)
    content = content.replace("{{TIMESTAMP}}", timestamp)
    content = content.replace("{{COUNT}}", str(total_playlist_count))
    
    # Write the markdown file
    with open("docs/index.md", "w") as f:
        f.write(content)

    # Also dump raw data to JSON for alternative static HTML generation
    json_data = {
        'stations': stations,
        'timestamp': timestamp,
        'total_playlist_count': total_playlist_count,
    }
    with open("docs/playlists.json", "w") as jf:
        json.dump(json_data, jf, indent=2)
    
    print("✅ Website update complete!")
    print(f"📊 Updated with {total_playlist_count} playlists across {len(stations)} stations")
    print("📄 Generated: docs/index.md and docs/playlists.json")

if __name__ == "__main__":
    main()
