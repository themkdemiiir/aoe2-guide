#!/usr/bin/env node
// scripts/build-civilizations.mjs
// Reads .cache/aoe2-data/{civilizations.csv, data.json, strings-en.json, strings-tr.json}.
// Writes:
//   src/data/civilizations.json              (EN data for every civ in data.json — 53)
//   src/content/civilizations/<slug>.yaml    (one bilingual {en,tr} file per civ; always regenerated)
//
// Sources:
//   SiegeEngineers/aoe2techtree (MIT) — data.json civ list + EN/TR help strings parsed by
//     parseHelp() for bonuses, team bonus, unique-tech effects, the civ archetype, AND the
//     unique unit name(s) (uniqueUnitNames → slugified into entry.uniqueUnits).
//   aalises/age-of-empires-II-api (BSD-3-Clause) — region/expansion per civ.
//   Only editorial copy remains hand-written: the EN/TR region labels and tagline templates.
//   `strategy` prose is carried forward verbatim from the existing per-civ YAML.

import { mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import yaml from "js-yaml";
import { parseHelp } from "./lib/parse-help.mjs";

const CACHE_DIR = path.resolve(".cache/aoe2-data");
const DATA_OUT = path.resolve("src/data/civilizations.json");
const CONTENT_CIVS = path.resolve("src/content/civilizations");
const ICON_MAP = path.resolve("src/data/icon-map.json");

// ---------------------------------------------------------------------------
// Slugify helpers
// ---------------------------------------------------------------------------
function slugify(str) {
  return str
    .toLowerCase()
    .replace(/['']/g, "")
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

// ---------------------------------------------------------------------------
// CSV parser (minimal — handles quoted fields with semicolons inside)
// ---------------------------------------------------------------------------
function parseCsv(text) {
  const lines = text.split("\n").filter(Boolean);
  const rawHeader = lines[0];
  const headers = rawHeader.split(",").map((h) => h.trim());
  const rows = [];
  for (let i = 1; i < lines.length; i++) {
    const line = lines[i].trim();
    if (!line) continue;
    // Split on commas but respect quoted fields
    const cols = splitCsvLine(line);
    const row = {};
    headers.forEach((h, idx) => {
      row[h] = (cols[idx] || "").trim();
    });
    rows.push(row);
  }
  return rows;
}

function splitCsvLine(line) {
  const result = [];
  let cur = "";
  let inQuote = false;
  for (let i = 0; i < line.length; i++) {
    const ch = line[i];
    if (ch === '"') {
      inQuote = !inQuote;
    } else if (ch === "," && !inQuote) {
      result.push(cur);
      cur = "";
    } else {
      cur += ch;
    }
  }
  result.push(cur);
  return result;
}

// ---------------------------------------------------------------------------
// Region mapping from expansion / army_type
// ---------------------------------------------------------------------------
const REGION_MAP = {
  "Age of Kings": "Medieval European",
  "The Conquerors": "Americas / Asian",
  "Forgotten Empires": "Mediterranean / Asian",
  "African Kingdoms": "African",
  "Rise of Rajas": "Southeast Asian",
  "The Last Khans": "Central Asian / Eastern European",
  "Lords of the West": "Western European",
  "Dawn of the Dukes": "Eastern European",
  "Dynasties of India": "South Asian",
  "Return of Rome": "Ancient Mediterranean",
  "The Mountain Royals": "Middle Eastern / Caucasian",
  "Victors and Vanquished": "Various",
};

// Override region per-civ for accuracy
const REGION_OVERRIDE = {
  aztecs: "Mesoamerican",
  mayans: "Mesoamerican",
  incas: "South American",
  huns: "Central Asian",
  mongols: "East Asian",
  chinese: "East Asian",
  japanese: "East Asian",
  koreans: "East Asian",
  byzantines: "Eastern Mediterranean",
  persians: "Middle Eastern",
  saracens: "Middle Eastern",
  turks: "Middle Eastern",
  teutons: "Central European",
  celts: "Western European",
  franks: "Western European",
  britons: "Western European",
  vikings: "Northern European",
  goths: "Northern European",
  slavs: "Eastern European",
  bulgarians: "Eastern European",
  bohemians: "Eastern European",
  poles: "Eastern European",
  lithuanians: "Eastern European",
  cumans: "Central Asian",
  tatars: "Central Asian",
  berbers: "North African",
  malians: "West African",
  ethiopians: "East African",
  malay: "Southeast Asian",
  burmese: "Southeast Asian",
  khmer: "Southeast Asian",
  vietnamese: "Southeast Asian",
  italians: "Southern European",
  spanish: "Southern European",
  portuguese: "Southern European",
  sicilians: "Southern European",
  burgundians: "Western European",
  magyars: "Eastern European",
  hindustanis: "South Asian",
  dravidians: "South Asian",
  bengalis: "South Asian",
  gurjaras: "South Asian",
  georgians: "Caucasian",
  armenians: "Caucasian",
  mapuche: "South American",
  // newer DLC
  romans: "Ancient Mediterranean",
  shu: "East Asian",
  wei: "East Asian",
  wu: "East Asian",
  jurchens: "East Asian",
  khitans: "East Asian",
  tupi: "South American",
  muisca: "South American",
};

// ---------------------------------------------------------------------------
// Parse aalises civilizations.csv
// ---------------------------------------------------------------------------
async function loadAalises() {
  const text = await readFile(path.join(CACHE_DIR, "civilizations.csv"), "utf8");
  const rows = parseCsv(text);

  const civMap = {};
  for (const row of rows) {
    const name = row.name.trim();
    if (!name) continue;
    const slug = slugify(name === "Indians" ? "Hindustanis" : name);

    const rawUnique = row.unique_unit || "";
    const uniqueUnits = rawUnique
      .split(";")
      .map((u) => slugify(u.trim()))
      .filter(Boolean);

    const rawTech = row.unique_tech || "";
    const techs = rawTech
      .split(";")
      .map((t) => t.trim())
      .filter(Boolean);
    const castleTech = techs[0] || "";
    const imperialTech = techs[1] || "";

    const rawBonuses = row.civilization_bonus || "";
    const bonuses = rawBonuses
      .split(";")
      .map((b) => b.trim())
      .filter(Boolean);

    const teamBonus = (row.team_bonus || "").trim();
    const expansion = (row.expansion || "").trim();
    const armyType = (row.army_type || "").trim();

    civMap[slug] = {
      slug,
      displayName: name === "Indians" ? "Hindustanis" : name,
      expansion,
      armyType,
      uniqueUnits,
      castleTech,
      imperialTech,
      bonuses,
      teamBonus,
    };
  }
  return civMap;
}

// ---------------------------------------------------------------------------
// Load aoe2techtree data.json to get civ list and Tech name lookup
// ---------------------------------------------------------------------------
async function loadAoe2TT() {
  const text = await readFile(path.join(CACHE_DIR, "data.json"), "utf8");
  return JSON.parse(text);
}

// ---------------------------------------------------------------------------
// Main
// ---------------------------------------------------------------------------
async function run() {
  const [iconMapText, aalisesMap, aoe2ttData, stringsEnText, stringsTrText] = await Promise.all([
    readFile(ICON_MAP, "utf8"),
    loadAalises(),
    loadAoe2TT(),
    readFile(path.join(CACHE_DIR, "strings-en.json"), "utf8").catch(() => "{}"),
    readFile(path.join(CACHE_DIR, "strings-tr.json"), "utf8").catch(() => "{}"),
  ]);

  const iconMap = JSON.parse(iconMapText);
  const tt_civs = aoe2ttData.civs;
  const stringsEn = JSON.parse(stringsEnText);
  const stringsTr = JSON.parse(stringsTrText);

  // help_string_id per civ slug → re-source bonuses from locale strings.
  const helpIdBySlug = {};
  for (const [civName, civ] of Object.entries(tt_civs)) {
    if (civ?.help_string_id != null) helpIdBySlug[slugify(civName)] = civ.help_string_id;
  }

  // Build full civ list from aoe2techtree (it has the most complete set)
  const allCivSlugs = new Set();
  for (const civName of Object.keys(tt_civs)) {
    // aoe2techtree uses "Hindustanis" as internal_name but key is "Hindustanis"
    const slug = slugify(civName);
    allCivSlugs.add(slug);
  }
  // Also include anything from aalises that might be missing
  for (const slug of Object.keys(aalisesMap)) {
    allCivSlugs.add(slug);
  }

  console.log(`Total unique civ slugs: ${allCivSlugs.size}`);

  // Load existing civilizations.json only to preserve the patch field. uniqueUnits are now
  // sourced from each civ's EN help string (see below) — no carryover, no icon-map filtering.
  let existingData = { patch: "v100.1.84", civs: [] };
  try {
    const existing = await readFile(DATA_OUT, "utf8");
    existingData = JSON.parse(existing);
  } catch (_) {}
  const iconUnits = iconMap.units || {};

  await mkdir(CONTENT_CIVS, { recursive: true });

  const civEntries = [];
  const iconMissing = []; // { slug, unit } for unique units with no icon-map entry (warn only)
  let written = 0;

  // Process civs in sorted order
  for (const slug of [...allCivSlugs].sort()) {
    // Check if slug is in icon-map for civs
    if (iconMap.civs && !iconMap.civs[slug]) {
      console.warn(`[WARN] civ "${slug}" not in icon-map civs — skipping`);
      continue;
    }

    const aalises = aalisesMap[slug];

    // Determine display name
    let displayName = slug.charAt(0).toUpperCase() + slug.slice(1);
    // For multi-word slugs, title-case each word
    displayName = slug
      .split("-")
      .map((w) => w.charAt(0).toUpperCase() + w.slice(1))
      .join(" ");

    // Base entry: region falls back to the expansion map. uniqueUnits are sourced from the EN
    // help string below (see parseHelp().uniqueUnitNames). Bonuses, team bonus, unique-tech
    // effects, and specialty are all (re-)sourced from parseHelp below.
    const region = REGION_OVERRIDE[slug] || REGION_MAP[aalises?.expansion] || "Unknown";
    const entry = {
      slug,
      region,
      specialty: aalises?.armyType ?? "Unknown",
      uniqueUnits: [],
      civBonuses: [],
      teamBonus: "",
      uniqueTechs: {
        castle: { name: "", effect: "" },
        imperial: { name: "", effect: "" },
      },
    };

    // Source bonuses + team + unique techs + specialty from the aoe2techtree help strings,
    // in BOTH languages. EN feeds civilizations.json (validate-data expects EN strings);
    // TR is carried on entry.tr for the bilingual YAML output.
    const helpId = helpIdBySlug[entry.slug];
    const en =
      helpId != null ? parseHelp(stringsEn[helpId] ?? stringsEn[String(helpId)], "en") : null;
    const tr =
      helpId != null ? parseHelp(stringsTr[helpId] ?? stringsTr[String(helpId)], "tr") : null;

    // NO DEFAULT VALUES (owner directive): every civ fact must be source-derived. Fail loud
    // instead of silently substituting an EN-in-TR fallback or an empty string. Each AoE2 civ
    // has EN+TR help and exactly 2 unique techs (castle + imperial); if any of that is missing,
    // the source/cache is wrong — stop the build rather than emit a default.
    if (!en) {
      console.error(`[FATAL] ${entry.slug}: no EN help string (help_string_id=${helpId})`);
      process.exit(1);
    }
    if (!tr) {
      console.error(`[FATAL] ${entry.slug}: no TR help string (help_string_id=${helpId})`);
      process.exit(1);
    }
    if (en.uniqueTechs.length < 2) {
      console.error(
        `[FATAL] ${entry.slug}: EN help has ${en.uniqueTechs.length} unique tech(s), expected 2`,
      );
      process.exit(1);
    }
    if (tr.uniqueTechs.length < 2) {
      console.error(
        `[FATAL] ${entry.slug}: TR help has ${tr.uniqueTechs.length} unique tech(s), expected 2`,
      );
      process.exit(1);
    }

    // Unique units: UNION of two trusted sources, because neither is complete on its own:
    //   - aalises civilizations.csv `unique_unit` (aalises.uniqueUnits, already slugified): the
    //     in-game help omits secondary unique units for some civs (e.g. Incas' Slinger).
    //   - the EN help string's "Unique Unit(s)" section (en.uniqueUnitNames, slugified): the
    //     aalises CSV is stale for some civs (e.g. Persians missing Savar).
    // Dedupe by slug; preserve stable order — aalises entries first (in CSV order), then any
    // help-only slugs not already present. slugify() is the single normalizer so identical
    // names from either source collapse cleanly. No defaults: if the union is EMPTY, fail loud
    // rather than emit [] (which would silently hide a real unit) — and never fall back to the
    // removed hand-coded prior-file / icon-map-filtered carryover.
    // Source names whose slug differs from the canonical icon-map / content slug.
    const UNIT_SLUG_ALIASES = {
      boyars: "boyar", // aalises "Boyars" (Slavs)
      "imperial-camel": "imperial-camel-rider", // aalises "Imperial Camel" (Hindustanis)
    };
    const canon = (u) => UNIT_SLUG_ALIASES[u] ?? u;
    const helpUnits = (en.uniqueUnitNames ?? []).map(slugify).map(canon).filter(Boolean);
    const aalisesUnits = (aalises?.uniqueUnits ?? []).map(canon).filter(Boolean);
    const uniqueUnits = [];
    const seen = new Set();
    for (const u of [...aalisesUnits, ...helpUnits]) {
      if (seen.has(u)) continue;
      seen.add(u);
      uniqueUnits.push(u);
    }
    if (uniqueUnits.length === 0) {
      console.error(`[FATAL] ${entry.slug}: no unique unit from any source`);
      process.exit(1);
    }
    entry.uniqueUnits = uniqueUnits;
    // Icon cross-check (warn only): the unit name is the data we want; a missing icon is a
    // separate asset-sync concern and must NOT drop the slug.
    for (const u of uniqueUnits) {
      if (!iconUnits[u]) iconMissing.push({ slug: entry.slug, unit: u });
    }

    // EN civilizations.json fields stay EN (validate-data expects EN strings). Assign sourced
    // values directly — no `?? ""`, no cross-language fallback.
    entry.civBonuses = en.civBonuses;
    entry.teamBonus = en.teamBonus;
    entry.specialty = fixSpecialty(en.civType);
    entry.uniqueTechs = {
      castle: { name: en.uniqueTechs[0].name, effect: en.uniqueTechs[0].effect },
      imperial: { name: en.uniqueTechs[1].name, effect: en.uniqueTechs[1].effect },
    };
    // Localized payload for buildMarkdown (YAML output) — sourced directly from TR help.
    entry.tr = {
      civBonuses: tr.civBonuses,
      teamBonus: tr.teamBonus,
      civType: tr.civType,
      uniqueTechs: {
        castle: tr.uniqueTechs[0].effect,
        imperial: tr.uniqueTechs[1].effect,
      },
    };
    entry.regionNoun = REGION_NOUN[entry.region] ?? entry.region;

    civEntries.push(entry);

    // Carry only `strategy` prose forward from the existing bilingual YAML; everything
    // else is freshly sourced. Always regenerate — never skip; stale files were the root
    // cause of wrong content.
    const existing = await readExistingCivYaml(slug);
    const mdPath = path.join(CONTENT_CIVS, `${slug}.yaml`);
    const md = buildMarkdown(entry, displayName, existing);
    await writeFile(mdPath, md, "utf8");
    console.log(`  [WRITE] ${mdPath}`);
    written++;
  }

  // Write civilizations.json — strip the build-time-only `tr` payload (used by the YAML
  // output) so data.json keeps its EN-only shape.
  const output = {
    patch: existingData.patch || "v100.1.84",
    civs: civEntries.map(({ tr, ...rest }) => rest),
  };

  await writeFile(DATA_OUT, `${JSON.stringify(output, null, 2)}\n`, "utf8");

  // Icon cross-check summary: one line per civ→slug whose unique unit has no icon yet, so the
  // owner knows which unit assets to sync later. This never fails the build.
  for (const { slug, unit } of iconMissing) {
    console.log(`[ICON-MISSING] ${slug} → ${unit}`);
  }

  console.log(`\nDone.`);
  console.log(`  Civs in JSON:          ${civEntries.length}`);
  console.log(`  Content files written: ${written}`);
  console.log(`  Unique units missing icons: ${iconMissing.length}`);
}

// ---------------------------------------------------------------------------
// Markdown template
// ---------------------------------------------------------------------------
const REGION_NOUN = {
  "Ancient Mediterranean": "the Ancient Mediterranean",
  "Ancient Middle Eastern": "the Ancient Middle East",
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

// Turkish ablative region labels ("…dan/…den") for the TR tagline. Editorial geographic
// copy — the only hand-written civ strings that remain after sourcing from aoe2techtree.
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

const _titleWord = (w) => w.charAt(0).toUpperCase() + w.slice(1).toLowerCase();
function fixSpecialty(s) {
  let x = s
    .replace(/Calvary/gi, "Cavalry")
    .replace(/\s*\bCivilzation\b/gi, "")
    .trim();
  x = x
    .split(/\s+and\s+/i)
    .map((p) => p.split(/\s+/).map(_titleWord).join(" "))
    .join(" and ");
  x = x.replace(/\bArchers\b/g, "Archer");
  if (x === "Cavalry Infantry") x = "Cavalry and Infantry";
  return x;
}

async function readExistingCivYaml(slug) {
  try {
    const text = await readFile(path.join(CONTENT_CIVS, `${slug}.yaml`), "utf8");
    return { fm: yaml.load(text) };
  } catch (_) {
    return { fm: null };
  }
}

function buildMarkdown(entry, displayName, existing) {
  const { slug, region, regionNoun, specialty, civBonuses, teamBonus, uniqueTechs, tr } = entry;
  const place = regionNoun || region;
  const art = /^[aeiou]/i.test(specialty) ? "an" : "a";
  const taglineEn = `${displayName} — ${art} ${specialty} civilization from ${place}.`;
  const placeTr = REGION_NOUN_TR[region] ?? place;
  const taglineTr = `${displayName}, ${placeTr} bir ${tr.civType} uygarlığıdır.`;

  const ex = existing?.fm ?? {};
  const strategy = ex.strategy && (ex.strategy.en || ex.strategy.tr) ? ex.strategy : null;

  const fm = {
    slug,
    name: { en: displayName, tr: String(ex.name?.tr ?? displayName) },
    tagline: { en: taglineEn, tr: taglineTr },
    bonuses: { en: civBonuses, tr: tr.civBonuses },
    teamBonus: { en: teamBonus, tr: tr.teamBonus },
    uniqueTechs: {
      castle: {
        name: { en: uniqueTechs.castle.name, tr: uniqueTechs.castle.name },
        effect: { en: uniqueTechs.castle.effect, tr: tr.uniqueTechs.castle },
      },
      imperial: {
        name: { en: uniqueTechs.imperial.name, tr: uniqueTechs.imperial.name },
        effect: { en: uniqueTechs.imperial.effect, tr: tr.uniqueTechs.imperial },
      },
    },
    ...(strategy ? { strategy } : {}),
  };
  return yaml.dump(fm, { lineWidth: 120 });
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
