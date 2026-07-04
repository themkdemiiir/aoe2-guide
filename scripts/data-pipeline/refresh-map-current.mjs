#!/usr/bin/env node
// scripts/data-pipeline/refresh-map-current.mjs
//
// Refresh the 1v1 map rankings in map-meta.json with the CURRENT self-collected
// World's Edge crawl (all crawl sources via lib/crawl-stream.mjs, last
// CURRENT_WINDOW_DAYS, ranked-RM 1v1 only). aggregate-maps builds map-meta from
// the frozen aoestats archive only; this splices the live crawl's current
// map × civ × elo 1v1 rankings over the top (per map, where the crawl has
// enough volume). Team rankings are refreshed by refresh-team-current.
// The per-match map is REPLAY-PARSED truth only (lib/relic-map.mjs
// loadReplayMapTruth) — the API mapname is wrong for most matches, so matches
// without a parsed replay contribute NO map slice. Civ ids are the Relic API
// space (relic-civ-id-map.json).
//
// Runs on the box that holds data-cache (the VM).
//   node scripts/data-pipeline/refresh-map-current.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { eloBucket } from "./lib/buckets.mjs";
import { CURRENT_WINDOW_DAYS, crawlRecords } from "./lib/crawl-stream.mjs";
import {
  canonToKeyIndex,
  isRanked1v1,
  loadReplayMapTruth,
  relicCivSlug,
} from "./lib/relic-map.mjs";
import { pct } from "./lib/stats.mjs";

const META = path.resolve("src/data/map-meta.json");

const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map(
    (c) => c.slug,
  ),
);
const meta = JSON.parse(readFileSync(META, "utf8"));

const MIN_MAP = 3000; // map needs this many current 1v1 matches to be included
const MIN_ALL = 200; // per-civ gate, "all" bucket
const MIN_BUCKET = 60; // per-civ gate, single bucket
const MIN_RANKED_CIVS = 10; // a NEW (crawl-only) map needs this many civs in "all" to render a page (matches src/lib/data-maps.ts)

// canon(map name) -> map-meta key (prefer the higher-volume variant)
const canonToKey = canonToKeyIndex(meta);

// Crawl-only maps (not in the frozen archive) get a fresh underscore key derived
// from the replay-truth map NAME, so "Border Dispute" → border_dispute → the
// route renders /maps/border-dispute with a clean prettified name. Existing maps
// still resolve to their archive key (the higher-volume spelling).
const mkKey = (name) =>
  String(name)
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "_")
    .replace(/^_+|_+$/g, "");
const mapTruth = await loadReplayMapTruth();

// --- aggregate crawl: key -> bucket -> civ -> {g,w} (+ "all") ---
const acc = {};
let used = 0;
let skippedNullElo = 0;
for await (const m of crawlRecords({ recentDays: CURRENT_WINDOW_DAYS })) {
  if (!isRanked1v1(m)) continue;
  const truth = mapTruth.get(m.match_id);
  if (!truth) continue; // no replay-verified map — the API mapname is untrustworthy
  const key = canonToKey[truth.canon]?.key ?? mkKey(truth.name);
  used++;
  const mp = (acc[key] ??= {});
  for (const pl of m.players ?? []) {
    const slug = relicCivSlug(pl.civ_id);
    if (!guideCivs.has(slug)) continue;
    const eb = eloBucket(pl.rating);
    if (eb == null) {
      skippedNullElo++;
      continue;
    }
    for (const bk of [eb, "all"]) {
      const cw = ((mp[bk] ??= {})[slug] ??= { g: 0, w: 0 });
      cw.g++;
      if (pl.won) cw.w++;
    }
  }
}

// --- splice current 1v1 rankings into map-meta: overlay existing archive maps,
//     and ADD crawl-only maps that clear the volume + ranked-civ gates ---
let refreshed = 0;
let added = 0;
const addedKeys = [];
for (const [key, buckets] of Object.entries(acc)) {
  const total = Object.values(buckets.all ?? {}).reduce((s, v) => s + v.g, 0);
  if (total < MIN_MAP) continue;
  const byElo = {};
  for (const [bk, civs] of Object.entries(buckets)) {
    const min = bk === "all" ? MIN_ALL : MIN_BUCKET;
    const list = Object.entries(civs)
      .filter(([, v]) => v.g >= min)
      .map(([civ, v]) => ({ civ, games: v.g, winRate: pct(v.w / v.g) }))
      .sort((a, b) => b.winRate - a.winRate);
    if (list.length) byElo[bk] = list;
  }
  if (!Object.keys(byElo).length) continue;
  if (meta.maps[key]) {
    // existing archive map: overlay current 1v1 rankings, keep team as-is
    meta.maps[key]["1v1"] = byElo;
    meta.maps[key].games["1v1"] = total;
    refreshed++;
  } else if ((byElo.all?.length ?? 0) >= MIN_RANKED_CIVS) {
    // crawl-only map: add a new data-only entry (no team — the crawl is 1v1 only)
    meta.maps[key] = { games: { "1v1": total, team: 0 }, "1v1": byElo, team: null };
    added++;
    addedKeys.push(`${key} (${total})`);
  }
}

meta.source =
  "aoestats.io ranked archive (team) + self-collected World's Edge live ladder (1v1, current)";
meta.generated = new Date().toISOString().slice(0, 10);
writeFileSync(META, `${JSON.stringify(meta, null, 2)}\n`, "utf8");
console.log(
  `refresh-map-current: ${used} crawl rows · ${refreshed} maps refreshed · ${added} crawl-only maps added · ${skippedNullElo} null-elo dropped → ${META}`,
);
if (added) console.log(`  added: ${addedKeys.sort().join(", ")}`);
