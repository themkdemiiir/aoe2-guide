#!/usr/bin/env node
// scripts/data-pipeline/ingest-stream.mjs
//
// Ingest fresh crawl shards (from stream-relic.mjs) into the DuckDB `games`
// fact table, deduping by match_id against everything already there — this is
// the scaling fix: the crawler stays stateless and DuckDB is the single source
// of truth for "have we seen this match." Idempotent: re-running ingests nothing
// new. Ingested shards are archived under relic-stream/ingested/<ladder>/.
//
// Match-level dedup: a match has many player-rows; we insert ALL rows for
// matches whose match_id is absent from `games`, dropping intra-batch dups too.
// Crawl rows have no replay-derived fields, so opening/age timings land NULL and
// source='crawl', patch=gamemod_id — matching how the merged build wrote crawl rows.
//
// Correctness gates (2026-07): only ranked-RM matchtypes (6=1v1, 7/8/9=team —
// the record's own `ladder` field; the shard DIRECTORY only reflects player
// count and also contains EW/DM/quickplay); only the current Relic civ-id-space
// era; civ mapped via relic_civmap (regenerated from the committed
// src/data/relic-civ-id-map.json every run) with a FAIL-LOUD error() on any
// unmapped id; map is NULL — the API mapname is wrong for most matches, replay
// parsing is the only map truth (backfilled separately).
//
// Runs ON THE VM (needs the duckdb binary + the .duckdb file unlocked — sweep.sh
// stops the UI first). Usage:
//   node scripts/data-pipeline/ingest-stream.mjs
//   node scripts/data-pipeline/ingest-stream.mjs --db <path> --stream-dir <dir> --duckdb <bin>

import { execFileSync } from "node:child_process";
import { existsSync, mkdirSync, readFileSync, readdirSync, unlinkSync, writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { parseArgs } from "node:util";
import { gzipSync } from "node:zlib";
import { eloCaseSql } from "./lib/buckets.mjs";
import { ERA_START, relicCivmapSql } from "./lib/relic-map.mjs";

const { values: args } = parseArgs({
  options: {
    db: { type: "string" },
    "stream-dir": { type: "string" },
    duckdb: { type: "string" },
  },
  strict: true,
});
const DB = path.resolve(args.db ?? `${os.homedir()}/aoe2-guide/data-cache/aoe2.duckdb`);
const STREAM_DIR = path.resolve(args["stream-dir"] ?? `${os.homedir()}/aoe2-guide/data-cache/relic-stream`);
const DUCKDB = args.duckdb ?? `${os.homedir()}/bin/duckdb`;

const ELO_CASE = eloCaseSql("f.p.rating");

// shards directly under <stream>/<ladder>/ (NOT under ingested/)
function shardsFor(ladder) {
  const dir = path.join(STREAM_DIR, ladder);
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter((f) => f.endsWith(".ndjson")).map((f) => path.join(dir, f));
}
const sqlList = (files) => `[${files.map((f) => `'${f}'`).join(", ")}]`;

const byDir = { "1v1": shardsFor("1v1"), team: shardsFor("team") };
const files = [...byDir["1v1"], ...byDir.team];
if (!files.length) { console.log("ingest-stream: no new shards — nothing to ingest."); process.exit(0); }

// QUALIFY drops intra-batch dups (same match across two sweep shards); the NOT IN
// drops matches already in games (from aoestats or an earlier crawl). The ladder
// comes from the record's matchtype, NOT the shard directory. The exclusion
// counts are printed so gate drops are never silent in the cron log.
const sql = `
.mode box
${relicCivmapSql()}
SELECT count(*) AS games_before FROM games;
CREATE TEMP TABLE raw_batch AS
  SELECT match_id, completed, gamemod_id, ladder AS matchtype, team_size, players
  FROM read_json(${sqlList(files)}, format='newline_delimited', union_by_name=true);
SELECT count(DISTINCT match_id) FILTER (matchtype IS NULL) AS dropped_null_matchtype,
       count(DISTINCT match_id) FILTER (matchtype IS NOT NULL AND matchtype NOT IN (6,7,8,9)) AS dropped_non_rm,
       count(DISTINCT match_id) FILTER (matchtype IN (6,7,8,9) AND completed < ${ERA_START}) AS dropped_pre_era
FROM raw_batch;
INSERT INTO games
WITH raw AS (
  SELECT * FROM raw_batch
),
fresh AS (
  SELECT * FROM raw
  WHERE matchtype IN (6, 7, 8, 9)          -- ranked RM only (6=1v1, 7/8/9=team)
    AND completed >= ${ERA_START}           -- current Relic civ-id-space era only
    AND match_id NOT IN (SELECT match_id FROM games)
  QUALIFY row_number() OVER (PARTITION BY match_id ORDER BY completed) = 1
),
flat AS (SELECT matchtype, match_id, completed, gamemod_id, team_size, UNNEST(players) AS p FROM fresh)
SELECT
  'crawl' AS source,
  CASE WHEN f.matchtype = 6 THEN '1v1' ELSE 'team' END AS ladder,
  f.match_id,
  CAST(to_timestamp(f.completed) AS TIMESTAMP) AS played_at,
  strftime(to_timestamp(f.completed), '%Y-%m') AS month,
  CAST(NULL AS VARCHAR) AS map,            -- API mapname is untrustworthy; replay backfill only
  COALESCE(f.team_size, 2) AS team_size,
  f.p.profile_id AS profile_id,
  CASE WHEN cm.civ_slug IS NULL
       THEN error('ingest-stream: unmapped Relic civ id ' || COALESCE(CAST(f.p.civ_id AS VARCHAR), 'NULL') || ' — a DLC likely shifted the API id space; re-derive src/data/relic-civ-id-map.json')
       ELSE cm.civ_slug END AS civ,
  f.p.rating AS rating,
  ${ELO_CASE} AS elo_bucket,
  f.p.won AS won,
  CAST(NULL AS VARCHAR) AS opening,
  CAST(NULL AS DOUBLE) AS feudal_t,
  CAST(NULL AS DOUBLE) AS castle_t,
  CAST(NULL AS DOUBLE) AS imperial_t,
  CAST(f.gamemod_id AS VARCHAR) AS patch
FROM flat f LEFT JOIN relic_civmap cm ON cm.civ_id = f.p.civ_id
-- a player row with NO civilization_id is a malformed API record: drop the ROW
-- (matches lib/relic-map.mjs relicCivSlug(null) returning null). The error()
-- above then fires only for unknown NUMERIC ids — a real id-space shift —
-- instead of one poison record wedging the cron ingest forever.
WHERE f.p.civ_id IS NOT NULL;
SELECT count(*) AS games_after FROM games;
`;

const sqlPath = path.join(os.tmpdir(), `ingest-stream-${process.pid}.sql`);
writeFileSync(sqlPath, sql, "utf8");

console.log(`ingest-stream: ingesting ${Object.entries(byDir).map(([d, fs]) => `${d}:${fs.length}`).join(", ")} shard(s) into ${DB}`);
const out = execFileSync(DUCKDB, [DB, "-f", sqlPath], { encoding: "utf8" });
process.stdout.write(out);

// archive ingested shards GZIPPED — the raw records (incl. non-RM matchtypes we
// don't ingest) stay recoverable here; safe to purge if the disk gets tight.
for (const [dir, fs] of Object.entries(byDir)) {
  if (!fs.length) continue;
  const dest = path.join(STREAM_DIR, "ingested", dir);
  mkdirSync(dest, { recursive: true });
  for (const f of fs) {
    writeFileSync(path.join(dest, `${path.basename(f)}.gz`), gzipSync(readFileSync(f)));
    unlinkSync(f);
  }
}
console.log(`ingest-stream: archived ${files.length} shard(s) gzipped → relic-stream/ingested/`);
