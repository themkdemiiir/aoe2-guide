#!/usr/bin/env node
// Nest the three reference CSVs (winner-openings.csv, winner-ecotech.csv — see
// build-winner-refs.sql; winner-meds.csv — see build-winner-meds.sql) into
// public/winner-refs.json:
//   { source, generated,
//     openings: { <civ>: { <bucket>: { winners, list: [{opening, pct}] } } },
//     ecoTechByCastle: { <bucket>: { winners, wheelbarrow, loom, dba, horseCollar, goldMining } },
//     medsByBucket: { <bucket>: { winners, firstMilitaryMs, loomMs, dbaMs, wheelbarrowMs, horseCollarMs } },
//     medsByMap: { <mapSlug>: { <bucket>: { ...same meds shape } } } }
// Fetched lazily by /analyzer for the You|Opponent|Reference matrix.
//
// winner-meds.csv (GROUPING SETS output) carries an all-maps rollup row (map='all')
// PLUS one row per map. The 'all' rows become medsByBucket (the map-agnostic fallback
// the analyzer uses when a map is missing); the per-map rows become medsByMap. All
// three CSVs are required — readFileSync throws if any is absent, so this never
// silently ships a winner-refs.json with a dropped section.
//
//   node scripts/data-pipeline/build-winner-refs.mjs \
//     /tmp/winner-openings.csv /tmp/winner-ecotech.csv /tmp/winner-meds.csv
import { readFileSync, writeFileSync } from "node:fs";

const rows = (file) => {
  const lines = readFileSync(file, "utf8").trim().split("\n");
  const head = lines.shift().split(",");
  return lines.map((l) => Object.fromEntries(l.split(",").map((v, i) => [head[i], v])));
};

const openings = {};
for (const r of rows(process.argv[2] ?? "/tmp/winner-openings.csv")) {
  const cell = ((openings[r.civ] ??= {})[r.bucket] ??= { winners: +r.winners_n, list: [] });
  if (cell.list.length < 5) cell.list.push({ opening: r.opening, pct: +r.share_pct });
}

const ecoTechByCastle = {};
for (const r of rows(process.argv[3] ?? "/tmp/winner-ecotech.csv")) {
  ecoTechByCastle[r.bucket] = {
    winners: +r.winners_n,
    wheelbarrow: +r.wheelbarrow_pct,
    loom: +r.loom_pct,
    dba: +r.dba_pct,
    horseCollar: +r.horse_collar_pct,
    goldMining: +r.gold_mining_pct,
  };
}

// Winner event-timing medians (Plan C). build-winner-meds.sql emits, via GROUPING
// SETS, one all-maps rollup row (map='all') plus one row per map. 'all' → medsByBucket
// (fallback); per-map → medsByMap[mapSlug][bucket]. Same slug space as games.map and
// the engine's benchmark.json (lowercase alphanumeric of the map name).
const medsByBucket = {};
const medsByMap = {};
for (const r of rows(process.argv[4] ?? "/tmp/winner-meds.csv")) {
  const cell = {
    winners: +r.winners_n,
    firstMilitaryMs: +r.first_military_ms,
    loomMs: +r.loom_ms,
    dbaMs: +r.dba_ms,
    wheelbarrowMs: +r.wheelbarrow_ms,
    horseCollarMs: +r.horse_collar_ms,
  };
  if (r.map === "all") medsByBucket[r.bucket] = cell;
  else (medsByMap[r.map] ??= {})[r.bucket] = cell;
}

const out = {
  source:
    "aoestats replay archive: ranked 1v1 RM WINNERS — opening distribution per civ × elo (share ≥8%, ≥100 winners) and eco-upgrade-researched-before-Castle shares per elo; replay event corpus: winner first-military + eco-tech research medians per elo (medsByBucket) and per map × elo (medsByMap)",
  generated: new Date().toISOString().slice(0, 10),
  openings,
  ecoTechByCastle,
  medsByBucket,
  medsByMap,
};
writeFileSync("public/winner-refs.json", JSON.stringify(out));
console.log(
  `winner-refs: ${Object.keys(openings).length} civs openings · ${Object.keys(ecoTechByCastle).length} elo buckets ecoTech · ${Object.keys(medsByBucket).length} buckets meds · ${Object.keys(medsByMap).length} maps meds → public/winner-refs.json`,
);
