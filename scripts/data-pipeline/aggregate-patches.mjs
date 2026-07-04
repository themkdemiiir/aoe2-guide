#!/usr/bin/env node
// scripts/data-pipeline/aggregate-patches.mjs
//
// Build the patch axis from REAL game builds ("Update 179158"…). Back months
// (≤ archive end) come from the aoestats archive's per-match `patch` column;
// the crawl era maps gamemod_id → build via src/data/patch-index.json
// (replay-verified for recent builds, release-date-aligned for the rest).
// Aggregates per civ × build, then splices byPatch into civ-meta.json.
// Overall / byElo / byMap stay whatever the refresh scripts wrote.
// Civ ids are the Relic API space (relic-civ-id-map.json).
//
// Runs on the box that holds data-cache (the VM).
//   node scripts/data-pipeline/aggregate-patches.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { AOESTATS_END_MONTH } from "./lib/buckets.mjs";
import { crawlRecords } from "./lib/crawl-stream.mjs";
import { duck } from "./lib/duck.mjs";
import { loadGuideCivs } from "./lib/guide-civs.mjs";
import { buildOf, patchLabel } from "./lib/patch-axis.mjs";
import { isRanked1v1, relicCivSlug } from "./lib/relic-map.mjs";
import { pct } from "./lib/stats.mjs";

const META = path.resolve("src/data/civ-meta.json");

const guideCivs = loadGuideCivs();

const MIN_PATCH_MATCHES = 3000; // a build needs this many matches to count as a "patch"
const MIN_CIV_GAMES = 100; // per civ × patch gate (self-collected sample is smaller than aoestats)
const MAX_PATCHES = 16; // keep the most recent N patches

const monthKey = (t) => new Date(t * 1000).toISOString().slice(0, 7);

// --- accumulate per BUILD + per civ × build ----------------------------------
const gm = {}; // build -> { matches }
const civPatch = {}; // slug -> build -> { g, w }

// Back months (<= archive end) come from the aoestats archive: full corpus,
// name-derived civs, real per-match dates — the crawl only samples history.
const HOME = process.env.HOME;
const HIST_M = `SELECT game_id, CAST(patch AS VARCHAR) AS build, strftime(started_timestamp, '%Y-%m') AS month
  FROM read_parquet('${HOME}/aoestats/m_*.parquet')
  WHERE leaderboard = 'random_map' AND started_timestamp >= TIMESTAMP '2024-07-01'`;
for (const r of duck(
  `WITH m AS (${HIST_M}) SELECT build, count(*) AS matches FROM m WHERE month <= '${AOESTATS_END_MONTH}' GROUP BY 1`,
)) {
  (gm[r.build] ??= { matches: 0 }).matches += Number(r.matches);
}
for (const r of duck(`WITH m AS (${HIST_M})
  SELECT m.build, p.civ, count(*) AS g, sum(p.winner::int) AS w
  FROM read_parquet('${HOME}/aoestats/p_*.parquet') p JOIN m USING (game_id)
  WHERE m.month <= '${AOESTATS_END_MONTH}'
  GROUP BY 1, 2`)) {
  if (!guideCivs.has(r.civ)) continue;
  const cp = ((civPatch[r.civ] ??= {})[r.build] ??= { g: 0, w: 0 });
  cp.g += Number(r.g);
  cp.w += Number(r.w);
}

let lines = 0;
for await (const m of crawlRecords()) {
  if (!isRanked1v1(m)) continue;
  if (monthKey(m.completed) <= AOESTATS_END_MONTH) continue; // archive owns the back months
  const g = buildOf(m); // throws on unmapped gamemods — never defaults
  if (!g) continue; // documented anomaly gamemods (see patch-index.json)
  lines++;
  (gm[g] ??= { matches: 0 }).matches++;
  for (const pl of m.players ?? []) {
    const slug = relicCivSlug(pl.civ_id);
    if (!guideCivs.has(slug)) continue;
    const cp = ((civPatch[slug] ??= {})[g] ??= { g: 0, w: 0 });
    cp.g++;
    if (pl.won) cp.w++;
  }
}

// --- pick the patches: recent BUILDS with enough matches (builds are monotonic) ---
const patches = Object.entries(gm)
  .filter(([, v]) => v.matches >= MIN_PATCH_MATCHES)
  .sort((a, b) => Number(b[0]) - Number(a[0]))
  .slice(0, MAX_PATCHES)
  .map(([g, v]) => ({ patch: g, label: patchLabel(g), matches: v.matches }));
const keep = new Set(patches.map((p) => String(p.patch)));

// --- splice byPatch into civ-meta -------------------------------------------
const meta = JSON.parse(readFileSync(META, "utf8"));
let civsUpdated = 0;
for (const [slug, m] of Object.entries(meta.civs)) {
  if (!m["1v1"]) continue;
  const byPatch = {};
  for (const [g, v] of Object.entries(civPatch[slug] ?? {})) {
    if (!keep.has(g) || v.g < MIN_CIV_GAMES) continue;
    byPatch[g] = { games: v.g, winRate: pct(v.w / v.g) };
  }
  if (Object.keys(byPatch).length) {
    m["1v1"].byPatch = byPatch;
    civsUpdated++;
  }
}

meta.patches = patches; // [{patch:"179158", label:"Update 179158", matches}]
meta.patchSource =
  "official game builds — aoestats archive patch column + gamemod→build index for the crawl era (ranked RM 1v1)";
meta.generated = new Date().toISOString().slice(0, 10);
writeFileSync(META, `${JSON.stringify(meta, null, 2)}\n`, "utf8");

console.log(
  `patches: ${lines} crawl rows · ${patches.length} builds on the axis · ${civsUpdated} civs got byPatch`,
);
console.log(`newest patch: ${patches[0]?.label} (${patches[0]?.matches} matches)`);
console.log(`→ ${META}`);
