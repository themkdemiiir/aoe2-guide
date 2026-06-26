// Nest build-benchmark.sql's CSV (civ,map,bucket,mode,feudal_s,castle_s,imperial_s,n) into
// replay-rs/data/benchmark.json: { civs: { civ: { map: { bucket: { mode: {feudal_s,castle_s,imperial_s} } } } } }.
// Run from repo root: node scripts/data-pipeline/build-benchmark.mjs bench.csv
//
// NOTE: this emits the RICH 4-dimension shape (civ × map × bucket × mode). The current
// committed benchmark.json is a 3-dimension all-map placeholder and data.rs's loader reads
// civ→bucket→mode; when you adopt this richer file, add the `map` level to data.rs
// (BenchmarkFile + Benchmark::slice fallback: civ→map→bucket→mode, falling back map→"all").
import { readFileSync, writeFileSync } from "node:fs";
const csv = readFileSync(process.argv[2] || "bench.csv", "utf8").trim().split("\n");
const head = csv.shift().split(",");
const civs = {};
for (const line of csv) {
  const r = Object.fromEntries(line.split(",").map((v, i) => [head[i], v]));
  const num = (x) => (x === "" || x == null ? null : +x);
  (((civs[r.civ] ??= {})[r.mapname] ??= {})[r.bucket] ??= {})[r.mode] = {
    feudal_s: num(r.feudal_s), castle_s: num(r.castle_s), imperial_s: num(r.imperial_s),
  };
}
const doc = { _source: "games.{feudal,castle,imperial}_t medians (age COMPLETION seconds) by civ×map×elo_bucket×mode, with map/civ rollups under the 'all' key", civs };
writeFileSync("scripts/data-pipeline/replay-rs/data/benchmark.json", JSON.stringify(doc));
console.log(`benchmark.json: ${Object.keys(civs).length} civs`);
