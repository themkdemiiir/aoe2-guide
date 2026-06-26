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
import { ELO_BUCKETS_WITH_ALL, eloCaseSql } from "./lib/buckets.mjs";

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

// ---- per-map breakdown: civ vs opp, split by map ----
console.log("Aggregating per-map matchups…");
const MIN_MAP = 200;
const mapRows = duck(`
  SELECT a.civ civ, b.civ opp, m.map mapname, count(*) g, sum(a.winner::int) w
  FROM read_parquet('${P}') a
  JOIN read_parquet('${P}') b USING (game_id)
  JOIN read_parquet('${M}') m USING (game_id)
  WHERE m.leaderboard='random_map' AND a.winner <> b.winner AND a.civ <> b.civ
  GROUP BY 1, 2, 3
  HAVING count(*) >= ${MIN_MAP}`);

const byMap = {};
for (const r of mapRows) {
  if (!guideCivs.has(r.civ) || !guideCivs.has(r.opp)) continue;
  (((byMap[r.civ] ??= {})[r.mapname] ??= []).push({ opp: r.opp, games: Number(r.g), winRate: pct(r.w / r.g) }));
}
for (const c in byMap) for (const mp in byMap[c]) byMap[c][mp].sort((a, b) => b.winRate - a.winRate);

const OUT2 = path.resolve("src/data/civ-matchups-by-map.json");
const out2 = {
  source: out.source,
  generated: out.generated,
  ladder: "1v1",
  minGames: MIN_MAP,
  note: "winRate = how often <civ> beats <opp> on <map> in 1v1 RM. Mirrors excluded.",
  civs: byMap,
};
writeFileSync(OUT2, `${JSON.stringify(out2, null, 2)}\n`, "utf8");
const cells = Object.values(byMap).reduce((s, maps) => s + Object.values(maps).reduce((t, a) => t + a.length, 0), 0);
console.log(`civ-matchups-by-map: ${Object.keys(byMap).length} civs · ${cells} cells → ${OUT2}`);

// ---- team-ladder overall matchups: a's team win rate when b is an enemy ----
// Confounded (4 civs/side), so it's labeled as such and kept overall-only.
console.log("Aggregating team matchups…");
const teamRows = duck(`
  SELECT a.civ civ, b.civ opp, count(*) g, sum(a.winner::int) w
  FROM read_parquet('${P}') a
  JOIN read_parquet('${P}') b USING (game_id)
  JOIN read_parquet('${M}') m USING (game_id)
  WHERE m.leaderboard='team_random_map' AND a.winner <> b.winner AND a.civ <> b.civ
  GROUP BY 1, 2
  HAVING count(*) >= 500`);
const teamCivs = {};
for (const r of teamRows) {
  if (!guideCivs.has(r.civ) || !guideCivs.has(r.opp)) continue;
  (teamCivs[r.civ] ??= []).push({ opp: r.opp, games: Number(r.g), winRate: pct(r.w / r.g) });
}
for (const k in teamCivs) teamCivs[k].sort((a, b) => b.winRate - a.winRate);
const OUT3 = path.resolve("src/data/civ-matchups-team.json");
writeFileSync(OUT3, `${JSON.stringify({
  source: out.source,
  generated: out.generated,
  ladder: "team",
  minGames: 500,
  note: "winRate = how often <civ>'s team wins when <opp> is on the enemy team (team RM; confounded by the 3 other civs per side).",
  civs: teamCivs,
}, null, 2)}\n`, "utf8");
console.log(`civ-matchups-team: ${Object.keys(teamCivs).length} civs → ${OUT3}`);

// ---- 1v1 head-to-head by elo bucket (for the compare page's elo filter) ----
console.log("Aggregating 1v1 h2h by elo bucket…");
const ELO = eloCaseSql("a.new_rating");
const MIN_BUCKET = 150;
const eloRows = duck(`
  SELECT a.civ civ, b.civ opp, ${ELO} bucket, count(*) g, sum(a.winner::int) w
  FROM read_parquet('${P}') a
  JOIN read_parquet('${P}') b USING (game_id)
  JOIN read_parquet('${M}') m USING (game_id)
  WHERE m.leaderboard='random_map' AND a.winner <> b.winner AND a.civ <> b.civ
  GROUP BY 1, 2, 3
  HAVING count(*) >= ${MIN_BUCKET}`);
const acc = {}; // civ -> opp -> { bucket: {g,w} }
for (const r of eloRows) {
  if (!guideCivs.has(r.civ) || !guideCivs.has(r.opp)) continue;
  ((acc[r.civ] ??= {})[r.opp] ??= {})[r.bucket] = { g: Number(r.g), w: Number(r.w) };
}
const eloCivs = {};
for (const [civ, opps] of Object.entries(acc)) {
  eloCivs[civ] = {};
  for (const [opp, buckets] of Object.entries(opps)) {
    const slice = {};
    let ag = 0;
    let aw = 0;
    for (const [b, v] of Object.entries(buckets)) {
      slice[b] = [pct(v.w / v.g), v.g];
      ag += v.g;
      aw += v.w;
    }
    if (ag >= 300) slice.all = [pct(aw / ag), ag];
    eloCivs[civ][opp] = slice;
  }
}
const OUT4 = path.resolve("src/data/civ-matchups-by-elo.json");
writeFileSync(OUT4, `${JSON.stringify({
  source: out.source,
  generated: out.generated,
  ladder: "1v1",
  eloBuckets: ELO_BUCKETS_WITH_ALL,
  minGames: { bucket: MIN_BUCKET, all: 300 },
  note: "[winRate, games] of <civ> vs <opp> per elo bucket (a's rating). Mirrors excluded.",
  civs: eloCivs,
}, null, 2)}\n`, "utf8");
const eloCells = Object.values(eloCivs).reduce((s, o) => s + Object.values(o).reduce((t, sl) => t + Object.keys(sl).length, 0), 0);
console.log(`civ-matchups-by-elo: ${Object.keys(eloCivs).length} civs · ${eloCells} cells → ${OUT4}`);
