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

const API = "https://aoe-api.worldsedgelink.com/community/leaderboard";
const TITLE = "age2";
const LEADERBOARD_1V1_RM = 3; // verified: id 3 = 1v1 Random Map ranked ladder

// --- CLI args (tiny parser) ---------------------------------------------------
const args = Object.fromEntries(
  process.argv.slice(2).reduce((acc, a, i, arr) => {
    if (a.startsWith("--")) acc.push([a.slice(2), arr[i + 1]?.startsWith("--") ? true : arr[i + 1] ?? true]);
    return acc;
  }, []),
);
const MAX_PLAYERS = Number(args.players ?? 2000); // top-N ranked profiles to seed from
const OUT_DIR = path.resolve(args.out ?? "data-cache/relic");
const THROTTLE_MS = Number(args.throttle ?? 300); // ~3 req/s — polite, no SLA
const PAGE = 200; // leaderboard page size

// --- helpers ------------------------------------------------------------------
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
      await sleep(THROTTLE_MS * attempt * 3); // back off
    }
  }
}

// --- 1) seed: top-N ranked profile_ids from the 1v1 RM leaderboard ------------
async function fetchProfileIds(limit) {
  const ids = new Set();
  for (let start = 1; ids.size < limit; start += PAGE) {
    const url = `${API}/getLeaderBoard2?title=${TITLE}&leaderboard_id=${LEADERBOARD_1V1_RM}&start=${start}&count=${PAGE}&sortBy=1`;
    const data = await getJson(url);
    const groups = data?.statGroups ?? [];
    if (!groups.length) break;
    for (const g of groups) for (const m of g.members ?? []) if (m.profile_id != null) ids.add(m.profile_id);
    process.stderr.write(`\r  leaderboard: ${ids.size} profiles seeded`);
    await sleep(THROTTLE_MS);
  }
  process.stderr.write("\n");
  return [...ids].slice(0, limit);
}

// --- 2) per-player recent match history -> normalized 1v1 RM rows -------------
function normalizeMatches(history) {
  const out = [];
  for (const m of history?.matchHistoryStats ?? []) {
    if (m.description !== "AUTOMATCH") continue; // ranked matchmaking only (drops customs/lobbies)
    const members = m.matchhistorymember ?? [];
    if (members.length !== 2) continue; // 1v1 only
    out.push({
      match_id: m.id,
      completed: m.completiontime,
      map_raw: m.mapname ?? null,
      ladder: m.matchtype_id ?? null,
      players: members.map((p) => ({
        profile_id: p.profile_id,
        civ_id: p.civilization_id, // NUMERIC — mapped to slug later via a verified map
        rating: p.newrating ?? p.oldrating ?? null,
        won: p.outcome === 1,
      })),
    });
  }
  return out;
}

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

  console.log(`Seeding up to ${MAX_PLAYERS} ranked profiles from the 1v1 RM ladder…`);
  const profileIds = await fetchProfileIds(MAX_PLAYERS);
  console.log(`Got ${profileIds.length} profiles. Crawling match history (resume: ${doneProfiles.size} already done)…`);

  let newRows = 0;
  let processed = 0;
  for (const pid of profileIds) {
    if (doneProfiles.has(pid)) { processed++; continue; }
    const url = `${API}/getRecentMatchHistory?title=${TITLE}&profile_ids=%5B${pid}%5D`;
    let rows = [];
    try { rows = normalizeMatches(await getJson(url)); } catch (e) {
      process.stderr.write(`\n  [warn] profile ${pid}: ${e.message} — skipping\n`);
    }
    const fresh = rows.filter((r) => !seenMatches.has(r.match_id));
    if (fresh.length) {
      await appendFile(outPath, fresh.map((r) => JSON.stringify(r)).join("\n") + "\n", "utf8");
      for (const r of fresh) seenMatches.add(r.match_id);
      newRows += fresh.length;
    }
    doneProfiles.add(pid);
    processed++;
    if (processed % 25 === 0) {
      await writeFile(ckPath, JSON.stringify({ doneProfiles: [...doneProfiles], seenMatches: [...seenMatches] }), "utf8");
      process.stderr.write(`\r  crawled ${processed}/${profileIds.length} players · ${seenMatches.size} unique matches`);
    }
    await sleep(THROTTLE_MS);
  }
  await writeFile(ckPath, JSON.stringify({ doneProfiles: [...doneProfiles], seenMatches: [...seenMatches] }), "utf8");
  process.stderr.write("\n");
  console.log(`Done. +${newRows} new matches this run · ${seenMatches.size} unique ranked 1v1 RM matches total → ${outPath}`);
  console.log(`Next: build a VERIFIED civ-id map, then aggregate (node scripts/data-pipeline/aggregate-civmeta.mjs).`);
}

run().catch((e) => { console.error(e); process.exit(1); });
