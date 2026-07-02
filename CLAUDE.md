# AOE2 Guide

A bilingual (EN/TR) static guide site for Age of Empires II, deployed to Cloudflare Pages.

For repo-shared GitHub Copilot, custom agent, VS Code, and MCP setup, see `.github/copilot-instructions.md`, `AGENTS.md`, and `docs/ai-development.md`.

## Stack invariants

- **Astro 5+** in static-output mode. No SSR, no API routes, no server-side runtime.
- **TypeScript strict** (extends `astro/tsconfigs/strict`).
- **TailwindCSS v4** via `@tailwindcss/vite` plugin. Theme tokens live in `src/styles/globals.css` inside `@theme {}` — NOT in a JS config file. Do not install `@astrojs/tailwind` (deprecated).
- **Content Layer API** with `loader: glob({ pattern, base, generateId: pathId })`. Render bodies via `await render(entry)` from `astro:content` — NEVER `entry.render()`.
- **Pagefind** (via `astro-pagefind` integration) for client-side search. Per-language indexes are auto-detected from `<html lang>`.
- Package manager: **pnpm** (v9+).

## Languages

EN (source of truth) and TR. Missing TR content falls back to EN through locale-aware content helpers in `src/lib/content.ts`.

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
| `pnpm build:wasm` | Rebuild the `/analyzer` WASM engine (needs `wasm-pack` + `rustup target add wasm32-unknown-unknown`; glue → `src/wasm/pkg/`, binary → `public/analyzer/pkg/` — both committed) |

## Layout map

| Path | Purpose |
|---|---|
| `docs/superpowers/specs/2026-05-23-aoe2-guide-design.md` | Architecture spec |
| `docs/superpowers/plans/2026-05-23-aoe2-guide-implementation.md` | Implementation plan |
| `src/content/config.ts` | All collection schemas (single source of truth for content shape) |
| `src/content/<type>/<slug>.yaml` | Bilingual content (civilizations, build-orders, units, maps, glossary) — each text field is `{ en: "...", tr: "..." }` |
| `src/content/{beginner,articles}/{en,tr}/*.md` | Long-form MD content that remains in separate locale dirs |
| `src/data/*.json` | Language-agnostic structured data (stats, slug refs, patch info) |
| `src/components/{layout,content,ui,islands}/` | Astro components |
| `src/i18n/{locales,ui,utils}.ts` | i18n config, UI strings, helpers |
| `src/lib/content.ts` | Locale-aware content helpers (`getLocalizedEntries`, `getLocalizedEntry`) |
| `md/` | Raw source guides (not built — Hera build-order verification sources) |
| `public/images/aoe2/` | Synced AOE2 icons (bundled in repo for offline builds) |
| `scripts/` | sync-assets, build-icon-map, build-civilizations, audit-yaml-translations, check-translations |

**Civ content:** EN+TR text (bonuses, team bonus, unique-tech effects, unique units) is sourced from `aoe2techtree` locale strings via `pnpm build:civilizations` (`scripts/build-civilizations.mjs`). Do not hand-edit the generated YAML fields — re-run the script instead.

**Two civ-id spaces — never conflate them:** `src/data/civ-id-map.json` maps GAME/replay `civ_id`s (used by the analyzer + replay data); `src/data/relic-civ-id-map.json` maps the Relic API's own `civilization_id`s (used by every crawl consumer via `scripts/data-pipeline/lib/relic-map.mjs`). Applying the wrong table silently shuffles every civ label — that was a real June-2026 production bug. Guard tests: `tests/relic-civ-map.test.mjs`.

**Build orders:** steps are verified against the Hera video sources in `md/build-orders/`.

## Workflow for new content

### YAML types (civilizations, build-orders, units, maps, glossary)

1. Create `src/content/<type>/<slug>.yaml` with all text fields as `{ en: "...", tr: "..." }`.
2. Fill required schema fields — see `src/content/config.ts` for the authoritative schema.
3. `pnpm build` runs `audit-yaml-translations` as a prebuild gate: any field where `en === tr` (excluding allow-listed proper nouns: `name`, `term`, unique-tech names) fails the build.

### MD types (beginner, articles)

1. Add `src/content/<type>/en/<slug>.md` (EN source of truth).
2. `pnpm new:guide <type> <slug>` scaffolds a TR placeholder under `src/content/<type>/tr/`.
3. Translate the TR file; missing TR falls back to EN automatically.
4. `pnpm build` validates schemas; broken frontmatter fails the build (and CI).

## Attribution requirement

Footer text on every page must credit Microsoft's "Game Content Usage Rules" for AOE2 assets. The Footer component handles this — do not remove or paraphrase. Icons come from `SiegeEngineers/aoe2techtree` (MIT code + MS Game Content Usage Rules for assets).

## Don'ts

- Don't write `@astrojs/tailwind` anywhere — it's deprecated.
- Don't add SSR adapters or API routes — static-only invariant.
- Don't use `entry.render()` — use `render(entry)` from `astro:content`.
- Don't put `tailwind.config.js`/`.mjs` in the repo. Tailwind v4 is CSS-first.
- Don't commit Pagefind indices (`.pagefind/`) or `dist/` — they're regenerated on build.
