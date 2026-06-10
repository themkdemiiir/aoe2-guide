#!/usr/bin/env node
// Build-time data validation (Epic 10): schema + typo denylist over civilizations.json.
// Guards the bug classes fixed in Phase 1 so a future regen can't silently reintroduce
// them: Calvary/Civilzation-class typos, missing fields, lowercase specialty words,
// missing regionNoun (which is what made taglines read "from Eastern European").
// Wired into `prebuild`, so a bad regen fails the build/CI.

import { readFileSync } from "node:fs";
import * as v from "valibot";

const FILE = "src/data/civilizations.json";
const REGIONS = [
  "Ancient Mediterranean",
  "Ancient Middle Eastern",
  "Caucasian",
  "Central Asian",
  "Central European",
  "East African",
  "East Asian",
  "Eastern European",
  "Eastern Mediterranean",
  "Mesoamerican",
  "Middle Eastern",
  "North African",
  "Northern European",
  "South American",
  "South Asian",
  "Southeast Asian",
  "Southern European",
  "West African",
  "Western European",
];

const TYPO = /\b(Calvary|Civilzation)\b/i;
const noTypo = (msg) => v.check((s) => !TYPO.test(s), msg);

const UniqueTech = v.object({ name: v.string(), effect: v.string() });
const CivSchema = v.object({
  slug: v.pipe(v.string(), v.regex(/^[a-z0-9-]+$/, "slug must be kebab-case")),
  region: v.picklist(REGIONS, "unknown region (add to REGION_NOUN if real)"),
  regionNoun: v.pipe(v.string(), v.minLength(1, "missing regionNoun")),
  specialty: v.pipe(
    v.string(),
    v.minLength(1, "missing specialty"),
    noTypo("specialty has a typo (Calvary/Civilzation)"),
    v.check(
      (s) =>
        s.split(/\s+and\s+/i).every((part) => part.split(/\s+/).every((w) => /^[A-Z]/.test(w))),
      "specialty words must be Title-cased (e.g. 'Naval' not 'naval')",
    ),
  ),
  uniqueUnits: v.array(v.string()),
  civBonuses: v.pipe(
    v.array(v.pipe(v.string(), noTypo("civ bonus has a typo"))),
    v.minLength(1, "needs at least one civ bonus"),
  ),
  teamBonus: v.pipe(v.string(), noTypo("teamBonus has a typo")),
  uniqueTechs: v.object({ castle: UniqueTech, imperial: UniqueTech }),
});

const data = JSON.parse(readFileSync(FILE, "utf8"));
let issueCount = 0;
for (const civ of data.civs) {
  const result = v.safeParse(CivSchema, civ, { abortEarly: false, abortPipeEarly: false });
  if (!result.issues) continue;
  for (const iss of result.issues) {
    const path = (iss.path ?? []).map((p) => p.key).join(".");
    console.error(`  ✗ ${civ.slug}${path ? `.${path}` : ""}: ${iss.message}`);
    issueCount++;
  }
}

// Belt-and-suspenders: scan the whole file for typos anywhere (not just modeled fields).
const rawTypos = [...readFileSync(FILE, "utf8").matchAll(/\b(Calvary|Civilzation)\b/gi)];
if (rawTypos.length) {
  console.error(`  ✗ ${rawTypos.length} raw typo occurrence(s) of Calvary/Civilzation in ${FILE}`);
  issueCount += rawTypos.length;
}

if (issueCount) {
  console.error(`\nvalidate-data: ${issueCount} issue(s) in ${FILE}`);
  process.exit(1);
}
console.log(`validate-data: ${data.civs.length} civs OK ✓`);
