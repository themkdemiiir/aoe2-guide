#!/usr/bin/env node
// scripts/data-pipeline/refresh-civ-current.mjs
//
// Refresh the 1v1 civ stats in civ-meta.json with the CURRENT self-collected
// World's Edge crawl (data-cache/relic-patched/matches.ndjson). aggregate-rich
// built these from the frozen aoestats archive; this recomputes the 1v1
// overall (winRate, ci95, tier, playRate), byElo, and byMap from the live
// ladder so the default civ view is current. byPatch (already crawl-derived)
// and the team ladder (aoestats; the crawl is 1v1) are preserved.
//
// Runs LOCALLY (reads the desktop crawl backup + civ-meta.json + map-meta.json).
//   node scripts/data-pipeline/refresh-civ-current.mjs

import { createReadStream, readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import path from "node:path";

const IN = path.resolve("data-cache/relic-patched/matches.ndjson");
const META = path.resolve("src/data/civ-meta.json");

const civIdMap = JSON.parse(readFileSync(path.resolve("src/data/civ-id-map.json"), "utf8"));
const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const meta = JSON.parse(readFileSync(META, "utf8"));
const mapMeta = JSON.parse(readFileSync(path.resolve("src/data/map-meta.json"), "utf8"));

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

const MIN_ELO = 100; // per-bucket gate (crawl is thinner at the top than aoestats)
const MIN_MAP = 200; // per-map gate

// crawl map filename → map-meta key (so byMap keys stay consistent with map pages)
const canonToKey = {};
for (const [k, v] of Object.entries(mapMeta.maps)) {
  const cc = canon(k);
  const g = (v.games?.["1v1"] ?? 0) + (v.games?.team ?? 0);
  if (!canonToKey[cc] || g > canonToKey[cc].g) canonToKey[cc] = { key: k, g };
}
const mapKeyFor = (raw) => canonToKey[canon(String(raw).replace(/\.[a-z0-9]+$/i, ""))]?.key ?? null;

// --- aggregate the crawl per civ ---
const civ = {};
let totalApp = 0;
const rl = createInterface({ input: createReadStream(IN), crlfDelay: Infinity });
for await (const line of rl) {
  if (!line.trim()) continue;
  let m;
  try { m = JSON.parse(line); } catch { continue; }
  const mapKey = m.map_raw ? mapKeyFor(m.map_raw) : null;
  for (const pl of m.players ?? []) {
    const slug = civIdMap[String(pl.civ_id)];
    if (!slug || !guideCivs.has(slug)) continue;
    totalApp++;
    const c = (civ[slug] ??= { g: 0, w: 0, byElo: {}, byMap: {} });
    const won = pl.won ? 1 : 0;
    c.g++; c.w += won;
    const b = eloBucket(pl.rating ?? 0);
    const be = (c.byElo[b] ??= { g: 0, w: 0 }); be.g++; be.w += won;
    if (mapKey) { const bm = (c.byMap[mapKey] ??= { g: 0, w: 0 }); bm.g++; bm.w += won; }
  }
}

// --- splice current 1v1 stats into civ-meta ---
let updated = 0;
for (const [slug, c] of Object.entries(civ)) {
  const o = meta.civs[slug]?.["1v1"];
  if (!o || c.g < 500) continue; // need a usable civ sample
  const [lo, hi] = wilson(c.w, c.g);
  o.games = c.g;
  o.winRate = pct(c.w / c.g);
  o.ci95 = [pct(lo), pct(hi)];
  o.tier = tierOf((c.w / c.g) * 100);
  o.playRate = pct(c.g / totalApp);
  o.byElo = Object.fromEntries(
    Object.entries(c.byElo).filter(([, v]) => v.g >= MIN_ELO).map(([b, v]) => [b, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  o.byMap = Object.fromEntries(
    Object.entries(c.byMap).filter(([, v]) => v.g >= MIN_MAP).map(([k, v]) => [k, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  updated++;
}

meta.appearances = { ...(meta.appearances ?? {}), "1v1": totalApp };
meta.source = "self-collected World's Edge live ladder (1v1, current) + aoestats archive (team)";
meta.generated = new Date().toISOString().slice(0, 10);
writeFileSync(META, `${JSON.stringify(meta, null, 2)}\n`, "utf8");
console.log(`refresh-civ-current: ${totalApp} crawl appearances · ${updated} civs got current 1v1 overall/byElo/byMap → ${META}`);
