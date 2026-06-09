#!/usr/bin/env node
// One-shot (Phase 1, Epic 2 P0): remove the placeholder "## Stats summary" table
// (wrong values like "Attack 10 / Cost Free") and the "## Notes" stub from unit md.
// The real stats render structurally from the unit-stats collection in [unit].astro;
// the body table only contradicts it. Keeps the intro + any non-placeholder section.
//
// Usage: node scripts/strip-unit-placeholders.mjs [--dry]

import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import { join } from "node:path";

const DRY = process.argv.includes("--dry");
const DROP = new Set(["Stats summary", "Notes"]);
const STUB = /Standard unit\. See the stats table above\./;

let changed = 0;
const flagged = [];
for (const locale of ["en", "tr"]) {
  const dir = `src/content/units/${locale}`;
  let files;
  try {
    files = readdirSync(dir).filter((f) => f.endsWith(".md"));
  } catch {
    continue;
  }
  for (const file of files) {
    const p = join(dir, file);
    const raw = readFileSync(p, "utf8");
    const m = raw.match(/^(---\n[\s\S]*?\n---\n)([\s\S]*)$/);
    if (!m) continue;
    const [, fm, body] = m;
    const parts = body.split(/(?=^## )/m);
    const kept = parts.filter((part) => {
      const h = part.match(/^## (.+?)\s*$/m);
      if (!h) return true; // intro (no heading)
      const title = h[1].trim();
      if (DROP.has(title)) {
        // Drop the placeholder, but preserve (and flag) any real Notes content.
        const keepNonStubNotes = title === "Notes" && !STUB.test(part);
        if (keepNonStubNotes) flagged.push(`${locale}/${file}: non-stub Notes kept-for-review`);
        return keepNonStubNotes;
      }
      return true;
    });
    const newBody = `${kept
      .join("")
      .replace(/\n{3,}/g, "\n\n")
      .replace(/\s+$/, "")}\n`;
    const next = fm + newBody;
    if (next !== raw) {
      changed++;
      if (!DRY) writeFileSync(p, next, "utf8");
    }
  }
}
console.log(`unit md: ${changed} ${DRY ? "would change" : "updated"}`);
if (flagged.length) {
  console.log("Flagged for review (non-stub Notes preserved):");
  for (const f of flagged) console.log(`  ${f}`);
}
