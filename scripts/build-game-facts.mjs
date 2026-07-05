#!/usr/bin/env node
// scripts/build-game-facts.mjs
//
// Emits src/data/game-facts.json — per-unit AGE, training building, cost, and
// attack bonus — derived ENTIRELY from primary-source data, never hand-written.
//
// Source (run `pnpm sync:game-data` first to cache it):
//   aalises/age-of-empires-II-api  data/units.csv   (BSD-3-Clause)
//     columns: name, description, expansion, age, created_in, cost,
//              build_time, ..., attack, armor, attack_bonus, armor_bonus, ...
//
// These facts are what content (build orders, counters) is verified against by
// scripts/verify-build-facts.mjs. Provenance is recorded in docs/SOURCES.md.
//
// Refresh: `pnpm sync:game-data` (bumps nothing; uses the pinned SHA) then
// `pnpm build:facts`.

import { readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { AALISES_SHA } from "./lib/aoe2-sources.mjs";

const CACHE = path.resolve(".cache/aoe2-data");
const OUT = path.resolve("src/data/game-facts.json");

// Display name (as in aalises units.csv) -> our content slug.
// Mirrors CANONICAL_UNITS in build-units.mjs (kept in sync deliberately).
const NAME_TO_SLUG = {
  Militia: "militia",
  "Man-at-Arms": "man-at-arms",
  "Long Swordsman": "long-swordsman",
  "Two-Handed Swordsman": "two-handed-swordsman",
  Champion: "champion",
  Spearman: "spearman",
  Pikeman: "pikeman",
  Halberdier: "halberdier",
  "Eagle Warrior": "eagle-warrior",
  "Elite Eagle Warrior": "elite-eagle-warrior",
  "Eagle Scout": "eagle-scout",
  Archer: "archer",
  Crossbowman: "crossbowman",
  Arbalest: "arbalester",
  Skirmisher: "skirmisher",
  "Elite Skirmisher": "elite-skirmisher",
  "Cavalry Archer": "cavalry-archer",
  "Heavy Cavalry Archer": "heavy-cavalry-archer",
  "Hand Cannoneer": "hand-cannoneer",
  "Scout Cavalry": "scout-cavalry",
  "Light Cavalry": "light-cavalry",
  Hussar: "hussar",
  Knight: "knight",
  Cavalier: "cavalier",
  Paladin: "paladin",
  Camel: "camel-rider",
  "Heavy Camel": "heavy-camel-rider",
  Monk: "monk",
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
  Galley: "galley",
  "War Galley": "war-galley",
  Galleon: "galleon",
  "Fire Ship": "fire-ship",
  "Fast Fire Ship": "fast-fire-ship",
  "Fire Galley": "fire-galley",
  "Demolition Ship": "demolition-ship",
  "Heavy Demolition Ship": "heavy-demolition-ship",
  "Demolition Raft": "demolition-raft",
  "Cannon Galleon": "cannon-galleon",
  "Elite Cannon Galleon": "elite-cannon-galleon",
  "Fishing Ship": "fishing-ship",
  "Transport Ship": "transport-ship",
};

function splitCsvLine(line) {
  const out = [];
  let cur = "";
  let q = false;
  for (const ch of line) {
    if (ch === '"') q = !q;
    else if (ch === "," && !q) {
      out.push(cur);
      cur = "";
    } else cur += ch;
  }
  out.push(cur);
  return out;
}

function parseCsv(text) {
  const lines = text.split("\n").filter((l) => l.trim());
  const headers = lines[0].split(",").map((h) => h.trim());
  return lines.slice(1).map((line) => {
    const cols = splitCsvLine(line);
    const row = {};
    headers.forEach((h, i) => {
      row[h] = (cols[i] || "").trim();
    });
    return row;
  });
}

function parseCost(costStr) {
  const cost = {};
  const inner = (costStr || "").replace(/[{}"]/g, "");
  for (const part of inner.split(";")) {
    const m = part.trim().match(/(\w+)\s*:\s*(\d+)/);
    if (m) cost[m[1].toLowerCase()] = Number(m[2]);
  }
  return cost;
}

async function run() {
  const csv = await readFile(path.join(CACHE, "units.csv"), "utf8").catch(() => {
    throw new Error("Missing .cache/aoe2-data/units.csv — run `pnpm sync:game-data` first.");
  });
  const rows = parseCsv(csv);

  const units = {};
  let mapped = 0;
  for (const row of rows) {
    const slug = NAME_TO_SLUG[row.name];
    if (!slug) continue;
    const age = (row.age || "").trim().toLowerCase(); // dark | feudal | castle | imperial
    if (!["dark", "feudal", "castle", "imperial"].includes(age)) continue;
    units[slug] = {
      name: row.name,
      age,
      building: (row.created_in || "").trim(),
      cost: parseCost(row.cost),
      attackBonus: (row.attack_bonus || "").replace(/[{}"]/g, "").trim() || null,
    };
    mapped++;
  }

  const out = {
    _generated: "scripts/build-game-facts.mjs — DO NOT EDIT BY HAND",
    _source: {
      repo: "aalises/age-of-empires-II-api",
      file: "data/units.csv",
      sha: AALISES_SHA,
      license: "BSD-3-Clause",
      url: `https://github.com/aalises/age-of-empires-II-api/blob/${AALISES_SHA}/data/units.csv`,
    },
    ageRank: { dark: 1, feudal: 2, castle: 3, imperial: 4 },
    units,
  };
  await writeFile(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
  console.log(
    `Wrote ${OUT} — ${mapped} units (age/building/cost/attackBonus) from aalises units.csv.`,
  );
}

run().catch((e) => {
  console.error(e.message);
  process.exit(1);
});
