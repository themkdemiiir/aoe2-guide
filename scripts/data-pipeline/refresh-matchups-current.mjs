#!/usr/bin/env node
// scripts/data-pipeline/refresh-matchups-current.mjs
//
// Regenerate civ-vs-civ matchups from the self-collected crawls (replacing the
// frozen aoestats matchup data), so every data surface shares ONE source. Mirrors
// the output shape of aggregate-matchups.mjs:
//   civ-matchups.json         1v1 overall      civs[civ] = [{opp,games,winRate}]
//   civ-matchups-by-map.json  1v1 per map      civs[civ][mapKey] = [{opp,…}]
//   civ-matchups-by-elo.json  1v1 per elo      civs[civ][opp] = {bucket:[wr,g], all:[wr,g]}
//   civ-matchups-team.json    team overall     civs[civ] = [{opp,games,winRate}] (confounded)
// 1v1 from data-cache/relic-patched, team from data-cache/relic-team.
//
//   node scripts/data-pipeline/refresh-matchups-current.mjs

import { createReadStream, readFileSync, writeFileSync } from "node:fs";
import { createInterface } from "node:readline";
import path from "node:path";

const IN_1V1 = path.resolve("data-cache/relic-patched/matches.ndjson");
const IN_TEAM = path.resolve("data-cache/relic-team/matches.ndjson");

const civIdMap = JSON.parse(readFileSync(path.resolve("src/data/civ-id-map.json"), "utf8"));
const guideCivs = new Set(JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug));
const mapMeta = JSON.parse(readFileSync(path.resolve("src/data/map-meta.json"), "utf8"));

const pct = (x) => +(x * 100).toFixed(2);
const canon = (s) => String(s).toLowerCase().replace(/\.[a-z0-9]+$/i, "").replace(/[^a-z0-9]/g, "");
const eloBucket = (r) =>
  r < 1000 ? "<1000" : r < 1200 ? "1000-1199" : r < 1400 ? "1200-1399" : r < 1650 ? "1400-1649"
  : r < 1800 ? "1650-1799" : r < 2000 ? "1800-1999" : r < 2200 ? "2000-2199" : r < 2500 ? "2200-2499" : "2500+";
const canonToKey = {};
for (const [k, v] of Object.entries(mapMeta.maps)) {
  const c = canon(k);
  const g = (v.games?.["1v1"] ?? 0) + (v.games?.team ?? 0);
  if (!canonToKey[c] || g > canonToKey[c].g) canonToKey[c] = { key: k, g };
}
const mapKeyFor = (raw) => canonToKey[canon(raw)]?.key ?? null;

const SOURCE = "self-collected World's Edge live ladder (1v1 + team, current)";
const TODAY = new Date().toISOString().slice(0, 10);
const MIN = 300, MIN_MAP = 200, MIN_BUCKET = 150, MIN_ALL = 300, MIN_TEAM = 500;
const bump = (o, k, won) => { const a = (o[k] ??= [0, 0]); a[0]++; if (won) a[1]++; };

// --- 1v1: overall + by-map + by-elo (both perspectives per match) ---
const ov = {}, bm = {}, be = {};
const r1 = createInterface({ input: createReadStream(IN_1V1), crlfDelay: Infinity });
for await (const line of r1) {
  if (!line.trim()) continue;
  let m;
  try { m = JSON.parse(line); } catch { continue; }
  const ps = (m.players ?? []).map((p) => ({ civ: civIdMap[String(p.civ_id)], won: !!p.won, rating: p.rating ?? 0 })).filter((p) => p.civ && guideCivs.has(p.civ));
  if (ps.length !== 2) continue;
  const [a, b] = ps;
  if (a.civ === b.civ) continue;
  const mapKey = m.map_raw ? mapKeyFor(m.map_raw) : null;
  for (const [x, y] of [[a, b], [b, a]]) {
    bump(ov, `${x.civ}|${y.civ}`, x.won);
    if (mapKey) bump(bm, `${x.civ}|${y.civ}|${mapKey}`, x.won);
    bump(be, `${x.civ}|${y.civ}|${eloBucket(x.rating)}`, x.won);
  }
}

// civ-matchups.json (overall)
const civs = {};
for (const [k, [g, w]] of Object.entries(ov)) {
  if (g < MIN) continue;
  const [civ, opp] = k.split("|");
  (civs[civ] ??= []).push({ opp, games: g, winRate: pct(w / g) });
}
for (const c in civs) civs[c].sort((a, b) => b.winRate - a.winRate);

// civ-matchups-by-map.json
const byMap = {};
for (const [k, [g, w]] of Object.entries(bm)) {
  if (g < MIN_MAP) continue;
  const [civ, opp, mapKey] = k.split("|");
  (((byMap[civ] ??= {})[mapKey] ??= []).push({ opp, games: g, winRate: pct(w / g) }));
}
for (const c in byMap) for (const mp in byMap[c]) byMap[c][mp].sort((a, b) => b.winRate - a.winRate);

// civ-matchups-by-elo.json — civs[civ][opp] = { bucket: [wr,g], all: [wr,g] }
const beAcc = {}; // civ -> opp -> bucket -> [g,w]
for (const [k, [g, w]] of Object.entries(be)) {
  const [civ, opp, bucket] = k.split("|");
  (((beAcc[civ] ??= {})[opp] ??= {})[bucket] = [g, w]);
}
const byElo = {};
for (const [civ, opps] of Object.entries(beAcc)) {
  const oOut = {};
  for (const [opp, buckets] of Object.entries(opps)) {
    const slice = {};
    let ag = 0, aw = 0;
    for (const [bk, [g, w]] of Object.entries(buckets)) {
      ag += g; aw += w;
      if (g >= MIN_BUCKET) slice[bk] = [pct(w / g), g];
    }
    if (ag >= MIN_ALL) slice.all = [pct(aw / ag), ag];
    if (Object.keys(slice).length) oOut[opp] = slice;
  }
  if (Object.keys(oOut).length) byElo[civ] = oOut;
}

// --- team: confounded cross-team matchups from the team crawl ---
const tv = {};
const rt = createInterface({ input: createReadStream(IN_TEAM), crlfDelay: Infinity });
for await (const line of rt) {
  if (!line.trim()) continue;
  let m;
  try { m = JSON.parse(line); } catch { continue; }
  const ps = (m.players ?? []).map((p) => ({ civ: civIdMap[String(p.civ_id)], won: !!p.won })).filter((p) => p.civ && guideCivs.has(p.civ));
  if (ps.length < 4) continue;
  for (let i = 0; i < ps.length; i++) for (let j = 0; j < ps.length; j++) {
    if (i === j) continue;
    const x = ps[i], y = ps[j];
    if (x.won === y.won || x.civ === y.civ) continue; // cross-team only
    bump(tv, `${x.civ}|${y.civ}`, x.won);
  }
}
const teamCivs = {};
for (const [k, [g, w]] of Object.entries(tv)) {
  if (g < MIN_TEAM) continue;
  const [civ, opp] = k.split("|");
  (teamCivs[civ] ??= []).push({ opp, games: g, winRate: pct(w / g) });
}
for (const c in teamCivs) teamCivs[c].sort((a, b) => b.winRate - a.winRate);

const write = (file, extra, civsObj) => {
  writeFileSync(path.resolve(`src/data/${file}`), `${JSON.stringify({ source: SOURCE, generated: TODAY, ...extra, civs: civsObj }, null, 2)}\n`, "utf8");
};
write("civ-matchups.json", { ladder: "1v1", minGames: MIN, note: "winRate = how often <civ> beats <opp> in 1v1 RM. Mirrors excluded." }, civs);
write("civ-matchups-by-map.json", { ladder: "1v1", minGames: MIN_MAP, note: "winRate = how often <civ> beats <opp> on <map> in 1v1 RM. Mirrors excluded." }, byMap);
write("civ-matchups-by-elo.json", { ladder: "1v1", eloBuckets: ["all", "<1000", "1000-1199", "1200-1399", "1400-1649", "1650-1799", "1800-1999", "2000-2199", "2200-2499", "2500+"], minGames: { bucket: MIN_BUCKET, all: MIN_ALL }, note: "[winRate, games] of <civ> vs <opp> per elo bucket. Mirrors excluded." }, byElo);
write("civ-matchups-team.json", { ladder: "team", minGames: MIN_TEAM, note: "winRate = how often <civ>'s team wins when <opp> is on the enemy team (team RM; confounded by the 3 other civs per side)." }, teamCivs);

const sum = (o) => Object.values(o).reduce((s, a) => s + (Array.isArray(a) ? a.length : Object.keys(a).length), 0);
console.log(`refresh-matchups: 1v1 ${Object.keys(civs).length} civs/${sum(civs)} pairs · by-map ${sum(byMap)} · by-elo ${Object.keys(byElo).length} civs · team ${Object.keys(teamCivs).length} civs`);
