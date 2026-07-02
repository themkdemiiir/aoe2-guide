#!/usr/bin/env node
// scripts/data-pipeline/aggregate-civmeta.mjs
//
// Reads collected ranked 1v1 RM matches (NDJSON from collect-relic.mjs) + the
// VERIFIED civ-id map, computes per-civ win rate / play rate with a Wilson 95%
// confidence interval and a sample-size gate, and emits src/data/civ-meta.json.
//
// Pure Node (no DuckDB) — the self-collected corpus fits comfortably in a stream.
// (DuckDB is only needed for the aoestats-parquet bulk-history path.)
//
// Honesty rules: NO default/fabricated values. A civ below the sample gate gets
// tier "NR" + null rates (never a point estimate off thin data). An UNKNOWN civ_id
// (no map entry) is FAIL-LOUD, not silently dropped. Chronicles/removed civs that
// map to a non-guide slug are skipped (not part of this guide's 53).
//
// Usage (on the box, after collect-relic):
//   node scripts/data-pipeline/aggregate-civmeta.mjs --min-games 400 [--min-elo 1000]

import { readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import { parseArgs } from "node:util";
import { crawlRecords } from "./lib/crawl-stream.mjs";
import { RELIC_CIV_MAP, isRanked1v1 } from "./lib/relic-map.mjs";
import { tierOf, wilson } from "./lib/stats.mjs";

const { values: args } = parseArgs({
  options: {
    in: { type: "string" },
    out: { type: "string" },
    "min-games": { type: "string" },
    "min-elo": { type: "string" },
  },
  strict: true,
});
// Default input = ALL crawl sources via lib/crawl-stream.mjs (deduped, gated);
// --in <file> narrows to one NDJSON file (still gated + deduped).
const IN = args.in ? [path.resolve(args.in)] : undefined;
const OUT = path.resolve(args.out ?? "src/data/civ-meta.json");
const MIN_GAMES = Number(args["min-games"] ?? 400);
const MIN_ELO = Number(args["min-elo"] ?? 0);

// Crawl civ ids are the Relic API space — NOT civ-id-map.json (game/replay space).
const civIdMap = RELIC_CIV_MAP;
const guideCivs = new Set(
  JSON.parse(readFileSync(path.resolve("src/data/civilizations.json"), "utf8")).civs.map((c) => c.slug),
);

async function run() {
  const tally = {}; // slug -> { games, wins }
  let matches = 0;
  let appearances = 0;
  let skippedExcluded = 0;
  let minDate = Infinity;
  let maxDate = 0;
  const unknownIds = new Set();

  for await (const m of crawlRecords({ sources: IN })) {
    if (!isRanked1v1(m)) continue; // RM + era gates already applied by crawlRecords
    matches++;
    if (m.completed) { minDate = Math.min(minDate, m.completed); maxDate = Math.max(maxDate, m.completed); }
    for (const p of m.players ?? []) {
      if (MIN_ELO && (p.rating == null || p.rating < MIN_ELO)) continue;
      const slug = civIdMap[String(p.civ_id)];
      if (slug === undefined) { unknownIds.add(p.civ_id); continue; }
      if (!guideCivs.has(slug)) { skippedExcluded++; continue; } // Chronicles/removed civ — not in this guide
      const t = (tally[slug] ??= { games: 0, wins: 0 });
      t.games++;
      if (p.won) t.wins++;
      appearances++;
    }
  }

  // Fail loud: an unmapped civ_id means our civ-id map is incomplete — surface it, don't ship blind.
  if (unknownIds.size) {
    console.error(`[FATAL] ${unknownIds.size} Relic civilization_id(s) have no relic-civ-id-map entry: ${[...unknownIds].sort((a, b) => a - b).join(", ")}`);
    console.error("A new DLC likely shifted the API id space — re-derive src/data/relic-civ-id-map.json (races table + replay join) and re-run. No data emitted.");
    process.exit(1);
  }
  if (matches === 0) {
    console.error(`[FATAL] no matches in ${IN}. Run collect-relic.mjs first.`);
    process.exit(1);
  }

  const civs = {};
  let rated = 0;
  for (const slug of [...guideCivs].sort()) {
    const t = tally[slug];
    const games = t?.games ?? 0;
    if (games < MIN_GAMES) {
      civs[slug] = { games, winRate: null, ci95: null, playRate: null, tier: "NR" };
      continue;
    }
    rated++;
    const wr = t.wins / games;
    const [lo, hi] = wilson(t.wins, games);
    civs[slug] = {
      games,
      winRate: +(wr * 100).toFixed(2),
      ci95: [+(lo * 100).toFixed(2), +(hi * 100).toFixed(2)],
      playRate: +((games / appearances) * 100).toFixed(2),
      tier: tierOf(wr * 100),
    };
  }

  const iso = (s) => (Number.isFinite(s) && s > 0 ? new Date(s * 1000).toISOString().slice(0, 10) : null);
  const out = {
    source: "self-collected — World's Edge Link API (aoe-api.worldsedgelink.com), 1v1 RM ranked AUTOMATCH",
    ladder: "1v1 RM",
    minElo: MIN_ELO || null,
    minGames: MIN_GAMES,
    matches,
    sampleAppearances: appearances,
    dataRange: { from: iso(minDate), to: iso(maxDate) },
    generated: new Date().toISOString().slice(0, 10),
    tierMethod: "Win rate: ≥53 S | 51–53 A | 49–51 B | 47–49 C | <47 D. Below minGames → NR (insufficient data).",
    civs,
  };
  writeFileSync(OUT, `${JSON.stringify(out, null, 2)}\n`, "utf8");
  console.log(
    `civ-meta: ${matches} matches · ${appearances} civ appearances · ` +
      `${rated}/${guideCivs.size} civs above the ${MIN_GAMES}-game gate (rest = NR) → ${OUT}`,
  );
  if (skippedExcluded) console.log(`  (skipped ${skippedExcluded} appearances of non-guide civs — Chronicles/removed)`);
}

run().catch((e) => { console.error(e); process.exit(1); });
