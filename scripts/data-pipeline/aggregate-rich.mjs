#!/usr/bin/env node
// scripts/data-pipeline/aggregate-rich.mjs
//
// Rich civ stats from the aoestats ranked Parquet archive (via DuckDB), split by
// ladder (1v1 RM + Team RM), plus the self-collected World's Edge "live" 1v1 data.
// Per civ, per ladder:
//   overall {games, winRate, ci95, playRate, tier}
//   byPatch {<patch>: {games, winRate}}    byElo {<bucket>: {games, winRate}}
//   byMap   {<map>:   {games, winRate}}     openings [{opening, share}]
//   ageUp   {feudal, castle, imperial}  (median uptimes, seconds; 1v1 only is meaningful)
//
// Runs ON THE BOX (needs ~/bin/duckdb + ~/aoestats/*.parquet).
//   node scripts/data-pipeline/aggregate-rich.mjs [--relic data-cache/relic/matches.ndjson]

import { execSync } from "node:child_process";
import { existsSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { ELO_BUCKETS, eloBucket, eloCaseSql } from "./lib/buckets.mjs";

const SOURCE_DATE = "2026-02"; // aoestats archive corpus month (frozen) — confirm before changing

const HOME = process.env.HOME;
const DUCK = `${HOME}/bin/duckdb`;
const M = `${HOME}/aoestats/m_*.parquet`;
const P = `${HOME}/aoestats/p_*.parquet`;
const OUT = path.resolve("src/data/civ-meta.json");

const args = Object.fromEntries(process.argv.slice(2).reduce((a, x, i, r) => {
  if (x.startsWith("--")) a.push([x.slice(2), r[i + 1]?.startsWith("--") ? true : r[i + 1] ?? true]);
  return a;
}, []));
const RELIC = args.relic ?? "data-cache/relic/matches.ndjson";

const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug),
);
const civIdMap = JSON.parse(readFileSync(path.resolve("src/data/civ-id-map.json"), "utf8"));

function duck(sql) {
  const out = execSync(`${DUCK} -json -c ${JSON.stringify(sql)}`, { maxBuffer: 1 << 29 }).toString().trim();
  return out ? JSON.parse(out) : [];
}
function wilson(wins, n, z = 1.96) {
  if (!n) return [0, 0];
  const p = wins / n, d = 1 + z * z / n;
  const c = (p + z * z / (2 * n)) / d;
  const m = z * Math.sqrt(p * (1 - p) / n + z * z / (4 * n * n)) / d;
  return [c - m, c + m];
}
const tier = (w) => (w >= 53 ? "S" : w >= 51 ? "A" : w >= 49 ? "B" : w >= 47 ? "C" : "D");
const pct = (x) => +(x * 100).toFixed(2);
const ELO = eloCaseSql("p.new_rating");
const idx = (rows) => { const m = {}; for (const r of rows) (m[r.civ] ??= []).push(r); return m; };

// ---- aggregate one ladder (a leaderboard value) into per-civ breakdowns ----
function aggregateLadder(leaderboard) {
  const BASE = `read_parquet('${P}') p JOIN read_parquet('${M}') m USING (game_id) WHERE m.leaderboard='${leaderboard}'`;
  const overall = duck(`SELECT p.civ civ, count(*) games, sum(p.winner::int) wins FROM ${BASE} GROUP BY 1`);
  const totalApp = overall.reduce((s, r) => s + Number(r.games), 0);
  const byElo = idx(duck(`SELECT p.civ civ, ${ELO} bucket, count(*) games, avg(p.winner::int) wr FROM ${BASE} GROUP BY 1,2`));
  const byPatch = idx(duck(`SELECT p.civ civ, m.patch patch, count(*) games, avg(p.winner::int) wr FROM ${BASE} GROUP BY 1,2`));
  const byMap = idx(duck(`SELECT p.civ civ, m.map mapname, count(*) games, avg(p.winner::int) wr FROM ${BASE} GROUP BY 1,2 HAVING count(*)>=500`));
  const openings = idx(duck(`SELECT civ, opening, games FROM (SELECT p.civ civ, p.opening opening, count(*) games, row_number() OVER (PARTITION BY p.civ ORDER BY count(*) DESC) rn FROM ${BASE} AND p.opening IS NOT NULL AND p.opening<>'' GROUP BY 1,2) WHERE rn<=3`));
  const ageup = Object.fromEntries(duck(`SELECT p.civ civ, median(p.feudal_age_uptime) feudal, median(p.castle_age_uptime) castle, median(p.imperial_age_uptime) imperial FROM ${BASE} AND p.feudal_age_uptime IS NOT NULL GROUP BY 1`).map((r) => [r.civ, r]));
  const patches = duck(`SELECT patch, count(*) n FROM read_parquet('${M}') WHERE leaderboard='${leaderboard}' GROUP BY 1 ORDER BY patch DESC`);
  return { overall: Object.fromEntries(overall.map((r) => [r.civ, r])), totalApp, byElo, byPatch, byMap, openings, ageup, patches };
}

console.log("Aggregating aoestats archive by ladder (1v1 RM + Team RM) via DuckDB…");
const L = { "1v1": aggregateLadder("random_map"), team: aggregateLadder("team_random_map") };

// ---- self-collected live 1v1 data, merged into the 1v1 ladder ----
let liveApp = 0;
let skippedNullElo = 0;
const liveTally = {}; // civ -> {games, wins, byElo:{bucket:{games,wins}}}
if (existsSync(path.resolve(RELIC))) {
  for (const line of readFileSync(path.resolve(RELIC), "utf8").split("\n")) {
    if (!line.trim()) continue;
    let m; try { m = JSON.parse(line); } catch { continue; }
    for (const pl of m.players ?? []) {
      const slug = civIdMap[String(pl.civ_id)];
      if (!slug || !guideCivs.has(slug)) continue;
      const t = (liveTally[slug] ??= { games: 0, wins: 0, byElo: {} });
      t.games++; if (pl.won) t.wins++;
      const eb = eloBucket(pl.rating); if (eb == null) { skippedNullElo++; continue; }
      (t.byElo[eb] ??= { games: 0, wins: 0 });
      t.byElo[eb].games++; if (pl.won) t.byElo[eb].wins++;
      liveApp++;
    }
  }
}

// ---- assemble: per civ -> { '1v1': {...}, 'team': {...} } ----
function buildLadder(slug, agg, live) {
  const o = agg.overall[slug];
  if (!o && !live) return null;
  const games = (o ? Number(o.games) : 0) + (live?.games ?? 0);
  const wins = (o ? Number(o.wins) : 0) + (live?.wins ?? 0);
  if (!games) return null;
  const [lo, hi] = wilson(wins, games);
  const cur = agg.patches[0]?.patch;
  const obj = (rows, key, val) => Object.fromEntries((rows ?? []).map((r) => [r[key], val(r)]));
  // byElo (merge live)
  const be = {};
  for (const r of (agg.byElo[slug] ?? [])) be[r.bucket] = { games: Number(r.games), wins: Math.round(r.wr * r.games) };
  if (live) for (const [b, v] of Object.entries(live.byElo)) { be[b] ??= { games: 0, wins: 0 }; be[b].games += v.games; be[b].wins += v.wins; }
  const byElo = Object.fromEntries(Object.entries(be).map(([b, v]) => [b, { games: v.games, winRate: pct(v.wins / v.games) }]));
  // byPatch (live folded into current patch)
  const bp = obj(agg.byPatch[slug], "patch", (r) => ({ games: Number(r.games), winRate: pct(r.wr) }));
  if (live && cur != null) { const c = bp[cur] ?? { games: 0, winRate: 0 }; const cg = c.games + live.games, cw = Math.round((c.winRate / 100) * c.games) + live.wins; bp[cur] = { games: cg, winRate: pct(cw / cg) }; }
  const ag = agg.ageup[slug];
  return {
    games, winRate: pct(wins / games), ci95: [pct(lo), pct(hi)],
    playRate: pct(games / (agg.totalApp + (live ? liveApp : 0))), tier: tier((wins / games) * 100),
    byPatch: bp, byElo,
    byMap: Object.fromEntries((agg.byMap[slug] ?? []).sort((a, b) => Number(b.games) - Number(a.games)).slice(0, 14).map((r) => [r.mapname, { games: Number(r.games), winRate: pct(r.wr) }])),
    openings: (agg.openings[slug] ?? []).map((r) => ({ opening: r.opening, share: o ? pct(Number(r.games) / Number(o.games)) : null })),
    ageUp: ag ? { feudal: Math.round(ag.feudal), castle: Math.round(ag.castle), imperial: Math.round(ag.imperial) } : null,
  };
}

const civs = {};
for (const slug of [...guideCivs].sort()) {
  const one = buildLadder(slug, L["1v1"], liveTally[slug]);
  const team = buildLadder(slug, L.team, null);
  civs[slug] = { "1v1": one, team };
}

const out = {
  source: "aoestats.io ranked Parquet archive (jerkeeler)" + (liveApp ? " + self-collected World's Edge (live 1v1)" : ""),
  ladders: ["1v1", "team"],
  generated: new Date().toISOString().slice(0, 10),
  sourceDate: SOURCE_DATE,
  appearances: { "1v1": L["1v1"].totalApp, team: L.team.totalApp, live: liveApp },
  patches: L["1v1"].patches.map((r) => ({ patch: r.patch, matches: Number(r.n) })),
  eloBuckets: ELO_BUCKETS,
  tierMethod: "Win rate: ≥53 S | 51–53 A | 49–51 B | 47–49 C | <47 D. No data → null.",
  civs,
};
writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
const r1 = Object.values(civs).filter((c) => c["1v1"]).length;
const rt = Object.values(civs).filter((c) => c.team).length;
console.log(`civ-meta: 1v1 ${(L["1v1"].totalApp / 1e6).toFixed(1)}M (+${liveApp} live) · team ${(L.team.totalApp / 1e6).toFixed(1)}M · ${r1} civs 1v1 / ${rt} team · ${out.patches.length} patches · ${skippedNullElo} null-elo dropped → ${OUT}`);
