#!/usr/bin/env node
// scripts/sync-game-data.mjs
// Fetches game data from two upstream repos at pinned SHAs.
//
// Sources:
//   1. aalises/age-of-empires-II-api  — BSD-3-Clause
//      civilizations.csv  (31 civs)
//      units.csv          (104 units)
//   2. SiegeEngineers/aoe2techtree    — MIT (code)
//      data/data.json     (53 civs, unit stats)
//
// Refresh policy: bump the SHA constants below in a deliberate PR, or pass
// env vars AALISES_SHA and AOE2TT_SHA to override.

import { mkdir, writeFile } from "node:fs/promises";
import path from "node:path";

// Pinned SHAs — fetched 2026-05-23
const AALISES_SHA = process.env.AALISES_SHA || "3ec582fa0ebd5ea11b2d1ff405e61836c6f3a99d";
const AOE2TT_SHA = process.env.AOE2TT_SHA || "b34082d13c31932d89788ad35af984896cbe050c";

const CACHE_DIR = path.resolve(".cache/aoe2-data");

const URLS = [
  {
    url: `https://raw.githubusercontent.com/aalises/age-of-empires-II-api/${AALISES_SHA}/data/civilizations.csv`,
    dest: path.join(CACHE_DIR, "civilizations.csv"),
  },
  {
    url: `https://raw.githubusercontent.com/aalises/age-of-empires-II-api/${AALISES_SHA}/data/units.csv`,
    dest: path.join(CACHE_DIR, "units.csv"),
  },
  {
    url: `https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/${AOE2TT_SHA}/data/data.json`,
    dest: path.join(CACHE_DIR, "data.json"),
  },
];

async function fetchFile({ url, dest }) {
  console.log(`Fetching ${url}`);
  const res = await fetch(url);
  if (!res.ok) throw new Error(`HTTP ${res.status} for ${url}`);
  const text = await res.text();
  await writeFile(dest, text, "utf8");
  console.log(`  → ${dest} (${text.length} bytes)`);
}

async function run() {
  await mkdir(CACHE_DIR, { recursive: true });
  for (const entry of URLS) {
    await fetchFile(entry);
  }
  console.log("Done. Cached files:");
  for (const entry of URLS) {
    console.log(" ", entry.dest);
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
