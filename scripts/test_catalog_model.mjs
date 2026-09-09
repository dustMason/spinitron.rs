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

test("rendered samples escape ordinary punctuation and use valid inline summary content", () => {
  const html = renderRow(row(1));
  assert.ok(html.includes("Don&#39;t Stop — 音楽"));
  assert.ok(html.includes("Artist &amp; Friends"));
  assert.ok(html.includes("A song &lt;live&gt;"));
  assert.ok(html.includes('<span class="sample-strip"><span class="song">'));
  assert.ok(html.includes('<ul><li class="song">'));
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
