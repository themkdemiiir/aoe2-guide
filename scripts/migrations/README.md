# scripts/migrations — completed one-time content migrations

These are **completed, one-off content-migration scripts**, kept here for
provenance only. They are NOT part of any build, `pnpm` task, or CI step — each
was run once against the content tree to perform a structural change and then
retired. They are safe to ignore and safe to delete later.

| script | what it did (once) |
|---|---|
| `migrate-builds-to-bilingual.mjs` | converted build-order YAML text fields to the `{ en, tr }` bilingual shape |
| `migrate-locale-pairs.mjs` | restructured per-locale content into bilingual locale pairs |
| `fix-civ-content.mjs` | one-off cleanup pass over civilization content YAML |
| `strip-attribution.mjs` | removed inline attribution text now handled by the Footer component |
| `strip-unit-placeholders.mjs` | stripped placeholder/stub fields out of unit content YAML |

> Not moved: `scripts/fill-unit-roles.mjs` stays in `scripts/` because it
> relative-imports `./lib/role-tr.mjs`; moving it would break that import.
