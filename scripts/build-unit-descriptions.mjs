#!/usr/bin/env node
// scripts/build-unit-descriptions.mjs
//
// THIN WRAPPER — the aoe2techtree help-string PARSING this script used to do (candidate resolution
// by slugified bold name, <b>/<br>/<i>Upgrades:> extraction) has moved to Rust:
// pipeline/crates/refdata/src/unit_help.rs, which reuses techtree::TechTree::resolve (the SAME
// resolver unit_stats.rs uses for numeric stats) rather than a second aoe2techtree parser. This
// script now only reads unit-descriptions.json and rewrites two YAML fields per unit:
//
//   description: {en, tr}   the help-text body ("Siege Weapon with ranged blast
//                           attack ... Strong vs. ... Weak vs. ...")
//   upgrades:    {en, tr}   the trailing "Upgrades: ..." line, label stripped (OMITTED from the
//                           YAML entirely when the unit's help string has no such line)
//
// All other YAML fields (slug, name, role, civ, line, lineRank) pass through untouched, exactly as
// before. Any unit missing from unit-descriptions.json fails the run — never emits a default.
//
// Prerequisite: `cd pipeline && cargo run --bin refdata -- --out <DIR>` (see that crate's task
// report for the full VM checklist). Point this script at `<DIR>` via the REFDATA_OUT env var
// (defaults to `.cache/refdata-out`).

import { readdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";
import yaml from "js-yaml";

const REFDATA_OUT = path.resolve(process.env.REFDATA_OUT || ".cache/refdata-out");
const CONTENT_UNITS = path.resolve("src/content/units");

function fail(msg) {
  console.error(`[build-unit-descriptions] ${msg}`);
  process.exit(1);
}

async function readUnitDescriptions() {
  const p = path.join(REFDATA_OUT, "unit-descriptions.json");
  let text;
  try {
    text = await readFile(p, "utf8");
  } catch (e) {
    fail(`cannot read ${p} (run \`cd pipeline && cargo run --bin refdata -- --out ${REFDATA_OUT}\` first): ${e.message}`);
  }
  return JSON.parse(text).units;
}

async function run() {
  const descriptions = await readUnitDescriptions();

  const files = (await readdir(CONTENT_UNITS)).filter((f) => /\.ya?ml$/.test(f)).sort();
  let updated = 0;

  for (const file of files) {
    const slug = file.replace(/\.ya?ml$/, "");
    const entry = descriptions[slug];
    if (!entry) {
      fail(`${slug}: no unit-descriptions.json entry (out of sync with src/content/units/ — check refdata's CANONICAL_UNITS)`);
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
      description: entry.description,
      ...(entry.upgrades != null && { upgrades: entry.upgrades }),
    };
    await writeFile(fmPath, yaml.dump(next, { lineWidth: 120 }), "utf8");
    updated++;
  }

  console.log(`Done. ${updated} unit YAML files sourced from unit-descriptions.json.`);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
