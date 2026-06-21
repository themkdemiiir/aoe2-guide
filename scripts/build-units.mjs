#!/usr/bin/env node
// scripts/build-units.mjs  — DATA ONLY
// Reads .cache/aoe2-data/units.csv and data.json.
// Writes:
//   src/data/unit-stats.json            (all units — numeric stats only)
//
// Unit content (descriptions, roles, names) lives in bilingual YAML files at
// src/content/units/<slug>.yaml — those are the source of truth and are NOT
// generated or modified by this script.
//
// Sources:
//   aalises/age-of-empires-II-api  BSD-3-Clause  (units.csv, 104 units)
//   SiegeEngineers/aoe2techtree    MIT            (data.json, unit stats)

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const CACHE_DIR = path.resolve(".cache/aoe2-data");
const DATA_OUT = path.resolve("src/data/unit-stats.json");

// ---------------------------------------------------------------------------
// CSV parser
// ---------------------------------------------------------------------------
function parseCsv(text) {
  const lines = text.split("\n").filter(Boolean);
  const headers = lines[0].split(",").map((h) => h.trim());
  const rows = [];
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) continue;
    const cols = splitCsvLine(line);
    const row = {};
    headers.forEach((h, idx) => {
      row[h] = (cols[idx] || "").trim();
    });
    rows.push(row);
  }
  return rows;
}

function splitCsvLine(line) {
  const result = [];
  let cur = "";
  let inQuote = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === '"') {
      inQuote = !inQuote;
    } else if (ch === "," && !inQuote) {
      result.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  result.push(cur);
  return result;
}

// ---------------------------------------------------------------------------
// Parse cost string like {"Wood": 25; "Gold": 45} or {"Food": 60; "Gold": 20}
// ---------------------------------------------------------------------------
function parseCost(costStr) {
  const cost = { food: 0, wood: 0, gold: 0, stone: 0 };
  if (!costStr || costStr.includes("info")) return cost;
  // Remove braces and any stray quotes (CSV parser strips them when inside fields)
  const inner = costStr.replace(/[{}"]/g, "").trim();
  // Split on semicolon
  const parts = inner.split(";");
  for (const part of parts) {
    // Match both "Food: 60" and "Food: 60" (with or without quotes, after stripping)
    const m = part.trim().match(/(\w+)\s*:\s*(\d+)/);
    if (m) {
      const key = m[1].toLowerCase();
      const val = parseInt(m[2], 10);
      if (key in cost) cost[key] = val;
    }
  }
  return cost;
}

// ---------------------------------------------------------------------------
// Parse range string like "5", "1-4", "4-16"
// Returns { range, minRange }
// ---------------------------------------------------------------------------
function parseRange(rangeStr) {
  if (!rangeStr || rangeStr.trim() === "0" || rangeStr.trim() === "") {
    return { range: 0, minRange: 0 };
  }
  const s = rangeStr.trim();
  if (s.includes("-")) {
    const parts = s.split("-");
    return { range: parseInt(parts[1], 10) || 0, minRange: parseInt(parts[0], 10) || 0 };
  }
  return { range: parseInt(s, 10) || 0, minRange: 0 };
}

// ---------------------------------------------------------------------------
// Parse armor string like "0/0", "1/0", "0/3"
// Returns { melee, pierce }
// ---------------------------------------------------------------------------
function parseArmor(armorStr) {
  if (!armorStr?.includes("/")) return { melee: 0, pierce: 0 };
  const parts = armorStr.split("/");
  return {
    melee: parseInt(parts[0], 10) || 0,
    pierce: parseInt(parts[1], 10) || 0,
  };
}

// ---------------------------------------------------------------------------
// The canonical list of trainable units we want to include.
// Maps display name (as it appears in aalises) -> slug
// This controls which units get emitted. If a unit is in aalises but not here,
// it won't be included. Unique units from all civs are added separately.
// ---------------------------------------------------------------------------
const CANONICAL_UNITS = {
  // Militia line
  Militia: "militia",
  "Man-at-Arms": "man-at-arms",
  "Long Swordsman": "long-swordsman",
  "Two-Handed Swordsman": "two-handed-swordsman",
  Champion: "champion",
  // Spear line
  Spearman: "spearman",
  Pikeman: "pikeman",
  Halberdier: "halberdier",
  // Eagle line (Meso civs)
  "Eagle Warrior": "eagle-warrior",
  "Elite Eagle Warrior": "elite-eagle-warrior",
  // Archer line
  Archer: "archer",
  Crossbowman: "crossbowman",
  Arbalest: "arbalester",
  // Skirmisher line
  Skirmisher: "skirmisher",
  "Elite Skirmisher": "elite-skirmisher",
  // Cavalry Archer line
  "Cavalry Archer": "cavalry-archer",
  "Heavy Cavalry Archer": "heavy-cavalry-archer",
  // Hand Cannoneer
  "Hand Cannoneer": "hand-cannoneer",
  // Scout line
  "Scout Cavalry": "scout-cavalry",
  "Light Cavalry": "light-cavalry",
  Hussar: "hussar",
  // Knight line
  Knight: "knight",
  Cavalier: "cavalier",
  Paladin: "paladin",
  // Camel line
  Camel: "camel-rider",
  "Heavy Camel": "heavy-camel-rider",
  // Monk
  Monk: "monk",
  // Siege
  "Battering Ram": "battering-ram",
  "Capped Ram": "capped-ram",
  "Siege Ram": "siege-ram",
  Mangonel: "mangonel",
  Onager: "onager",
  "Siege Onager": "siege-onager",
  Scorpion: "scorpion",
  "Heavy Scorpion": "heavy-scorpion",
  "Bombard Cannon": "bombard-cannon",
  Trebuchet: "trebuchet",
  // Navy
  Galley: "galley",
  "War Galley": "war-galley",
  Galleon: "galleon",
  "Fire Ship": "fire-ship",
  "Fast Fire Ship": "fast-fire-ship",
  "Demolition Ship": "demolition-ship",
  "Heavy Demolition Ship": "heavy-demolition-ship",
  "Cannon Galleon": "cannon-galleon",
  "Elite Cannon Galleon": "elite-cannon-galleon",
  "Fishing Ship": "fishing-ship",
  "Transport Ship": "transport-ship",
  // Unique units from aalises
  Berserk: "berserk",
  "Elite Berserk": "elite-berserk",
  Cataphract: "cataphract",
  "Elite Cataphract": "elite-cataphract",
  "Chu Ko Nu": "chu-ko-nu",
  "Elite Chu Ko Nu": "elite-chu-ko-nu",
  Conquistador: "conquistador",
  "Elite Conquistador": "elite-conquistador",
  Huskarl: "huskarl",
  "Elite Huskarl": "elite-huskarl",
  "Jaguar Warrior": "jaguar-warrior",
  "Elite Jaguar Warrior": "elite-jaguar-warrior",
  Janissary: "janissary",
  "Elite Janissary": "elite-janissary",
  Longbowman: "longbowman",
  "Elite Longbowman": "elite-longbowman",
  Mameluke: "mameluke",
  "Elite Mameluke": "elite-mameluke",
  Mangudai: "mangudai",
  "Elite Mangudai": "elite-mangudai",
  "Plumed Archer": "plumed-archer",
  "Elite Plumed Archer": "elite-plumed-archer",
  Samurai: "samurai",
  "Elite Samurai": "elite-samurai",
  Tarkan: "tarkan",
  "Elite Tarkan": "elite-tarkan",
  "Teutonic Knight": "teutonic-knight",
  "Elite Teutonic Knight": "elite-teutonic-knight",
  "Throwing Axeman": "throwing-axeman",
  "Elite Throwing Axeman": "elite-throwing-axeman",
  "War Elephant": "war-elephant",
  "Elite War Elephant": "elite-war-elephant",
  "War Wagon": "war-wagon",
  "Elite War Wagon": "elite-war-wagon",
  "Woad Raider": "woad-raider",
  "Elite Woad Raider": "elite-woad-raider",
  "Karambit Warrior": "karambit-warrior",
  "Elite Karambit Warrior": "elite-karambit-warrior",
  Longboat: "longboat",
  "Elite Longboat": "elite-longboat",
  "Turtle Ship": "turtle-ship",
  "Elite Turtle Ship": "elite-turtle-ship",
};

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
async function run() {
  const unitsCsvText = await readFile(path.join(CACHE_DIR, "units.csv"), "utf8");
  const csvRows = parseCsv(unitsCsvText);

  // Build a map: canonical slug -> csv row (use first occurrence for duplicates)
  const rowBySlug = {};
  for (const row of csvRows) {
    const name = row.name.trim();
    // Map display names to canonical slugs
    const canonSlug = CANONICAL_UNITS[name];
    if (!canonSlug) continue;
    if (!rowBySlug[canonSlug]) {
      rowBySlug[canonSlug] = row;
    } else {
      // For duplicates (Eagle Warrior appears twice — Dark Age and Castle Age)
      // Prefer non-zero cost
      const existing = rowBySlug[canonSlug];
      const existCost = parseCost(existing.cost || "");
      const newCost = parseCost(row.cost || "");
      if (existCost.food === 0 && existCost.gold === 0 && newCost.food > 0) {
        rowBySlug[canonSlug] = row;
      }
    }
  }

  // Load existing unit-stats.json to preserve patch field
  let existingData = { patch: "v100.1.84", units: [] };
  try {
    const existing = await readFile(DATA_OUT, "utf8");
    existingData = JSON.parse(existing);
  } catch (_) {}

  const existingUnitMap = {};
  for (const u of existingData.units || []) {
    existingUnitMap[u.slug] = u;
  }

  const unitEntries = [];
  let warnings = 0;

  // Process canonical units in defined order
  for (const [displayName, slug] of Object.entries(CANONICAL_UNITS)) {
    // Preserve existing stats for longbowman from previous run
    if (slug === "longbowman" && existingUnitMap.longbowman) {
      unitEntries.push(existingUnitMap.longbowman);
      continue;
    }

    const csvRow = rowBySlug[slug];
    let entry;

    if (csvRow) {
      const cost = parseCost(csvRow.cost || "");
      const { range, minRange } = parseRange(csvRow.range || "0");
      const armor = parseArmor(csvRow.armor || "0/0");
      const hp = parseInt(csvRow.hit_points, 10) || 0;
      const attack = parseInt(csvRow.attack, 10) || 0;
      const trainTime = parseInt(csvRow.build_time, 10) || 0;

      // Spread any extra hand-authored fields (e.g. line, lineRank) from the
      // existing JSON before overwriting with fresh CSV stats.
      const existing = existingUnitMap[slug] || {};
      entry = {
        ...existing,
        slug,
        hp,
        attack,
        range,
        minRange,
        cost,
        trainTime,
        armorMelee: armor.melee,
        armorPiercing: armor.pierce,
      };
    } else {
      // No CSV data — emit minimal entry
      console.warn(`[WARN] No CSV data for unit "${slug}" (${displayName})`);
      warnings++;
      entry = {
        slug,
        hp: 0,
        attack: 0,
        range: 0,
        minRange: 0,
        cost: { food: 0, wood: 0, gold: 0, stone: 0 },
        trainTime: 0,
        armorMelee: 0,
        armorPiercing: 0,
      };
    }

    unitEntries.push(entry);
  }

  // Write unit-stats.json
  const output = {
    patch: existingData.patch || "v100.1.84",
    units: unitEntries,
  };
  await writeFile(DATA_OUT, `${JSON.stringify(output, null, 2)}\n`, "utf8");

  console.log(`\nDone.`);
  console.log(`  Units in JSON: ${unitEntries.length}`);
  console.log(`  Written:       ${DATA_OUT}`);
  if (warnings > 0) {
    console.log(`  Warnings (no CSV data): ${warnings}`);
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
