#!/usr/bin/env node

// scripts/build-unit-tech-names.mjs
//
// Regenerates the pipeline's units/techs dimension source:
//   src/data/unit-names.json   (game DAT unit_id  -> English display name)
//   src/data/tech-names.json   (game DAT tech_id  -> English display name)
//
// These are the COMMITTED source `pipeline/crates/core/src/{units,techs}.rs` bake in via
// `include_str!` (behind the `refdata` feature) — the same role `src/data/civ-id-map.json` plays
// for civs. `.cache/aoe2-data/` is gitignored, so this script exists to make that cache
// reproducible into a committed file, never hand-typed.
//
// Strategy (source-derived only — never hand-type a name):
//   1. Read `.cache/aoe2-data/data.json`'s `data.Unit`/`data.Tech` maps (run `pnpm sync:game-data`
//      first if missing) for the full id set + each id's `internal_name`.
//   2. Fetch every BASE-GAME civ tech-tree file (`data/trees/*.json` at the pinned
//      aoe2techtree SHA — see `scripts/lib/aoe2-sources.mjs`). Each file's `units_techs[]` entries
//      carry `node_id` (the same id space as `data.Unit`/`data.Tech`) alongside `name`, already
//      resolved to English display text — exactly what `build-icon-map.mjs` reads from the same
//      per-civ tree JSON (there via `item.name` for icon-slug matching; here via `item.node_id`
//      for id matching).
//   3. Deliberately EXCLUDES `chronicles/data/trees` (Return of Rome): those reskin some shared
//      ids with different flavor names (e.g. Militia -> "Levy" for the Macedonians tree) — a
//      real, verified conflict (see the conflict-detection below), not a data error. This pipeline
//      is base-game AoE2 DE only (`replay::config::WATCHED_TECHS`/`OPENER_LINES` ids were verified
///  against base-game names), so RoR reskins must never leak into the dim.
//   4. A `data.Unit`/`data.Tech` id with NO resolvable base-game tree name (there are a handful —
//      e.g. Cartography/Tracking, legacy techs never surfaced on any civ's visible tree; a unit's
//      internal alt-form id like the Ratha's dismounted/melee variant, toggled in-place rather
//      than a separate tree node) is SKIPPED, never guessed. These ids are not reachable by a
//      normal `train`/`research` command, so `match_player_units`/`match_player_techs` will never
//      need them — see the migrations' forward-compat doc comments.
//   5. A `node_id` that resolves to two DIFFERENT names across base-game civ files is a fail-loud
//      build error (would mean this script's base-game-only assumption broke) — never silently
//      picks one.
//
// Usage:
//   pnpm build:unit-tech-names

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import { AOE2TECHTREE_SHA } from "./lib/aoe2-sources.mjs";

const SHA = AOE2TECHTREE_SHA;
const CACHE_DIR = path.resolve(".cache/aoe2-data");
const DATA_JSON = path.join(CACHE_DIR, "data.json");
const TREE_PATH = "data/trees"; // base game only — NOT chronicles/data/trees (Return of Rome reskins)
const UNIT_OUT = path.resolve("src/data/unit-names.json");
const TECH_OUT = path.resolve("src/data/tech-names.json");

async function fetchJson(url) {
  const res = await fetch(url, { headers: { "User-Agent": "aoe2-guide-unit-tech-names" } });
  if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
  return res.json();
}

async function listTreeFiles() {
  const url = `https://api.github.com/repos/SiegeEngineers/aoe2techtree/contents/${TREE_PATH}?ref=${SHA}`;
  const res = await fetch(url, {
    headers: { Accept: "application/vnd.github+json", "User-Agent": "aoe2-guide-unit-tech-names" },
  });
  if (!res.ok) throw new Error(`fetch ${url}: ${res.status}`);
  const entries = await res.json();
  return entries.filter((entry) => entry.type === "file" && entry.name.endsWith(".json"));
}

// Fetches every base-game civ tree file and returns `{ units: Map<id, name>, techs: Map<id,
// name> }`. Throws on a same-id/different-name conflict across civ files (see module doc §5).
async function loadBaseGameNames() {
  const files = await listTreeFiles();
  console.log(`Fetching ${files.length} base-game civ tree files at ${SHA}…`);

  const units = new Map();
  const techs = new Map();
  for (const file of files) {
    const rawUrl = `https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/${SHA}/${file.path}`;
    const treeJson = await fetchJson(rawUrl);
    for (const item of treeJson.units_techs ?? []) {
      if (item.node_status === "NotAvailable") continue;
      if (item.use_type !== "Unit" && item.use_type !== "Tech") continue;
      const table = item.use_type === "Unit" ? units : techs;
      const prev = table.get(item.node_id);
      if (prev !== undefined && prev !== item.name) {
        throw new Error(
          `conflicting base-game name for ${item.use_type} node_id ${item.node_id}: ` +
            `${JSON.stringify(prev)} vs ${JSON.stringify(item.name)} (${file.path})`,
        );
      }
      table.set(item.node_id, item.name);
    }
  }
  return { units, techs };
}

function buildDim(idInfo, names, kind) {
  const map = {};
  const internalNames = {};
  const skipped = [];
  for (const [idStr, info] of Object.entries(idInfo)) {
    const id = Number(idStr);
    const name = names.get(id);
    if (name === undefined) {
      skipped.push({ id, internalName: info.internal_name });
      continue;
    }
    map[idStr] = name;
    if (info.internal_name) internalNames[idStr] = info.internal_name;
  }
  if (skipped.length > 0) {
    console.log(`  ${kind}: skipped ${skipped.length} id(s) with no base-game tree name:`);
    for (const s of skipped) {
      console.log(`    ${s.id} (internal_name: ${s.internalName ?? "?"})`);
    }
  }
  return {
    map,
    internalNames,
    resolved: Object.keys(map).length,
    total: Object.keys(idInfo).length,
  };
}

async function run() {
  const dataJsonText = await readFile(DATA_JSON, "utf8").catch(() => {
    throw new Error(`missing ${DATA_JSON} — run \`pnpm sync:game-data\` first`);
  });
  const dataJson = JSON.parse(dataJsonText);
  const unitIds = dataJson.data?.Unit;
  const techIds = dataJson.data?.Tech;
  if (!unitIds || !techIds) {
    throw new Error(`${DATA_JSON} is missing data.Unit/data.Tech — re-run \`pnpm sync:game-data\``);
  }

  const { units: unitNames, techs: techNames } = await loadBaseGameNames();

  const unitDim = buildDim(unitIds, unitNames, "units");
  const techDim = buildDim(techIds, techNames, "techs");

  const provenanceNote =
    "id -> English display name. Ids + internal_name from aoe2techtree data/data.json's " +
    "data.Unit/data.Tech; names resolved from base-game civ tree files (data/trees/*.json " +
    "units_techs[].name, matched by node_id) at the same pinned SHA — NOT chronicles/data/trees " +
    "(Return of Rome reskins some shared ids with different flavor names). Ids with no resolvable " +
    "base-game tree name are omitted, never guessed — see scripts/build-unit-tech-names.mjs.";

  await mkdir(path.dirname(UNIT_OUT), { recursive: true });
  await writeFile(
    UNIT_OUT,
    `${JSON.stringify(
      {
        provenance: { source: "SiegeEngineers/aoe2techtree", sha: SHA, note: provenanceNote },
        map: unitDim.map,
        internalNames: unitDim.internalNames,
      },
      null,
      2,
    )}\n`,
  );
  await writeFile(
    TECH_OUT,
    `${JSON.stringify(
      {
        provenance: { source: "SiegeEngineers/aoe2techtree", sha: SHA, note: provenanceNote },
        map: techDim.map,
        internalNames: techDim.internalNames,
      },
      null,
      2,
    )}\n`,
  );

  console.log(`Wrote ${UNIT_OUT} (${unitDim.resolved}/${unitDim.total} units)`);
  console.log(`Wrote ${TECH_OUT} (${techDim.resolved}/${techDim.total} techs)`);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
