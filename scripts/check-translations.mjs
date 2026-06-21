#!/usr/bin/env node
// Bilingual content lives in single YAML files; per-field en==tr is audited by
// scripts/audit-yaml-translations.mjs. The remaining MD pairs (beginner, articles)
// are covered by scripts/i18n-coverage.mjs. This script delegates to both.
import { execFileSync } from "node:child_process";
for (const s of ["audit-yaml-translations.mjs", "i18n-coverage.mjs"]) {
  try { execFileSync("node", [`scripts/${s}`], { stdio: "inherit" }); }
  catch (_) { process.exitCode = 1; }
}
