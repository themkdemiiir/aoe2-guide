#!/usr/bin/env node
// Fill NULL games.map from the replay gameplay-file map truth (meta shard map_id
// -> maps.tsv slug). The API mapname is junk (~57% wrong) and ingest writes map
// NULL, so this is where every crawl row gets its REAL map. Idempotent + targeted
// (UPDATE only rows still NULL), so it is safe to run every sweep: newly-parsed
// replays get their map, and a map_id missing from maps.tsv FAILS LOUD (via
// loadReplayMapTruth) rather than silently persisting a NULL map.
//
//   node scripts/data-pipeline/backfill-map-current.mjs [--db <path>] [--duckdb <bin>]
//
// Runs ON THE VM. Needs the DuckDB write lock free (sweep.sh kills duckui first).
import { execFileSync } from "node:child_process";
import { writeFileSync } from "node:fs";
import os from "node:os";
import path from "node:path";
import { loadReplayMapTruth } from "./lib/relic-map.mjs";

const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, arr) => {
    if (a.startsWith("--"))
      acc.push([a.slice(2), arr[i + 1]?.startsWith("--") ? true : (arr[i + 1] ?? true)]);
    return acc;
  }, []),
);
const ROOT = path.resolve(args.root ?? `${os.homedir()}/aoe2-guide`);
const DB = path.resolve(args.db ?? `${ROOT}/data-cache/aoe2.duckdb`);
const DUCKDB = args.duckdb ?? `${os.homedir()}/bin/duckdb`;

// Gameplay-file map truth (fails loud on an empty result or an unmapped map_id).
const truth = await loadReplayMapTruth({
  metaPath: path.join(ROOT, "data-cache/replays/shards/meta.ndjson.gz"),
  mapsTsvPath: path.join(ROOT, "scripts/data-pipeline/replay-rs/data/maps.tsv"),
});
const csv = path.join(os.tmpdir(), "backfill-map-truth.csv");
writeFileSync(csv, [...truth].map(([id, m]) => `${id},${m.canon}`).join("\n"), "utf8");
console.log(`map-truth: ${truth.size} matches from gameplay files`);

const sql = `
.mode box
CREATE OR REPLACE TABLE replay_map_truth AS
  SELECT column0 AS match_id, column1 AS map
  FROM read_csv('${csv}', header=false, columns={'column0':'BIGINT','column1':'VARCHAR'});
SELECT count(*) AS null_map_before FROM games WHERE map IS NULL;
UPDATE games SET map = t.map
  FROM replay_map_truth t
  WHERE games.map IS NULL AND games.match_id = t.match_id;
SELECT count(*) AS null_map_after FROM games WHERE map IS NULL;
SELECT map, count(*) AS rows FROM games WHERE map IS NOT NULL GROUP BY 1 ORDER BY rows DESC LIMIT 6;
`;
const sqlPath = path.join(os.tmpdir(), `backfill-map-${process.pid}.sql`);
writeFileSync(sqlPath, sql, "utf8");
console.log(`backfill-map: UPDATE games.map WHERE NULL → ${DB}`);
process.stdout.write(
  execFileSync(DUCKDB, [DB, "-f", sqlPath], { encoding: "utf8", maxBuffer: 1 << 26 }),
);
