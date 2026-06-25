#!/usr/bin/env node
// scripts/data-pipeline/refresh-team-current.mjs
//
// Refresh the TEAM ladder in civ-meta.json + map-meta.json with the CURRENT
// self-collected World's Edge TEAM RM crawl (data-cache/relic-team/matches.ndjson,
// collected via `collect-relic --team`). The team ladder was frozen aoestats; this
// recomputes the current team civ overall (winRate, ci95, tier, playRate), byElo,
// byMap, and the per-map team civ rankings. Where the crawl is too thin for a map
// (or a bucket), the frozen aoestats value is PRESERVED — never replaced with nothing.
// byPatch / openings / ageUp on the civ team block are left as-is (not in the crawl).
//
// Runs LOCALLY (reads the desktop team-crawl + both meta files).
//   node scripts/data-pipeline/refresh-team-current.mjs

import { createReadStream, readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import path from "node:path";

const IN = path.resolve("data-cache/relic-team/matches.ndjson");
const CIV_META = path.resolve("src/data/civ-meta.json");
const MAP_META = path.resolve("src/data/map-meta.json");

const civIdMap = JSON.parse(readFileSync(path.resolve("src/data/civ-id-map.json"), "utf8"));
const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const civMeta = JSON.parse(readFileSync(CIV_META, "utf8"));
const mapMeta = JSON.parse(readFileSync(MAP_META, "utf8"));

const pct = (x) => +(x * 100).toFixed(2);
const canon = (s) => s.toLowerCase().replace(/[^a-z0-9]/g, "");
const eloBucket = (r) =>
  r < 1000 ? "<1000" : r < 1200 ? "1000-1199" : r < 1400 ? "1200-1399" : r < 1650 ? "1400-1649"
  : r < 1800 ? "1650-1799" : r < 2000 ? "1800-1999" : r < 2200 ? "2000-2199" : r < 2500 ? "2200-2499" : "2500+";
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

// crawl map filename → map-meta key (keep byMap keys consistent with the map pages)
const canonToKey = {};
for (const [k, v] of Object.entries(mapMeta.maps)) {
  const cc = canon(k);
  const g = (v.games?.["1v1"] ?? 0) + (v.games?.team ?? 0);
  if (!canonToKey[cc] || g > canonToKey[cc].g) canonToKey[cc] = { key: k, g };
}
const mapKeyFor = (raw) => canonToKey[canon(String(raw).replace(/\.[a-z0-9]+$/i, ""))]?.key ?? null;

// --- one streaming pass: aggregate per-civ AND per-map×bucket×civ ---
const civ = {}; // slug -> { g, w, byElo, byMap }
const mapAgg = {}; // mapKey -> bucket -> slug -> { g, w }
let totalApp = 0;
const rl = createInterface({ input: createReadStream(IN), crlfDelay: Infinity });
for await (const line of rl) {
  if (!line.trim()) continue;
  let m;
  try { m = JSON.parse(line); } catch { continue; }
  const mapKey = m.map_raw ? mapKeyFor(m.map_raw) : null;
  const mp = mapKey ? (mapAgg[mapKey] ??= {}) : null;
  for (const pl of m.players ?? []) {
    const slug = civIdMap[String(pl.civ_id)];
    if (!slug || !guideCivs.has(slug)) continue;
    totalApp++;
    const won = pl.won ? 1 : 0;
    const b = eloBucket(pl.rating ?? 0);
    // civ aggregate
    const c = (civ[slug] ??= { g: 0, w: 0, byElo: {}, byMap: {} });
    c.g++; c.w += won;
    const be = (c.byElo[b] ??= { g: 0, w: 0 }); be.g++; be.w += won;
    if (mapKey) { const bm = (c.byMap[mapKey] ??= { g: 0, w: 0 }); bm.g++; bm.w += won; }
    // map aggregate (per bucket + "all")
    if (mp) {
      for (const bk of [b, "all"]) {
        const cw = ((mp[bk] ??= {})[slug] ??= { g: 0, w: 0 });
        cw.g++; cw.w += won;
      }
    }
  }
}

// --- overlay civ-meta team blocks ---
let civUpdated = 0;
for (const [slug, c] of Object.entries(civ)) {
  const o = civMeta.civs[slug]?.team;
  if (!o || c.g < MIN_CIV) continue; // need a usable team sample; preserve frozen otherwise
  const [lo, hi] = wilson(c.w, c.g);
  o.games = c.g;
  o.winRate = pct(c.w / c.g);
  o.ci95 = [pct(lo), pct(hi)];
  o.tier = tierOf((c.w / c.g) * 100);
  o.playRate = pct(c.g / totalApp);
  o.byElo = Object.fromEntries(
    Object.entries(c.byElo).filter(([, v]) => v.g >= MIN_ELO).map(([bk, v]) => [bk, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  o.byMap = Object.fromEntries(
    Object.entries(c.byMap).filter(([, v]) => v.g >= MIN_CIV_MAP).map(([k, v]) => [k, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
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
civMeta.source = "self-collected World's Edge live ladder (1v1 + team, current)";
civMeta.generated = today;
mapMeta.source = "self-collected World's Edge live ladder (1v1 + team, current)";
mapMeta.generated = today;
writeFileSync(CIV_META, `${JSON.stringify(civMeta, null, 2)}\n`, "utf8");
writeFileSync(MAP_META, `${JSON.stringify(mapMeta, null, 2)}\n`, "utf8");
console.log(`refresh-team-current: ${totalApp} team appearances · ${civUpdated} civs + ${mapUpdated} maps got current team data`);
console.log(`  → ${CIV_META}`);
console.log(`  → ${MAP_META}`);
