#!/usr/bin/env node
// scripts/data-pipeline/collect-relic.mjs
//
// Self-collect RANKED 1v1 RM match data from the live World's Edge Link API
// (the game's own community endpoint — the durable source after aoe2.net's API
// and aoestats' raw dumps went dead/frozen). No auth, no key. Reverse-engineered,
// no SLA — so we self-throttle, cache, and checkpoint.
//
// Pipeline: leaderboard(1v1 RM) -> profile_ids -> getRecentMatchHistory per player
//           -> keep AUTOMATCH 1v1 -> dedupe by match_id -> NDJSON rows.
//
// Output: data-cache/relic/matches-<snapshot>.ndjson  (one JSON row per match)
//   { match_id, completed, map_raw, ladder, players: [{profile_id, civ_id, rating, won}] }
// civ_id stays NUMERIC here — name mapping happens in aggregate (with a VERIFIED
// civ-id map, never a guessed one). Collection must be correct independent of names.
//
// Usage:
//   node scripts/data-pipeline/collect-relic.mjs --players 2000 --out data-cache/relic
//   (run on the Proxmox box; safe to re-run — it resumes from the checkpoint)

import { mkdir, readFile, writeFile, appendFile } from "node:fs/promises";
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
const MAX_PLAYERS = Number(args.players ?? 2000); // top-N ranked profiles to seed from
const OUT_DIR = path.resolve(args.out ?? "data-cache/relic");
const THROTTLE_MS = Number(args.throttle ?? 120); // per-worker delay between requests
const CONCURRENCY = Number(args.concurrency ?? 12); // parallel in-flight requests
const TEAM = !!args.team; // --team: seed the Team RM ladder + keep team-sized AUTOMATCH matches
const LEADERBOARD = Number(args.leaderboard ?? (TEAM ? LEADERBOARD_TEAM_RM : LEADERBOARD_1V1_RM));
// Keep predicate per mode: 1v1 = exactly 2 members; team = 4/6/8 (2v2/3v3/4v4)
const keepBySize = keepBySizeFor(TEAM);
const MODE = TEAM ? "team RM" : "1v1 RM";

// shared client (lib/relic-api.mjs) — the crawler pair uses ONE implementation
const { getJson, fetchProfileIds } = makeClient({ throttleMs: THROTTLE_MS });

// --- main ---------------------------------------------------------------------
async function run() {
  await mkdir(OUT_DIR, { recursive: true });
  const ckPath = path.join(OUT_DIR, "checkpoint.json");
  const outPath = path.join(OUT_DIR, "matches.ndjson");

  // resume: which profile_ids + match_ids we already have
  let ck = { doneProfiles: [], seenMatches: [] };
  try { ck = JSON.parse(await readFile(ckPath, "utf8")); } catch (_) {}
  const doneProfiles = new Set(ck.doneProfiles);
  const seenMatches = new Set(ck.seenMatches);

  console.log(`Seeding up to ${MAX_PLAYERS} ranked profiles from the ${MODE} ladder (leaderboard_id=${LEADERBOARD})…`);
  const profileIds = await fetchProfileIds({ leaderboard: LEADERBOARD, limit: MAX_PLAYERS });
  console.log(`Got ${profileIds.length} profiles. Crawling match history (resume: ${doneProfiles.size} already done)…`);

  let newRows = 0;
  const todo = profileIds.filter((pid) => !doneProfiles.has(pid));
  let processed = profileIds.length - todo.length;
  let qi = 0;
  let ckBusy = false;
  const saveCk = async () => {
    if (ckBusy) return; // a write is already in flight; the next checkpoint will catch up
    ckBusy = true;
    try { await writeFile(ckPath, JSON.stringify({ doneProfiles: [...doneProfiles], seenMatches: [...seenMatches] }), "utf8"); }
    finally { ckBusy = false; }
  };

  // CONCURRENCY workers pull from a shared queue. Match-ids are claimed in
  // seenMatches BEFORE append so two workers never write the same match.
  async function handle(pid) {
    const url = `${API}/getRecentMatchHistory?title=${TITLE}&profile_ids=%5B${pid}%5D`;
    let rows = [];
    try { rows = normalizeMatches(await getJson(url), keepBySize); } catch (e) {
      process.stderr.write(`\n  [warn] profile ${pid}: ${e.message}\n`);
    }
    const fresh = rows.filter((r) => !seenMatches.has(r.match_id));
    for (const r of fresh) seenMatches.add(r.match_id);
    if (fresh.length) {
      await appendFile(outPath, `${fresh.map((r) => JSON.stringify(r)).join("\n")}\n`, "utf8");
      newRows += fresh.length;
    }
    doneProfiles.add(pid);
    processed++;
    if (processed % 150 === 0) {
      await saveCk();
      process.stderr.write(`\r  crawled ${processed}/${profileIds.length} · ${seenMatches.size} matches · ${CONCURRENCY}x parallel`);
    }
    if (THROTTLE_MS) await sleep(THROTTLE_MS);
  }
  async function worker() { while (qi < todo.length) await handle(todo[qi++]); }
  await Promise.all(Array.from({ length: CONCURRENCY }, () => worker()));
  await saveCk();
  process.stderr.write("\n");
  console.log(`Done. +${newRows} new matches this run · ${seenMatches.size} unique ranked ${MODE} matches total → ${outPath}`);
  console.log(`Next: build a VERIFIED civ-id map, then aggregate (node scripts/data-pipeline/aggregate-civmeta.mjs).`);
}

run().catch((e) => { console.error(e); process.exit(1); });
