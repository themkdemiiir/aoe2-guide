#!/usr/bin/env node
// One-shot: remove author/source attribution from published content (keep the builds).
//   build-orders/{en,tr}/*.md — drop the `source:` frontmatter block.
//   civilizations/{en,tr}/*.md — drop the "*Strategy synthesized…*" credit line.

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

let builds = 0;
let civs = 0;

// --- build-orders: strip the `source:` block (the key + its indented children) ---
for (const locale of ["en", "tr"]) {
  const dir = `src/content/build-orders/${locale}`;
  for (const f of readdirSync(dir).filter((x) => x.endsWith(".md"))) {
    const p = join(dir, f);
    const raw = readFileSync(p, "utf8");
    const m = raw.match(/^(---\n)([\s\S]*?)(\n---\n[\s\S]*)$/);
    if (!m) continue;
    const lines = m[2].split("\n");
    const out = [];
    for (let i = 0; i < lines.length; i++) {
      if (/^source:/.test(lines[i])) {
        while (i + 1 < lines.length && /^\s+\S/.test(lines[i + 1])) i++;
        continue;
      }
      out.push(lines[i]);
    }
    const next = m[1] + out.join("\n") + m[3];
    if (next !== raw) {
      writeFileSync(p, next, "utf8");
      builds++;
    }
  }
}

// --- civilizations: strip the strategy-attribution credit line ---
for (const locale of ["en", "tr"]) {
  const dir = `src/content/civilizations/${locale}`;
  for (const f of readdirSync(dir).filter((x) => x.endsWith(".md"))) {
    const p = join(dir, f);
    const raw = readFileSync(p, "utf8");
    const kept = raw.split("\n").filter((l) => {
      const t = l.trim();
      return !(t.startsWith("*Strate") && (t.includes("synthesized") || t.includes("sentezlen")));
    });
    const next = `${kept
      .join("\n")
      .replace(/\n{3,}/g, "\n\n")
      .replace(/\s+$/, "")}\n`;
    if (next !== raw) {
      writeFileSync(p, next, "utf8");
      civs++;
    }
  }
}

console.log(`builds: ${builds} source blocks removed; civs: ${civs} attribution lines removed`);
