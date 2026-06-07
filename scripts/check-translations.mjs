#!/usr/bin/env node
// scripts/check-translations.mjs
// Walks src/content/<type>/{en,tr}/*.md and reports per-language coverage.

import { readdir } from "node:fs/promises";
import path from "node:path";

const TYPES = [
  "civilizations",
  "build-orders",
  "units",
  "maps",
  "matchups",
  "beginner",
  "glossary",
];
const LANGS = ["en", "tr"];

async function listSlugs(type, lang) {
  try {
    const dir = path.resolve("src/content", type, lang);
    const files = await readdir(dir);
    return files
      .filter((f) => f.endsWith(".md") || f.endsWith(".mdx"))
      .map((f) => f.replace(/\.(md|mdx)$/i, ""));
  } catch {
    return [];
  }
}

async function run() {
  let totalMissing = 0;
  let totalEntries = 0;
  console.log("Translation coverage report\n");
  console.log("type                  lang  count  missing");
  console.log("─".repeat(60));

  for (const type of TYPES) {
    const en = new Set(await listSlugs(type, "en"));
    totalEntries += en.size;
    console.log(`${type.padEnd(22)}en    ${String(en.size).padStart(5)}`);

    for (const lang of LANGS.filter((l) => l !== "en")) {
      const have = new Set(await listSlugs(type, lang));
      const missing = [...en].filter((s) => !have.has(s));
      const summary = missing.length === 0 ? "" : `  ${missing.join(", ")}`;
      console.log(
        `${type.padEnd(22)}${lang}    ${String(have.size).padStart(5)}/${en.size}${summary}`,
      );
      totalMissing += missing.length;
    }
  }

  console.log("─".repeat(60));
  console.log(`Total EN entries: ${totalEntries}`);
  console.log(`Total missing translations: ${totalMissing}`);
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
