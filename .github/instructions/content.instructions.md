---
name: "AOE2 Content Rules"
description: "Use when editing Astro content collections, Markdown frontmatter, EN/TR translations, slugs, build orders, or guide prose."
applyTo: "src/content/**/*.{md,mdx}"
---

# Content Rules

- Treat [src/content/config.ts](../../src/content/config.ts) as the source of truth for required frontmatter fields.
- Keep slugs English-canonical and kebab-case in every locale.
- EN is the source of truth. TR entries should keep the same slug and translate prose, labels, and notes only.
- For TR translations, follow [docs/i18n/GLOSSARY.md](../../docs/i18n/GLOSSARY.md) (canonical EN↔TR terms) and [docs/i18n/STYLE.md](../../docs/i18n/STYLE.md) (voice/register) exactly. The `/translate` prompt automates this per file.
- Do not move numeric stats, counters, icon references, or unit-line facts into localized prose if they belong in `src/data/*.json`.
- Build-order `steps[]` should be concise and useful: `villagers`, optional `time`, localized `assign`, optional `note`, and optional `icons`.
- Use existing content patterns before inventing new frontmatter fields. Schema-breaking content must be resolved in code and content together.
- Preserve source attribution blocks. Do not remove external guide source URLs when present.
- After content edits, prefer `pnpm check`, `pnpm validate:icons`, and `pnpm check:translations` when locale coverage changed.