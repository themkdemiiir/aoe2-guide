#!/usr/bin/env node
// Nest build-winner-comps.sql's CSV into public/winner-comps.json:
//   { source, generated, civs: { <civ>: { <bucket>: { winners, units: [{unit, pct, med}] } } } }
// Fetched lazily by the /analyzer page ("winners with your civ at your elo
// typically produce …"). Top 6 units per cell, producer-share ordered.
//
// Run from repo root on the box, after the SQL:
//   node scripts/data-pipeline/build-winner-comps.mjs /tmp/winner-comps.csv
import { readFileSync, writeFileSync } from "node:fs";

const csv = readFileSync(process.argv[2] ?? "/tmp/winner-comps.csv", "utf8").trim().split("\n");
const head = csv.shift().split(",");
const civs = {};
for (const line of csv) {
  const r = Object.fromEntries(line.split(",").map((v, i) => [head[i], v]));
  const cell = ((civs[r.civ] ??= {})[r.bucket] ??= { winners: +r.winners_n, units: [] });
  if (cell.units.length < 6) {
    cell.units.push({ unit: r.unit, pct: +r.producer_pct, med: +r.med_count });
  }
}
const out = {
  source:
    "aoestats replay archive (match_ages): units produced through Castle Age by WINNERS of ranked 1v1 RM, per civ × elo bucket (producer share ≥15%, ≥100 winners/cell)",
  generated: new Date().toISOString().slice(0, 10),
  civs,
};
writeFileSync("public/winner-comps.json", JSON.stringify(out));
const cells = Object.values(civs).reduce((s, b) => s + Object.keys(b).length, 0);
console.log(`winner-comps: ${Object.keys(civs).length} civs · ${cells} civ×elo cells → public/winner-comps.json`);
