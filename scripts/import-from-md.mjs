#!/usr/bin/env node
// scripts/import-from-md.mjs
// Usage: node scripts/import-from-md.mjs guide-sources/<type>/<source-file>.md
// Scaffolds an EN content entry from a raw source file, preserving body content
// and adding placeholder frontmatter the user fills in.

import { access, mkdir, readFile, writeFile } from "node:fs/promises";
import path from "node:path";

function slugify(s) {
  return s
    .toLowerCase()
    .normalize("NFKD")
    .replace(/[^\w\s-]/g, "")
    .trim()
    .replace(/\s+/g, "-");
}

async function exists(p) {
  try {
    await access(p);
    return true;
  } catch {
    return false;
  }
}

async function run() {
  const file = process.argv[2];
  if (!file) {
    console.error("Usage: import-from-md.mjs guide-sources/<type>/<source-file>.md");
    process.exit(1);
  }

  const abs = path.resolve(file);
  const parts = path.relative(path.resolve("guide-sources"), abs).split(path.sep);
  if (parts.length < 2) {
    console.error("File must be under guide-sources/<type>/<file>.md");
    process.exit(1);
  }
  const type = parts[0];
  const basename = path.basename(parts.at(-1), ".md");
  const slug = slugify(basename.replace(/^[^-]*-/, ""));

  const target = path.resolve("src/content", type, "en", `${slug}.md`);
  if (await exists(target)) {
    console.error("Target already exists:", target);
    process.exit(1);
  }

  const source = await readFile(abs, "utf8");
  const m = source.match(/^#\s+(.+)$/m);
  const name = m ? m[1] : slug;

  const frontmatter = `---
slug: ${slug}
name: ${JSON.stringify(name)}
difficulty: intermediate
targetAge: feudal
durationMin: 14
civsRecommended: []
steps: []
source:
  author: "TODO"
  url: ""
---

`;

  await mkdir(path.dirname(target), { recursive: true });
  await writeFile(target, frontmatter + source);
  console.log("Scaffolded", target);
  console.log("Next: fill in steps:[] and civsRecommended:[], then translate.");
}

run().catch((e) => {
  console.error(e);
  process.exit(1);
});
