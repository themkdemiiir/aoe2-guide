#!/usr/bin/env node
// Nest the two reference CSVs (winner-openings.csv, winner-ecotech.csv — see
// build-winner-refs.sql) into public/winner-refs.json:
//   { source, generated,
//     openings: { <civ>: { <bucket>: { winners, list: [{opening, pct}] } } },
//     ecoTechByCastle: { <bucket>: { winners, wheelbarrow, loom, dba, horseCollar, goldMining } } }
// Fetched lazily by /analyzer for the You|Opponent|Reference matrix.
//
//   node scripts/data-pipeline/build-winner-refs.mjs /tmp/winner-openings.csv /tmp/winner-ecotech.csv
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

const out = {
  source:
    "aoestats replay archive: ranked 1v1 RM WINNERS — opening distribution per civ × elo (share ≥8%, ≥100 winners) and eco-upgrade-researched-before-Castle shares per elo",
  generated: new Date().toISOString().slice(0, 10),
  openings,
  ecoTechByCastle,
};
writeFileSync("public/winner-refs.json", JSON.stringify(out));
console.log(
  `winner-refs: ${Object.keys(openings).length} civs openings · ${Object.keys(ecoTechByCastle).length} elo buckets ecoTech → public/winner-refs.json`,
);
