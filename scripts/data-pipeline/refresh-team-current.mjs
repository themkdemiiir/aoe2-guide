#!/usr/bin/env node
// scripts/data-pipeline/refresh-team-current.mjs
//
// Refresh the TEAM ladder in civ-meta.json + map-meta.json with the CURRENT
// self-collected World's Edge TEAM RM crawl (all crawl sources via
// lib/crawl-stream.mjs, last CURRENT_WINDOW_DAYS, ranked-RM team only). The
// team ladder was frozen aoestats; this recomputes the current team civ
// overall (winRate, ci95, tier, playRate), byElo, byMap, and the per-map team
// civ rankings. Where the crawl is too thin for a map (or a bucket), the
// frozen aoestats value is PRESERVED — never replaced with nothing.
// byPatch / openings / ageUp on the civ team block are left as-is (not in the
// crawl). Civ ids are the Relic API space (relic-civ-id-map.json); per-map
// slices use replay-parsed map truth only (the API mapname is junk).
//
// Runs on the box that holds data-cache (the VM).
//   node scripts/data-pipeline/refresh-team-current.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { eloBucket } from "./lib/buckets.mjs";
import { CURRENT_WINDOW_DAYS, crawlRecords } from "./lib/crawl-stream.mjs";
import { canonToKeyIndex, isRankedTeam, loadReplayMapTruth, relicCivSlug } from "./lib/relic-map.mjs";

const CIV_META = path.resolve("src/data/civ-meta.json");
const MAP_META = path.resolve("src/data/map-meta.json");

const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const civMeta = JSON.parse(readFileSync(CIV_META, "utf8"));
const mapMeta = JSON.parse(readFileSync(MAP_META, "utf8"));

const pct = (x) => +(x * 100).toFixed(2);
const tierOf = (w) => (w >= 53 ? "S" : w >= 51 ? "A" : w >= 49 ? "B" : w >= 47 ? "C" : "D");
function wilson(wins, n, z = 1.96) {
  if (!n) return [0, 0];
  const p = wins / n, d = 1 + (z * z) / n;
  const c = (p + (z * z) / (2 * n)) / d;
  const m = (z * Math.sqrt((p * (1 - p)) / n + (z * z) / (4 * n * n))) / d;
  return [c - m, c + m];
}

const MIN_CIV = 500; // per-civ team-overall gate
const MIN_ELO = 100; // per-civ byElo bucket gate
const MIN_CIV_MAP = 200; // per-civ byMap gate
const MIN_MAP = 3000; // per-map total team appearances to refresh the map's team rankings
const MIN_ALL = 200; // per-civ gate inside a map's "all" bucket
const MIN_BUCKET = 60; // per-civ gate inside a map's single elo bucket
const MIN_RANKED_CIVS = 10; // a map's team "all" bucket needs this many civs to overlay

// replay-truth map canon → map-meta key (keep byMap keys consistent with the map pages)
const canonToKey = canonToKeyIndex(mapMeta);
const mapTruth = await loadReplayMapTruth();

// --- one streaming pass: aggregate per-civ AND per-map×bucket×civ ---
const civ = {}; // slug -> { g, w, byElo, byMap }
const mapAgg = {}; // mapKey -> bucket -> slug -> { g, w }
let totalApp = 0;
let skippedNullElo = 0;
for await (const m of crawlRecords({ recentDays: CURRENT_WINDOW_DAYS })) {
  if (!isRankedTeam(m)) continue;
  const truth = mapTruth.get(m.match_id);
  const mapKey = truth ? (canonToKey[truth.canon]?.key ?? null) : null;
  const mp = mapKey ? (mapAgg[mapKey] ??= {}) : null;
  for (const pl of m.players ?? []) {
    const slug = relicCivSlug(pl.civ_id);
    if (!guideCivs.has(slug)) continue;
    totalApp++;
    const won = pl.won ? 1 : 0;
    const eb = eloBucket(pl.rating); if (eb == null) { skippedNullElo++; continue; }
    // civ aggregate
    const c = (civ[slug] ??= { g: 0, w: 0, byElo: {}, byMap: {} });
    c.g++; c.w += won;
    const be = (c.byElo[eb] ??= { g: 0, w: 0 }); be.g++; be.w += won;
    if (mapKey) { const bm = (c.byMap[mapKey] ??= { g: 0, w: 0 }); bm.g++; bm.w += won; }
    // map aggregate (per bucket + "all")
    if (mp) {
      for (const bk of [eb, "all"]) {
        const cw = ((mp[bk] ??= {})[slug] ??= { g: 0, w: 0 });
        cw.g++; cw.w += won;
      }
    }
  }
}

// --- overlay civ-meta team blocks ---
let civUpdated = 0;
for (const [slug, c] of Object.entries(civ)) {
  const entry = civMeta.civs[slug];
  if (!entry || c.g < MIN_CIV) continue; // need a usable team sample; preserve frozen otherwise
  // Post-archive-freeze civs have a null team block — create it from the crawl.
  const o = (entry.team ??= { byPatch: {}, openings: [], ageUp: null });
  const [lo, hi] = wilson(c.w, c.g);
  o.games = c.g;
  o.winRate = pct(c.w / c.g);
  o.ci95 = [pct(lo), pct(hi)];
  o.tier = tierOf((c.w / c.g) * 100);
  o.playRate = pct(c.g / totalApp);
  o.byElo = Object.fromEntries(
    Object.entries(c.byElo).filter(([, v]) => v.g >= MIN_ELO).map(([bk, v]) => [bk, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  // byMap is replay-VERIFIED matches only — preserve previous slices when thin.
  const byMap = Object.fromEntries(
    Object.entries(c.byMap).filter(([, v]) => v.g >= MIN_CIV_MAP).map(([k, v]) => [k, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  if (Object.keys(byMap).length) o.byMap = byMap;
  civUpdated++;
}

// --- overlay map-meta team rankings (only where the crawl has the volume) ---
let mapUpdated = 0;
for (const [key, buckets] of Object.entries(mapAgg)) {
  const total = Object.values(buckets.all ?? {}).reduce((s, v) => s + v.g, 0);
  if (total < MIN_MAP || !mapMeta.maps[key]) continue; // preserve frozen team for thin/unknown maps
  const byElo = {};
  for (const [bk, civs] of Object.entries(buckets)) {
    const min = bk === "all" ? MIN_ALL : MIN_BUCKET;
    const list = Object.entries(civs)
      .filter(([, v]) => v.g >= min)
      .map(([cv, v]) => ({ civ: cv, games: v.g, winRate: pct(v.w / v.g) }))
      .sort((a, b) => b.winRate - a.winRate);
    if (list.length) byElo[bk] = list;
  }
  if ((byElo.all?.length ?? 0) < MIN_RANKED_CIVS) continue;
  mapMeta.maps[key].team = byElo;
  mapMeta.maps[key].games.team = total;
  mapUpdated++;
}

civMeta.appearances = { ...(civMeta.appearances ?? {}), team: totalApp };
const today = new Date().toISOString().slice(0, 10);
civMeta.source = `self-collected World's Edge live ladder (ranked RM, last ${CURRENT_WINDOW_DAYS} days; maps replay-verified)`;
civMeta.generated = today;
mapMeta.source = `self-collected World's Edge live ladder (ranked RM, last ${CURRENT_WINDOW_DAYS} days; maps replay-verified)`;
mapMeta.generated = today;
writeFileSync(CIV_META, `${JSON.stringify(civMeta, null, 2)}\n`, "utf8");
writeFileSync(MAP_META, `${JSON.stringify(mapMeta, null, 2)}\n`, "utf8");
console.log(`refresh-team-current: ${totalApp} team appearances · ${civUpdated} civs + ${mapUpdated} maps got current team data · ${skippedNullElo} null-elo dropped`);
console.log(`  → ${CIV_META}`);
console.log(`  → ${MAP_META}`);
