const number = value => Number(value).toLocaleString("en-US");
const escape = value => String(value ?? "").replace(/[&<>"']/g, ch => ({"&":"&amp;","<":"&lt;",">":"&gt;",'"':"&quot;","'":"&#39;"}[ch]));
const searchable = value => value.normalize("NFKC").toLocaleLowerCase();

export function filterRows(rows, query = "", station = "") {
  const terms = searchable(query).trim().split(/\s+/).filter(Boolean);
  return rows.filter(row => {
    if (station && row.station !== station) return false;
    const text = searchable([row.name, row.station, ...row.preview.flatMap(t => [t.name, ...t.artists])].join(" "));
    return terms.every(term => text.includes(term));
  });
}

export function pageRows(rows, requested, size = 25) {
  const pages = Math.max(1, Math.ceil(rows.length / size));
  const page = Math.min(pages, Math.max(1, Number.isFinite(Number(requested)) ? Math.floor(Number(requested)) : 1));
  return {page, pages, rows: rows.slice((page - 1) * size, page * size),
    first: rows.length ? (page - 1) * size + 1 : 0, last: Math.min(page * size, rows.length)};
}

function song(track) {
  const artists = escape(track.artists.join(", ") || "Unknown artist");
  const name = escape(track.name);
  const image = track.image_url
    ? `<img src="${escape(track.image_url)}" alt="" loading="lazy" width="32" height="32">`
    : '<span class="cover-placeholder" aria-hidden="true">♪</span>';
  return `<li class="song">${image}<span class="song-text"><span class="song-artist" title="${artists}">${artists}</span><span class="song-title" title="${name}">${name}</span></span></li>`;
}

function artistPreview(tracks) {
  const artists = [...new Set(tracks.flatMap(t => t.artists).map(a => a.trim()).filter(Boolean))].slice(0, 3);
  return (artists.length ? artists : ["Unknown artist"]).map(a => `<span class="preview-artist">${escape(a)}</span>`).join(" · ");
}

export function renderRow(row) {
  const title = row.url
    ? `<a class="playlist-title" href="${escape(row.url)}" target="_blank" rel="noopener">${escape(row.title)}<span aria-hidden="true"> ↗</span></a>`
    : `<span class="playlist-title">${escape(row.title)}</span>`;
  const meta = row.broadcast_label ? `Broadcast ${row.broadcast_label}` : `${row.date_kind} ${row.date_label}`;
  const preview = row.preview.length
    ? `<details class="song-preview"><summary aria-label="Expand ${row.preview.length}-song sample for ${escape(row.name)}"><span class="sample-strip">${artistPreview(row.preview)}</span><span class="sample-toggle"><span class="closed-label">+ ${row.preview.length} songs</span><span class="open-label">− Close</span></span></summary><div class="sample-expanded"><p>Song sample · ${row.preview.length} tracks</p><ul>${row.preview.map(t => song(t)).join("")}</ul></div></details>`
    : '<span class="no-sample">No song sample available</span>';
  return `<article class="playlist-row" data-playlist-id="${escape(row.id)}"><span class="station-code">${escape(row.station)}</span><div class="playlist-info">${title}<span class="playlist-meta">${escape(meta)}</span></div><div class="preview-cell">${preview}</div><span class="track-count" title="${row.count_label === "—" ? "Track count unavailable" : "Tracks"}">${escape(row.count_label)}<span>tracks</span></span></article>`;
}

const columnHead = '<div class="column-head" aria-hidden="true"><span>Station</span><span>Playlist / broadcast</span><span>A few artists inside</span><span>Tracks</span></div>';
const pageHref = page => page === 1 ? "index.html" : `page-${page}.html`;

function pagination(rows, state, config) {
  const p = pageRows(rows, state.page, config.pageSize);
  const link = (n, label, attrs = "") => {
    const params = new URLSearchParams();
    if (state.query) params.set("q", state.query);
    if (state.station) params.set("station", state.station);
    const suffix = params.size ? `?${params}` : "";
    return `<a href="${pageHref(n)}${escape(suffix)}" data-page="${n}" ${attrs}>${label}</a>`;
  };
  const indices = [...new Set([1, p.pages, ...Array.from({length: 5}, (_, i) => p.page - 2 + i).filter(n => n > 0 && n <= p.pages)])].sort((a, b) => a - b);
  let previous = 0;
  const numbers = indices.map(n => {
    const gap = n > previous + 1 ? '<span class="page-gap">…</span>' : "";
    previous = n;
    return gap + link(n, n, `aria-label="Page ${n}"${n === p.page ? ' aria-current="page"' : ""}`);
  }).join("");
  return `<nav class="pagination" aria-label="Archive pages"><span class="page-range">${number(p.first)}–${number(p.last)} of ${number(rows.length)}</span><div class="page-links">${p.page > 1 ? link(p.page - 1, "← Previous") : '<span aria-disabled="true">← Previous</span>'}${numbers}${p.page < p.pages ? link(p.page + 1, "Next →") : '<span aria-disabled="true">Next →</span>'}</div></nav>`;
}

function renderDays(rows, allRows, config, filtered) {
  return config.days.map(day => {
    const matches = rows.filter(r => r.day === day.date);
    const total = allRows.filter(r => r.day === day.date).length;
    const first = matches.slice(0, config.dayPreview).map(renderRow).join("");
    const rest = matches.slice(config.dayPreview);
    const empty = filtered && total ? "No playlists match these filters." : "No imports on this day.";
    const body = first ? columnHead + first : `<p class="empty-day">${empty}</p>`;
    const more = rest.length ? `<details class="day-overflow"><summary>Show ${number(rest.length)} more playlists <span aria-hidden="true">↓</span></summary>${rest.map(renderRow).join("")}</details>` : "";
    return `<section class="day-section" id="day-${day.date}" aria-labelledby="heading-${day.date}"><header class="section-heading"><h2 id="heading-${day.date}"><span class="day-weekday">${day.weekday}</span><time datetime="${day.date}">${day.label}</time>${day.today ? '<span class="today-label">Today</span>' : ""}</h2><span>${number(matches.length)} playlist${matches.length === 1 ? "" : "s"}</span></header>${body}${more}</section>`;
  }).join("");
}

async function start() {
  const config = JSON.parse(document.querySelector("#catalog-config").textContent);
  const form = document.querySelector("#filters");
  const search = document.querySelector("#search");
  const station = document.querySelector("#station");
  const status = document.querySelector("#filter-status");
  const results = document.querySelector("#playlist-results");
  const counts = document.querySelector("#view-count");
  let rows;
  try {
    const response = await fetch(`${config.base}assets/catalog-data.json?v=${config.dataVersion}`, {signal: AbortSignal.timeout(15000)});
    if (!response.ok) throw new Error("Catalog unavailable");
    rows = await response.json();
  } catch {
    status.textContent = "Search unavailable. Browse the pages below.";
    return;
  }
  for (const control of form.elements) control.disabled = false;
  const state = {query:"", station:"", page:config.page};
  const readURL = () => {
    const params = new URLSearchParams(location.search);
    state.query = params.get("q") || "";
    state.station = params.get("station") || "";
    const pathPage = /\/page-(\d+)\.html$/.exec(location.pathname)?.[1] || 1;
    state.page = pageRows(rows, params.get("page") || pathPage, config.pageSize).page;
    if (![...station.options].some(o => o.value === state.station)) state.station = "";
    search.value = state.query;
    station.value = state.station;
  };
  function render() {
    const matches = filterRows(rows, state.query, state.station);
    const filtered = Boolean(state.query.trim() || state.station);
    const recentDays = new Set(config.days.map(d => d.date));
    const visibleCount = config.view === "recent" ? matches.filter(r => recentDays.has(r.day)).length : matches.length;
    const label = `${number(visibleCount)} playlist${visibleCount === 1 ? "" : "s"}${filtered ? " found" : ""}`;
    status.textContent = label;
    counts.textContent = config.view === "archive" ? label + " · newest first" : label;
    if (config.view === "recent") {
      results.innerHTML = renderDays(matches, rows, config, filtered);
      const cta = document.querySelector("#archive-cta");
      cta.innerHTML = `${filtered ? "Search" : "Browse"} all ${number(matches.length)} ${filtered ? "matching " : ""}playlists in the archive <span>→</span>`;
      const params = new URLSearchParams();
      if (state.query) params.set("q", state.query);
      if (state.station) params.set("station", state.station);
      cta.href = `archive/index.html${params.size ? "?" + params : ""}`;
    } else {
      const p = pageRows(matches, state.page, config.pageSize);
      state.page = p.page;
      results.innerHTML = p.rows.length ? columnHead + p.rows.map(renderRow).join("")
        : '<p class="empty-results">No playlists match these filters.<br>Try another show, artist, or song, or clear the filters.</p>';
      document.querySelector("#pagination").innerHTML = pagination(matches, state, config);
    }
    for (const item of document.querySelectorAll(".day-index [data-day]")) {
      item.querySelector(".day-count").textContent = matches.filter(r => r.day === item.dataset.day).length;
    }
    const params = new URLSearchParams();
    if (state.query) params.set("q", state.query);
    if (state.station) params.set("station", state.station);
    for (const link of document.querySelectorAll(".site-header nav a,.history-link,.day-index a")) {
      const url = new URL(link.href);
      url.search = params.toString();
      link.href = url;
    }
  }
  function writeURL(push = false, keepHash = false) {
    const url = new URL(location.href);
    url.search = "";
    if (state.query) url.searchParams.set("q", state.query);
    if (state.station) url.searchParams.set("station", state.station);
    if (config.view === "archive") url.pathname = new URL(pageHref(state.page), location.href).pathname;
    if (!keepHash) url.hash = "";
    history[push ? "pushState" : "replaceState"](null, "", url);
  }
  readURL();
  // Preserve the native initial page and any preview opened while data loaded.
  if (location.search) {
    render(); writeURL(false, true);
    document.getElementById(location.hash.slice(1))?.scrollIntoView();
  }
  form.addEventListener("submit", event => event.preventDefault());
  let pending;
  const applyFilters = () => {
    state.query = search.value; state.station = station.value; state.page = 1;
    render(); writeURL();
  };
  search.addEventListener("input", () => { clearTimeout(pending); pending = setTimeout(applyFilters, 100); });
  station.addEventListener("change", () => { clearTimeout(pending); applyFilters(); });
  form.addEventListener("reset", event => {
    event.preventDefault(); clearTimeout(pending);
    search.value = ""; station.value = ""; applyFilters();
  });
  document.querySelector("#pagination")?.addEventListener("click", event => {
    const link = event.target.closest("a[data-page]");
    if (!link || event.metaKey || event.ctrlKey || event.shiftKey || event.altKey) return;
    event.preventDefault();
    clearTimeout(pending);
    state.page = Number(link.dataset.page);
    render(); writeURL(true);
    document.querySelector("#main").focus({preventScroll:true});
    document.querySelector("#main").scrollIntoView({block:"start"});
  });
  window.addEventListener("popstate", () => { clearTimeout(pending); readURL(); render(); });
}

if (typeof document !== "undefined") start();
