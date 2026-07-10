# /md — Raw Source Material (NOT built into the site)

Files in this directory are **inputs**, not site content. They are sourced from external guides (forums, YouTube transcripts, wiki articles, user transcriptions) and serve as the starting material that gets curated into `src/content/`.

## What lives here

Raw markdown notes, copy-paste from external guides, build-order outlines. Always include source URL and author in a frontmatter block.

## What does NOT live here

- Published site content (that's in `src/content/`).
- Images (we link to source URLs only; no copyrighted images here).
- Localized translations (those live in `src/content/`).

## Pipeline

```
md/<type>/<source>-<topic>.md   ← raw notes
                │
                ▼  pnpm import:md <path>
src/content/<type>/en/<slug>.md ← scaffolded EN entry
                │
                ▼  pnpm new:guide <type> <slug>
src/content/<type>/{tr,es,de}/<slug>.md
```

## For Claude

- **Do not** import this directory in any page, component, or script.
- **Do not** include it in the build pipeline.
- **Do not** treat these files as source of truth — they're historical record of source material.
- When the user pastes a new build order or guide, save it here first with a `proposed_slug` and source frontmatter. Then promote via `pnpm import:md`.
