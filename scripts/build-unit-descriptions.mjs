#!/usr/bin/env node
// scripts/build-unit-descriptions.mjs
//
// Sources the bilingual text of src/content/units/<slug>.yaml from the official
// in-game unit help strings, mirroring how build-civilizations.mjs sources civ
// text. Reads .cache/aoe2-data/{data.json, strings-en.json, strings-tr.json}
// (populated by `pnpm sync:game-data`) and rewrites two YAML fields per unit:
//
//   description: {en, tr}   the help-text body ("Siege Weapon with ranged blast
//                           attack ... Strong vs. ... Weak vs. ...")
//   upgrades:    {en, tr}   the trailing "Upgrades: ..." line, label stripped
//
// All other YAML fields (slug, name, role, civ, line, lineRank) pass through
// untouched. Any missing or unparseable source string fails the run — never
// emit a default.
//
// Help-string convention (verified against aoe2techtree data):
//   LanguageHelpId = LanguageNameId + 21000
//   "Create <b>Name</b> (‹cost›)<br>\n{body}<br>\n<i>Upgrades: ...</i>‹DEFAULT›..."
//
// Sources:
//   SiegeEngineers/aoe2techtree  MIT code / MS Game Content Usage Rules assets
//   (data.json unit ids + EN/TR locale strings)

import { readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import yaml from "js-yaml";

const CACHE_DIR = path.resolve(".cache/aoe2-data");
const CONTENT_UNITS = path.resolve("src/content/units");

const UPGRADES_LABELS = { en: "Upgrades:", tr: "Yükseltmeler:" };

function fail(msg) {
  console.error(`[build-unit-descriptions] ${msg}`);
  process.exit(1);
}

// Same normalization as build-icon-map.mjs — game display name → content slug.
function slugify(name) {
  return String(name)
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[̀-ͯ]/g, "")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

function stripTags(s) {
  return s
    .replace(/<br>/g, " ")
    .replace(/<[^>]+>/g, "")
    .replace(/\s+/g, " ")
    .trim();
}

// "Create <b>Name</b> (‹cost›)<br>\n{body}<br>\n<i>Upgrades: ...</i>‹DEFAULT›..."
// → { name, body, upgrades } — upgrades is null when the string has no <i> part.
function parseHelp(raw, lang, context) {
  const cut = raw.split("‹DEFAULT›")[0];
  const bold = cut.match(/<b>(.+?)<\/b>/);
  if (!bold) fail(`${context}: no <b>name</b> in ${lang} help string: ${raw.slice(0, 80)}`);
  const firstBr = cut.indexOf("<br>");
  if (firstBr === -1) fail(`${context}: no <br> after title line in ${lang} help string`);
  let rest = cut.slice(firstBr + 4);
  let upgrades = null;
  const italic = rest.match(/<i>([\s\S]*?)<\/i>/);
  if (italic) {
    upgrades = stripTags(italic[1]);
    const label = UPGRADES_LABELS[lang];
    if (!upgrades.startsWith(label)) {
      fail(
        `${context}: ${lang} upgrades line does not start with "${label}": ${upgrades.slice(0, 60)}`,
      );
    }
    upgrades = upgrades.slice(label.length).trim();
    rest = rest.slice(0, italic.index);
  }
  const body = stripTags(rest);
  if (!body) fail(`${context}: empty ${lang} help body`);
  return { name: bold[1], body, upgrades };
}

async function loadJson(file) {
  const p = path.join(CACHE_DIR, file);
  try {
    return JSON.parse(await readFile(p, "utf8"));
  } catch (e) {
    fail(`cannot read ${p} (run \`pnpm sync:game-data\` first): ${e.message}`);
  }
}

async function run() {
  const [data, stringsEn, stringsTr] = await Promise.all([
    loadJson("data.json"),
    loadJson("strings-en.json"),
    loadJson("strings-tr.json"),
  ]);

  // slug → sorted candidate list of {id, helpEn, helpTr}; keyed by the EN name
  // embedded in the help string itself (LanguageNameId has no direct EN string).
  const candidates = new Map();
  for (const [id, unit] of Object.entries(data.data.Unit)) {
    const helpId = String(unit.LanguageNameId + 21000);
    const helpEn = stringsEn[helpId];
    if (!helpEn) continue; // not every game object has a help string; only our 90 must
    const bold = helpEn.match(/<b>(.+?)<\/b>/);
    if (!bold) continue;
    const slug = slugify(bold[1]);
    if (!candidates.has(slug)) candidates.set(slug, []);
    candidates.get(slug).push({ id: Number(id), helpEn, helpTr: stringsTr[helpId] });
  }
  for (const list of candidates.values()) list.sort((a, b) => a.id - b.id);

  const files = (await readdir(CONTENT_UNITS)).filter((f) => /\.ya?ml$/.test(f)).sort();
  let updated = 0;

  for (const file of files) {
    const slug = file.replace(/\.ya?ml$/, "");
    const list = candidates.get(slug);
    if (!list) fail(`${slug}: no unit in data.json whose EN help name slugifies to this`);
    // Multiple game ids can share one display name (e.g. task variants). That is
    // only safe when their help text is identical — otherwise picking one would
    // be a silent guess.
    const distinct = new Set(list.map((c) => c.helpEn));
    if (distinct.size > 1) {
      fail(
        `${slug}: ${list.length} ids (${list.map((c) => c.id).join(", ")}) with differing help text — resolve explicitly`,
      );
    }
    const { helpEn, helpTr, id } = list[0];
    if (!helpTr) fail(`${slug}: EN help exists (id ${id}) but TR string is missing`);

    const en = parseHelp(helpEn, "en", slug);
    const tr = parseHelp(helpTr, "tr", slug);
    if ((en.upgrades === null) !== (tr.upgrades === null)) {
      fail(`${slug}: upgrades line present in one language but not the other`);
    }

    const fmPath = path.join(CONTENT_UNITS, file);
    const fm = yaml.load(await readFile(fmPath, "utf8"));
    const next = {
      slug: fm.slug,
      name: fm.name,
      role: fm.role,
      ...(fm.civ !== undefined && { civ: fm.civ }),
      ...(fm.line !== undefined && { line: fm.line }),
      ...(fm.lineRank !== undefined && { lineRank: fm.lineRank }),
      description: { en: en.body, tr: tr.body },
      ...(en.upgrades !== null && { upgrades: { en: en.upgrades, tr: tr.upgrades } }),
    };
    await writeFile(fmPath, yaml.dump(next, { lineWidth: 120 }), "utf8");
    updated++;
  }

  console.log(`Done. ${updated} unit YAML files sourced from in-game help strings.`);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
