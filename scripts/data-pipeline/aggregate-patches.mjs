#!/usr/bin/env node
// scripts/data-pipeline/aggregate-patches.mjs
//
// Build a REAL, date-aligned patch axis from the self-collected World's Edge
// crawl (all sources via lib/crawl-stream.mjs, ranked-RM 1v1, whole current
// id-space era). The API exposes no build number, but months are a clean dated
// axis (gamemod_id flips too often). We aggregate per civ × month, then splice
// that byPatch into civ-meta.json, replacing the frozen aoestats build-number
// patches. Overall / byElo / byMap stay whatever the refresh scripts wrote.
// Civ ids are the Relic API space (relic-civ-id-map.json).
//
// Runs on the box that holds data-cache (the VM).
//   node scripts/data-pipeline/aggregate-patches.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { crawlRecords } from "./lib/crawl-stream.mjs";
import { isRanked1v1, relicCivSlug } from "./lib/relic-map.mjs";

const META = path.resolve("src/data/civ-meta.json");

const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));

const pct = (x) => +(x * 100).toFixed(2);
const MIN_PATCH_MATCHES = 3000; // a gamemod needs this many matches to count as a "patch"
const MIN_CIV_GAMES = 100; // per civ × patch gate (self-collected sample is smaller than aoestats)
const MAX_PATCHES = 16; // keep the most recent N patches

const MONTHS = ["Jan", "Feb", "Mar", "Apr", "May", "Jun", "Jul", "Aug", "Sep", "Oct", "Nov", "Dec"];
// gamemod_id flips several times a month (hotfixes), so bucket by CALENDAR MONTH
// for a clean, unambiguous dated patch axis.
const monthKey = (t) => { const d = new Date(t * 1000); return `${d.getUTCFullYear()}-${String(d.getUTCMonth() + 1).padStart(2, "0")}`; };
const monthLabel = (key) => { const [y, mo] = key.split("-"); return `${MONTHS[+mo - 1]} ${y}`; };

// --- pass 1: accumulate per month + per civ × month --------------------------
const gm = {}; // monthKey -> { matches }
const civPatch = {}; // slug -> monthKey -> { g, w }

let lines = 0;
for await (const m of crawlRecords()) {
  if (!isRanked1v1(m)) continue;
  const g = monthKey(m.completed);
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

// --- pick the patches: recent months with enough matches ---------------------
const patches = Object.entries(gm)
  .filter(([, v]) => v.matches >= MIN_PATCH_MATCHES)
  .sort((a, b) => b[0].localeCompare(a[0]))
  .slice(0, MAX_PATCHES)
  .map(([g, v]) => ({ patch: g, label: monthLabel(g), matches: v.matches }));
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
  if (Object.keys(byPatch).length) { m["1v1"].byPatch = byPatch; civsUpdated++; }
}

meta.patches = patches; // now [{patch:gamemod, label:"Jun 2026", matches}]
meta.patchSource = "self-collected World's Edge crawl (ranked RM 1v1, monthly dated patch axis)";
meta.generated = new Date().toISOString().slice(0, 10);
writeFileSync(META, `${JSON.stringify(meta, null, 2)}\n`, "utf8");

console.log(`patches: ${lines} crawl rows · ${patches.length} dated patches · ${civsUpdated} civs got byPatch`);
console.log(`newest patch: gamemod ${patches[0]?.patch} = ${patches[0]?.label} (${patches[0]?.matches} matches)`);
console.log(`→ ${META}`);
