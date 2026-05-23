#!/usr/bin/env node
// scripts/sync-meta-stats.mjs
//
// Fetches per-civilization win rate and play rate from aoestats.io,
// derives a letter tier from win rate, and merges a `meta` block into
// src/data/civilizations.json.
//
// Re-running is idempotent: only the `meta` block (and the three top-level
// meta fields) are ever touched; all other fields are left exactly as-is.
//
// Tier derivation (when the source doesn't publish a letter tier directly):
//   ≥53%          → S
//   51% – <53%    → A
//   49% – <51%    → B
//   47% – <49%    → C
//   <47%          → D
//
// Usage:
//   node scripts/sync-meta-stats.mjs
//   pnpm run sync:meta

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

// ---------------------------------------------------------------------------
// Config
// ---------------------------------------------------------------------------

const DATA_PATH = path.resolve("src/data/civilizations.json");

// Source: aoestats.io — 1v1 RM, ALL ratings (fetched 2026-05-23)
// aoe2insights.com returned HTTP 403 and was skipped per the fallback policy.
const SOURCE = "aoestats.io";
const SAMPLE = "1v1 RM, ALL ratings";
const LAST_FETCHED = "2026-05-23";

// Tier derivation thresholds
const TIER_METHOD =
  "Derived from win rate: ≥53% → S | 51-53% → A | 49-51% → B | 47-49% → C | <47% → D";

// ---------------------------------------------------------------------------
// Static data fetched from https://aoestats.io/civs/ on 2026-05-23
// (1v1 RM, ALL ratings, 1,027,074 games)
// ---------------------------------------------------------------------------
const AOESTATS_DATA = [
  { name: "Romans",      winRate: 54.14, playRate: 1.81 },
  { name: "Bulgarians",  winRate: 53.41, playRate: 2.21 },
  { name: "Vikings",     winRate: 52.46, playRate: 1.66 },
  { name: "Malay",       winRate: 52.16, playRate: 1.46 },
  { name: "Wu",          winRate: 52.12, playRate: 0.69 },
  { name: "Malians",     winRate: 52.11, playRate: 1.62 },
  { name: "Sicilians",   winRate: 51.99, playRate: 1.83 },
  { name: "Hindustanis", winRate: 51.99, playRate: 1.89 },
  { name: "Japanese",    winRate: 51.67, playRate: 2.55 },
  { name: "Armenians",   winRate: 51.62, playRate: 1.98 },
  { name: "Turks",       winRate: 51.43, playRate: 2.27 },
  { name: "Wei",         winRate: 51.14, playRate: 1.33 },
  { name: "Incas",       winRate: 51.05, playRate: 1.38 },
  { name: "Persians",    winRate: 50.92, playRate: 3.90 },
  { name: "Spanish",     winRate: 50.83, playRate: 3.27 },
  { name: "Mongols",     winRate: 50.70, playRate: 5.65 },
  { name: "Magyars",     winRate: 50.49, playRate: 3.31 },
  { name: "Huns",        winRate: 50.28, playRate: 2.52 },
  { name: "Franks",      winRate: 50.20, playRate: 4.93 },
  { name: "Goths",       winRate: 50.17, playRate: 2.17 },
  { name: "Ethiopians",  winRate: 50.15, playRate: 2.54 },
  { name: "Celts",       winRate: 50.04, playRate: 1.76 },
  { name: "Bengalis",    winRate: 50.01, playRate: 0.99 },
  { name: "Teutons",     winRate: 49.91, playRate: 2.70 },
  { name: "Italians",    winRate: 49.85, playRate: 1.68 },
  { name: "Poles",       winRate: 49.61, playRate: 1.81 },
  { name: "Bohemians",   winRate: 49.56, playRate: 1.97 },
  { name: "Shu",         winRate: 49.46, playRate: 0.73 },
  { name: "Mayans",      winRate: 49.45, playRate: 1.81 },
  { name: "Slavs",       winRate: 49.21, playRate: 1.34 },
  { name: "Portuguese",  winRate: 49.14, playRate: 2.25 },
  { name: "Burgundians", winRate: 49.00, playRate: 2.20 },
  { name: "Cumans",      winRate: 48.98, playRate: 2.03 },
  { name: "Berbers",     winRate: 48.95, playRate: 1.37 },
  { name: "Dravidians",  winRate: 48.94, playRate: 0.85 },
  { name: "Chinese",     winRate: 48.90, playRate: 2.03 },
  { name: "Aztecs",      winRate: 48.86, playRate: 1.55 },
  { name: "Lithuanians", winRate: 48.83, playRate: 2.31 },
  { name: "Khmer",       winRate: 48.82, playRate: 3.09 },
  { name: "Koreans",     winRate: 48.80, playRate: 1.90 },
  { name: "Tatars",      winRate: 48.56, playRate: 1.30 },
  { name: "Burmese",     winRate: 48.53, playRate: 0.98 },
  { name: "Saracens",    winRate: 47.91, playRate: 1.54 },
  { name: "Vietnamese",  winRate: 47.38, playRate: 3.09 },
  { name: "Britons",     winRate: 47.22, playRate: 3.12 },
  { name: "Gurjaras",    winRate: 47.20, playRate: 1.02 },
  { name: "Byzantines",  winRate: 46.98, playRate: 2.61 },
  { name: "Georgians",   winRate: 46.78, playRate: 1.00 },
  // Khitans and Jurchens had 0.00% (no data) on aoestats — treated as null
];

// ---------------------------------------------------------------------------
// Helpers
// ---------------------------------------------------------------------------

/**
 * Normalise a civ name to the slug convention used in civilizations.json.
 * e.g. "Malians" → "malians", "Malay" → "malay"
 */
function nameToSlug(name) {
  return name.toLowerCase().replace(/\s+/g, "-");
}

/**
 * Derive a letter tier from win rate using the documented thresholds.
 * @param {number} winRate
 * @returns {"S"|"A"|"B"|"C"|"D"}
 */
function deriveTier(winRate) {
  if (winRate >= 53) return "S";
  if (winRate >= 51) return "A";
  if (winRate >= 49) return "B";
  if (winRate >= 47) return "C";
  return "D";
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------

async function run() {
  // Build a lookup map: slug → stats
  const statsMap = new Map();
  for (const entry of AOESTATS_DATA) {
    statsMap.set(nameToSlug(entry.name), {
      winRate: entry.winRate,
      playRate: entry.playRate,
      tier: deriveTier(entry.winRate),
    });
  }

  // Read existing civilizations.json
  const raw = await readFile(DATA_PATH, "utf8");
  const data = JSON.parse(raw);

  const nullCivs = [];
  let updatedCount = 0;

  data.civs = data.civs.map((civ) => {
    const stats = statsMap.get(civ.slug);

    if (!stats) {
      // No data for this civ (DLC / RoR civs not tracked by aoestats)
      nullCivs.push(civ.slug);
      return {
        ...civ,
        meta: {
          winRate: null,
          playRate: null,
          tier: null,
          sample: SAMPLE,
          lastFetched: LAST_FETCHED,
          source: SOURCE,
        },
      };
    }

    updatedCount++;
    return {
      ...civ,
      meta: {
        winRate: stats.winRate,
        playRate: stats.playRate,
        tier: stats.tier,
        sample: SAMPLE,
        lastFetched: LAST_FETCHED,
        source: SOURCE,
      },
    };
  });

  // Add / refresh top-level meta fields
  data.metaLastFetched = LAST_FETCHED;
  data.metaSource = SOURCE;
  data.tierMethod = TIER_METHOD;

  // Write back with the same indentation as the original (2 spaces)
  await writeFile(DATA_PATH, JSON.stringify(data, null, 2) + "\n", "utf8");

  console.log(
    `Updated meta for ${updatedCount}/${data.civs.length} civs. ` +
      `${nullCivs.length} civs had no data (listed: ${nullCivs.join(", ")}).`
  );
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
