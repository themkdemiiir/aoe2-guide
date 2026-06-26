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

const API = "https://aoe-api.worldsedgelink.com/community/leaderboard";
const TITLE = "age2";
const LEADERBOARD_1V1_RM = 3; // SOLO_RM_RANKED
const LEADERBOARD_TEAM_RM = 4; // TEAM_RM_RANKED

// --- CLI args -----------------------------------------------------------------
const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, arr) => {
    if (a.startsWith("--")) acc.push([a.slice(2), arr[i + 1]?.startsWith("--") ? true : arr[i + 1] ?? true]);
    return acc;
  }, []),
);
const TEAM = !!args.team;
const LADDER = TEAM ? "team" : "1v1";
const LEADERBOARD = Number(args.leaderboard ?? (TEAM ? LEADERBOARD_TEAM_RM : LEADERBOARD_1V1_RM));
const MAX_PLAYERS = args.players != null ? Number(args.players) : Infinity; // default: the whole ladder
const OUT_DIR = path.resolve(args.out ?? "data-cache/relic-stream");
const THROTTLE_MS = Number(args.throttle ?? 120);
const CONCURRENCY = Number(args.concurrency ?? 12);
const PAGE = 200;
// 1v1 = exactly 2 members; team = 4/6/8 (2v2/3v3/4v4)
const keepBySize = (n) => (TEAM ? n >= 4 && n % 2 === 0 : n === 2);

const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

async function getJson(url, tries = 4) {
  for (let attempt = 1; attempt <= tries; attempt++) {
    try {
      const res = await fetch(url, { headers: { "user-agent": "aoe2guide-stats/1.0 (self-collect)" } });
      if (res.status === 429 || res.status >= 500) throw new Error(`HTTP ${res.status}`);
      if (!res.ok) throw new Error(`HTTP ${res.status}`);
      return await res.json();
    } catch (e) {
      if (attempt === tries) throw e;
      await sleep(THROTTLE_MS * attempt * 3);
    }
  }
}

// Enumerate the FULL ladder (paginate to rankTotal, capped only by --players).
async function fetchProfileIds(limit) {
  const ids = new Set();
  for (let start = 1; ids.size < limit; start += PAGE) {
    const url = `${API}/getLeaderBoard2?title=${TITLE}&leaderboard_id=${LEADERBOARD}&start=${start}&count=${PAGE}&sortBy=1`;
    let data;
    try { data = await getJson(url); } catch (e) { process.stderr.write(`\n  [warn] leaderboard page ${start}: ${e.message}\n`); break; }
    const groups = data?.statGroups ?? [];
    if (!groups.length) break;
    for (const g of groups) for (const m of g.members ?? []) if (m.profile_id != null) ids.add(m.profile_id);
    if (process.stderr.isTTY) process.stderr.write(`\r  leaderboard: ${ids.size} profiles seeded`);
    await sleep(THROTTLE_MS);
  }
  process.stderr.write("\n");
  return [...ids].slice(0, limit === Infinity ? ids.size : limit);
}

function normalizeMatches(history) {
  const out = [];
  for (const m of history?.matchHistoryStats ?? []) {
    if (m.description !== "AUTOMATCH") continue; // ranked matchmaking only
    const members = m.matchhistorymember ?? [];
    if (!keepBySize(members.length)) continue;
    out.push({
      match_id: m.id,
      completed: m.completiontime,
      gamemod_id: m.gamemod_id ?? null, // patch proxy
      map_raw: m.mapname ?? null,
      ladder: m.matchtype_id ?? null,
      team_size: members.length,
      players: members.map((p) => ({
        profile_id: p.profile_id,
        civ_id: p.civilization_id,
        rating: p.newrating ?? p.oldrating ?? null,
        won: p.outcome === 1,
      })),
    });
  }
  return out;
}

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
  const profileIds = await fetchProfileIds(MAX_PLAYERS);
  console.log(`Seeded ${profileIds.length} profiles. Crawling recent histories (${CONCURRENCY}x)…`);

  const seen = new Set(); // per-sweep dedup ONLY — never persisted; DuckDB dedups across sweeps
  let processed = 0, written = 0, qi = 0;

  async function handle(pid) {
    const url = `${API}/getRecentMatchHistory?title=${TITLE}&profile_ids=%5B${pid}%5D`;
    let rows = [];
    try { rows = normalizeMatches(await getJson(url)); } catch (e) {
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
