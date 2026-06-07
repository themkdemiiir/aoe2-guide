#!/usr/bin/env node

// scripts/build-icon-map.mjs
//
// Single source of truth for src/data/icon-map.json. Replaces the previous
// build-icon-map + rebuild-icon-map split.
//
// Strategy:
//   1. Load aoe2techtree data/trees/*.json from the pinned upstream SHA.
//   2. Read each tree item's `name`, `use_type`, and `picture_index`.
//   3. Walk public/images/aoe2/{Unit,Building,Tech,Civs}/ for actual PNGs.
//   4. Emit Unit/Tech icon paths from `picture_index`, not `node_id` / game
//      object ID. The image folders are generated from `Picture Index`, so using
//      game IDs can silently show wrong art (e.g. Crossbowman node_id=24,
//      picture_index=18).
//   5. Do not emit Building mappings from this source. The available Building
//      PNGs are command/tech-tree node art rather than reliable building artwork
//      for this guide (e.g. Archery Range maps to a bow icon). Build-order steps
//      render building slugs as text labels instead.
//   6. Civs come from PNG filenames directly (already kebab-case).
//   7. Output is sorted alphabetically per bucket for stable diffs.
//
// Coverage note: if a referenced slug is still missing, either the current tree
// data does not expose it with a matching picture_index, or the image file is
// absent from the pinned asset bundle. Those slugs render a text fallback.
//
// Usage:
//   pnpm icons:map              # build from local caches
//   pnpm icons:map --refresh    # re-fetch tree JSON metadata
//
// Run `pnpm validate:icons` separately to fail builds on missing content slugs.

import { existsSync } from "node:fs";
import { mkdir, readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

const AOE2TT_SHA = process.env.AOE2TT_SHA || "b34082d13c31932d89788ad35af984896cbe050c";
const TREE_ICON_ITEMS_CACHE = ".cache/aoe2-data/tree-icon-items.json";
const TREE_PATHS = ["data/trees", "chronicles/data/trees"];
const ASSETS_DIR = "public/images/aoe2";
const OUT = "src/data/icon-map.json";

function slugify(name) {
  return String(name)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

async function fetchJson(url) {
  const res = await fetch(url, { headers: { "User-Agent": "aoe2-guide-icon-map" } });
  if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
  return res.json();
}

async function listTreeFiles(treePath) {
  const url = `https://api.github.com/repos/SiegeEngineers/aoe2techtree/contents/${treePath}?ref=${AOE2TT_SHA}`;
  const res = await fetch(url, {
    headers: { Accept: "application/vnd.github+json", "User-Agent": "aoe2-guide-icon-map" },
  });
  if (res.status === 404) return [];
  if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
  const entries = await res.json();
  return entries.filter((entry) => entry.type === "file" && entry.name.endsWith(".json"));
}

function collectIconItems(treeJson, sourcePath) {
  const items = [];
  for (const item of [...(treeJson.buildings ?? []), ...(treeJson.units_techs ?? [])]) {
    if (
      !item.name ||
      !item.use_type ||
      item.picture_index === undefined ||
      item.picture_index === null
    ) {
      continue;
    }
    if (item.node_status === "NotAvailable") continue;
    items.push({
      name: item.name,
      useType: item.use_type,
      pictureIndex: item.picture_index,
      sourcePath,
    });
  }
  return items;
}

async function loadTreeIconItems(refresh) {
  if (!refresh && existsSync(TREE_ICON_ITEMS_CACHE)) {
    const cached = JSON.parse(await readFile(TREE_ICON_ITEMS_CACHE, "utf8"));
    if (Array.isArray(cached)) return cached;
    if (cached.sha === AOE2TT_SHA) return cached.items ?? [];
  }

  console.log(`Fetching aoe2techtree tree metadata at ${AOE2TT_SHA}…`);
  const items = [];
  for (const treePath of TREE_PATHS) {
    const files = await listTreeFiles(treePath);
    for (const file of files) {
      const rawUrl = `https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/${AOE2TT_SHA}/${file.path}`;
      const treeJson = await fetchJson(rawUrl);
      items.push(...collectIconItems(treeJson, file.path));
    }
  }

  await mkdir(path.dirname(TREE_ICON_ITEMS_CACHE), { recursive: true });
  await writeFile(
    TREE_ICON_ITEMS_CACHE,
    `${JSON.stringify({ sha: AOE2TT_SHA, items }, null, 2)}\n`,
  );
  return items;
}

function sortObject(obj) {
  return Object.fromEntries(Object.entries(obj).sort(([a], [b]) => a.localeCompare(b)));
}

async function run() {
  const refresh = process.argv.includes("--refresh");

  const treeIconItems = await loadTreeIconItems(refresh);

  // Disk inventory
  const diskUnitFiles = await readdir(path.join(ASSETS_DIR, "Unit"));
  const diskBuildingFiles = await readdir(path.join(ASSETS_DIR, "Building"));
  const diskTechFiles = await readdir(path.join(ASSETS_DIR, "Tech"));
  const diskCivFiles = await readdir(path.join(ASSETS_DIR, "Civs"));

  const diskUnitIds = new Set(diskUnitFiles.map((f) => f.replace(/\.png$/, "")));
  const diskBuildingIds = new Set(diskBuildingFiles.map((f) => f.replace(/\.png$/, "")));
  const diskTechIds = new Set(diskTechFiles.map((f) => f.replace(/\.png$/, "")));

  const units = {};
  const techs = {};
  const buildings = {};

  for (const item of treeIconItems) {
    const pictureIndex = String(item.pictureIndex);
    const slug = slugify(item.name);
    if (!slug) continue;

    if (item.useType === "Unit" && diskUnitIds.has(pictureIndex) && !units[slug]) {
      units[slug] = `/images/aoe2/Unit/${pictureIndex}.png`;
    } else if (item.useType === "Tech" && diskTechIds.has(pictureIndex) && !techs[slug]) {
      techs[slug] = `/images/aoe2/Tech/${pictureIndex}.png`;
    }
  }

  if (units["trebuchet-packed"] && !units.trebuchet) units.trebuchet = units["trebuchet-packed"];

  // Resolve civs (filename-based; PNGs are already kebab-case)
  const civs = {};
  for (const file of diskCivFiles) {
    if (!file.endsWith(".png")) continue;
    const slug = file.replace(/\.png$/, "").toLowerCase();
    civs[slug] = `/images/aoe2/Civs/${file}`;
  }

  // Build final sorted map
  const map = {
    units: sortObject(units),
    techs: sortObject(techs),
    buildings: sortObject(buildings),
    civs: sortObject(civs),
  };

  await mkdir(path.dirname(OUT), { recursive: true });
  await writeFile(OUT, `${JSON.stringify(map, null, 2)}\n`);

  // Coverage report
  console.log(`Wrote ${OUT}`);
  console.log(`  units:     ${Object.keys(map.units).length} (disk: ${diskUnitIds.size})`);
  console.log(`  techs:     ${Object.keys(map.techs).length} (disk: ${diskTechIds.size})`);
  console.log(`  buildings: ${Object.keys(map.buildings).length} (disk: ${diskBuildingIds.size})`);
  console.log(`  civs:      ${Object.keys(map.civs).length}`);

  // Spot-check key base units to catch fingerprint drift
  console.log("\nSpot-check (commonly-referenced units):");
  const checkSlugs = [
    "archer",
    "crossbowman",
    "skirmisher",
    "longbowman",
    "mangonel",
    "hussar",
    "monk",
    "scout-cavalry",
    "knight",
    "cavalier",
    "paladin",
    "camel-rider",
    "pikeman",
    "hand-cannoneer",
    "trebuchet",
    "militia",
    "spearman",
    "halberdier",
    "eagle-warrior",
    "villager",
  ];
  for (const slug of checkSlugs) {
    const p = map.units[slug];
    if (!p) {
      console.log(`  ${slug}: MISSING`);
      continue;
    }
    console.log(`  ${slug}: ${p}`);
  }
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
