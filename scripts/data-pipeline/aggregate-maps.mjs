#!/usr/bin/env node
// scripts/data-pipeline/aggregate-maps.mjs
//
// Map-centric civ rankings from the aoestats ranked Parquet archive (via DuckDB),
// per ladder (1v1 RM + Team RM), at two elo tiers (all / 1650+). For each map:
// which civs win the most there. This is the inverse of civ-meta's per-civ byMap —
// it answers "on African Clearing, which civs are strong?".
//
// Runs ON THE BOX (needs ~/bin/duckdb + ~/aoestats/*.parquet).
//   node scripts/data-pipeline/aggregate-maps.mjs

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

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
  const out = execSync(`${DUCK} -json -c ${JSON.stringify(oneLine)}`, { maxBuffer: 1 << 29 }).toString().trim();
  return out ? JSON.parse(out) : [];
}
const pct = (x) => +(x * 100).toFixed(2);

const MIN_ALL = 200; // min games for an "all elo" civ entry on a map
const MIN_HIGH = 60; // min games for a "1650+" civ entry on a map
const MIN_MAP = 3000; // min total games for a map to be listed

function ladder(lb) {
  const rows = duck(`
    SELECT m.map mapname, p.civ civ,
      count(*) g, sum(p.winner::int) w,
      count(*) FILTER (WHERE p.new_rating>=1650) hg,
      sum(p.winner::int) FILTER (WHERE p.new_rating>=1650) hw
    FROM read_parquet('${P}') p JOIN read_parquet('${M}') m USING (game_id)
    WHERE m.leaderboard='${lb}'
    GROUP BY 1,2`);
  const maps = {};
  for (const r of rows) {
    if (!guideCivs.has(r.civ)) continue;
    const mp = (maps[r.mapname] ??= { total: 0, all: [], high: [] });
    mp.total += Number(r.g);
    if (Number(r.g) >= MIN_ALL) mp.all.push({ civ: r.civ, games: Number(r.g), winRate: pct(r.w / r.g) });
    if (Number(r.hg) >= MIN_HIGH) mp.high.push({ civ: r.civ, games: Number(r.hg), winRate: pct(r.hw / r.hg) });
  }
  for (const mp of Object.values(maps)) {
    mp.all.sort((a, b) => b.winRate - a.winRate);
    mp.high.sort((a, b) => b.winRate - a.winRate);
  }
  return maps;
}

console.log("Aggregating map-centric civ rankings (1v1 + team) via DuckDB…");
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
    "1v1": o ? { all: o.all, high: o.high } : null,
    team: t ? { all: t.all, high: t.high } : null,
  };
}

const out = {
  source: "aoestats.io ranked Parquet archive",
  generated: new Date().toISOString().slice(0, 10),
  eloTiers: { all: "All Elo", high: "1650+" },
  minGames: { all: MIN_ALL, high: MIN_HIGH, map: MIN_MAP },
  maps,
};
writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
console.log(`map-meta: ${Object.keys(maps).length} maps → ${OUT}`);
