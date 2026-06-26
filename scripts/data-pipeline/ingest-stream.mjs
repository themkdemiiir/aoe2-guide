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
// Runs ON THE VM (needs the duckdb binary + the .duckdb file unlocked — sweep.sh
// stops the UI first). Usage:
//   node scripts/data-pipeline/ingest-stream.mjs
//   node scripts/data-pipeline/ingest-stream.mjs --db <path> --stream-dir <dir> --duckdb <bin>

import { readdirSync, mkdirSync, renameSync, writeFileSync, existsSync } from "node:fs";
import { execFileSync } from "node:child_process";
import path from "node:path";
import os from "node:os";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, arr) => {
    if (a.startsWith("--")) acc.push([a.slice(2), arr[i + 1]?.startsWith("--") ? true : arr[i + 1] ?? true]);
    return acc;
  }, []),
);
const DB = path.resolve(args.db ?? `${os.homedir()}/aoe2-guide/data-cache/aoe2.duckdb`);
const STREAM_DIR = path.resolve(args["stream-dir"] ?? `${os.homedir()}/aoe2-guide/data-cache/relic-stream`);
const DUCKDB = args.duckdb ?? `${os.homedir()}/bin/duckdb`;

const ELO_CASE =
  "CASE WHEN f.p.rating IS NULL THEN 'unknown' WHEN f.p.rating<1000 THEN '<1000' WHEN f.p.rating<1200 THEN '1000-1199' WHEN f.p.rating<1400 THEN '1200-1399' WHEN f.p.rating<1650 THEN '1400-1649' WHEN f.p.rating<1800 THEN '1650-1799' WHEN f.p.rating<2000 THEN '1800-1999' WHEN f.p.rating<2200 THEN '2000-2199' WHEN f.p.rating<2500 THEN '2200-2499' ELSE '2500+' END";

// shards directly under <stream>/<ladder>/ (NOT under ingested/)
function shardsFor(ladder) {
  const dir = path.join(STREAM_DIR, ladder);
  if (!existsSync(dir)) return [];
  return readdirSync(dir).filter((f) => f.endsWith(".ndjson")).map((f) => path.join(dir, f));
}
const sqlList = (files) => `[${files.map((f) => `'${f}'`).join(", ")}]`;

const ladders = [
  { name: "1v1", files: shardsFor("1v1"), teamExpr: "CAST(NULL AS BIGINT)" },
  { name: "team", files: shardsFor("team"), teamExpr: "team_size" },
].filter((l) => l.files.length);

if (!ladders.length) { console.log("ingest-stream: no new shards — nothing to ingest."); process.exit(0); }

const rawBranches = ladders
  .map((l) => `SELECT '${l.name}' AS ladder, match_id, completed, gamemod_id, map_raw, ${l.teamExpr} AS team_size, players
       FROM read_json(${sqlList(l.files)}, format='newline_delimited')`)
  .join("\n  UNION ALL BY NAME\n  ");

// QUALIFY drops intra-batch dups (same match across two sweep shards); the NOT IN
// drops matches already in games (from aoestats or an earlier crawl).
const sql = `
.mode box
SELECT count(*) AS games_before FROM games;
INSERT INTO games
WITH raw AS (
  ${rawBranches}
),
fresh AS (
  SELECT * FROM raw
  WHERE match_id NOT IN (SELECT match_id FROM games)
  QUALIFY row_number() OVER (PARTITION BY match_id ORDER BY completed) = 1
),
flat AS (SELECT ladder, match_id, completed, gamemod_id, map_raw, team_size, UNNEST(players) AS p FROM fresh)
SELECT
  'crawl' AS source,
  f.ladder,
  f.match_id,
  CAST(to_timestamp(f.completed) AS TIMESTAMP) AS played_at,
  strftime(to_timestamp(f.completed), '%Y-%m') AS month,
  regexp_replace(lower(regexp_replace(f.map_raw, '\\.[a-z0-9]+$', '')), '[^a-z0-9]', '', 'g') AS map,
  COALESCE(f.team_size, 2) AS team_size,
  f.p.profile_id AS profile_id,
  cm.civ_slug AS civ,
  f.p.rating AS rating,
  ${ELO_CASE} AS elo_bucket,
  f.p.won AS won,
  CAST(NULL AS VARCHAR) AS opening,
  CAST(NULL AS DOUBLE) AS feudal_t,
  CAST(NULL AS DOUBLE) AS castle_t,
  CAST(NULL AS DOUBLE) AS imperial_t,
  CAST(f.gamemod_id AS VARCHAR) AS patch
FROM flat f LEFT JOIN civmap cm ON cm.civ_id = f.p.civ_id;
SELECT count(*) AS games_after FROM games;
`;

const sqlPath = path.join(os.tmpdir(), `ingest-stream-${process.pid}.sql`);
writeFileSync(sqlPath, sql, "utf8");

console.log(`ingest-stream: ingesting ${ladders.map((l) => `${l.name}:${l.files.length}`).join(", ")} shard(s) into ${DB}`);
const out = execFileSync(DUCKDB, [DB, "-f", sqlPath], { encoding: "utf8" });
process.stdout.write(out);

// archive ingested shards GZIPPED — they're redundant with DuckDB, so keep them
// only as a compact backup (~10x smaller). Safe to purge relic-stream/ingested/
// anytime the disk gets tight; the data already lives in the games table.
for (const l of ladders) {
  const dest = path.join(STREAM_DIR, "ingested", l.name);
  mkdirSync(dest, { recursive: true });
  for (const f of l.files) {
    execFileSync("gzip", ["-f", f]);                                  // f -> f.gz in place
    renameSync(`${f}.gz`, path.join(dest, `${path.basename(f)}.gz`));
  }
}
console.log(`ingest-stream: archived ${ladders.reduce((n, l) => n + l.files.length, 0)} shard(s) gzipped → relic-stream/ingested/`);
