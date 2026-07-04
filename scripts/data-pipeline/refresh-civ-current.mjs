#!/usr/bin/env node
// scripts/data-pipeline/refresh-civ-current.mjs
//
// Refresh the 1v1 civ stats in civ-meta.json with the CURRENT self-collected
// World's Edge crawl (all crawl sources via lib/crawl-stream.mjs, last
// CURRENT_WINDOW_DAYS, ranked-RM 1v1 only). aggregate-rich built these from
// the frozen aoestats archive; this recomputes the 1v1 overall (winRate,
// ci95, tier, playRate), byElo, and byMap from the live ladder so the default
// civ view is current. byPatch (already crawl-derived) and the team ladder
// (refresh-team-current) are preserved. Civ ids are the Relic API space
// (relic-civ-id-map.json); per-map slices use replay-parsed map truth only
// (the API mapname is wrong for most matches).
//
// Runs on the box that holds data-cache (the VM).
//   node scripts/data-pipeline/refresh-civ-current.mjs

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
import { pct, tierOf, wilson } from "./lib/stats.mjs";

const META = path.resolve("src/data/civ-meta.json");

const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map(
    (c) => c.slug,
  ),
);
const meta = JSON.parse(readFileSync(META, "utf8"));
const mapMeta = JSON.parse(readFileSync(path.resolve("src/data/map-meta.json"), "utf8"));

const MIN_ELO = 100; // per-bucket gate (crawl is thinner at the top than aoestats)
const MIN_MAP = 200; // per-map gate

// replay-truth map canon → map-meta key (so byMap keys stay consistent with map pages)
const canonToKey = canonToKeyIndex(mapMeta);
const mapTruth = await loadReplayMapTruth();

// --- aggregate the crawl per civ ---
const civ = {};
let totalApp = 0;
let skippedNullElo = 0;
for await (const m of crawlRecords({ recentDays: CURRENT_WINDOW_DAYS })) {
  if (!isRanked1v1(m)) continue;
  const truth = mapTruth.get(m.match_id);
  const mapKey = truth ? (canonToKey[truth.canon]?.key ?? null) : null;
  for (const pl of m.players ?? []) {
    const slug = relicCivSlug(pl.civ_id);
    if (!guideCivs.has(slug)) continue;
    totalApp++;
    const c = (civ[slug] ??= { g: 0, w: 0, byElo: {}, byMap: {} });
    const won = pl.won ? 1 : 0;
    c.g++;
    c.w += won;
    const eb = eloBucket(pl.rating);
    if (eb == null) {
      skippedNullElo++;
      continue;
    }
    const be = (c.byElo[eb] ??= { g: 0, w: 0 });
    be.g++;
    be.w += won;
    if (mapKey) {
      const bm = (c.byMap[mapKey] ??= { g: 0, w: 0 });
      bm.g++;
      bm.w += won;
    }
  }
}

// --- splice current 1v1 stats into civ-meta ---
let updated = 0;
for (const [slug, c] of Object.entries(civ)) {
  const entry = meta.civs[slug];
  if (!entry || c.g < 500) continue; // need a usable civ sample
  // Civs missing from the frozen aoestats archive (post-freeze DLC: muisca,
  // mapuche, tupi, …) have a null 1v1 block — the crawl IS their only source.
  const o = (entry["1v1"] ??= { byPatch: {}, openings: [], ageUp: null });
  const [lo, hi] = wilson(c.w, c.g);
  o.games = c.g;
  o.winRate = pct(c.w / c.g);
  o.ci95 = [pct(lo), pct(hi)];
  o.tier = tierOf((c.w / c.g) * 100);
  o.playRate = pct(c.g / totalApp);
  o.byElo = Object.fromEntries(
    Object.entries(c.byElo)
      .filter(([, v]) => v.g >= MIN_ELO)
      .map(([b, v]) => [b, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  // byMap is limited to replay-VERIFIED matches; when coverage is too thin to
  // say anything, PRESERVE the previous slices rather than wiping the chart.
  const byMap = Object.fromEntries(
    Object.entries(c.byMap)
      .filter(([, v]) => v.g >= MIN_MAP)
      .map(([k, v]) => [k, { games: v.g, winRate: pct(v.w / v.g) }]),
  );
  if (Object.keys(byMap).length) o.byMap = byMap;
  updated++;
}

meta.appearances = { ...(meta.appearances ?? {}), "1v1": totalApp };
meta.source = `self-collected World's Edge live ladder (ranked RM, last ${CURRENT_WINDOW_DAYS} days; maps replay-verified)`;
meta.generated = new Date().toISOString().slice(0, 10);
writeFileSync(META, `${JSON.stringify(meta, null, 2)}\n`, "utf8");
console.log(
  `refresh-civ-current: ${totalApp} crawl appearances · ${updated} civs got current 1v1 overall/byElo/byMap · ${skippedNullElo} null-elo dropped → ${META}`,
);
