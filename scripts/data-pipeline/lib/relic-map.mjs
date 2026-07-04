// scripts/data-pipeline/lib/relic-map.mjs
//
// Single ingestion-boundary source for interpreting Relic API crawl records.
// The API's civilization_id is Relic's OWN id space (see
// src/data/relic-civ-id-map.json provenance) — NEVER map it through
// src/data/civ-id-map.json (that's the game/replay id space).
//
// Three gates every crawl consumer must apply, in order:
//   1. isRankedRm(rec)      — matchtype 6 = 1v1 RM, 7/8/9 = team RM; everything
//                             else (EW, DM, quickplay, events, co-op) is excluded.
//   2. inCurrentEra(rec)    — the API id space re-shuffles at patch boundaries;
//                             the committed map is only valid from ERA_START on.
//   3. relicCivSlug(civId)  — fails LOUD on an unknown id (a new DLC id must
//                             break the build, not silently vanish).

import { readFileSync } from "node:fs";
import path from "node:path";
import { fileURLToPath } from "node:url";

const HERE = path.dirname(fileURLToPath(import.meta.url));
const REPO = path.resolve(HERE, "../../..");

const relicDoc = JSON.parse(
  readFileSync(path.join(REPO, "src/data/relic-civ-id-map.json"), "utf8"),
);

export const RELIC_CIV_MAP = relicDoc.map;

// Matches completed before this are in an OLDER API id space. Single source:
// the map file's own provenance.validFrom — when a DLC re-shuffles the table
// and the map is re-derived, the era boundary moves with it automatically.
const validFrom = relicDoc.provenance?.validFrom;
if (!/^\d{4}-\d{2}-\d{2}$/.test(validFrom ?? "")) {
  throw new Error(
    "relic-map: relic-civ-id-map.json provenance.validFrom missing/invalid — refusing to guess the era boundary",
  );
}
export const ERA_START = Date.parse(`${validFrom}T00:00:00Z`) / 1000;

// Ranked RM matchtype_ids (record field `ladder`), per getAvailableLeaderboards:
// SOLO_RM_RANKED -> 6, TEAM_RM_RANKED -> 7/8/9.
export const RM_1V1_LADDER = 6;
export const RM_TEAM_LADDERS = new Set([7, 8, 9]);

export function isRanked1v1(rec) {
  return rec.ladder === RM_1V1_LADDER;
}
export function isRankedTeam(rec) {
  return RM_TEAM_LADDERS.has(rec.ladder);
}
export function isRankedRm(rec) {
  return isRanked1v1(rec) || isRankedTeam(rec);
}

export function inCurrentEra(rec) {
  return typeof rec.completed === "number" && rec.completed >= ERA_START;
}

// SQL face of the same map: (re)creates the DuckDB relic_civmap table from the
// one committed source of truth. Used by ingest-stream + rebuild migrations so
// the literal never forks.
export function relicCivmapSql() {
  const values = Object.entries(RELIC_CIV_MAP)
    .map(([id, slug]) => `(${Number(id)}, '${slug}')`)
    .join(", ");
  return `CREATE OR REPLACE TABLE relic_civmap(civ_id INTEGER, civ_slug VARCHAR);\nINSERT INTO relic_civmap VALUES ${values};`;
}

// Returns null for a MISSING civ id (malformed player row — callers' guide-civ
// filter drops it, matching pre-fix behavior). Throws ONLY for an unknown
// NUMERIC id: that means the API id space itself moved and every stat would be
// mislabeled — the regen must stop, not guess.
export function relicCivSlug(civId) {
  if (civId == null) return null;
  const slug = RELIC_CIV_MAP[String(civId)];
  if (!slug) {
    throw new Error(
      `relic-map: unknown Relic civilization_id ${civId} — a new DLC likely shifted the API id space. ` +
        `Re-derive src/data/relic-civ-id-map.json (races table + replay-join) before regenerating stats.`,
    );
  }
  return slug;
}

// ---- Replay-derived map truth ---------------------------------------------
// The API's per-match `mapname`/map_raw is WRONG for ~57% of matches (measured
// against 133k replay-parsed matches, 2026-06). The ONLY per-match map truth
// for crawl records is the replay pipeline's meta shard. Returns
// Map<match_id, { canon, name }> (canon = lowercase-alnum slug, name = the
// maps.tsv display name). Matches absent from it have UNKNOWN map.
//
// The shard is multi-member gzip and its tail member can be corrupt/mid-append
// (the pipeline appends per run) — STREAM it and salvage every line that
// inflates; gunzipSync would throw the whole file away.
import { createReadStream } from "node:fs";
import { createInterface } from "node:readline";
import { createGunzip } from "node:zlib";
import { canonMap } from "./buckets.mjs";

// map-meta key index: canon(map key) -> the higher-volume spelling's key.
// aoestats naming variants (golden_pit / goldenpit) collapse to one page key;
// every generator that keys map slices MUST resolve through this same index.
export function canonToKeyIndex(mapMeta) {
  const idx = {};
  for (const [k, v] of Object.entries(mapMeta.maps)) {
    const c = canonMap(k);
    const g = (v.games?.["1v1"] ?? 0) + (v.games?.team ?? 0);
    if (!idx[c] || g > idx[c].g) idx[c] = { key: k, g };
  }
  return idx;
}

export async function loadReplayMapTruth({
  metaPath = path.join(REPO, "data-cache/replays/shards/meta.ndjson.gz"),
  mapsTsvPath = path.join(REPO, "scripts/data-pipeline/replay-rs/data/maps.tsv"),
} = {}) {
  const idToCanon = new Map();
  for (const line of readFileSync(mapsTsvPath, "utf8").split("\n")) {
    if (!line || line.startsWith("#")) continue;
    const [id, name] = line.split("\t");
    if (id && name) idToCanon.set(Number(id), { canon: canonMap(name), name });
  }
  const src = createReadStream(metaPath);
  const input = metaPath.endsWith(".gz") ? src.pipe(createGunzip()) : src;
  const truth = new Map();
  try {
    for await (const line of createInterface({ input, crlfDelay: Infinity })) {
      if (!line) continue;
      let rec;
      try {
        rec = JSON.parse(line);
      } catch {
        continue; // truncated tail line
      }
      const entry = idToCanon.get(rec.map_id);
      if (entry) truth.set(rec.match_id, entry);
    }
  } catch (e) {
    // Corrupt/mid-append tail member: readline rejects with the zlib error —
    // keep every line that inflated before it.
    if (e?.code !== "Z_DATA_ERROR" && e?.code !== "Z_BUF_ERROR") throw e;
  }
  if (!truth.size) {
    throw new Error(
      `relic-map: replay map truth is EMPTY (${metaPath}) — refusing to regen map slices blind`,
    );
  }
  return truth;
}
