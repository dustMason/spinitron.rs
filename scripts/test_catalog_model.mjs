import test from "node:test";
import assert from "node:assert/strict";
import {filterRows, pageRows, renderRow} from "./website/catalog.mjs";

const row = (id, station = "KALX") => ({
  id:String(id), station, name:"Don't Stop — 音楽", title:"Don't Stop — 音楽",
  url:"https://open.spotify.com/playlist/example", day:"2026-09-08",
  track_count:15, count_label:"15", date_kind:"Imported", date_label:"Sep 08, 2026", broadcast_label:"",
  preview:[{name:"A song <live>", artists:["Artist & Friends"], image_url:"https://example.com/art.jpg"}],
});

test("search matches show, artist and song terms together, combined with station", () => {
  const rows = [row(1), row(2, "KPOO")];
  assert.deepEqual(filterRows(rows, "ARTIST live", "KALX").map(r => r.id), ["1"]);
  assert.equal(filterRows(rows, "音楽").length, 2);
  assert.equal(filterRows(rows, "no such song").length, 0);
  assert.equal(filterRows(rows, "   ").length, 2);
});

test("search finds dates added to catalog titles without changing their labels", () => {
  const rows = [row(1), row(2)];
  rows[0].title = "FREEFORM · 2026-09-01 · 01:00 -0700";
  rows[1].title = "FREEFORM · 2026-09-01 · 07:00 -0700";
  const matches = filterRows(rows, "FREEFORM 2026-09-01 07:00");
  assert.deepEqual(matches.map(r => r.id), ["2"]);
  assert.ok(renderRow(matches[0]).includes("FREEFORM · 2026-09-01 · 07:00 -0700"));
});

test("pagination clamps out-of-range pages and keeps zero-track records", () => {
  const rows = Array.from({length: 51}, (_, i) => row(i));
  rows[0].track_count = 0;
  assert.equal(pageRows(rows, 1).rows.length, 25);
  assert.equal(pageRows(rows, 2).rows[0].id, "25");
  assert.deepEqual(pageRows(rows, 999).rows.map(r => r.id), ["50"]);
  assert.equal(pageRows(rows, -3).page, 1);
  assert.equal(pageRows(rows, "invalid").page, 1);
  assert.deepEqual(pageRows([], 3), {page:1,pages:1,rows:[],first:0,last:0});
});

test("collapsed samples show three distinct artists while expanded samples keep every song", () => {
  const playlist = row(1);
  playlist.preview.push(
    {...playlist.preview[0], artists:["Artist & Friends"]},
    {...playlist.preview[0], artists:["Second artist", "Third artist"]},
    {...playlist.preview[0], artists:["Fourth artist"]},
  );
  const html = renderRow(playlist);
  const summary = html.match(/<summary[^>]*>(.*?)<\/summary>/s)[1];
  assert.ok(html.includes("Don&#39;t Stop — 音楽"));
  assert.ok(summary.includes("Artist &amp; Friends"));
  assert.ok(summary.includes("Second artist"));
  assert.ok(summary.includes("Third artist"));
  assert.equal((summary.match(/class="preview-artist"/g) || []).length, 3);
  assert.ok(!summary.includes("Fourth artist"));
  assert.ok(!summary.includes("<img"));
  assert.ok(!summary.includes('class="song-title"'));
  assert.ok(html.includes("A song &lt;live&gt;"));
  assert.ok(html.includes('<ul><li class="song"><img'));
  assert.equal((html.match(/<li class="song">/g) || []).length, 4);
  assert.ok(html.includes("Fourth artist"));
  assert.ok(!html.includes("<live>"));
});

test("empty preview and unavailable count have explicit compact states", () => {
  const empty = row(1);
  empty.preview = [];
  empty.count_label = "—";
  const html = renderRow(empty);
  assert.ok(html.includes("No song sample available"));
  assert.ok(html.includes("Track count unavailable"));
});
