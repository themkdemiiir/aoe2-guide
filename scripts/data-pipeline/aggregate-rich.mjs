#!/usr/bin/env node
// scripts/data-pipeline/aggregate-rich.mjs
//
// Rich civ stats from the aoestats ranked Parquet archive (via DuckDB), split by
// ladder (1v1 RM + Team RM). Per civ, per ladder:
//   overall {games, winRate, ci95, playRate, tier}
//   byPatch {<patch>: {games, winRate}}    byElo {<bucket>: {games, winRate}}
//   byMap   {<map>:   {games, winRate}}     openings [{opening, share}]
//   ageUp   {feudal, castle, imperial}  (median uptimes, seconds; 1v1 only is meaningful)
//
// This is the FROZEN-ARCHIVE baseline only. Live crawl data is layered on
// afterwards by refresh-{civ,team}-current + aggregate-patches (the single
// live source — they own the Relic civ-id mapping).
//
// Runs ON THE BOX (needs ~/bin/duckdb + ~/aoestats/*.parquet).
//   node scripts/data-pipeline/aggregate-rich.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { ELO_BUCKETS, eloCaseSql } from "./lib/buckets.mjs";
import { duck } from "./lib/duck.mjs";
import { pct, TIER_METHOD, tierOf, wilson } from "./lib/stats.mjs";

const SOURCE_DATE = "2026-02"; // aoestats archive corpus month (frozen) — confirm before changing

const HOME = process.env.HOME;
const M = `${HOME}/aoestats/m_*.parquet`;
const P = `${HOME}/aoestats/p_*.parquet`;
const OUT = path.resolve("src/data/civ-meta.json");

// No flags anymore: the old --relic live-merge moved to the refresh-* scripts
// (which own the Relic civ-id mapping). Reject stale invocations loudly.
if (process.argv.length > 2) {
  console.error(
    `aggregate-rich: unexpected args ${process.argv.slice(2).join(" ")} — the --relic live-merge was removed; run the refresh-* scripts for live data.`,
  );
  process.exit(1);
}

const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map(
    (c) => c.slug,
  ),
);

const ELO = eloCaseSql("p.new_rating");
const idx = (rows) => {
  const m = {};
  for (const r of rows) (m[r.civ] ??= []).push(r);
  return m;
};

// ---- aggregate one ladder (a leaderboard value) into per-civ breakdowns ----
function aggregateLadder(leaderboard) {
  const BASE = `read_parquet('${P}') p JOIN read_parquet('${M}') m USING (game_id) WHERE m.leaderboard='${leaderboard}'`;
  const overall = duck(
    `SELECT p.civ civ, count(*) games, sum(p.winner::int) wins FROM ${BASE} GROUP BY 1`,
  );
  const totalApp = overall.reduce((s, r) => s + Number(r.games), 0);
  const byElo = idx(
    duck(
      `SELECT p.civ civ, ${ELO} bucket, count(*) games, avg(p.winner::int) wr FROM ${BASE} GROUP BY 1,2`,
    ),
  );
  const byPatch = idx(
    duck(
      `SELECT p.civ civ, m.patch patch, count(*) games, avg(p.winner::int) wr FROM ${BASE} GROUP BY 1,2`,
    ),
  );
  const byMap = idx(
    duck(
      `SELECT p.civ civ, m.map mapname, count(*) games, avg(p.winner::int) wr FROM ${BASE} GROUP BY 1,2 HAVING count(*)>=500`,
    ),
  );
  const openings = idx(
    duck(
      `SELECT civ, opening, games FROM (SELECT p.civ civ, p.opening opening, count(*) games, row_number() OVER (PARTITION BY p.civ ORDER BY count(*) DESC) rn FROM ${BASE} AND p.opening IS NOT NULL AND p.opening<>'' GROUP BY 1,2) WHERE rn<=3`,
    ),
  );
  const ageup = Object.fromEntries(
    duck(
      `SELECT p.civ civ, median(p.feudal_age_uptime) feudal, median(p.castle_age_uptime) castle, median(p.imperial_age_uptime) imperial FROM ${BASE} AND p.feudal_age_uptime IS NOT NULL GROUP BY 1`,
    ).map((r) => [r.civ, r]),
  );
  const patches = duck(
    `SELECT patch, count(*) n FROM read_parquet('${M}') WHERE leaderboard='${leaderboard}' GROUP BY 1 ORDER BY patch DESC`,
  );
  return {
    overall: Object.fromEntries(overall.map((r) => [r.civ, r])),
    totalApp,
    byElo,
    byPatch,
    byMap,
    openings,
    ageup,
    patches,
  };
}

console.log("Aggregating aoestats archive by ladder (1v1 RM + Team RM) via DuckDB…");
const L = { "1v1": aggregateLadder("random_map"), team: aggregateLadder("team_random_map") };

// ---- assemble: per civ -> { '1v1': {...}, 'team': {...} } ----
function buildLadder(slug, agg) {
  const o = agg.overall[slug];
  if (!o) return null;
  const games = Number(o.games);
  const wins = Number(o.wins);
  if (!games) return null;
  const [lo, hi] = wilson(wins, games);
  const obj = (rows, key, val) => Object.fromEntries((rows ?? []).map((r) => [r[key], val(r)]));
  const byElo = Object.fromEntries(
    (agg.byElo[slug] ?? []).map((r) => [r.bucket, { games: Number(r.games), winRate: pct(r.wr) }]),
  );
  const bp = obj(agg.byPatch[slug], "patch", (r) => ({
    games: Number(r.games),
    winRate: pct(r.wr),
  }));
  const ag = agg.ageup[slug];
  return {
    games,
    winRate: pct(wins / games),
    ci95: [pct(lo), pct(hi)],
    playRate: pct(games / agg.totalApp),
    tier: tierOf((wins / games) * 100),
    byPatch: bp,
    byElo,
    byMap: Object.fromEntries(
      (agg.byMap[slug] ?? [])
        .sort((a, b) => Number(b.games) - Number(a.games))
        .slice(0, 14)
        .map((r) => [r.mapname, { games: Number(r.games), winRate: pct(r.wr) }]),
    ),
    openings: (agg.openings[slug] ?? []).map((r) => ({
      opening: r.opening,
      share: o ? pct(Number(r.games) / Number(o.games)) : null,
    })),
    ageUp: ag
      ? {
          feudal: Math.round(ag.feudal),
          castle: Math.round(ag.castle),
          imperial: Math.round(ag.imperial),
        }
      : null,
  };
}

const civs = {};
for (const slug of [...guideCivs].sort()) {
  const one = buildLadder(slug, L["1v1"]);
  const team = buildLadder(slug, L.team);
  civs[slug] = { "1v1": one, team };
}

const out = {
  source: "aoestats.io ranked Parquet archive (jerkeeler)",
  ladders: ["1v1", "team"],
  generated: new Date().toISOString().slice(0, 10),
  sourceDate: SOURCE_DATE,
  appearances: { "1v1": L["1v1"].totalApp, team: L.team.totalApp },
  patches: L["1v1"].patches.map((r) => ({ patch: r.patch, matches: Number(r.n) })),
  eloBuckets: ELO_BUCKETS,
  tierMethod: TIER_METHOD,
  civs,
};
writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
const r1 = Object.values(civs).filter((c) => c["1v1"]).length;
const rt = Object.values(civs).filter((c) => c.team).length;
console.log(
  `civ-meta: 1v1 ${(L["1v1"].totalApp / 1e6).toFixed(1)}M · team ${(L.team.totalApp / 1e6).toFixed(1)}M · ${r1} civs 1v1 / ${rt} team · ${out.patches.length} patches → ${OUT}`,
);
