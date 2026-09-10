#!/usr/bin/env python3
"""Generate a compact, searchable playlist catalog with static archive pages."""

import argparse
from collections import Counter
from datetime import datetime, timedelta, timezone
import hashlib
from html import escape
import json
import re
from pathlib import Path
import shutil
import unicodedata
from urllib.parse import urlparse
from zoneinfo import ZoneInfo

PAGE_SIZE = 25
DAY_PREVIEW = 10
DEFAULT_TIMEZONE = "America/Los_Angeles"
ASSETS = Path(__file__).with_name("website")


def parse_time(value):
    if not value:
        return None
    try:
        parsed = datetime.fromisoformat(value.replace("Z", "+00:00"))
    except ValueError:
        try:
            parsed = datetime.strptime(value, "%Y-%m-%d %H:%M UTC").replace(tzinfo=timezone.utc)
        except ValueError:
            return None
    return parsed if parsed.tzinfo is not None else None


def safe_url(value):
    if not isinstance(value, str):
        return ""
    parsed = urlparse(value)
    return value if parsed.scheme in ("https", "http") and parsed.netloc else ""


def clock_time(value):
    return value.strftime("%I:%M%p").lstrip("0").lower()


def normalize(row, zone):
    station = str(row.get("station") or "Unknown")
    name = str(row.get("name") or "Untitled playlist")
    title = str(row.get("display_name") or name).removeprefix(station + " - ")
    imported = parse_time(row.get("imported_at"))
    recorded = imported or parse_time(row.get("last_updated"))
    local = recorded.astimezone(zone) if recorded else None
    broadcast = parse_time(row.get("broadcast_start"))
    if broadcast and not row.get("display_name"):
        title = title.removeprefix(broadcast.strftime("%Y-%m-%d %H:%M") + " - ")
        # Accept raw exports of either naming generation without doubling dates.
        title = re.sub(r" - " + broadcast.strftime("%Y-%m-%d") +
                       r"(?: \d{1,2}:\d{2}(?:am|pm)(?: [+-]\d{4})?(?: \[[^\]]+\])?)?$", "", title)
    # Keep the source broadcast date visible; import time can be days later.
    # Legacy collections have no episode date, only their recorded update time.
    title_time = ""
    if broadcast:
        if not row.get("display_name"):
            title += broadcast.strftime(" - %Y-%m-%d")
            title_time = clock_time(broadcast)
    elif local:
        kind = "imported" if imported else "updated"
        title += f" · {kind} {local:%Y-%m-%d}"
        title_time = clock_time(local)
    else:
        title += " · date unavailable"
    preview = []
    for track in row.get("preview") or []:
        preview.append({
            "name": str(track.get("name") or "Untitled track"),
            "artists": [str(a) for a in track.get("artists", [])],
            "image_url": safe_url(track.get("image_url")),
        })
    count = max(0, int(row.get("track_count") or 0))
    return {
        "id": hashlib.sha256(str(row.get("url", "")).encode()).hexdigest()[:16],
        "station": station, "name": name, "title": title,
        "_title_time": title_time,
        "_title_offset": (broadcast or local).strftime("%z") if broadcast or local else "",
        "url": safe_url(row.get("url")), "source_url": safe_url(row.get("source_url")),
        "track_count": count,
        "count_label": str(count) if count >= len(preview[:12]) else "—",
        "day": local.date().isoformat() if local else None,
        "timestamp": recorded.timestamp() if recorded else 0,
        "date_label": f"{local:%b %d, %Y} · {clock_time(local)} {local:%Z}" if local else "Date unavailable",
        "date_kind": "Imported" if imported else "Updated",
        "broadcast_label": f"{broadcast:%b %d, %Y} · {clock_time(broadcast)} {broadcast:%z}" if broadcast else "",
        "preview": preview[:12],
    }


def unique_titles(rows):
    """Disambiguate across the entire archive before pagination or filtering."""
    def key(row):
        return tuple(unicodedata.normalize("NFKC", row[field]).casefold()
                     for field in ("station", "title"))

    counts = Counter(key(row) for row in rows)
    for row in rows:
        time = row.pop("_title_time")
        if counts[key(row)] > 1 and time:
            row["title"] += " " + time
    counts = Counter(key(row) for row in rows)
    for row in rows:
        offset = row.pop("_title_offset")
        if counts[key(row)] > 1 and offset:
            row["title"] += " " + offset
    # Some legacy records share even the update minute. Use a stable identifier
    # as a last resort rather than inventing an episode date or renumbering them.
    counts = Counter(key(row) for row in rows)
    for row in rows:
        if counts[key(row)] > 1:
            row["title"] += " · " + row["id"]
    if len({key(row) for row in rows}) != len(rows):
        raise ValueError("Catalog still contains duplicate playlist titles")


def json_for_html(value):
    return json.dumps(value, ensure_ascii=False, separators=(",", ":")).replace("<", "\\u003c")


def sample(track):
    name = escape(track["name"])
    artists = escape(", ".join(track["artists"]) or "Unknown artist")
    art = (f'<img src="{escape(track["image_url"])}" alt="" loading="lazy" width="32" height="32">'
           if track["image_url"] else '<span class="cover-placeholder" aria-hidden="true">♪</span>')
    return f'<li class="song">{art}<span class="song-text"><span class="song-artist" title="{artists}">{artists}</span><span class="song-title" title="{name}">{name}</span></span></li>'


def playlist_row(row):
    title = escape(row["title"])
    link = (f'<a class="playlist-title" href="{escape(row["url"])}" target="_blank" rel="noopener">{title}<span aria-hidden="true"> ↗</span></a>'
            if row["url"] else f'<span class="playlist-title">{title}</span>')
    context = f'{row["date_kind"]} {row["date_label"]}'
    if row["broadcast_label"]:
        context = "Broadcast " + row["broadcast_label"]
    preview = row["preview"]
    if preview:
        artists = list(dict.fromkeys(a.strip() for t in preview for a in t["artists"] if a.strip()))[:3]
        compact = ' · '.join(f'<span class="preview-artist">{escape(a)}</span>' for a in artists or ["Unknown artist"])
        full = ''.join(sample(t) for t in preview)
        songs = f'''<details class="song-preview"><summary aria-label="Expand {len(preview)}-song sample for {escape(row['station'] + ' - ' + row['title'])}"><span class="sample-strip">{compact}</span><span class="sample-toggle"><span class="closed-label">+ {len(preview)} songs</span><span class="open-label">− Close</span></span></summary><div class="sample-expanded"><p>Song sample · {len(preview)} tracks</p><ul>{full}</ul></div></details>'''
    else:
        songs = '<span class="no-sample">No song sample available</span>'
    return f'''<article class="playlist-row" data-playlist-id="{row['id']}"><span class="station-code">{escape(row['station'])}</span><div class="playlist-info">{link}<span class="playlist-meta">{escape(context)}</span></div><div class="preview-cell">{songs}</div><span class="track-count" title="{'Track count unavailable' if row['count_label'] == '—' else 'Tracks'}">{row['count_label']}<span>tracks</span></span></article>'''


def rows_html(rows):
    return ''.join(playlist_row(row) for row in rows)


def column_head():
    return '<div class="column-head" aria-hidden="true"><span>Station</span><span>Playlist / broadcast</span><span>A few artists inside</span><span>Tracks</span></div>'


def day_sections(rows, days):
    sections = []
    for day in days:
        records = [r for r in rows if r["day"] == day["date"]]
        first, rest = records[:DAY_PREVIEW], records[DAY_PREVIEW:]
        body = column_head() + rows_html(first) if first else '<p class="empty-day">No imports on this day.</p>'
        if rest:
            body += f'<details class="day-overflow"><summary>Show {len(rest)} more playlists <span aria-hidden="true">↓</span></summary>{rows_html(rest)}</details>'
        today = '<span class="today-label">Today</span>' if day['today'] else ''
        sections.append(f'''<section class="day-section" id="day-{day['date']}" aria-labelledby="heading-{day['date']}"><header class="section-heading"><h2 id="heading-{day['date']}"><span class="day-weekday">{day['weekday']}</span><time datetime="{day['date']}">{day['label']}</time>{today}</h2><span>{len(records):,} playlist{'s' if len(records) != 1 else ''}</span></header>{body}</section>''')
    return ''.join(sections)


def page_href(page):
    return "index.html" if page == 1 else f"page-{page}.html"


def pagination(page, total):
    pages = max(1, (total + PAGE_SIZE - 1) // PAGE_SIZE)
    first = (page - 1) * PAGE_SIZE + 1 if total else 0
    last = min(page * PAGE_SIZE, total)
    links = []
    for label, target in [("← Previous", page - 1), ("Next →", page + 1)]:
        links.append(f'<a href="{page_href(target)}" data-page="{target}">{label}</a>' if 1 <= target <= pages else f'<span aria-disabled="true">{label}</span>')
    numbered = []
    previous = 0
    for n in sorted({1, pages, *range(max(1, page - 2), min(pages, page + 2) + 1)}):
        if n > previous + 1:
            numbered.append('<span class="page-gap">…</span>')
        current = ' aria-current="page"' if n == page else ''
        numbered.append(f'<a href="{page_href(n)}" data-page="{n}" aria-label="Page {n}"{current}>{n}</a>')
        previous = n
    return f'<nav class="pagination" aria-label="Archive pages"><span class="page-range">{first:,}–{last:,} of {total:,}</span><div class="page-links">{links[0]}{"".join(numbered)}{links[1]}</div></nav>'


def render_page(rows, config, view="recent", page=1):
    recent = view == "recent"
    base = "./" if recent else "../"
    counts = Counter(r["station"] for r in rows)
    options = ''.join(f'<option value="{escape(station)}">{escape(station)} ({count:,})</option>' for station, count in sorted(counts.items()))
    recent_count = sum(r["day"] in {d["date"] for d in config["days"]} for r in rows)
    days_nav = ''.join(f'<a href="{base}index.html#day-{day["date"]}" data-day="{day["date"]}"><span>{day["short"]}</span><span class="day-count">{sum(r["day"] == day["date"] for r in rows)}</span></a>' for day in config["days"])
    if recent:
        content = f'<div id="playlist-results">{day_sections(rows, config["days"])}</div><a id="archive-cta" class="archive-cta" href="archive/index.html">Browse all {len(rows):,} playlists in the archive <span>→</span></a>'
        heading, count_label = "Last 7 days", f"{recent_count:,} playlists"
    else:
        selected = rows[(page - 1) * PAGE_SIZE:page * PAGE_SIZE]
        content = f'<div id="playlist-results">{column_head()}{rows_html(selected)}</div><div id="pagination">{pagination(page, len(rows))}</div>'
        heading, count_label = "All playlists", f"{len(rows):,} playlists · newest first"
    page_config = {**config, "view": view, "page": page, "base": base, "pageSize": PAGE_SIZE, "dayPreview": DAY_PREVIEW}
    return f'''<!doctype html>
<html lang="en"><head><meta charset="utf-8"><meta name="viewport" content="width=device-width, initial-scale=1"><meta name="color-scheme" content="light"><title>{heading} — Spinitron</title><link rel="stylesheet" href="{base}assets/catalog.css?v={config['assetVersion']}"><script type="module" src="{base}assets/catalog.mjs?v={config['assetVersion']}"></script></head>
<body><a class="skip-link" href="#main">Skip to playlists</a><div class="site-shell">
<header class="site-header"><a class="wordmark" href="{base}index.html"><span aria-hidden="true">▥</span> SPINITRON</a><span class="site-description">Radio, collected.</span><nav aria-label="Catalog views"><a href="{base}index.html"{' aria-current="page"' if recent else ''}>Recent imports</a><a href="{base}archive/index.html"{' aria-current="page"' if not recent else ''}>All playlists <span>{len(rows):,}</span></a></nav></header>
<div class="intro-line"><p>A playlist for every broadcast. Save the ones you want in Spotify.</p><span>Updated {config['updatedLabel']}</span></div>
<form id="filters" class="filters" role="search"><label class="search-label"><span>Search playlists & songs</span><input id="search" type="search" placeholder="Show, artist, or song…" autocomplete="off" disabled></label><label class="station-label"><span>Station</span><select id="station" disabled><option value="">All stations</option>{options}</select></label><button id="clear-filters" type="reset" disabled>Clear</button><span id="filter-status" class="filter-status" role="status" aria-live="polite">{count_label}</span></form>
<noscript><p class="notice">Search needs JavaScript. All playlists and archive pages are available below.</p></noscript>
<div class="catalog-layout"><aside class="sidebar"><div class="sidebar-inner"><h2>Import days</h2><nav class="day-index" aria-label="Import days">{days_nav}</nav><a class="history-link" href="{base}archive/index.html">Full archive <span>↗</span></a><p>{len(rows):,} playlists<br>{len(counts)} stations</p><p class="date-note">Dates in {escape(config['timezoneLabel'])}.<br>Older records use their last update date.</p><a class="download-link" href="{base}playlists.jsonl">Download catalog ↓</a></div></aside>
<main id="main" tabindex="-1"><div class="view-heading"><h1>{heading}</h1><span id="view-count">{count_label}</span></div>{content}</main></div>
<footer class="site-footer"><span>SPINITRON / COMMUNITY RADIO ARCHIVE</span><span>Open a playlist → listen in Spotify → save if you like.</span></footer></div><script id="catalog-config" type="application/json">{json_for_html(page_config)}</script></body></html>'''


def main(infile, output_dir="docs", now=None, timezone_name=DEFAULT_TIMEZONE):
    zone = ZoneInfo(timezone_name)
    now = now or datetime.now(timezone.utc)
    local_now = now.astimezone(zone)
    rows = []
    with Path(infile).open(encoding="utf-8") as source:
        for number, line in enumerate(source, 1):
            if line.strip():
                try:
                    rows.append(normalize(json.loads(line), zone))
                except (ValueError, TypeError, AttributeError) as error:
                    raise ValueError(f"Invalid playlist on line {number}: {error}") from error
    unique_titles(rows)
    rows.sort(key=lambda r: (-r["timestamp"], r["station"], r["name"], r["url"]))
    days = []
    for offset in range(7):
        day = local_now.date() - timedelta(days=offset)
        days.append({"date": day.isoformat(), "label": day.strftime("%d %B %Y"),
                     "short": day.strftime("%a %d %b"), "weekday": day.strftime("%a"), "today": offset == 0})
    payload = json.dumps(rows, ensure_ascii=False, separators=(",", ":"))
    config = {
        "days": days, "timezone": timezone_name, "updatedLabel": f"{local_now:%d %b %Y} · {clock_time(local_now)} {local_now:%Z}",
        "timezoneLabel": "Pacific time" if timezone_name == DEFAULT_TIMEZONE else timezone_name,
        "dataVersion": hashlib.sha256(payload.encode()).hexdigest()[:12],
        "assetVersion": hashlib.sha256((ASSETS / "catalog.css").read_bytes() + (ASSETS / "catalog.mjs").read_bytes()).hexdigest()[:12],
    }
    out = Path(output_dir)
    (out / "archive").mkdir(parents=True, exist_ok=True)
    (out / "assets").mkdir(exist_ok=True)
    if Path(infile).resolve() != (out / "playlists.jsonl").resolve():
        shutil.copyfile(infile, out / "playlists.jsonl")
    for asset in ("catalog.css", "catalog.mjs"):
        shutil.copyfile(ASSETS / asset, out / "assets" / asset)
    (out / "assets/catalog-data.json").write_text(payload + "\n", encoding="utf-8")
    (out / "index.html").write_text(render_page(rows, config), encoding="utf-8")
    pages = max(1, (len(rows) + PAGE_SIZE - 1) // PAGE_SIZE)
    expected = {page_href(page) for page in range(1, pages + 1)}
    for stale in (out / "archive").glob("page-*.html"):
        if stale.name not in expected:
            stale.unlink()
    for page in range(1, pages + 1):
        (out / "archive" / page_href(page)).write_text(render_page(rows, config, "archive", page), encoding="utf-8")
    print(f"Generated seven import days and {pages} archive pages for {len(rows):,} playlists.")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("infile", type=Path)
    parser.add_argument("--output-dir", type=Path, default=Path("docs"))
    parser.add_argument("--timezone", default=DEFAULT_TIMEZONE)
    args = parser.parse_args()
    main(args.infile, args.output_dir, timezone_name=args.timezone)
