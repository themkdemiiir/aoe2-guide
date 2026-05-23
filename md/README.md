# `/md` — Raw Source Material

This directory holds **raw guides, build orders, civilization notes, and reference material** sourced from existing AOE2 guides (forums, YouTube transcripts, wiki articles, personal notes, etc.).

> **This is NOT the published site content.** Published content lives in `src/content/`. Files here are the inputs that get curated, cleaned, translated, and converted into the structured content collections.

## Workflow

```
md/build-orders/hera-21pop-archer.md   ← raw notes you drop here
                  │
                  ▼  (curation + schema fill + translation pass)
src/content/build-orders/en/21pop-archer.md   ← published English version
src/content/build-orders/tr/21pop-archer.md   ← Turkish translation
src/content/build-orders/es/21pop-archer.md
src/content/build-orders/de/21pop-archer.md
```

## Folder layout

| Folder              | What goes here                                                |
|---------------------|---------------------------------------------------------------|
| `build-orders/`     | Timed villager allocation guides (Hera builds, pro builds, etc.) |
| _(add as needed)_   | `civilizations/`, `units/`, `maps/`, `matchups/`, `beginner/` |

## Conventions for raw files

- **Name the file by source + topic**, e.g. `hera-21pop-archer.md`, `t90-flush-vikings.md`
- **Always note the source** in a top frontmatter block or at the top of the file:

```markdown
---
source: https://aoecompanion.com/b/wgr1aplwu1
author: Hera
date_captured: 2026-05-23
language: en
---
```

- **Copy/paste is fine** at this stage — structure happens during the curation pass into `src/content/`
- **No need to translate here** — translations happen when promoting to `src/content/`

## What you should NOT do here

- Don't edit these files after curation — they're a historical record of source material
- Don't link to files here from the site; they're not part of the build
- Don't put copyrighted images here — only text references and source URLs
