#!/usr/bin/env node
// One-shot migration (Phase 1, Epic 1 + 2): bring existing civ content into line
// with the re-sourced civilizations.json and the STYLE.md tagline template.
// The generator (build-civilizations.mjs) SKIPS existing md files, so these fixes
// can't ride a regen — this script edits the 53 EN civ md + the JSON directly.
//
// Does, idempotently:
//   civilizations.json — fix `specialty` typos/casing (Calvary→Cavalry, drop stray
//     "Civilzation", "Archers"→"Archer", lowercase→Title), add `regionNoun`.
//   each en/<civ>.md   — sync frontmatter `bonuses`/`teamBonus` from JSON (detail
//     pages render these and were STALE), rewrite `tagline` + intro to the canonical
//     "{Civ} — a/an {specialty} civilization from {regionNoun}." template, and strip
//     the four duplicate body sections (## Civ Bonuses / Team Bonus / Unique Units /
//     Unique Techs — already rendered structurally) while preserving "## How to Play".
//
// Usage: node scripts/fix-civ-content.mjs [--dry]

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const DRY = process.argv.includes("--dry");
const DATA = "src/data/civilizations.json";
const EN_DIR = "src/content/civilizations/en";

// Region adjective → noun (geography facts, not assumptions).
const REGION_NOUN = {
  "Ancient Mediterranean": "the Ancient Mediterranean",
  Caucasian: "the Caucasus",
  "Central Asian": "Central Asia",
  "Central European": "Central Europe",
  "East African": "East Africa",
  "East Asian": "East Asia",
  "Eastern European": "Eastern Europe",
  "Eastern Mediterranean": "the Eastern Mediterranean",
  Mesoamerican: "Mesoamerica",
  "Middle Eastern": "the Middle East",
  "North African": "North Africa",
  "Northern European": "Northern Europe",
  "South American": "South America",
  "South Asian": "South Asia",
  "Southeast Asian": "Southeast Asia",
  "Southern European": "Southern Europe",
  "West African": "West Africa",
  "Western European": "Western Europe",
};

const titleWord = (w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase();
function fixSpecialty(s) {
  let x = s
    .replace(/Calvary/gi, "Cavalry")
    .replace(/\s*\bCivilzation\b/gi, "")
    .trim();
  x = x
    .split(/\s+and\s+/i)
    .map((p) => p.split(/\s+/).map(titleWord).join(" "))
    .join(" and ");
  x = x.replace(/\bArchers\b/g, "Archer");
  if (x === "Cavalry Infantry") x = "Cavalry and Infantry";
  return x;
}
const article = (s) => (/^[aeiou]/i.test(s) ? "an" : "a");
const esc = (s) => s.replace(/"/g, '\\"');

// ---------- PART A: civilizations.json ----------
const data = JSON.parse(readFileSync(DATA, "utf8"));
const bySlug = {};
let specFixes = 0;
for (const civ of data.civs) {
  const fixed = fixSpecialty(civ.specialty);
  if (fixed !== civ.specialty) {
    console.log(`  specialty: ${civ.slug}: "${civ.specialty}" → "${fixed}"`);
    civ.specialty = fixed;
    specFixes++;
  }
  const regionNoun = REGION_NOUN[civ.region];
  if (!regionNoun) throw new Error(`No regionNoun mapping for "${civ.region}" (${civ.slug})`);
  civ.regionNoun = regionNoun;
  bySlug[civ.slug] = civ;
}
console.log(
  `civilizations.json: ${specFixes} specialty fixes, regionNoun set on ${data.civs.length} civs`,
);
if (!DRY) writeFileSync(DATA, `${JSON.stringify(data, null, 2)}\n`, "utf8");

// ---------- PART B: en/<civ>.md ----------
let mdChanged = 0;
const SECTIONS = /^## (Civ Bonuses|Team Bonus|Unique Units|Unique Techs)\b/;
for (const file of readdirSync(EN_DIR).filter((f) => f.endsWith(".md"))) {
  const slug = file.replace(/\.md$/, "");
  const civ = bySlug[slug];
  if (!civ) {
    console.warn(`  [skip] ${slug}: not in civilizations.json`);
    continue;
  }
  const raw = readFileSync(join(EN_DIR, file), "utf8");
  const m = raw.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
  if (!m) {
    console.warn(`  [skip] ${slug}: no frontmatter`);
    continue;
  }
  const name = (m[1].match(/^name:\s*"?(.+?)"?\s*$/m) || [])[1] || slug;
  const spec = civ.specialty;
  const region = civ.regionNoun;
  const art = article(spec);
  const tagline = `${name} — ${art} ${spec} civilization from ${region}.`;
  const intro = `${name} are ${art} ${spec} civilization from ${region}.`;

  // Frontmatter: rewrite tagline, sync bonuses + teamBonus from JSON.
  const fmLines = m[1].split("\n");
  const fm = [];
  for (let i = 0; i < fmLines.length; i++) {
    const line = fmLines[i];
    if (/^tagline:/.test(line)) {
      fm.push(`tagline: "${esc(tagline)}"`);
    } else if (/^bonuses:/.test(line)) {
      fm.push("bonuses:");
      for (const b of civ.civBonuses) fm.push(`  - "${esc(b)}"`);
      while (i + 1 < fmLines.length && /^\s+-\s/.test(fmLines[i + 1])) i++;
    } else if (/^teamBonus:/.test(line)) {
      fm.push(`teamBonus: "${esc(civ.teamBonus)}"`);
    } else {
      fm.push(line);
    }
  }

  // Body: rewrite intro, strip the four duplicate sections (keep everything from the
  // first non-duplicate heading — i.e. "## How to Play" — onward).
  const bodyLines = m[2].split("\n");
  const introIdx = bodyLines.findIndex((l) => l.trim() !== "");
  if (introIdx >= 0) bodyLines[introIdx] = intro;
  const firstDup = bodyLines.findIndex((l) => SECTIONS.test(l));
  let kept = bodyLines;
  if (firstDup >= 0) {
    const resume = bodyLines.findIndex((l, idx) => idx > firstDup && /^## How to Play/.test(l));
    kept =
      resume >= 0
        ? [...bodyLines.slice(0, firstDup), ...bodyLines.slice(resume)]
        : bodyLines.slice(0, firstDup);
  }
  const body = kept
    .join("\n")
    .replace(/\n{3,}/g, "\n\n")
    .replace(/^\n+/, "");

  const next = `---\n${fm.join("\n")}\n---\n\n${body.replace(/\s+$/, "")}\n`;
  if (next !== raw) {
    mdChanged++;
    if (!DRY) writeFileSync(join(EN_DIR, file), next, "utf8");
  }
}
console.log(`en civ md: ${mdChanged} files ${DRY ? "would change" : "updated"}`);
console.log(DRY ? "\n(dry run — nothing written)" : "\nDone.");
