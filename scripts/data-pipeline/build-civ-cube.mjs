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
// Runs LOCALLY (reads the desktop crawl + civ-meta + map-meta).
//   node scripts/data-pipeline/build-civ-cube.mjs

import { createReadStream, readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import path from "node:path";

const IN = path.resolve("data-cache/relic-patched/matches.ndjson");
const OUT = path.resolve("public/civ-cube.json");
const DIMS = path.resolve("src/data/civ-cube-dims.json"); // tiny: dropdown lists, imported at build

const civIdMap = JSON.parse(readFileSync(path.resolve("src/data/civ-id-map.json"), "utf8"));
const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const civMeta = JSON.parse(readFileSync(path.resolve("src/data/civ-meta.json"), "utf8"));
const mapMeta = JSON.parse(readFileSync(path.resolve("src/data/map-meta.json"), "utf8"));

const canon = (s) => String(s).toLowerCase().replace(/\.[a-z0-9]+$/i, "").replace(/[^a-z0-9]/g, "");
const eloBucket = (r) =>
  r < 1000 ? "<1000" : r < 1200 ? "1000-1199" : r < 1400 ? "1200-1399" : r < 1650 ? "1400-1649"
  : r < 1800 ? "1650-1799" : r < 2000 ? "1800-1999" : r < 2200 ? "2000-2199" : r < 2500 ? "2200-2499" : "2500+";
const monthKey = (t) => { const d = new Date(t * 1000); return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}`; };
const prettify = (s) => s.split(/[-_]/).map((w) => w.charAt(0).toUpperCase() + w.slice(1)).join(" ");

// The patch axis = the same dated months civ-meta exposes (so the filter matches).
const patchMeta = civMeta.patches ?? [];
const keepMonths = new Set(patchMeta.map((p) => p.patch));
const monthLabel = Object.fromEntries(patchMeta.map((p) => [p.patch, p.label]));

// crawl map name → map-meta key (so the map filter lines up with the map pages)
const canonToKey = {};
for (const [k, v] of Object.entries(mapMeta.maps)) {
  const c = canon(k);
  const g = (v.games?.["1v1"] ?? 0) + (v.games?.team ?? 0);
  if (!canonToKey[c] || g > canonToKey[c].g) canonToKey[c] = { key: k, g };
}
const mapKeyFor = (raw) => canonToKey[canon(raw)]?.key ?? null;

// --- aggregate: "civ|elo|map|month" -> [games, wins] ---
const cube = {};
const mapGames = {};
let rows = 0;
const rl = createInterface({ input: createReadStream(IN), crlfDelay: Infinity });
for await (const line of rl) {
  if (!line.trim()) continue;
  let m;
  try { m = JSON.parse(line); } catch { continue; }
  if (!m.completed) continue;
  const mo = monthKey(m.completed);
  if (!keepMonths.has(mo)) continue;
  const mk = m.map_raw ? mapKeyFor(m.map_raw) : null;
  if (!mk) continue;
  for (const pl of m.players ?? []) {
    const slug = civIdMap[String(pl.civ_id)];
    if (!slug || !guideCivs.has(slug)) continue;
    const k = `${slug}|${eloBucket(pl.rating ?? 0)}|${mk}|${mo}`;
    const cw = (cube[k] ??= [0, 0]);
    cw[0]++; if (pl.won) cw[1]++;
    mapGames[mk] = (mapGames[mk] ?? 0) + 1;
    rows++;
  }
}

// --- pack with dictionaries + integer-indexed rows ---
const civs = [...new Set(Object.keys(cube).map((k) => k.split("|")[0]))].sort();
const elos = (civMeta.eloBuckets ?? []).filter((b) => b !== "all");
const maps = Object.keys(mapGames).sort((a, b) => mapGames[b] - mapGames[a]); // most-played first
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
  source: civMeta.source ?? "self-collected World's Edge live ladder (1v1)",
  civs,
  elos,
  maps,
  mapNames: maps.map(prettify),
  months: months.map((mo) => ({ patch: mo, label: monthLabel[mo] ?? mo })),
  rows: packed,
};
writeFileSync(OUT, JSON.stringify(out));
// Tiny dims file (no rows) for the page to import at build time → dropdowns that
// exactly match the cube. The big `rows` stay in public/ and are fetched on demand.
writeFileSync(DIMS, `${JSON.stringify({ generated: out.generated, elos, maps, mapNames: out.mapNames, months: out.months }, null, 2)}\n`);
console.log(`build-civ-cube: ${rows} appearances · ${packed.length} cells · ${civs.length} civs × ${elos.length} elos × ${maps.length} maps × ${months.length} months`);
console.log(`  → ${OUT}`);
console.log(`  → ${DIMS}`);
