#!/usr/bin/env node
// One-time migration: merge EN+TR build order MD pairs → single bilingual MD.
// Run: node scripts/migrate-builds-to-bilingual.mjs
// After verifying output, delete src/content/build-orders/en/ and tr/.

import { readdirSync, readFileSync, writeFileSync, existsSync } from "node:fs";
import { join } from "node:path";
import yaml from "js-yaml";

const EN_DIR = "src/content/build-orders/en";
const TR_DIR = "src/content/build-orders/tr";
const OUT_DIR = "src/content/build-orders";

function splitFrontmatter(raw) {
  const m = raw.match(/^---\r?\n([\s\S]*?)\r?\n---\r?\n([\s\S]*)$/);
  if (!m) throw new Error("No frontmatter found");
  return { fm: yaml.load(m[1]), body: m[2].trim() };
}

function parseBody(body) {
  const lines = body.split("\n");
  const content = lines.filter((l) => !l.startsWith("# ")).join("\n").trim();
  const h2 = content.indexOf("\n## ");
  const introPart = h2 !== -1 ? content.slice(0, h2).trim() : content;
  const stratPart = h2 !== -1 ? content.slice(h2 + 4) : "";
  const intro = introPart.split(/\n\n/)[0]?.replace(/\n/g, " ").trim() ?? "";
  const strategy = stratPart
    .split("\n")
    .filter((l) => l.startsWith("- "))
    .map((l) => l.slice(2).trim());
  return { intro, strategy };
}

function mergeSteps(enSteps, trSteps) {
  return enSteps.map((en, i) => {
    const tr = trSteps[i] ?? en;
    const step = {};
    if (en.villagers != null) step.villagers = en.villagers;
    if (en.phase) step.phase = en.phase;
    if (en.time) step.time = en.time;
    step.assign = { en: String(en.assign), tr: String(tr.assign ?? en.assign) };
    if (en.note || tr.note) {
      step.note = { en: String(en.note ?? ""), tr: String(tr.note ?? en.note ?? "") };
    }
    if (en.icons?.length) step.icons = [...en.icons];
    return step;
  });
}

const enFiles = readdirSync(EN_DIR).filter((f) => f.endsWith(".md"));
let count = 0;

for (const fname of enFiles) {
  const enRaw = readFileSync(join(EN_DIR, fname), "utf8");
  const trPath = join(TR_DIR, fname);
  const trRaw = existsSync(trPath) ? readFileSync(trPath, "utf8") : enRaw;

  const { fm: en, body: enBody } = splitFrontmatter(enRaw);
  const { fm: tr, body: trBody } = splitFrontmatter(trRaw);

  const enProse = parseBody(enBody);
  const trProse = parseBody(trBody);

  const merged = {
    slug: en.slug,
    name: { en: String(en.name), tr: String(tr.name ?? en.name) },
    difficulty: en.difficulty,
    targetAge: en.targetAge,
    durationMin: en.durationMin,
    civsRecommended: [...(en.civsRecommended ?? [])],
    ...(en.source ? { source: en.source } : {}),
    steps: mergeSteps(en.steps ?? [], tr.steps ?? []),
    ...(enProse.intro
      ? { intro: { en: enProse.intro, tr: trProse.intro || enProse.intro } }
      : {}),
    ...(enProse.strategy.length
      ? {
          strategy: {
            en: [...enProse.strategy],
            tr: trProse.strategy.length ? [...trProse.strategy] : [...enProse.strategy],
          },
        }
      : {}),
  };

  const out = yaml.dump(merged, { lineWidth: 120, quotingType: '"', forceQuotes: false });
  const outName = fname.replace(/\.md$/, ".yaml");
  writeFileSync(join(OUT_DIR, outName), out, "utf8");
  count++;
  console.log(`  ✓  ${outName}`);
}
console.log(`\nMigrated ${count} build orders → ${OUT_DIR}/`);
