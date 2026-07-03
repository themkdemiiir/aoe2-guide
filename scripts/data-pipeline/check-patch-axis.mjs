#!/usr/bin/env node
// scripts/data-pipeline/check-patch-axis.mjs
//
// Continuous auditor for the gamemod→build map (src/data/patch-index.json):
// joins replay-header builds (ground truth) against crawl gamemod_ids and
// verifies every gamemod maps to exactly the indexed build. Catches both a
// NEW game update (gamemod missing from the index — prints the exact entry
// to add) and the silent case a mapping can't see on its own: a hotfix that
// changed the build WITHOUT bumping gamemod_id.
//
// Runs on the box that holds data-cache (the VM); sweep.sh calls it after the
// replay-truth refresh. Exit 1 on any violation.

import { spawnSync } from "node:child_process";
import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import path from "node:path";
import { duck } from "./lib/duck.mjs";

const patchIndex = JSON.parse(readFileSync(path.resolve("src/data/patch-index.json"), "utf8"));
const expected = patchIndex.gamemodToBuild;
const ignore = new Set(patchIndex.ignoreGamemods ?? []);

const META = path.resolve("data-cache/replays/shards/meta.ndjson.gz");
if (!existsSync(META)) {
  console.log("check-patch-axis: no replay meta shards — nothing to audit");
  process.exit(0);
}
// The shard is appended-to (multi-member gz) and its live tail can be mid-write;
// DuckDB's gzip reader rejects that, so inflate with zcat (keeps every complete
// member, exits non-zero on the tail — fine) and query the plain file instead.
const metaPlain = "/tmp/check-patch-axis-meta.ndjson";
spawnSync("bash", ["-c", `zcat '${META}' > '${metaPlain}' 2>/dev/null; true`]);
if (!existsSync(metaPlain) || statSync(metaPlain).size === 0) {
  console.error("[FATAL] check-patch-axis: could not inflate any replay meta rows");
  process.exit(1);
}
// only globs with files — an empty-dir glob crashes read_json
const globs = [
  "relic-stream/1v1",
  "relic-stream/team",
  "relic-stream/ingested/1v1",
  "relic-stream/ingested/team",
]
  .map((d) => path.resolve("data-cache", d))
  .filter((d) => existsSync(d) && readdirSync(d).some((f) => f.includes(".ndjson")))
  .map((d) => `'${d}/*.ndjson*'`)
  .join(", ");
if (!globs) {
  console.log("check-patch-axis: no crawl shards — nothing to audit");
  process.exit(0);
}

const MIN_N = 20; // pairs smaller than this are noise (corrupt tails, misjoins), not signal
const rows = duck(`WITH r AS (
    SELECT match_id, build FROM read_json('${metaPlain}', format='newline_delimited', ignore_errors=true)
  ), c AS (
    SELECT DISTINCT match_id, gamemod_id
    FROM read_json([${globs}], format='newline_delimited', union_by_name=true, ignore_errors=true)
    WHERE gamemod_id IS NOT NULL
  )
  SELECT c.gamemod_id, r.build, count(*) AS n
  FROM c JOIN r USING (match_id)
  GROUP BY 1, 2 HAVING count(*) >= ${MIN_N} ORDER BY 1, 2`);

let bad = 0;
for (const { gamemod_id, build, n } of rows) {
  if (ignore.has(Number(gamemod_id))) {
    console.log(`  ignored gamemod ${gamemod_id}: replay build ${build} (${n} matches)`);
    continue;
  }
  const want = expected[String(gamemod_id)];
  if (want == null) {
    bad++;
    console.error(
      `[FATAL] NEW gamemod ${gamemod_id} → replay-proven build ${build} (${n} matches).`,
    );
    console.error(
      `  Add to src/data/patch-index.json: gamemodToBuild "${gamemod_id}": ${build} + a builds[] entry for Update ${build}`,
    );
  } else if (String(want) !== String(build)) {
    bad++;
    console.error(
      `[FATAL] gamemod ${gamemod_id}: index says build ${want}, but ${n} replay headers say ${build} — hotfix without a gamemod bump?`,
    );
  } else {
    console.log(`  ok gamemod ${gamemod_id} → build ${build} (${n} matches)`);
  }
}
console.log(
  `check-patch-axis: ${rows.length} (gamemod × build) pairs audited · ${bad} violation(s)`,
);
process.exit(bad ? 1 : 0);
