# AOE2 Guide

A four-language static guide site for Age of Empires II, deployed to Cloudflare Pages.

## Stack invariants

- **Astro 5+** in static-output mode. No SSR, no API routes, no server-side runtime.
- **TypeScript strict** (extends `astro/tsconfigs/strict`).
- **TailwindCSS v4** via `@tailwindcss/vite` plugin. Theme tokens live in `src/styles/globals.css` inside `@theme {}` — NOT in a JS config file. Do not install `@astrojs/tailwind` (deprecated).
- **Content Layer API** with `loader: glob({ pattern, base, generateId: pathId })`. Render bodies via `await render(entry)` from `astro:content` — NEVER `entry.render()`.
- **Pagefind** (via `astro-pagefind` integration) for client-side search. Per-language indexes are auto-detected from `<html lang>`.
- Package manager: **pnpm** (v9+).

## Languages

EN (source of truth), TR, ES, DE. Missing translations fall back to EN via `i18n.routing.fallbackType: "rewrite"`.

## Slug policy

English-canonical across all locales. `/tr/civs/britons/` — not `/tr/uygarliklar/britonlar/`.

## Commands

| Command | Description |
|---|---|
| `pnpm dev` | Local dev server on `http://localhost:4321` (with HMR) |
| `pnpm build` | Production build + Pagefind index → `dist/` |
| `pnpm preview` | Preview the built site |
| `pnpm test` | Vitest unit tests |
| `pnpm check` | Astro/TS type checks |
| `pnpm assets:sync` | Refresh AOE2 icons from `SiegeEngineers/aoe2techtree` at the pinned SHA |
| `pnpm icons:map` | Regenerate `src/data/icon-map.json` from aoc-reference-data |
| `pnpm import:md <path>` | Promote a raw `md/<type>/<file>.md` source into a scaffolded EN content entry |

## Layout map

| Path | Purpose |
|---|---|
| `docs/superpowers/specs/2026-05-23-aoe2-guide-design.md` | Architecture spec |
| `docs/superpowers/plans/2026-05-23-aoe2-guide-implementation.md` | Implementation plan |
| `src/content/config.ts` | All 7 Zod schemas (single source of truth for content shape) |
| `src/content/<type>/{en,tr,es,de}/*.md` | Localized content |
| `src/data/*.json` | Language-agnostic structured data (stats, slug refs, patch info) |
| `src/components/{layout,content,ui,islands}/` | Astro components |
| `src/i18n/{locales,ui,utils}.ts` | i18n config, UI strings, helpers |
| `src/lib/content.ts` | Locale-aware content helpers (`getLocalizedEntries`, `getLocalizedEntry`) |
| `md/` | Raw source guides (not built — input layer for the curation pipeline) |
| `public/images/aoe2/` | Synced AOE2 icons (bundled in repo for offline builds) |
| `scripts/` | sync-assets, build-icon-map, import-from-md, new-guide, check-translations |

## Workflow for new content

1. Drop raw guide into `md/<type>/<source>-<topic>.md` with a frontmatter source block.
2. `pnpm import:md md/<type>/<file>.md` scaffolds an EN entry with frontmatter placeholders.
3. Edit the EN file: fill schema fields, write strategy notes.
4. `pnpm new:guide <type> <slug>` creates empty TR/ES/DE scaffolds for translation.
5. `pnpm build` validates schemas; broken frontmatter fails the build (and CI).

## Attribution requirement

Footer text on every page must credit Microsoft's "Game Content Usage Rules" for AOE2 assets. The Footer component handles this — do not remove or paraphrase. Icons come from `SiegeEngineers/aoe2techtree` (MIT code + MS Game Content Usage Rules for assets).

## Don'ts

- Don't write `@astrojs/tailwind` anywhere — it's deprecated.
- Don't add SSR adapters or API routes — static-only invariant.
- Don't use `entry.render()` — use `render(entry)` from `astro:content`.
- Don't put `tailwind.config.js`/`.mjs` in the repo. Tailwind v4 is CSS-first.
- Don't commit Pagefind indices (`.pagefind/`) or `dist/` — they're regenerated on build.
