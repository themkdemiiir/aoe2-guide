#!/usr/bin/env node
// scripts/migrations/2026-07-02-rebuild-crawl-rows.mjs  (one-off, runs ON THE VM)
//
// The Relic civ-id bug rebuild: every crawl-sourced row in the merged DuckDB
// `games` table carried a WRONG civ slug (API civilization_id interpreted in
// the game id space), non-RM matchtypes (EW/DM/quickplay/events) mislabeled as
// ranked RM, and a `map` canonicalized from the untrustworthy API mapname.
//
// This deletes ALL source='crawl' rows and re-inserts them from the raw crawl
// NDJSON archives with the corrected rules:
//   - ranked RM matchtypes only (record `ladder` field: 6=1v1, 7/8/9=team)
//   - current id-space era only (completed >= relic-civ-id-map.json validFrom)
//   - civ via relic_civmap (from src/data/relic-civ-id-map.json), FAIL-LOUD
//   - map from replay-parsed truth where a replay was parsed; NULL otherwise
//   - aoestats rows keep priority (match_id NOT IN games after the delete)
//
//   node scripts/migrations/2026-07-02-rebuild-crawl-rows.mjs [--db <path>] [--duckdb <bin>]

import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { eloCaseSql } from "../data-pipeline/lib/buckets.mjs";
import { defaultCrawlSources } from "../data-pipeline/lib/crawl-stream.mjs";
import { ERA_START, loadReplayMapTruth, relicCivmapSql } from "../data-pipeline/lib/relic-map.mjs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, arr) => {
    if (a.startsWith("--")) acc.push([a.slice(2), arr[i + 1]?.startsWith("--") ? true : arr[i + 1] ?? true]);
    return acc;
  }, []),
);
const ROOT = path.resolve(args.root ?? `${os.homedir()}/aoe2-guide`);
const DB = path.resolve(args.db ?? `${ROOT}/data-cache/aoe2.duckdb`);
const DUCKDB = args.duckdb ?? `${os.homedir()}/bin/duckdb`;

// Same source discovery as every other crawl consumer (lib/crawl-stream.mjs).
const sources = defaultCrawlSources(ROOT);

// Replay-parsed map truth via the same loader every generator uses (throws on
// an EMPTY result — a rebuild must never silently produce all-NULL maps).
const mapTruth = await loadReplayMapTruth({
  metaPath: path.join(ROOT, "data-cache/replays/shards/meta.ndjson.gz"),
  mapsTsvPath: path.join(ROOT, "scripts/data-pipeline/replay-rs/data/maps.tsv"),
});
const truthCsv = path.join(os.tmpdir(), "rebuild-crawl-truth.csv");
writeFileSync(truthCsv, [...mapTruth].map(([id, m]) => `${id},${m.canon}`).join("\n"), "utf8");

const sqlList = (files) => `[${files.map((f) => `'${f}'`).join(", ")}]`;

const sql = `
.mode box
SET temp_directory='/tmp/duckspill-rebuild';
${relicCivmapSql()}

-- replay-parsed per-match map truth (canonical slug), kept as a table so future
-- backfills can refresh it
CREATE OR REPLACE TABLE replay_map_truth AS
SELECT column0 AS match_id, column1 AS map
FROM read_csv('${truthCsv}', header=false, columns={'column0':'BIGINT','column1':'VARCHAR'});

SELECT count(*) AS crawl_rows_before FROM games WHERE source='crawl';
DELETE FROM games WHERE source='crawl';

INSERT INTO games
WITH raw AS (
  SELECT match_id, completed, gamemod_id, ladder AS matchtype, team_size, players
  FROM read_json(${sqlList(sources)}, format='newline_delimited', union_by_name=true)
),
fresh AS (
  SELECT * FROM raw
  WHERE matchtype IN (6, 7, 8, 9)
    AND completed >= ${ERA_START}
    AND match_id NOT IN (SELECT match_id FROM games)   -- aoestats keeps priority
  QUALIFY row_number() OVER (PARTITION BY match_id ORDER BY completed) = 1
),
flat AS (SELECT matchtype, match_id, completed, gamemod_id, team_size, UNNEST(players) AS p FROM fresh)
SELECT
  'crawl' AS source,
  CASE WHEN f.matchtype = 6 THEN '1v1' ELSE 'team' END AS ladder,
  f.match_id,
  CAST(to_timestamp(f.completed) AS TIMESTAMP) AS played_at,
  strftime(to_timestamp(f.completed), '%Y-%m') AS month,
  mt.map AS map,                       -- replay truth or NULL (API mapname is junk)
  COALESCE(f.team_size, 2) AS team_size,
  f.p.profile_id AS profile_id,
  CASE WHEN cm.civ_slug IS NULL
       THEN error('rebuild: unmapped Relic civ id ' || COALESCE(CAST(f.p.civ_id AS VARCHAR), 'NULL'))
       ELSE cm.civ_slug END AS civ,
  f.p.rating AS rating,
  ${eloCaseSql("f.p.rating")} AS elo_bucket,
  f.p.won AS won,
  CAST(NULL AS VARCHAR) AS opening,
  CAST(NULL AS DOUBLE) AS feudal_t,
  CAST(NULL AS DOUBLE) AS castle_t,
  CAST(NULL AS DOUBLE) AS imperial_t,
  CAST(f.gamemod_id AS VARCHAR) AS patch
FROM flat f
LEFT JOIN relic_civmap cm ON cm.civ_id = f.p.civ_id
LEFT JOIN replay_map_truth mt ON mt.match_id = f.match_id
WHERE f.p.civ_id IS NOT NULL;  -- malformed rows (no civ id) drop, matching relicCivSlug(null)

SELECT count(*) AS crawl_rows_after, count(DISTINCT match_id) AS crawl_matches,
       sum(CASE WHEN map IS NOT NULL THEN 1 ELSE 0 END) AS rows_with_replay_map
FROM games WHERE source='crawl';
SELECT civ, count(*) AS n FROM games WHERE source='crawl' GROUP BY 1 ORDER BY n DESC LIMIT 10;
`;

const sqlPath = path.join(os.tmpdir(), `rebuild-crawl-${process.pid}.sql`);
writeFileSync(sqlPath, sql, "utf8");
console.log(`rebuild-crawl-rows: ${sources.length} sources → ${DB}`);
const out = execFileSync(DUCKDB, [DB, "-f", sqlPath], { encoding: "utf8", maxBuffer: 1 << 26 });
process.stdout.write(out);
console.log("NOTE: workbench summary tables (civ_winrate/map_civ/civ_opening/civ_timing) are now stale — rebuild if needed.");
