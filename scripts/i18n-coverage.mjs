#!/usr/bin/env node
// i18n coverage check — reports TR content that is missing or untranslated.
// Covers (1) content collections (src/content/<type>/{en,tr}/*.md|mdx) and
// (2) prose embedded in src/data/counters.json (intro / why / note fields),
// which the EN-fallback renders verbatim on /tr/counters and which no
// file-based check can see.
//
// Paths verified against the repo on 2026-06-09 (en/tr segments under src/content).
//
// Usage:  node scripts/i18n-coverage.mjs
// CI:     exits 1 if any gaps are found, 2 on config error.

import { existsSync, readdirSync, readFileSync, statSync } from "node:fs";
import { join, sep } from "node:path";

const CONTENT_ROOT = "src/content";
const COUNTERS_JSON = "src/data/counters.json";
const EN_SEG = `${sep}en${sep}`;
const TR_SEG = `${sep}tr${sep}`;

function walk(dir) {
  const out = [];
  for (const name of readdirSync(dir)) {
    const p = join(dir, name);
    const st = statSync(p);
    if (st.isDirectory()) out.push(...walk(p));
    else if (p.endsWith(".md") || p.endsWith(".mdx")) out.push(p);
  }
  return out;
}

// Strip frontmatter + collapse whitespace so "identical" means identical *body*.
function body(file) {
  let txt = readFileSync(file, "utf8");
  txt = txt.replace(/^---\n[\s\S]*?\n---\n/, "");
  return txt.replace(/\s+/g, " ").trim();
}

// type = first path segment under CONTENT_ROOT (civilizations, units, …)
function typeOf(p) {
  const rel = p.slice(CONTENT_ROOT.length + 1);
  return rel.split(sep)[0];
}

if (!existsSync(CONTENT_ROOT)) {
  console.error(`Content root not found: ${CONTENT_ROOT}`);
  process.exit(2);
}

// ---------- 1) content collections ----------
// Exclude collections that use single bilingual files (no en/tr pair needed).
const BILINGUAL_COLLECTIONS = ["civilizations", "build-orders", "maps", "glossary", "units"];
const enFiles = walk(CONTENT_ROOT)
  .filter((f) => f.includes(EN_SEG))
  .filter((f) => !BILINGUAL_COLLECTIONS.some((c) => f.includes(`${sep}${c}${sep}`)));
const missing = [];
const untranslated = [];

for (const en of enFiles) {
  const tr = en.replace(EN_SEG, TR_SEG);
  if (!existsSync(tr)) missing.push(en);
  else if (body(en) === body(tr)) untranslated.push(tr);
}

const total = enFiles.length;
const done = total - missing.length - untranslated.length;

console.log(`\ni18n coverage: ${done}/${total} TR files translated\n`);

const perType = {};
for (const f of missing) {
  const t = typeOf(f);
  perType[t] ??= { missing: 0, identical: 0 };
  perType[t].missing++;
}
for (const f of untranslated) {
  const t = typeOf(f);
  perType[t] ??= { missing: 0, identical: 0 };
  perType[t].identical++;
}
if (Object.keys(perType).length) {
  console.log("By type:");
  for (const [t, c] of Object.entries(perType).sort())
    console.log(
      `  ${t.padEnd(16)} missing ${String(c.missing).padStart(3)}   identical ${c.identical}`,
    );
  console.log("");
}

if (missing.length) {
  console.log(`Missing TR files (${missing.length}):`);
  for (const f of missing) console.log(`  - ${f.replace(EN_SEG, TR_SEG)}`);
  console.log("");
}
if (untranslated.length) {
  console.log(`Untranslated (TR body identical to EN) (${untranslated.length}):`);
  for (const f of untranslated) console.log(`  - ${f}`);
  console.log("");
}

// ---------- 2) counters.json embedded prose ----------
// Current shape: ages.{feudal,castle}.{intro, relations[].why} and units.<slug>.note
// are plain EN strings. Target shape after localization: { en, tr } objects.
// This check reports both states honestly.
let dataGaps = 0;
if (existsSync(COUNTERS_JSON)) {
  const counters = JSON.parse(readFileSync(COUNTERS_JSON, "utf8"));
  const plainEN = [];
  const missingTR = [];

  const inspect = (val, path) => {
    if (typeof val === "string") plainEN.push(path);
    else if (val && typeof val === "object" && ("en" in val || "tr" in val)) {
      if (!val.tr || (typeof val.tr === "string" && !val.tr.trim())) missingTR.push(path);
    }
  };

  for (const [age, block] of Object.entries(counters.ages ?? {})) {
    inspect(block.intro, `ages.${age}.intro`);
    (block.relations ?? []).forEach((r, i) => {
      inspect(r.why, `ages.${age}.relations[${i}].why`);
    });
  }
  for (const [slug, u] of Object.entries(counters.units ?? {})) {
    inspect(u.note, `units.${slug}.note`);
  }

  dataGaps = plainEN.length + missingTR.length;
  if (plainEN.length) {
    console.log(
      `counters.json: ${plainEN.length} EN-only prose fields (not localized — rendered as English on /tr/counters):`,
    );
    for (const p of plainEN.slice(0, 10)) console.log(`  - ${p}`);
    if (plainEN.length > 10) console.log(`  … and ${plainEN.length - 10} more`);
    console.log("");
  }
  if (missingTR.length) {
    console.log(`counters.json: ${missingTR.length} fields missing a tr value:`);
    for (const p of missingTR) console.log(`  - ${p}`);
    console.log("");
  }
}

// ---------- verdict ----------
if (missing.length || untranslated.length || dataGaps) {
  console.log("Run `/translate <en-file>` on content gaps (worst-offending pages first).");
  if (dataGaps)
    console.log("counters.json prose needs the data-localization pass (see checklist Epic 9).");
  process.exit(1);
}
console.log("All TR content present, distinct from EN, and data prose localized. ✅");
