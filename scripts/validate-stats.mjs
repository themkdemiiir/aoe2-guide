#!/usr/bin/env node
// Build-time validation for the large self-collected stat JSONs that ship into
// the build but bypass validate-data.mjs (which only gates civilizations.json).
// Catches a bad VM/desktop regen (wrong winRate scale, NaN/null, missing keys,
// bad tier, corrupt cube) before it silently reaches the charts. Wired into prebuild.
//
// Shape-agnostic by design: it recursively checks every named `winRate`/`tier`
// field (so it survives the differing nestings of civ-meta/map-meta/matchups) plus
// per-file top-level keys and the cube's integer-tuple rows. Tuple-encoded winRates
// (e.g. matchups-by-elo's [winRate, games]) are not range-checked individually —
// the gate targets gross regressions, not exhaustive per-cell validation.

import { readFileSync } from "node:fs";

const FILES = [
  "src/data/civ-meta.json",
  "src/data/map-meta.json",
  "src/data/civ-matchups.json",
  "src/data/civ-matchups-by-map.json",
  "src/data/civ-matchups-by-elo.json",
  "src/data/civ-matchups-team.json",
];
const TIERS = new Set(["S", "A", "B", "C", "D"]);
const issues = [];
const fail = (file, msg) => issues.push(`  ✗ ${file}: ${msg}`);

function checkTop(file, o) {
  if (typeof o.source !== "string" || !o.source) fail(file, "missing/empty 'source'");
  if (typeof o.generated !== "string" || !/^\d{4}-\d{2}-\d{2}$/.test(o.generated))
    fail(file, "missing/invalid 'generated' (YYYY-MM-DD)");
}

// Recursively collect named winRate values + validate named tier values.
function scan(file, node, rates) {
  if (Array.isArray(node)) {
    for (const x of node) scan(file, x, rates);
    return;
  }
  if (node && typeof node === "object") {
    for (const [k, val] of Object.entries(node)) {
      if (k === "winRate" && val != null) {
        if (typeof val !== "number" || !Number.isFinite(val))
          fail(file, `winRate not a finite number: ${JSON.stringify(val)}`);
        else {
          rates.push(val);
          if (val < 0 || val > 100) fail(file, `winRate out of [0,100]: ${val}`);
        }
      } else if (k === "tier" && val != null) {
        if (!TIERS.has(val)) fail(file, `tier not in {S,A,B,C,D}: ${JSON.stringify(val)}`);
      } else {
        scan(file, val, rates);
      }
    }
  }
}

for (const file of FILES) {
  let o;
  try {
    o = JSON.parse(readFileSync(file, "utf8"));
  } catch (e) {
    fail(file, `unreadable/invalid JSON: ${e.message}`);
    continue;
  }
  checkTop(file, o);
  const rates = [];
  scan(file, o, rates);
  // scale guard: a 0-1 fraction slipped in where 0-100 percent is expected.
  if (rates.length && rates.every((r) => r <= 1))
    fail(file, "winRate looks like a 0-1 fraction, expected 0-100 percent");
}

// public/civ-cube.json — packed integer tuples [civ,elo,map,month,games,wins].
{
  const file = "public/civ-cube.json";
  let o = null;
  try {
    o = JSON.parse(readFileSync(file, "utf8"));
  } catch (e) {
    fail(file, `unreadable/invalid JSON: ${e.message}`);
  }
  if (o) {
    if (typeof o.source !== "string" || !o.source) fail(file, "missing/empty 'source'");
    const lens = ["civs", "elos", "maps", "months"].map((d) => {
      if (!Array.isArray(o[d]) || o[d].length === 0) {
        fail(file, `dim '${d}' empty/missing`);
        return 0;
      }
      return o[d].length;
    });
    const [nc, ne, nm, np] = lens;
    if (!Array.isArray(o.rows)) {
      fail(file, "missing 'rows' array");
    } else {
      for (let i = 0; i < o.rows.length; i++) {
        const row = o.rows[i];
        if (!Array.isArray(row) || row.length !== 6) {
          fail(file, `row ${i} is not a 6-tuple`);
          break;
        }
        const [ci, ei, mi, pi, g, w] = row;
        if (ci < 0 || ci >= nc || ei < 0 || ei >= ne || mi < 0 || mi >= nm || pi < 0 || pi >= np) {
          fail(file, `row ${i} category index out of range`);
          break;
        }
        if (!(Number.isInteger(g) && Number.isInteger(w) && w <= g && g >= 0)) {
          fail(file, `row ${i} bad games/wins (${w}/${g})`);
          break;
        }
      }
    }
  }
}

if (issues.length) {
  for (const i of issues) console.error(i);
  console.error(`\nvalidate-stats: ${issues.length} issue(s)`);
  process.exit(1);
}
console.log("validate-stats: stat JSONs OK ✓");
