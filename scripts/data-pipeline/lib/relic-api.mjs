// scripts/data-pipeline/lib/relic-api.mjs
//
// The ONE World's Edge API client for the crawler pair (stream-relic /
// collect-relic). Both scripts previously carried drifted copies of this —
// an API shape change had to be fixed twice. Behavior is the SAFER superset
// of the two: error-guarded leaderboard pagination + TTY-guarded progress.

import { setTimeout as sleep } from "node:timers/promises";

export const API = "https://aoe-api.worldsedgelink.com/community/leaderboard";
export const TITLE = "age2";
export const LEADERBOARD_1V1_RM = 3; // SOLO_RM_RANKED
export const LEADERBOARD_TEAM_RM = 4; // TEAM_RM_RANKED
export const PAGE = 200; // leaderboard page size

export { sleep };

// Keep predicate per mode: 1v1 = exactly 2 members; team = 4/6/8 (2v2/3v3/4v4).
export const keepBySize = (team) => (n) => (team ? n >= 4 && n % 2 === 0 : n === 2);

export function makeClient({ throttleMs = 120 } = {}) {
  async function getJson(url, tries = 4) {
    for (let attempt = 1; attempt <= tries; attempt++) {
      try {
        const res = await fetch(url, { headers: { "user-agent": "aoe2guide-stats/1.0 (self-collect)" } });
        if (res.status === 429 || res.status >= 500) throw new Error(`HTTP ${res.status}`);
        if (!res.ok) throw new Error(`HTTP ${res.status}`);
        return await res.json();
      } catch (e) {
        if (attempt === tries) throw e;
        await sleep(throttleMs * attempt * 3); // back off
      }
    }
  }

  // Enumerate the ladder (paginate to rankTotal, capped by limit). A failing
  // page WARNS and returns what was seeded so far — a sweep never dies on one
  // bad leaderboard page.
  async function fetchProfileIds({ leaderboard, limit = Infinity }) {
    const ids = new Set();
    for (let start = 1; ids.size < limit; start += PAGE) {
      const url = `${API}/getLeaderBoard2?title=${TITLE}&leaderboard_id=${leaderboard}&start=${start}&count=${PAGE}&sortBy=1`;
      let data;
      try {
        data = await getJson(url);
      } catch (e) {
        process.stderr.write(`\n  [warn] leaderboard page ${start}: ${e.message}\n`);
        break;
      }
      const groups = data?.statGroups ?? [];
      if (!groups.length) break;
      for (const g of groups) for (const m of g.members ?? []) if (m.profile_id != null) ids.add(m.profile_id);
      if (process.stderr.isTTY) process.stderr.write(`\r  leaderboard: ${ids.size} profiles seeded`);
      await sleep(throttleMs);
    }
    process.stderr.write("\n");
    return [...ids].slice(0, limit === Infinity ? ids.size : limit);
  }

  return { getJson, fetchProfileIds };
}

// getRecentMatchHistory response -> normalized crawl records. `keep` is the
// member-count predicate from keepBySize(team). civ_id stays NUMERIC (Relic's
// OWN id space — see src/data/relic-civ-id-map.json); map_raw is the RAW API
// mapname (wrong for most matches — replay parsing is the only map truth).
export function normalizeMatches(history, keep) {
  const out = [];
  for (const m of history?.matchHistoryStats ?? []) {
    if (m.description !== "AUTOMATCH") continue; // ranked matchmaking only
    const members = m.matchhistorymember ?? [];
    if (!keep(members.length)) continue;
    out.push({
      match_id: m.id,
      completed: m.completiontime,
      gamemod_id: m.gamemod_id ?? null, // patch proxy: monotonic, date-aligned
      map_raw: m.mapname ?? null,
      ladder: m.matchtype_id ?? null, // matchtype: 6 = 1v1 RM, 7/8/9 = team RM, rest = other modes
      team_size: members.length, // 2 = 1v1, 4 = 2v2, 6 = 3v3, 8 = 4v4
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
