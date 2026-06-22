#!/usr/bin/env node
// scripts/data-pipeline/aggregate-matchups.mjs
//
// Civ-vs-civ 1v1 win rates from the aoestats ranked Parquet archive (DuckDB).
// For each ordered pair (civ, opponent): how often <civ> beats <opponent>.
// Self-joins the two players of each 1v1 game on game_id; `a.winner <> b.winner`
// keeps exactly the two cross pairs (one per perspective). Mirrors excluded.
//
// Runs ON THE BOX (needs ~/bin/duckdb + ~/aoestats/*.parquet).
//   node scripts/data-pipeline/aggregate-matchups.mjs

import { execSync } from "node:child_process";
import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";

const HOME = process.env.HOME;
const DUCK = `${HOME}/bin/duckdb`;
const M = `${HOME}/aoestats/m_*.parquet`;
const P = `${HOME}/aoestats/p_*.parquet`;
const OUT = path.resolve("src/data/civ-matchups.json");

const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug),
);

function duck(sql) {
  const oneLine = sql.replace(/\s+/g, " ").trim();
  return JSON.parse(execSync(`${DUCK} -json -c ${JSON.stringify(oneLine)}`, { maxBuffer: 1 << 30 }).toString().trim() || "[]");
}
const pct = (x) => +(x * 100).toFixed(2);
const MIN = 300; // min games per matchup to be reported

console.log("Aggregating civ-vs-civ 1v1 matchups via DuckDB (self-join)…");
const rows = duck(`
  SELECT a.civ civ, b.civ opp, count(*) g, sum(a.winner::int) w
  FROM read_parquet('${P}') a
  JOIN read_parquet('${P}') b USING (game_id)
  JOIN read_parquet('${M}') m USING (game_id)
  WHERE m.leaderboard='random_map' AND a.winner <> b.winner AND a.civ <> b.civ
  GROUP BY 1, 2
  HAVING count(*) >= ${MIN}`);

const civs = {};
for (const r of rows) {
  if (!guideCivs.has(r.civ) || !guideCivs.has(r.opp)) continue;
  (civs[r.civ] ??= []).push({ opp: r.opp, games: Number(r.g), winRate: pct(r.w / r.g) });
}
for (const k in civs) civs[k].sort((a, b) => b.winRate - a.winRate);

const out = {
  source: "aoestats.io ranked Parquet archive (1v1 RM)",
  generated: new Date().toISOString().slice(0, 10),
  ladder: "1v1",
  minGames: MIN,
  note: "winRate = how often <civ> beats <opp> in 1v1 RM. Mirrors excluded.",
  civs,
};
writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
const pairs = Object.values(civs).reduce((s, a) => s + a.length, 0);
console.log(`civ-matchups: ${Object.keys(civs).length} civs · ${pairs} pairs → ${OUT}`);
