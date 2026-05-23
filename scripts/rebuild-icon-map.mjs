#!/usr/bin/env node
// scripts/rebuild-icon-map.mjs
//
// Rebuilds src/data/icon-map.json using:
//   1. Stat-based unit matching against .cache/aoe2-data/data.json (aoe2techtree source)
//   2. Verified building ID mapping (Building/<id>.png where files exist on disk)
//   3. Existing tech + civ entries preserved from current map
//   4. All existing unique-unit entries preserved; missing base units filled in
//
// Usage: node scripts/rebuild-icon-map.mjs
//        (or: pnpm icons:rebuild)

import { readFile, writeFile, readdir } from "node:fs/promises";
import { existsSync } from "node:fs";
import path from "node:path";

const DATA_JSON   = ".cache/aoe2-data/data.json";
const CURRENT_MAP = "src/data/icon-map.json";
const OUT         = "src/data/icon-map.json";
const ASSETS_DIR  = "public/images/aoe2";

// ---------------------------------------------------------------------------
// Stat fingerprints for canonical AOE2 DE base units.
// HP / Attack / Range / Cost are used for scoring; trust stats over name.
// ---------------------------------------------------------------------------
const UNIT_FINGERPRINTS = {
  militia:                { HP: 40,  Attack: 4,   Range: 0,  Cost: { Food: 60 } },
  "man-at-arms":          { HP: 45,  Attack: 6,   Range: 0,  Cost: { Food: 60, Gold: 20 } },
  "long-swordsman":       { HP: 60,  Attack: 9,   Range: 0,  Cost: { Food: 60, Gold: 20 } },
  "two-handed-swordsman": { HP: 60,  Attack: 11,  Range: 0,  Cost: { Food: 60, Gold: 20 } },
  champion:               { HP: 70,  Attack: 13,  Range: 0,  Cost: { Food: 60, Gold: 20 } },
  spearman:               { HP: 45,  Attack: 3,   Range: 0,  Cost: { Food: 35, Wood: 25 } },
  pikeman:                { HP: 55,  Attack: 4,   Range: 0,  Cost: { Food: 35, Wood: 25 } },
  halberdier:             { HP: 60,  Attack: 6,   Range: 0,  Cost: { Food: 35, Wood: 25 } },
  "eagle-scout":          { HP: 50,  Attack: 4,   Range: 0,  Cost: { Food: 20, Gold: 50 } },
  "eagle-warrior":        { HP: 55,  Attack: 7,   Range: 0,  Cost: { Food: 20, Gold: 50 } },
  "elite-eagle-warrior":  { HP: 60,  Attack: 9,   Range: 0,  Cost: { Food: 20, Gold: 50 } },
  archer:                 { HP: 30,  Attack: 4,   Range: 5,  Cost: { Wood: 25, Gold: 45 } },
  crossbowman:            { HP: 35,  Attack: 5,   Range: 5,  Cost: { Wood: 25, Gold: 45 } },
  arbalester:             { HP: 40,  Attack: 6,   Range: 5,  Cost: { Wood: 25, Gold: 45 } },
  skirmisher:             { HP: 30,  Attack: 2,   Range: 4,  Cost: { Wood: 25, Food: 35 } },
  "elite-skirmisher":     { HP: 35,  Attack: 3,   Range: 5,  Cost: { Wood: 25, Food: 35 } },
  "scout-cavalry":        { HP: 45,  Attack: 3,   Range: 0,  Cost: { Food: 80 } },
  "light-cavalry":        { HP: 60,  Attack: 7,   Range: 0,  Cost: { Food: 80 } },
  hussar:                 { HP: 75,  Attack: 7,   Range: 0,  Cost: { Food: 80 } },
  knight:                 { HP: 100, Attack: 10,  Range: 0,  Cost: { Food: 60, Gold: 75 } },
  cavalier:               { HP: 120, Attack: 12,  Range: 0,  Cost: { Food: 60, Gold: 75 } },
  paladin:                { HP: 160, Attack: 14,  Range: 0,  Cost: { Food: 60, Gold: 75 } },
  "camel-rider":          { HP: 100, Attack: 6,   Range: 0,  Cost: { Food: 55, Gold: 60 } },
  "heavy-camel-rider":    { HP: 120, Attack: 7,   Range: 0,  Cost: { Food: 55, Gold: 60 } },
  "imperial-camel-rider": { HP: 140, Attack: 9,   Range: 0,  Cost: { Food: 55, Gold: 60 } },
  "cavalry-archer":       { HP: 50,  Attack: 6,   Range: 4,  Cost: { Wood: 40, Gold: 60 } },
  "heavy-cavalry-archer": { HP: 60,  Attack: 7,   Range: 4,  Cost: { Wood: 40, Gold: 60 } },
  "hand-cannoneer":       { HP: 35,  Attack: 17,  Range: 7,  Cost: { Food: 45, Gold: 50 } },
  monk:                   { HP: 30,  Attack: 0,   Range: 9,  Cost: { Gold: 100 } },
  missionary:             { HP: 30,  Attack: 0,   Range: 9,  Cost: { Gold: 100 } },
  trebuchet:              { HP: 150, Attack: 200, Range: 16, Cost: { Wood: 200, Gold: 200 } },
  "bombard-cannon":       { HP: 80,  Attack: 40,  Range: 12, Cost: { Wood: 225, Gold: 225 } },
  "battering-ram":        { HP: 175, Attack: 2,   Range: 0,  Cost: { Wood: 160, Gold: 75 } },
  "capped-ram":           { HP: 200, Attack: 3,   Range: 0,  Cost: { Wood: 160, Gold: 75 } },
  "siege-ram":            { HP: 270, Attack: 4,   Range: 0,  Cost: { Wood: 160, Gold: 75 } },
  mangonel:               { HP: 50,  Attack: 40,  Range: 7,  Cost: { Wood: 160, Gold: 135 } },
  onager:                 { HP: 60,  Attack: 50,  Range: 8,  Cost: { Wood: 160, Gold: 135 } },
  "siege-onager":         { HP: 70,  Attack: 75,  Range: 8,  Cost: { Wood: 160, Gold: 135 } },
  scorpion:               { HP: 40,  Attack: 12,  Range: 7,  Cost: { Wood: 75, Gold: 75 } },
  "heavy-scorpion":       { HP: 50,  Attack: 16,  Range: 7,  Cost: { Wood: 75, Gold: 75 } },
  galley:                 { HP: 100, Attack: 6,   Range: 5,  Cost: { Wood: 90, Gold: 30 } },
  "war-galley":           { HP: 135, Attack: 7,   Range: 6,  Cost: { Wood: 90, Gold: 30 } },
  galleon:                { HP: 165, Attack: 8,   Range: 7,  Cost: { Wood: 90, Gold: 30 } },
  "fire-galley":          { HP: 100, Attack: 1,   Range: 2,  Cost: { Wood: 75, Gold: 45 } },
  "fire-ship":            { HP: 120, Attack: 2,   Range: 3,  Cost: { Wood: 75, Gold: 45 } },
  "fast-fire-ship":       { HP: 140, Attack: 3,   Range: 3,  Cost: { Wood: 75, Gold: 45 } },
  "demolition-raft":      { HP: 45,  Attack: 90,  Range: 0,  Cost: { Wood: 70, Gold: 50 } },
  "demolition-ship":      { HP: 60,  Attack: 110, Range: 0,  Cost: { Wood: 70, Gold: 50 } },
  "heavy-demolition-ship":{ HP: 70,  Attack: 140, Range: 0,  Cost: { Wood: 70, Gold: 50 } },
  "cannon-galleon":       { HP: 120, Attack: 35,  Range: 13, Cost: { Wood: 200, Gold: 150 } },
  "elite-cannon-galleon": { HP: 150, Attack: 45,  Range: 15, Cost: { Wood: 200, Gold: 150 } },
  "fishing-ship":         { HP: 60,  Attack: 0,   Range: 0,  Cost: { Wood: 75 } },
  "transport-ship":       { HP: 150, Attack: 0,   Range: 0,  Cost: { Wood: 125 } },
  villager:               { HP: 25,  Attack: 3,   Range: 0,  Cost: { Food: 50 } },
};

// ---------------------------------------------------------------------------
// Verified building slug → Building/<id>.png (IDs from data.Building that
// also exist on disk in public/images/aoe2/Building/).
// Only the 6 IDs verified to exist are included here.
// All other buildings use Unit/ paths from the existing icon-map.
// ---------------------------------------------------------------------------
const VERIFIED_BUILDING_IDS = {
  "town-center":   109,
  "market":        84,
  "archery-range": 87,
  "stable":        101,
  "blacksmith":    103,
  "monastery":     104,
};

// ---------------------------------------------------------------------------
// Helper: score how well a data.Unit entry matches a fingerprint
// ---------------------------------------------------------------------------
function scoreUnit(unit, fp) {
  let score = 0;

  // HP match (10 pts exact, 5 pts within 10, 2 pts within 25)
  const hpDiff = Math.abs((unit.HP ?? 0) - fp.HP);
  if (hpDiff === 0)       score += 10;
  else if (hpDiff <= 10)  score += 5;
  else if (hpDiff <= 25)  score += 2;

  // Attack match (10 pts exact, 5 pts within 2, 2 pts within 5)
  const atkDiff = Math.abs((unit.Attack ?? 0) - fp.Attack);
  if (atkDiff === 0)      score += 10;
  else if (atkDiff <= 2)  score += 5;
  else if (atkDiff <= 5)  score += 2;

  // Range match (10 pts exact, 5 pts within 1)
  const rngDiff = Math.abs(Math.round(unit.Range ?? 0) - fp.Range);
  if (rngDiff === 0)      score += 10;
  else if (rngDiff <= 1)  score += 5;

  // Cost match (3 pts per matching resource, 1 pt if within 15)
  const actCost = unit.Cost ?? {};
  const expCost = fp.Cost ?? {};
  for (const res of ["Food", "Wood", "Gold", "Stone"]) {
    const e = expCost[res] ?? 0;
    const a = actCost[res] ?? 0;
    if (e === a)                        score += 3;
    else if (e === 0 && a === 0)        score += 0;
    else if (Math.abs(e - a) <= 15)     score += 1;
  }

  return score;
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
async function run() {
  // Load data sources
  const dataJson    = JSON.parse(await readFile(DATA_JSON, "utf8"));
  const currentMap  = JSON.parse(await readFile(CURRENT_MAP, "utf8"));

  const dataUnits   = dataJson.data?.Unit   ?? {};
  // (data.Building and data.Tech are available but we use manual verified IDs)

  // Enumerate disk files for each image category
  const diskUnit     = new Set(await readdir(path.join(ASSETS_DIR, "Unit")));
  const diskBuilding = new Set(await readdir(path.join(ASSETS_DIR, "Building")));
  const diskTech     = new Set(await readdir(path.join(ASSETS_DIR, "Tech")));

  function unitExists(id)     { return diskUnit.has(`${id}.png`); }
  function buildingExists(id) { return diskBuilding.has(`${id}.png`); }
  function techExists(id)     { return diskTech.has(`${id}.png`); }

  // -------------------------------------------------------------------------
  // Step 1: Start with existing unit entries (preserves unique units, etc.)
  // -------------------------------------------------------------------------
  const units = { ...currentMap.units };

  // -------------------------------------------------------------------------
  // Step 2: Stat-based matching for all base unit fingerprints
  //         Only replaces / adds if we find a better or new match.
  // -------------------------------------------------------------------------
  const statResults = {}; // slug -> { id, score, unit }
  for (const [slug, fp] of Object.entries(UNIT_FINGERPRINTS)) {
    let bestId = null, bestScore = -1, bestUnit = null;
    for (const [uid, unit] of Object.entries(dataUnits)) {
      if (!unitExists(uid)) continue;
      const s = scoreUnit(unit, fp);
      if (s > bestScore) {
        bestScore = s;
        bestId    = uid;
        bestUnit  = unit;
      }
    }
    if (bestId) {
      statResults[slug] = { id: bestId, score: bestScore, unit: bestUnit };
    }
  }

  // Merge stat results into units map.
  // Policy: if a slug is already in the map AND the current path exists on disk, keep it.
  // If missing, add the stat-matched entry.
  for (const [slug, result] of Object.entries(statResults)) {
    const existingPath = units[slug];
    if (existingPath) {
      // Verify existing path file actually exists on disk
      const existingId = existingPath.match(/\/(\d+)\.png$/)?.[1];
      if (existingId && unitExists(existingId)) {
        // Existing is valid, keep it
        continue;
      }
    }
    // Add or replace with stat-matched result
    units[slug] = `/images/aoe2/Unit/${result.id}.png`;
  }

  // -------------------------------------------------------------------------
  // Step 3: Buildings — use Building/<id>.png for verified IDs, else use
  //         Unit/ paths from existing map (which already work for barracks, etc.)
  // -------------------------------------------------------------------------
  const buildings = { ...currentMap.buildings };
  for (const [slug, id] of Object.entries(VERIFIED_BUILDING_IDS)) {
    if (buildingExists(id)) {
      buildings[slug] = `/images/aoe2/Building/${id}.png`;
    }
  }

  // -------------------------------------------------------------------------
  // Step 4: Techs — preserve existing (already correct from current map)
  // -------------------------------------------------------------------------
  const techs = { ...currentMap.techs };
  // Add additional tech entries if present in data.Tech with disk files
  // (current map already has the main ones; this is a no-op for now)

  // -------------------------------------------------------------------------
  // Step 5: Civs — preserve exactly (named PNGs, already correct)
  // -------------------------------------------------------------------------
  const civs = { ...currentMap.civs };

  // -------------------------------------------------------------------------
  // Output
  // -------------------------------------------------------------------------
  const newMap = { units, techs, buildings, civs };

  await writeFile(OUT, JSON.stringify(newMap, null, 2) + "\n");

  // Summary
  console.log(`Wrote ${OUT}`);
  console.log(`  units:     ${Object.keys(units).length}`);
  console.log(`  techs:     ${Object.keys(techs).length}`);
  console.log(`  buildings: ${Object.keys(buildings).length}`);
  console.log(`  civs:      ${Object.keys(civs).length}`);

  // Spot-check the key slugs
  console.log("\nSpot-check (base units):");
  const checkSlugs = [
    "archer", "skirmisher", "mangonel", "hussar", "monk",
    "scout-cavalry", "knight", "cavalier", "paladin",
    "camel-rider", "pikeman", "hand-cannoneer", "trebuchet", "onager",
    "militia", "spearman", "halberdier", "eagle-scout", "eagle-warrior",
  ];
  for (const slug of checkSlugs) {
    const path = units[slug];
    if (!path) {
      console.log(`  ${slug}: MISSING`);
      continue;
    }
    const id = path.match(/\/(\d+)\.png$/)?.[1];
    if (id && dataUnits[id]) {
      const u = dataUnits[id];
      console.log(
        `  ${slug}: ${path} [HP=${u.HP} Atk=${u.Attack} Rng=${u.Range}]`
      );
    } else {
      console.log(`  ${slug}: ${path} [no data entry for this id]`);
    }
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
