# Contributing to AOE2 Guide

Thanks for wanting to contribute. This site is welcoming new build orders, civilization writeups, map strategies, beginner chapters, and translations.

## Setup

```bash
git clone https://github.com/themkdemiiir/aoe2-guide
cd aoe2-guide
pnpm install
pnpm dev
```

Visit `http://localhost:4321/` and pick a language to enter.

## What you can contribute

| Content type | Where | What you need |
|---|---|---|
| Civilization writeup | `src/content/civilizations/<lang>/<civ>.md` | Existing civ frontmatter; just fill in bonuses, unique techs, prose |
| Build order | `md/build-orders/<source>-<topic>.md` → `pnpm import:md ...` | Timed step list with villager allocations, source URL/author |
| Unit description | `src/content/units/<lang>/<unit>.md` + stats in `src/data/unit-stats.json` | Stats for the JSON, prose + role for the markdown |
| Map strategy | `src/content/maps/<lang>/<map>.md` | Map type, recommended civs, prose covering opening/mid/late game |
| Matchup | `src/content/matchups/<lang>/<civ-a>-vs-<civ-b>.md` | Civ slugs, difficulty rating, analysis |
| Beginner chapter | `src/content/beginner/<lang>/<NN>-<topic>.md` | Sequence number, title, prose |
| Glossary term | `src/content/glossary/<lang>/<term>.md` | Letter + term + definition prose |
| Translation | Existing `<slug>.md` in TR | Match the EN structure; localize names + prose |

## Step-by-step: adding a new build order

1. Save the raw guide (notes, transcript, screenshot text) at `md/build-orders/<author>-<topic>.md`:
   ```yaml
   ---
   source: https://example.com/source-url
   author: SomeAuthor
   date_captured: 2026-MM-DD
   language: en
   ---

   # Build name
   ...notes...
   ```

2. Promote into a scaffolded EN content entry:
   ```bash
   pnpm import:md md/build-orders/author-topic.md
   ```

3. Open the scaffolded `src/content/build-orders/en/<slug>.md`. Fill the frontmatter fields and write 3-4 short paragraphs of prose.

4. Scaffold the TR empty file:
   ```bash
   pnpm new:guide build-orders <slug>
   ```

5. Translate as time permits. Missing translations fall back to EN automatically.

6. Verify the build passes:
   ```bash
   pnpm build
   ```

7. Commit your work and open a PR.

## Slug convention

English-canonical, kebab-case, descriptive: `21pop-archers`, `britons-vs-franks`, `arabia`, `feudal-age-glossary-term`. The same slug is used across EN and TR directories.

## Schema requirements

`src/content/config.ts` is authoritative. If you break the schema, `pnpm build` will fail with a Zod error pointing to the offending field. See [`docs/content-schemas.md`](content-schemas.md) for a field-by-field reference.

## Build steps format

For build orders, each entry in `steps[]` must include:

- `villagers` (integer)
- `time` (`"m:ss"` format)
- `assign` (short string describing the action — localize per language)
- `note` (optional one-liner — localize per language)

Aim for 6–12 steps per build. The structured steps power the visual step table; long prose context goes in the body, not in `note` fields.

## Translation workflow

See [`docs/translation-workflow.md`](translation-workflow.md).

## CI checks

Every PR runs install, `pnpm check`, `pnpm check:all`, `pnpm test`, and `pnpm build`. Schema errors, broken imports, TypeScript errors, or lint/format errors will block merge.

## Attribution

If you copy text or strategy from an external guide, credit the source via the `source:` frontmatter block (and link to the page if possible). Don't paste copyrighted images — link to source URLs instead. The site already credits Microsoft for AOE2 assets at the page level.

## What we won't accept

- Hate speech, harassment, or personal attacks toward players in writeups.
- Commercial promotion (affiliate links, paid mod ads).
- Copy-pasted content without attribution.
- Translations that are clearly machine-translated without proofreading.

## Questions

Open an issue at https://github.com/themkdemiiir/aoe2-guide/issues with the label `question`.
