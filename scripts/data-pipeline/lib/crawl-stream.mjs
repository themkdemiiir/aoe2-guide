// scripts/data-pipeline/lib/crawl-stream.mjs
//
// One reader for ALL crawl NDJSON sources (snapshot files + sweep shards,
// plain or gzipped), deduped by match_id, with the correctness gates from
// relic-map.mjs applied up front:
//   - ranked RM only (matchtype 6/7/8/9) — EW/DM/quickplay/events excluded
//   - current era only (the API civ-id space re-shuffles at patch boundaries)
//   - optional recency window for "current meta" outputs
//
// Yields raw records ({ match_id, completed, gamemod_id, map_raw, ladder,
// team_size, players[] }); callers split 1v1/team via isRanked1v1/isRankedTeam
// and map civs via relicCivSlug.

import { createReadStream, existsSync, readdirSync } from "node:fs";
import path from "node:path";
import { createInterface } from "node:readline";
import { fileURLToPath } from "node:url";
import { createGunzip } from "node:zlib";
import { inCurrentEra, isRankedRm } from "./relic-map.mjs";

const REPO = path.resolve(path.dirname(fileURLToPath(import.meta.url)), "../../..");

// Window for "current meta" outputs — wide enough for per-map × per-elo cells,
// narrow enough to track the live patch cycle. Labeled in each output's source.
export const CURRENT_WINDOW_DAYS = 75;

// Every crawl source, oldest snapshots last (first-seen wins on dedup, so
// fresher sweep shards take precedence — records are identical either way).
export function defaultCrawlSources(root = REPO) {
  const dirs = [
    "data-cache/relic-stream/1v1",
    "data-cache/relic-stream/team",
    "data-cache/relic-stream/ingested/1v1",
    "data-cache/relic-stream/ingested/team",
  ];
  const files = [];
  for (const d of dirs) {
    const abs = path.join(root, d);
    if (!existsSync(abs)) continue;
    for (const f of readdirSync(abs).sort()) {
      if (f.endsWith(".ndjson") || f.endsWith(".ndjson.gz")) files.push(path.join(abs, f));
    }
  }
  for (const f of [
    "data-cache/relic-patched/matches.ndjson",
    "data-cache/relic-team/matches.ndjson",
    "data-cache/relic/matches.ndjson",
  ]) {
    const abs = path.join(root, f);
    if (existsSync(abs)) files.push(abs);
  }
  if (!files.length) {
    throw new Error(`crawl-stream: no crawl NDJSON sources found under ${root}/data-cache`);
  }
  return files;
}

async function* linesOf(file) {
  const src = createReadStream(file);
  const input = file.endsWith(".gz") ? src.pipe(createGunzip()) : src;
  try {
    yield* createInterface({ input, crlfDelay: Infinity });
  } catch (e) {
    // A shard with a truncated/corrupt tail gz member: readline rejects with
    // the zlib error — keep everything that inflated, move to the next file.
    if (e?.code !== "Z_DATA_ERROR" && e?.code !== "Z_BUF_ERROR") throw e;
    console.warn(`crawl-stream: salvaged truncated gzip tail in ${file}`);
  }
}

export async function* crawlRecords({
  sources = defaultCrawlSources(),
  recentDays = null,
  nowSec = Math.floor(Date.now() / 1000),
} = {}) {
  const cutoff = recentDays == null ? null : nowSec - recentDays * 86400;
  const seen = new Set();
  for (const file of sources) {
    for await (const line of linesOf(file)) {
      if (!line.trim()) continue;
      let rec;
      try {
        rec = JSON.parse(line);
      } catch {
        continue; // truncated tail line of an active shard
      }
      // Gates are deterministic per match, so dedup only needs to remember
      // matches that PASS them (a duplicate of a failing match fails again).
      if (!isRankedRm(rec)) continue;
      if (!inCurrentEra(rec)) continue;
      if (cutoff != null && rec.completed < cutoff) continue;
      if (seen.has(rec.match_id)) continue;
      seen.add(rec.match_id);
      yield rec;
    }
  }
}
