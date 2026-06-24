#!/usr/bin/env node
// scripts/data-pipeline/refresh-map-current.mjs
//
// Refresh the 1v1 map rankings in map-meta.json with the CURRENT self-collected
// World's Edge crawl (data-cache/relic-patched/matches.ndjson). aggregate-maps
// builds map-meta from the frozen aoestats archive only; this splices the live
// crawl's current map × civ × elo 1v1 rankings over the top (per map, where the
// crawl has enough volume). Team rankings stay aoestats (the crawl is 1v1).
//
// Runs LOCALLY (reads the desktop crawl backup + map-meta.json).
//   node scripts/data-pipeline/refresh-map-current.mjs

import { createReadStream, readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import path from "node:path";

const IN = path.resolve("data-cache/relic-patched/matches.ndjson");
const META = path.resolve("src/data/map-meta.json");

const civIdMap = JSON.parse(readFileSync(path.resolve("src/data/civ-id-map.json"), "utf8"));
const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const meta = JSON.parse(readFileSync(META, "utf8"));

const pct = (x) => +(x * 100).toFixed(2);
const canon = (s) => s.toLowerCase().replace(/[^a-z0-9]/g, "");
const eloBucket = (r) =>
  r < 1000 ? "<1000" : r < 1200 ? "1000-1199" : r < 1400 ? "1200-1399" : r < 1650 ? "1400-1649"
  : r < 1800 ? "1650-1799" : r < 2000 ? "1800-1999" : r < 2200 ? "2000-2199" : r < 2500 ? "2200-2499" : "2500+";

const MIN_MAP = 3000; // map needs this many current 1v1 matches to be refreshed
const MIN_ALL = 200; // per-civ gate, "all" bucket
const MIN_BUCKET = 60; // per-civ gate, single bucket

// canon(map name) -> map-meta key (prefer the higher-volume variant)
const canonToKey = {};
for (const [k, v] of Object.entries(meta.maps)) {
  const c = canon(k);
  const g = (v.games?.["1v1"] ?? 0) + (v.games?.team ?? 0);
  if (!canonToKey[c] || g > canonToKey[c].g) canonToKey[c] = { key: k, g };
}

// crawl map filename → map-meta key
const mapKeyFor = (raw) => {
  const c = canon(String(raw).replace(/\.[a-z0-9]+$/i, "")); // strip extension, canonicalize
  return canonToKey[c]?.key ?? null;
};

// --- aggregate crawl: key -> bucket -> civ -> {g,w} (+ "all") ---
const acc = {};
const rl = createInterface({ input: createReadStream(IN), crlfDelay: Infinity });
let used = 0;
for await (const line of rl) {
  if (!line.trim()) continue;
  let m;
  try { m = JSON.parse(line); } catch { continue; }
  const key = m.map_raw ? mapKeyFor(m.map_raw) : null;
  if (!key) continue;
  used++;
  const mp = (acc[key] ??= {});
  for (const pl of m.players ?? []) {
    const slug = civIdMap[String(pl.civ_id)];
    if (!slug || !guideCivs.has(slug)) continue;
    const b = eloBucket(pl.rating ?? 0);
    for (const bk of [b, "all"]) {
      const cw = (((mp[bk] ??= {})[slug] ??= { g: 0, w: 0 }));
      cw.g++;
      if (pl.won) cw.w++;
    }
  }
}

// --- splice current 1v1 rankings into map-meta ---
let refreshed = 0;
for (const [key, buckets] of Object.entries(acc)) {
  const total = Object.values(buckets.all ?? {}).reduce((s, v) => s + v.g, 0);
  if (total < MIN_MAP || !meta.maps[key]) continue;
  const byElo = {};
  for (const [bk, civs] of Object.entries(buckets)) {
    const min = bk === "all" ? MIN_ALL : MIN_BUCKET;
    const list = Object.entries(civs)
      .filter(([, v]) => v.g >= min)
      .map(([civ, v]) => ({ civ, games: v.g, winRate: pct(v.w / v.g) }))
      .sort((a, b) => b.winRate - a.winRate);
    if (list.length) byElo[bk] = list;
  }
  if (Object.keys(byElo).length) {
    meta.maps[key]["1v1"] = byElo;
    meta.maps[key].games["1v1"] = total;
    refreshed++;
  }
}

meta.source = "aoestats.io ranked archive (team) + self-collected World's Edge live ladder (1v1, current)";
meta.generated = new Date().toISOString().slice(0, 10);
writeFileSync(META, `${JSON.stringify(meta, null, 2)}\n`, "utf8");
console.log(`refresh-map-current: ${used} crawl rows matched a map · ${refreshed} maps got current 1v1 rankings → ${META}`);
