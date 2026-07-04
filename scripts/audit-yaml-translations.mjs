import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import yaml from "js-yaml";
import { auditEntry } from "./lib/audit-core.mjs";

const TYPES = ["civilizations", "build-orders", "units", "maps", "glossary"];
let total = 0;
for (const t of TYPES) {
  const dir = path.resolve("src/content", t);
  let files = [];
  try {
    files = readdirSync(dir).filter((f) => /\.ya?ml$/.test(f));
  } catch (_) {
    continue;
  }
  for (const f of files) {
    const data = yaml.load(readFileSync(path.join(dir, f), "utf8"));
    const issues = auditEntry(t, f, data);
    for (const p of issues) {
      console.error(`  ✗ ${t}/${f}: "${p}" is identical in en and tr (untranslated)`);
      total++;
    }
  }
}
if (total) {
  console.error(`\naudit-yaml-translations: ${total} untranslated field(s)`);
  process.exit(1);
}
console.log("audit-yaml-translations: all bilingual fields translated ✓");
