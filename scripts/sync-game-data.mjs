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
import { AALISES_SHA, AOE2TECHTREE_SHA } from "./lib/aoe2-sources.mjs";

// Pinned SHAs — fetched 2026-05-23; AOE2TT_SHA bumped 2026-07-01 (picks up DE Update 177723)
const AOE2TT_SHA = AOE2TECHTREE_SHA;

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
  {
    // Localized in-game strings — civ help text (bonuses/team bonus) by help_string_id.
    url: `https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/${AOE2TT_SHA}/data/locales/en/strings.json`,
    dest: path.join(CACHE_DIR, "strings-en.json"),
  },
  {
    // Official in-game Turkish help text — civ bonuses/techs by help_string_id.
    url: `https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/${AOE2TT_SHA}/data/locales/tr/strings.json`,
    dest: path.join(CACHE_DIR, "strings-tr.json"),
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
