---
name: "Translate Content"
description: "Translate one EN content file to Turkish using docs/i18n/GLOSSARY.md and STYLE.md; prose only, structure and numbers untouched."
agent: "Content Curator"
tools: [read, search, edit, execute]
argument-hint: "Path to the EN content file (e.g. src/content/units/en/knight.md)"
---

Read `docs/i18n/GLOSSARY.md` and `docs/i18n/STYLE.md` in full before doing anything.

Translate the English content file at **$1** into Turkish and write the result to the mirrored TR path (swap the locale segment: `src/content/<type>/en/<slug>.md` → `src/content/<type>/tr/<slug>.md`). If the TR file exists, update it; otherwise create it.

## Hard rules
- **Translate only human-readable prose.** Never change frontmatter keys, `slug`/`id` values, image paths, URLs, component markup, or markdown structure. Frontmatter *values* that are display text (e.g. `tagline`, `term`) DO get translated per the glossary; keys never change.
- **Keep all numbers, stats, costs, and timings identical** to the English source. Do not add, drop, or "improve" any fact.
- **Use the glossary's Turkish terms verbatim.** Keep everything on the "do not translate" list in English (unit / civ / tech / building names, established loanwords) with apostrophe suffixes (`counter'ları`, `Knight'ı`).
- **Ages:** Karanlık / Feodal / Kale Çağı / İmparatorluk Çağı — consistently.
- **Register:** casual, per `STYLE.md`. Mirror EN structure; don't expand or editorialize.
- **Placeholder guard:** if the EN body contains a `## Stats summary` table with `Free`/`Melee` placeholders (pre-Epic-2 cleanup), **do not translate that section** — omit it from the TR body and flag it in your output.
- **One file per invocation.** Do not touch other files.
- **Out of scope:** `src/data/counters.json` and other data-JSON prose — those have a separate localization workflow; stop and say so if $1 points at one.
- If you hit a term not in the glossary, pick the best Turkish term, use it consistently within the file, and collect it.

## After translating, output
1. A one-line confirmation that: prose-only was changed, glossary terms were used verbatim, the do-not-translate list was respected, and numbers/stats are unchanged.
2. A **"Glossary additions"** list of any new terms you had to invent (EN → TR), so they can be reviewed and added to `docs/i18n/GLOSSARY.md`.
3. Any **flags** (placeholder sections skipped, ambiguous terms, structure oddities).
4. Optionally run `pnpm i18n:coverage` and report the new count.

Do **not** translate multiple files or restructure content. If $1 is missing or not an English content file, stop and say so.
