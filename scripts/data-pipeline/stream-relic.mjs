#!/usr/bin/env node
// scripts/data-pipeline/stream-relic.mjs
//
// CONTINUOUS forward capture of RANKED matches from the live World's Edge Link
// API. Unlike collect-relic.mjs (a one-shot top-N seed with an ever-growing
// checkpoint), this sweeps the FULL active ladder every run and writes a fresh
// dated shard — dedup is delegated to DuckDB (ingest-stream.sql), so this stays
// stateless and scales to millions of games.
//
// The API has no global match index: the only path is
//   leaderboard(full) -> every profile_id -> getRecentMatchHistory -> recent games.
// So "all ranked games" = the union of every active player's recent history,
// captured often enough that games don't age out. Run this on a schedule
// (sweep.sh via cron, every ~3h) to capture the live ~25-28k ranked games/day.
//
// One run = one ladder. Output: <out>/<ladder>/<stamp>.ndjson (one JSON/match).
//   { match_id, completed, gamemod_id, map_raw, ladder, team_size,
//     players: [{ profile_id, civ_id, rating, won }] }
//
// Usage:
//   node scripts/data-pipeline/stream-relic.mjs                 # full 1v1 sweep
//   node scripts/data-pipeline/stream-relic.mjs --team          # full team sweep
//   node scripts/data-pipeline/stream-relic.mjs --players 500   # smoke test (cap)

import { mkdir, appendFile } from "node:fs/promises";
import path from "node:path";
import { parseArgs } from "node:util";
import {
  API, TITLE, LEADERBOARD_1V1_RM, LEADERBOARD_TEAM_RM,
  keepBySize as keepBySizeFor, makeClient, normalizeMatches, sleep,
} from "./lib/relic-api.mjs";

// --- CLI args (strict: a typo'd flag fails loud) --------------------------------
const { values: args } = parseArgs({
  options: {
    team: { type: "boolean" },
    leaderboard: { type: "string" },
    players: { type: "string" },
    out: { type: "string" },
    throttle: { type: "string" },
    concurrency: { type: "string" },
  },
  strict: true,
});
const TEAM = !!args.team;
const LADDER = TEAM ? "team" : "1v1";
const LEADERBOARD = Number(args.leaderboard ?? (TEAM ? LEADERBOARD_TEAM_RM : LEADERBOARD_1V1_RM));
const MAX_PLAYERS = args.players != null ? Number(args.players) : Infinity; // default: the whole ladder
const OUT_DIR = path.resolve(args.out ?? "data-cache/relic-stream");
const THROTTLE_MS = Number(args.throttle ?? 120);
const CONCURRENCY = Number(args.concurrency ?? 12);
// shared client (lib/relic-api.mjs) — the crawler pair uses ONE implementation
const keepBySize = keepBySizeFor(TEAM);
const { getJson, fetchProfileIds } = makeClient({ throttleMs: THROTTLE_MS });

// stamp like 2026-06-25T1030 (local) — one shard per sweep
function sweepStamp() {
  const d = new Date();
  const p = (n) => String(n).padStart(2, "0");
  return `${d.getFullYear()}-${p(d.getMonth() + 1)}-${p(d.getDate())}T${p(d.getHours())}${p(d.getMinutes())}`;
}

async function run() {
  const ladderDir = path.join(OUT_DIR, LADDER);
  await mkdir(ladderDir, { recursive: true });
  const shard = path.join(ladderDir, `${sweepStamp()}.ndjson`);

  const cap = MAX_PLAYERS === Infinity ? "full ladder" : `${MAX_PLAYERS} profiles (capped)`;
  console.log(`Sweep ${LADDER} (leaderboard_id=${LEADERBOARD}) — ${cap} → ${path.relative(process.cwd(), shard)}`);
  const profileIds = await fetchProfileIds({ leaderboard: LEADERBOARD, limit: MAX_PLAYERS });
  console.log(`Seeded ${profileIds.length} profiles. Crawling recent histories (${CONCURRENCY}x)…`);

  const seen = new Set(); // per-sweep dedup ONLY — never persisted; DuckDB dedups across sweeps
  let processed = 0, written = 0, qi = 0;

  async function handle(pid) {
    const url = `${API}/getRecentMatchHistory?title=${TITLE}&profile_ids=%5B${pid}%5D`;
    let rows = [];
    try { rows = normalizeMatches(await getJson(url), keepBySize); } catch (e) {
      process.stderr.write(`\n  [warn] profile ${pid}: ${e.message}\n`);
    }
    const fresh = rows.filter((r) => !seen.has(r.match_id));
    for (const r of fresh) seen.add(r.match_id);
    if (fresh.length) {
      await appendFile(shard, `${fresh.map((r) => JSON.stringify(r)).join("\n")}\n`, "utf8");
      written += fresh.length;
    }
    processed++;
    if (processed % 200 === 0 && process.stderr.isTTY) process.stderr.write(`\r  ${processed}/${profileIds.length} profiles · ${written} matches this sweep`);
    if (THROTTLE_MS) await sleep(THROTTLE_MS);
  }
  async function worker() { while (qi < profileIds.length) await handle(profileIds[qi++]); }
  await Promise.all(Array.from({ length: CONCURRENCY }, () => worker()));

  process.stderr.write("\n");
  console.log(`Done. ${written} ${LADDER} matches written to ${path.relative(process.cwd(), shard)} (DuckDB dedups on ingest).`);
}

run().catch((e) => { console.error(e); process.exit(1); });
