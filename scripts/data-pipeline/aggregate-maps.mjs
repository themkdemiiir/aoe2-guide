#!/usr/bin/env node
// scripts/data-pipeline/aggregate-maps.mjs
//
// Map-centric civ rankings from the aoestats ranked Parquet archive (via DuckDB),
// per ladder (1v1 RM + Team RM), broken down by the 9 elo buckets (+ "all"). For
// each map / ladder / elo bucket: which civs win the most there. Inverse of
// civ-meta's per-civ byMap.
//
// Runs ON THE BOX (needs ~/bin/duckdb + ~/aoestats/*.parquet).
//   node scripts/data-pipeline/aggregate-maps.mjs

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { ELO_BUCKETS, eloCaseSql } from "./lib/buckets.mjs";

const HOME = process.env.HOME;
const DUCK = `${HOME}/bin/duckdb`;
const M = `${HOME}/aoestats/m_*.parquet`;
const P = `${HOME}/aoestats/p_*.parquet`;
const OUT = path.resolve("src/data/map-meta.json");

const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug),
);

function duck(sql) {
  const oneLine = sql.replace(/\s+/g, " ").trim();
  const out = execSync(`${DUCK} -json -c ${JSON.stringify(oneLine)}`, { maxBuffer: 1 << 30 }).toString().trim();
  return out ? JSON.parse(out) : [];
}
const pct = (x) => +(x * 100).toFixed(2);

const ELO = eloCaseSql("p.new_rating");
const TIERS = ["all", ...ELO_BUCKETS]; // "all" first

const MIN_ALL = 200; // min games for an "all elo" civ entry on a map
const MIN_BUCKET = 60; // min games for a single-bucket civ entry on a map
const MIN_MAP = 3000; // min total games for a map to be listed

function ladder(lb) {
  const rows = duck(`
    SELECT m.map mapname, p.civ civ, ${ELO} bucket, count(*) g, sum(p.winner::int) w
    FROM read_parquet('${P}') p JOIN read_parquet('${M}') m USING (game_id)
    WHERE m.leaderboard='${lb}'
    GROUP BY 1, 2, 3`);
  // maps[map] = { total, acc: { bucket: { civ: {g,w} } } }   (with an "all" bucket summed)
  const maps = {};
  for (const r of rows) {
    if (!guideCivs.has(r.civ)) continue;
    const g = Number(r.g);
    const w = Number(r.w);
    const mp = (maps[r.mapname] ??= { total: 0, acc: {} });
    mp.total += g;
    const b = (mp.acc[r.bucket] ??= {});
    b[r.civ] = { g, w };
    const all = (mp.acc.all ??= {});
    const a = (all[r.civ] ??= { g: 0, w: 0 });
    a.g += g;
    a.w += w;
  }
  // build sorted, gated civ lists per bucket
  for (const mp of Object.values(maps)) {
    const byElo = {};
    for (const [bucket, civs] of Object.entries(mp.acc)) {
      const min = bucket === "all" ? MIN_ALL : MIN_BUCKET;
      const list = Object.entries(civs)
        .filter(([, v]) => v.g >= min)
        .map(([civ, v]) => ({ civ, games: v.g, winRate: pct(v.w / v.g) }))
        .sort((a, b) => b.winRate - a.winRate);
      if (list.length) byElo[bucket] = list;
    }
    mp.byElo = byElo;
  }
  return maps;
}

console.log("Aggregating map-centric civ rankings by elo bucket (1v1 + team) via DuckDB…");
const one = ladder("random_map");
const team = ladder("team_random_map");

const allMaps = new Set([...Object.keys(one), ...Object.keys(team)]);
const maps = {};
for (const mp of [...allMaps].sort()) {
  const o = one[mp];
  const t = team[mp];
  const total = (o?.total ?? 0) + (t?.total ?? 0);
  if (total < MIN_MAP) continue;
  maps[mp] = {
    games: { "1v1": o?.total ?? 0, team: t?.total ?? 0 },
    "1v1": o?.byElo ?? null,
    team: t?.byElo ?? null,
  };
}

const out = {
  source: "aoestats.io ranked Parquet archive",
  generated: new Date().toISOString().slice(0, 10),
  eloBuckets: TIERS,
  minGames: { all: MIN_ALL, bucket: MIN_BUCKET, map: MIN_MAP },
  maps,
};
writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
console.log(`map-meta: ${Object.keys(maps).length} maps · ${TIERS.length} elo tiers → ${OUT}`);
