#!/usr/bin/env node
// One-time migration: merge EN+TR locale-pair MD files → single bilingual YAML.
// Handles: maps, glossary, units
// Run: node scripts/migrate-locale-pairs.mjs
// After verifying output, delete the en/ and tr/ subdirs.

import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import yaml from "js-yaml";

function splitFrontmatter(raw) {
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$/);
  if (!m) throw new Error("No frontmatter found");
  return { fm: yaml.load(m[1]), body: m[2].trim() };
}

function mergeTeamComps(enComps, trComps) {
  if (!enComps) return undefined;
  const result = {};
  for (const format of ["2v2", "4v4"]) {
    const enFmt = enComps[format];
    if (!enFmt) continue;
    const trFmt = trComps?.[format] ?? {};
    result[format] = {};
    for (const slot of ["flank", "pocket"]) {
      const enSlot = enFmt[slot];
      if (!enSlot) continue;
      const trSlot = trFmt[slot] ?? {};
      result[format][slot] = {
        civs: [...(enSlot.civs ?? [])],
        strategy: {
          en: String(enSlot.strategy ?? ""),
          tr: String(trSlot.strategy ?? enSlot.strategy ?? ""),
        },
      };
    }
  }
  return result;
}

function migrate(collection, mergeData) {
  const enDir = `src/content/${collection}/en`;
  const trDir = `src/content/${collection}/tr`;
  const outDir = `src/content/${collection}`;
  const files = readdirSync(enDir).filter((f) => f.endsWith(".md"));
  let count = 0;
  for (const fname of files) {
    const enRaw = readFileSync(join(enDir, fname), "utf8");
    const trPath = join(trDir, fname);
    const trRaw = existsSync(trPath) ? readFileSync(trPath, "utf8") : enRaw;
    const en = splitFrontmatter(enRaw);
    const tr = splitFrontmatter(trRaw);
    const merged = mergeData(en, tr);
    const out = yaml.dump(merged, { lineWidth: 120, quotingType: '"', forceQuotes: false });
    writeFileSync(join(outDir, fname.replace(/\.md$/, ".yaml")), out, "utf8");
    count++;
    console.log(`  ✓  ${collection}/${fname.replace(".md", ".yaml")}`);
  }
  console.log(`  → ${count} files\n`);
}

// ── Maps ────────────────────────────────────────────────────────────────────
migrate("maps", ({ fm: en, body: enBody }, { fm: tr, body: trBody }) => ({
  slug: en.slug,
  name: { en: String(en.name), tr: String(tr.name ?? en.name) },
  type: en.type,
  ...(en.size ? { size: en.size } : {}),
  recommendedCivs: [...(en.recommendedCivs ?? [])],
  ...(en.teamComps
    ? { teamComps: mergeTeamComps(en.teamComps, tr.teamComps) }
    : {}),
  ...(enBody ? { body: { en: enBody, tr: trBody || enBody } } : {}),
}));

// ── Glossary ─────────────────────────────────────────────────────────────────
migrate("glossary", ({ fm: en, body: enBody }, { fm: tr, body: trBody }) => ({
  slug: en.slug,
  term: { en: String(en.term), tr: String(tr.term ?? en.term) },
  letter: String(en.letter),
  definition: { en: enBody, tr: trBody || enBody },
}));

// ── Units ─────────────────────────────────────────────────────────────────────
migrate("units", ({ fm: en, body: enBody }, { fm: tr, body: trBody }) => {
  const merged = {
    slug: en.slug,
    name: { en: String(en.name), tr: String(tr.name ?? en.name) },
    role: { en: String(en.role), tr: String(tr.role ?? en.role) },
  };
  if (en.civ) merged.civ = en.civ;
  if (en.line) merged.line = en.line;
  if (en.lineRank != null) merged.lineRank = en.lineRank;
  if (enBody) merged.description = { en: enBody, tr: trBody || enBody };
  return merged;
});

console.log("Migration complete.");
