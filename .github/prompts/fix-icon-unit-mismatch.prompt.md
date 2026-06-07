---
name: "Fix Icon Unit Mismatch"
description: "Investigate and fix mismatches among content icon references, icon-map data, unit stats, unit lines, and civ unique units."
agent: "Data Integrity Validator"
tools: [read, search, edit, execute]
argument-hint: "Slug, route, or failing output"
---

Investigate the reported icon or unit mismatch.

1. Reproduce with the narrowest command, usually `pnpm validate:icons` or the relevant data script.
2. Trace the slug through content, `src/data/icon-map.json`, `src/data/unit-stats.json`, `src/data/unit-lines.json`, and `src/data/civilizations.json` as needed.
3. Fix the validator, source data, or content reference at the controlling source.
4. Keep the missing-icon allowlist only for upstream-missing assets, not typo masking.
5. Rerun the same reproduction command and then `pnpm build` if launch behavior is affected.

Return root cause, files changed, and validation output.