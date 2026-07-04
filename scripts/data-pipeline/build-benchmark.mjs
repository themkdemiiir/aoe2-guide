// Nest build-benchmark.sql's CSV (civ,map,bucket,mode,feudal_s,castle_s,imperial_s,n) into
// replay-rs/data/benchmark.json: { civs: { civ: { map: { bucket: { mode: {feudal_s,castle_s,imperial_s} } } } } }.
// Run from repo root: node scripts/data-pipeline/build-benchmark.mjs bench.csv
//
// NOTE: this emits the RICH 4-dimension shape (civ × map × bucket × mode), which is exactly
// what the committed benchmark.json now is and what data.rs reads: BenchmarkFile decodes
// civ→map(slug)→bucket→mode and Benchmark::slice falls back WITHIN the map
// (exact → same-mode map rollup civ→map→"all"→mode → all-mode rollup civ→map→"all"→"all").
// `mode` is '1v1'|'team' derived from games.ladder (NOT team_size); the CSV carries it per row.
import { readFileSync, writeFileSync } from "node:fs";

const csv = readFileSync(process.argv[2] || "bench.csv", "utf8")
  .trim()
  .split("\n");
const head = csv.shift().split(",");
const civs = {};
for (const line of csv) {
  const r = Object.fromEntries(line.split(",").map((v, i) => [head[i], v]));
  const num = (x) => (x === "" || x == null ? null : +x);
  (((civs[r.civ] ??= {})[r.mapname] ??= {})[r.bucket] ??= {})[r.mode] = {
    feudal_s: num(r.feudal_s),
    castle_s: num(r.castle_s),
    imperial_s: num(r.imperial_s),
  };
}
const doc = {
  _source:
    "games.{feudal,castle,imperial}_t medians (age COMPLETION seconds) by civ×map×elo_bucket×mode, with map/civ rollups under the 'all' key",
  civs,
};
writeFileSync("scripts/data-pipeline/replay-rs/data/benchmark.json", JSON.stringify(doc));
console.log(`benchmark.json: ${Object.keys(civs).length} civs`);
