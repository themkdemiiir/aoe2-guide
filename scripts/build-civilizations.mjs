#!/usr/bin/env node
// scripts/build-civilizations.mjs
//
// THIN WRAPPER — the aoe2techtree/aalises PARSING this script used to do (data.json civ list,
// EN/TR help-string extraction via parse-help.mjs, aalises civilizations.csv region/unique-unit
// lookups) has all moved to Rust: pipeline/crates/refdata/src/{civs,civ_help,civ_region,
// civilizations}.rs. This script now only reads the two JSONs that Rust binary produces and merges
// them into Astro content — no source-of-truth parsing happens here anymore. See
// pipeline/crates/refdata/src/civilizations.rs's module doc for the authoritative wrapper contract
// this file implements (repeated in brief below).
//
// Prerequisite: `cd pipeline && cargo run --bin refdata -- --out <DIR>` (see that crate's README/
// task report for the full VM checklist). Point this script at `<DIR>` via the REFDATA_OUT env var
// (defaults to `.cache/refdata-out`).
//
// Reads REFDATA_OUT/{civilizations.json, civ-help-strings.json}.
// Writes:
//   src/data/civilizations.json           (copied through byte-for-byte from Rust's output — it
//                                           already matches the committed shape field-for-field)
//   src/content/civilizations/<slug>.yaml (one bilingual {en,tr} file per civ; always regenerated)
//
// What THIS script still owns (deliberately NOT ported to Rust — editorial/templating, not
// aoe2techtree source-of-truth facts):
//   - EN/TR tagline sentence templates (REGION_NOUN_TR below is the only remaining hand-written
//     table; the EN region-noun equivalent is already IN civilizations.json's `regionNoun` field).
//   - `name.tr` and `strategy` prose — carried forward from the EXISTING committed YAML file, never
//     regenerated (a human writes/edits these directly in the YAML).
//   - The icon-map cross-check warning (diagnostic only, never fails the build).
//
// IMPORTANT: `uniqueTechs.*.name` in the YAML is ALWAYS civilizations.json's EN name, for BOTH
// `en` and `tr` — never civ-help-strings.json's `tr.uniqueTechs[*].name`. Unique-tech proper nouns
// are allow-listed to stay EN-only site-wide (see this repo's CLAUDE.md / audit-yaml-translations).

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import yaml from "js-yaml";

const REFDATA_OUT = path.resolve(process.env.REFDATA_OUT || ".cache/refdata-out");
const DATA_OUT = path.resolve("src/data/civilizations.json");
const CONTENT_CIVS = path.resolve("src/content/civilizations");
const ICON_MAP = path.resolve("src/data/icon-map.json");

function fail(msg) {
  console.error(`[build-civilizations] ${msg}`);
  process.exit(1);
}

async function readJson(dir, file) {
  const p = path.join(dir, file);
  try {
    return JSON.parse(await readFile(p, "utf8"));
  } catch (e) {
    fail(
      `cannot read ${p} (run \`cd pipeline && cargo run --bin refdata -- --out ${dir}\` first): ${e.message}`,
    );
  }
}

// Turkish ablative region labels ("…dan/…den") for the TR tagline. Editorial geographic copy — the
// only hand-written civ string that remains after sourcing everything else from aoe2techtree (via
// Rust) — mirrors civilizations.json's own EN `regionNoun` field, just in Turkish.
const REGION_NOUN_TR = {
  "Ancient Mediterranean": "Antik Akdeniz'den",
  "Ancient Middle Eastern": "Antik Orta Doğu'dan",
  Caucasian: "Kafkasya'dan",
  "Central Asian": "Orta Asya'dan",
  "Central European": "Orta Avrupa'dan",
  "East African": "Doğu Afrika'dan",
  "East Asian": "Doğu Asya'dan",
  "Eastern European": "Doğu Avrupa'dan",
  "Eastern Mediterranean": "Doğu Akdeniz'den",
  Mesoamerican: "Mezoamerika'dan",
  "Middle Eastern": "Orta Doğu'dan",
  "North African": "Kuzey Afrika'dan",
  "Northern European": "Kuzey Avrupa'dan",
  "South American": "Güney Amerika'dan",
  "South Asian": "Güney Asya'dan",
  "Southeast Asian": "Güneydoğu Asya'dan",
  "Southern European": "Güney Avrupa'dan",
  "West African": "Batı Afrika'dan",
  "Western European": "Batı Avrupa'dan",
};

async function readExistingCivYaml(slug) {
  try {
    const text = await readFile(path.join(CONTENT_CIVS, `${slug}.yaml`), "utf8");
    return { fm: yaml.load(text) };
  } catch (_) {
    return { fm: null };
  }
}

function buildMarkdown(entry, help, displayName, existing) {
  const { slug, region, regionNoun, specialty, civBonuses, teamBonus, uniqueTechs } = entry;
  const place = regionNoun || region;
  const art = /^[aeiou]/i.test(specialty) ? "an" : "a";
  const taglineEn = `${displayName} — ${art} ${specialty} civilization from ${place}.`;
  const placeTr = REGION_NOUN_TR[region] ?? place;
  const taglineTr = `${displayName}, ${placeTr} bir ${help.tr.civType} uygarlığıdır.`;

  const ex = existing?.fm ?? {};
  const strategy = ex.strategy && (ex.strategy.en || ex.strategy.tr) ? ex.strategy : null;

  // help.tr.uniqueTechs is always [castle, imperial] (refdata's civ_help::require_parsed enforces
  // exactly 2) — same index convention civilizations.json's own uniqueTechs.castle/imperial uses.
  const [trCastleTech, trImperialTech] = help.tr.uniqueTechs;

  const fm = {
    slug,
    name: { en: displayName, tr: String(ex.name?.tr ?? displayName) },
    tagline: { en: taglineEn, tr: taglineTr },
    bonuses: { en: civBonuses, tr: help.tr.civBonuses },
    teamBonus: { en: teamBonus, tr: help.tr.teamBonus },
    uniqueTechs: {
      castle: {
        name: { en: uniqueTechs.castle.name, tr: uniqueTechs.castle.name },
        effect: { en: uniqueTechs.castle.effect, tr: trCastleTech.effect },
      },
      imperial: {
        name: { en: uniqueTechs.imperial.name, tr: uniqueTechs.imperial.name },
        effect: { en: uniqueTechs.imperial.effect, tr: trImperialTech.effect },
      },
    },
    ...(strategy ? { strategy } : {}),
  };
  return yaml.dump(fm, { lineWidth: 120 });
}

async function run() {
  const [civilizations, helpStrings, iconMapText] = await Promise.all([
    readJson(REFDATA_OUT, "civilizations.json"),
    readJson(REFDATA_OUT, "civ-help-strings.json"),
    readFile(ICON_MAP, "utf8"),
  ]);
  const iconUnits = JSON.parse(iconMapText).units || {};

  await mkdir(CONTENT_CIVS, { recursive: true });

  const iconMissing = []; // { slug, unit } — warn only, never fails the build.
  let written = 0;

  for (const entry of civilizations.civs) {
    const { slug } = entry;
    const help = helpStrings.civs[slug];
    if (!help)
      fail(`${slug}: no civ-help-strings.json entry (out of sync with civilizations.json)`);

    const displayName = slug
      .split("-")
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(" ");

    for (const unit of entry.uniqueUnits) {
      if (!iconUnits[unit]) iconMissing.push({ slug, unit });
    }

    const existing = await readExistingCivYaml(slug);
    const mdPath = path.join(CONTENT_CIVS, `${slug}.yaml`);
    const md = buildMarkdown(entry, help, displayName, existing);
    await writeFile(mdPath, md, "utf8");
    console.log(`  [WRITE] ${mdPath}`);
    written++;
  }

  // civilizations.json already matches the committed shape field-for-field (refdata's own
  // write_json uses the identical 2-space-indent + trailing-newline convention) — copy it through
  // unchanged rather than re-serializing.
  await writeFile(
    DATA_OUT,
    await readFile(path.join(REFDATA_OUT, "civilizations.json"), "utf8"),
    "utf8",
  );

  for (const { slug, unit } of iconMissing) {
    console.log(`[ICON-MISSING] ${slug} → ${unit}`);
  }

  console.log(`\nDone.`);
  console.log(`  Civs in JSON:          ${civilizations.civs.length}`);
  console.log(`  Content files written: ${written}`);
  console.log(`  Unique units missing icons: ${iconMissing.length}`);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
