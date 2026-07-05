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
// One pass over ALL crawl sources (lib/crawl-stream.mjs, last CURRENT_WINDOW_DAYS,
// ranked RM only). Civ ids are the Relic API space (relic-civ-id-map.json);
// per-map matchups use replay-parsed map truth only (the API mapname is junk).
//
//   node scripts/data-pipeline/refresh-matchups-current.mjs

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { ELO_BUCKETS_WITH_ALL, eloBucket } from "./lib/buckets.mjs";
import { CURRENT_WINDOW_DAYS, crawlRecords } from "./lib/crawl-stream.mjs";
import { loadGuideCivs } from "./lib/guide-civs.mjs";
import {
  canonToKeyIndex,
  isRanked1v1,
  isRankedTeam,
  loadReplayMapTruth,
  relicCivSlug,
} from "./lib/relic-map.mjs";
import { pct } from "./lib/stats.mjs";

const guideCivs = loadGuideCivs();
const mapMeta = JSON.parse(readFileSync(path.resolve("src/data/map-meta.json"), "utf8"));

const canonToKey = canonToKeyIndex(mapMeta);
const mapTruth = await loadReplayMapTruth();

const SOURCE = `self-collected World's Edge live ladder (ranked RM, last ${CURRENT_WINDOW_DAYS} days; maps replay-verified)`;
const TODAY = new Date().toISOString().slice(0, 10);
// Show data down to a small floor and let the UI flag low samples (games < 150
// render a "low sample" caveat), rather than hiding sparse pairings entirely —
// e.g. two low-play-rate civs like Dravidians vs Wu. Below these floors the
// number is pure noise, so they stay omitted.
const MIN = 50,
  MIN_MAP = 50,
  MIN_BUCKET = 30,
  MIN_ALL = 30,
  MIN_TEAM = 100;
const bump = (o, k, won) => {
  const a = (o[k] ??= [0, 0]);
  a[0]++;
  if (won) a[1]++;
};

// --- single pass over all crawl sources: 1v1 (overall/by-map/by-elo) + team ---
const ov = {},
  bm = {},
  be = {},
  tv = {};
let skippedNullElo = 0;
for await (const m of crawlRecords({ recentDays: CURRENT_WINDOW_DAYS })) {
  if (isRanked1v1(m)) {
    const ps = (m.players ?? [])
      .map((p) => ({ civ: relicCivSlug(p.civ_id), won: !!p.won, rating: p.rating }))
      .filter((p) => guideCivs.has(p.civ));
    if (ps.length !== 2) continue;
    const [a, b] = ps;
    if (a.civ === b.civ) continue;
    const truth = mapTruth.get(m.match_id);
    const mapKey = truth ? (canonToKey[truth.canon]?.key ?? null) : null;
    for (const [x, y] of [
      [a, b],
      [b, a],
    ]) {
      bump(ov, `${x.civ}|${y.civ}`, x.won);
      if (mapKey) bump(bm, `${x.civ}|${y.civ}|${mapKey}`, x.won);
      const eb = eloBucket(x.rating);
      if (eb == null) {
        skippedNullElo++;
        continue;
      }
      bump(be, `${x.civ}|${y.civ}|${eb}`, x.won);
    }
  } else if (isRankedTeam(m)) {
    const ps = (m.players ?? [])
      .map((p) => ({ civ: relicCivSlug(p.civ_id), won: !!p.won }))
      .filter((p) => guideCivs.has(p.civ));
    if (ps.length < 4) continue;
    for (let i = 0; i < ps.length; i++)
      for (let j = 0; j < ps.length; j++) {
        if (i === j) continue;
        const x = ps[i],
          y = ps[j];
        if (x.won === y.won || x.civ === y.civ) continue; // cross-team only
        bump(tv, `${x.civ}|${y.civ}`, x.won);
      }
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
  ((byMap[civ] ??= {})[mapKey] ??= []).push({ opp, games: g, winRate: pct(w / g) });
}
for (const c in byMap)
  for (const mp in byMap[c]) byMap[c][mp].sort((a, b) => b.winRate - a.winRate);

// civ-matchups-by-elo.json — civs[civ][opp] = { bucket: [wr,g], all: [wr,g] }
const beAcc = {}; // civ -> opp -> bucket -> [g,w]
for (const [k, [g, w]] of Object.entries(be)) {
  const [civ, opp, bucket] = k.split("|");
  ((beAcc[civ] ??= {})[opp] ??= {})[bucket] = [g, w];
}
const byElo = {};
for (const [civ, opps] of Object.entries(beAcc)) {
  const oOut = {};
  for (const [opp, buckets] of Object.entries(opps)) {
    const slice = {};
    let ag = 0,
      aw = 0;
    for (const [bk, [g, w]] of Object.entries(buckets)) {
      ag += g;
      aw += w;
      if (g >= MIN_BUCKET) slice[bk] = [pct(w / g), g];
    }
    if (ag >= MIN_ALL) slice.all = [pct(aw / ag), ag];
    if (Object.keys(slice).length) oOut[opp] = slice;
  }
  if (Object.keys(oOut).length) byElo[civ] = oOut;
}

// --- team: confounded cross-team matchups (accumulated in the single pass) ---
const teamCivs = {};
for (const [k, [g, w]] of Object.entries(tv)) {
  if (g < MIN_TEAM) continue;
  const [civ, opp] = k.split("|");
  (teamCivs[civ] ??= []).push({ opp, games: g, winRate: pct(w / g) });
}
for (const c in teamCivs) teamCivs[c].sort((a, b) => b.winRate - a.winRate);

const write = (file, extra, civsObj) => {
  writeFileSync(
    path.resolve(`src/data/${file}`),
    `${JSON.stringify({ source: SOURCE, generated: TODAY, ...extra, civs: civsObj }, null, 2)}\n`,
    "utf8",
  );
};
write(
  "civ-matchups.json",
  {
    ladder: "1v1",
    minGames: MIN,
    note: "winRate = how often <civ> beats <opp> in 1v1 RM. Mirrors excluded.",
  },
  civs,
);
write(
  "civ-matchups-by-map.json",
  {
    ladder: "1v1",
    minGames: MIN_MAP,
    note: "winRate = how often <civ> beats <opp> on <map> in 1v1 RM. Mirrors excluded.",
  },
  byMap,
);
write(
  "civ-matchups-by-elo.json",
  {
    ladder: "1v1",
    eloBuckets: ELO_BUCKETS_WITH_ALL,
    minGames: { bucket: MIN_BUCKET, all: MIN_ALL },
    note: "[winRate, games] of <civ> vs <opp> per elo bucket. Mirrors excluded.",
  },
  byElo,
);
write(
  "civ-matchups-team.json",
  {
    ladder: "team",
    minGames: MIN_TEAM,
    note: "winRate = how often <civ>'s team wins when <opp> is on the enemy team (team RM; confounded by the 3 other civs per side).",
  },
  teamCivs,
);

const sum = (o) =>
  Object.values(o).reduce((s, a) => s + (Array.isArray(a) ? a.length : Object.keys(a).length), 0);
console.log(
  `refresh-matchups: 1v1 ${Object.keys(civs).length} civs/${sum(civs)} pairs · by-map ${sum(byMap)} · by-elo ${Object.keys(byElo).length} civs · team ${Object.keys(teamCivs).length} civs · ${skippedNullElo} null-elo dropped`,
);
