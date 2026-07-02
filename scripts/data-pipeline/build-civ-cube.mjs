#!/usr/bin/env node
// scripts/data-pipeline/build-civ-cube.mjs
//
// Build the JOINT civ stats cube (civ × elo × map × month) from the 1v1 crawl, so
// the /civs page can offer COMBINABLE filters (e.g. "current patch + 1200 elo +
// Arabia") with no server — the page fetches this static file and filters it in
// the browser. civ-meta only stores the separate 1-D marginals (byElo/byPatch/
// byMap), which cannot be intersected; this cube can.
//
// Output: public/civ-cube.json — packed as dictionaries + integer rows to stay
// small (~300KB gzipped over the wire). Fetched on demand by /civs.
//   { generated, source, civs:[slug…], elos:[…], maps:[key…], mapNames:[…],
//     months:[{patch,label}…], rows:[[civI,eloI,mapI,monthI,games,wins]…] }
//
// Map dimension: replay-parsed truth only (the API mapname is junk). Matches
// without a verified map land in a "__unknown__" sentinel that is IN the cube
// rows (so elo/patch slices stay complete) but OUT of the dims dropdown lists.
// Civ ids are the Relic API space (relic-civ-id-map.json); ranked-RM 1v1 only.
//
// Runs on the box that holds data-cache (the VM).
//   node scripts/data-pipeline/build-civ-cube.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { eloBucket } from "./lib/buckets.mjs";
import { crawlRecords } from "./lib/crawl-stream.mjs";
import { canonToKeyIndex, isRanked1v1, loadReplayMapTruth, relicCivSlug } from "./lib/relic-map.mjs";

const OUT = path.resolve("public/civ-cube.json");
const DIMS = path.resolve("src/data/civ-cube-dims.json"); // tiny: dropdown lists, imported at build
const UNKNOWN_MAP = "__unknown__";

const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const civMeta = JSON.parse(readFileSync(path.resolve("src/data/civ-meta.json"), "utf8"));
const mapMeta = JSON.parse(readFileSync(path.resolve("src/data/map-meta.json"), "utf8"));

const monthKey = (t) => { const d = new Date(t * 1000); return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}`; };
const prettify = (s) => s.split(/[-_]/).map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");

// The patch axis = the same dated months civ-meta exposes (so the filter matches).
const patchMeta = civMeta.patches ?? [];
const keepMonths = new Set(patchMeta.map((p) => p.patch));
const monthLabel = Object.fromEntries(patchMeta.map((p) => [p.patch, p.label]));

// replay-truth map canon → map-meta key (so the map filter lines up with the map pages)
const canonToKey = canonToKeyIndex(mapMeta);
const mapTruth = await loadReplayMapTruth();

// --- aggregate: "civ|elo|map|month" -> [games, wins] ---
const cube = {};
const mapGames = {};
let rows = 0;
let skippedNullElo = 0;
for await (const m of crawlRecords()) {
  if (!isRanked1v1(m)) continue;
  const mo = monthKey(m.completed);
  if (!keepMonths.has(mo)) continue;
  const truth = mapTruth.get(m.match_id);
  const mk = truth ? (canonToKey[truth.canon]?.key ?? UNKNOWN_MAP) : UNKNOWN_MAP;
  for (const pl of m.players ?? []) {
    const slug = relicCivSlug(pl.civ_id);
    if (!guideCivs.has(slug)) continue;
    const eb = eloBucket(pl.rating); if (eb == null) { skippedNullElo++; continue; }
    const k = `${slug}|${eb}|${mk}|${mo}`;
    const cw = (cube[k] ??= [0, 0]);
    cw[0]++; if (pl.won) cw[1]++;
    if (mk !== UNKNOWN_MAP) mapGames[mk] = (mapGames[mk] ?? 0) + 1;
    rows++;
  }
}

// --- pack with dictionaries + integer-indexed rows ---
const civs = [...new Set(Object.keys(cube).map((k) => k.split("|")[0]))].sort();
const elos = (civMeta.eloBuckets ?? []).filter((b) => b !== "all");
// Most-played first; the unknown sentinel goes LAST and is excluded from dims.
const maps = [...Object.keys(mapGames).sort((a, b) => mapGames[b] - mapGames[a]), UNKNOWN_MAP];
const months = patchMeta.map((p) => p.patch).filter((mo) => Object.keys(cube).some((k) => k.endsWith(`|${mo}`)));
const ci = Object.fromEntries(civs.map((c, i) => [c, i]));
const ei = Object.fromEntries(elos.map((c, i) => [c, i]));
const mi = Object.fromEntries(maps.map((c, i) => [c, i]));
const pi = Object.fromEntries(months.map((c, i) => [c, i]));

const packed = Object.entries(cube).map(([k, v]) => {
  const [c, e, mp, mo] = k.split("|");
  return [ci[c], ei[e], mi[mp], pi[mo], v[0], v[1]];
});

const out = {
  generated: new Date().toISOString().slice(0, 10),
  // NOT civMeta.source: the cube spans the whole dated patch axis (its months
  // dim), not civ-meta's rolling current window.
  source: "self-collected World's Edge live ladder (ranked RM 1v1, monthly patch axis; maps replay-verified)",
  civs,
  elos,
  maps,
  mapNames: maps.map((k) => (k === UNKNOWN_MAP ? "Unknown" : prettify(k))),
  months: months.map((mo) => ({ patch: mo, label: monthLabel[mo] ?? mo })),
  rows: packed,
};
writeFileSync(OUT, JSON.stringify(out));
// Tiny dims file (no rows) for the page to import at build time → dropdowns that
// exactly match the cube. The unknown-map sentinel stays OUT of the dropdowns
// (it exists only so unfiltered elo/patch slices keep their full volume).
const dimMaps = maps.filter((k) => k !== UNKNOWN_MAP);
writeFileSync(DIMS, `${JSON.stringify({ generated: out.generated, elos, maps: dimMaps, mapNames: dimMaps.map(prettify), months: out.months }, null, 2)}\n`);
console.log(`build-civ-cube: ${rows} appearances · ${packed.length} cells · ${civs.length} civs × ${elos.length} elos × ${maps.length} maps × ${months.length} months · ${skippedNullElo} null-elo dropped`);
console.log(`  → ${OUT}`);
console.log(`  → ${DIMS}`);
