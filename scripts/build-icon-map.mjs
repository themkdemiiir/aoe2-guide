#!/usr/bin/env node
// scripts/build-icon-map.mjs
// Reads aoc-reference-data dataset 100 + the synced public/images/aoe2 tree,
// and writes src/data/icon-map.json mapping slugs to asset paths.
//
// aoc-reference-data has no declared license; we use it only as a factual
// ID→name lookup table and do not redistribute the raw data.

import { writeFile, readdir, mkdir } from "node:fs/promises";
import path from "node:path";

const REF_URL =
  "https://raw.githubusercontent.com/SiegeEngineers/aoc-reference-data/master/data/datasets/100.json";
const ASSETS_DIR = "public/images/aoe2";
const OUT = "src/data/icon-map.json";

function slugify(name) {
  return String(name)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

async function fetchRef() {
  const res = await fetch(REF_URL);
  if (!res.ok) throw new Error(`fetch ${REF_URL}: ${res.status}`);
  return res.json();
}

/**
 * Maps an ID→name object against the PNGs present in `folder`.
 * `idNameMap` is a plain object where keys are numeric ID strings and
 * values are the display name strings (as returned by aoc-reference-data).
 */
async function mapDir(folder, idNameMap) {
  const files = new Set(await readdir(path.join(ASSETS_DIR, folder)));
  const map = {};
  for (const [id, name] of Object.entries(idNameMap)) {
    if (!name) continue; // skip blank names
    const png = `${id}.png`;
    if (files.has(png)) {
      const slug = slugify(name);
      if (slug) {
        map[slug] = `/images/aoe2/${folder}/${png}`;
      }
    }
  }
  return map;
}

async function run() {
  const ref = await fetchRef();

  // 100.json shape:
  //   .objects       — { "id": "name", ... }
  //   .technologies  — { "id": "name", ... }
  //   .civilizations — { "id": { name, id }, ... }
  //
  // Critical: aoe2techtree's Building/ folder uses ITS OWN numbering, NOT the
  // .objects IDs from aoc-reference-data. So we cannot derive buildings from
  // .objects. Buildings start as {} and are populated by a manual override
  // (or a future second source) — handled outside this script.
  const objects = ref.objects ?? {};
  const technologies = ref.technologies ?? {};

  const map = {
    units: await mapDir("Unit", objects),
    techs: await mapDir("Tech", technologies),
    buildings: {},
    civs: {},
  };

  // Civs are named PNGs (e.g. britons.png), not numeric IDs.
  const civsDir = path.join(ASSETS_DIR, "Civs");
  const civFiles = await readdir(civsDir);
  for (const file of civFiles) {
    if (!file.endsWith(".png")) continue;
    const slug = file.replace(/\.png$/, "").toLowerCase();
    map.civs[slug] = `/images/aoe2/Civs/${file}`;
  }

  await mkdir(path.dirname(OUT), { recursive: true });
  await writeFile(OUT, JSON.stringify(map, null, 2) + "\n");
  console.log(
    "Wrote",
    OUT,
    "| units:",
    Object.keys(map.units).length,
    "| techs:",
    Object.keys(map.techs).length,
    "| buildings:",
    Object.keys(map.buildings).length,
    "| civs:",
    Object.keys(map.civs).length,
  );
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
