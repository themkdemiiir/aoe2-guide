# AOE2 Guide Copilot Instructions

Use these instructions for all GitHub Copilot work in this repository. Keep changes consistent with the existing Claude guidance in [CLAUDE.md](../CLAUDE.md) and the source-of-truth implementation files.

## Stack Invariants

- This is an Astro 5+ static site for Cloudflare Pages. Do not add SSR adapters, API routes, middleware, server-side runtime, KV, D1, R2, or Pages Functions unless the static-only invariant is explicitly changed.
- Use pnpm v9+ commands from [package.json](../package.json). Do not add npm lockfiles or yarn lockfiles.
- TailwindCSS is v4 and CSS-first through `@tailwindcss/vite`; theme tokens live in [src/styles/globals.css](../src/styles/globals.css) inside `@theme {}`. Do not create `tailwind.config.js` or install `@astrojs/tailwind`.
- Content uses Astro Content Layer loaders with path-based IDs. Render entries with `render(entry)` from `astro:content`; do not use `entry.render()`.
- Content schemas in [src/content/config.ts](../src/content/config.ts) are authoritative. Build failures from Zod schema validation should be fixed at the schema/content mismatch, not bypassed.

## Content And I18n

- Active production languages are EN and TR. EN is the source of truth; missing TR content falls back to EN through locale-aware content helpers in `src/lib/content.ts`.
- Slugs are English-canonical and kebab-case across all locales, for example `/tr/civs/britons/`, not localized URL slugs.
- Localized prose belongs in `src/content/<type>/{en,tr}/`. Numeric stats, unit references, counters, icon maps, and patch facts belong in `src/data/*.json`.
- New raw guides start in `md/<type>/<source>-<topic>.md`, then move through `pnpm import:md` and `pnpm new:guide`.
- Keep Microsoft Game Content Usage Rules attribution centralized in the Footer component. Do not remove or paraphrase it.

## Assets And Data

- AOE2 icons are bundled under `public/images/aoe2/` and mapped by [src/data/icon-map.json](../src/data/icon-map.json).
- Use `pnpm assets:sync` only when intentionally refreshing pinned upstream assets, then run `pnpm icons:map` and `pnpm validate:icons`.
- Treat unit/icon/civ data as integrity-sensitive. Prefer validation scripts over manual JSON edits when possible.
- Do not commit `dist/`, `.pagefind/`, `.astro/`, browser profiles, local auth files, or generated preview artifacts.

## Validation

- Before launch-ready changes, run at least `pnpm validate:icons`, `pnpm check`, `pnpm test`, and `pnpm build`.
- For content-only changes, also run `pnpm check:translations` when locale coverage is affected.
- For script/data changes, prefer adding or extending validators before changing generated data.
- For visual/page changes, use `pnpm dev` or `pnpm preview` plus Playwright checks where practical.

## Cloudflare Pages

- Deploy as a static Astro site with build command `pnpm install --frozen-lockfile && pnpm build` and output directory `dist`.
- Do not add `@astrojs/cloudflare` unless the project explicitly starts using SSR, bindings, or Pages Functions.
- Keep `site` in [astro.config.mjs](../astro.config.mjs) aligned with the production domain before launch.

## AI Workflow

- Use scoped instructions in `.github/instructions/` when editing content, Astro/UI, scripts, or data.
- Use custom agents in `.github/agents/` for focused tasks rather than mixing content curation, data validation, and launch checks in one pass.
- Use MCPs cautiously. Repo-shared config must not contain secrets; auth belongs in each user's MCP approval/OAuth flow or environment.