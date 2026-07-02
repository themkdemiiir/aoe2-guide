#!/usr/bin/env node
// Merge build-benchmark-vils.sql's CSV (civ,map,bucket,mode,n,vils_castle) into
// replay-rs/data/benchmark.json, adding `vils_castle` (median villagers trained
// by Castle Age among WINNERS of the slice) to EXISTING cells. Cells present in
// the vils CSV but absent from benchmark.json are counted and reported, never
// invented — the age benchmark (games, 13.9M matches) is the richer source and
// defines which slices exist.
//
// Run from repo root on the box, after the SQL:
//   node scripts/data-pipeline/build-benchmark-vils.mjs /tmp/bench-vils.csv
import { readFileSync, writeFileSync } from "node:fs";

const BENCH = "scripts/data-pipeline/replay-rs/data/benchmark.json";
const csv = readFileSync(process.argv[2] ?? "/tmp/bench-vils.csv", "utf8").trim().split("\n");
const head = csv.shift().split(",");
const doc = JSON.parse(readFileSync(BENCH, "utf8"));

let merged = 0;
let missing = 0;
for (const line of csv) {
  const r = Object.fromEntries(line.split(",").map((v, i) => [head[i], v]));
  const cell = doc.civs?.[r.civ]?.[r.map]?.[r.bucket]?.[r.mode];
  if (!cell) { missing++; continue; }
  cell.vils_castle = +r.vils_castle;
  merged++;
}
if (!merged) {
  console.error("[FATAL] no vils cells merged — CSV/benchmark key mismatch. Not writing.");
  process.exit(1);
}
doc._source += "; vils_castle = median villagers trained by Castle Age among slice WINNERS (match_ages dark+feudal windows — see build-benchmark-vils.sql)";
writeFileSync(BENCH, JSON.stringify(doc));
console.log(`benchmark.json: vils_castle merged into ${merged} cells (${missing} vils-only slices skipped)`);
