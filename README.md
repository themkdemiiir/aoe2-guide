# AOE2 Guide

Bilingual Age of Empires II guide — civilizations, build orders, units, maps, matchups, beginner curriculum, and glossary in **English** and **Türkçe**.

Static site, no server runtime. Built with **Astro 5 + Tailwind v4 + Pagefind**, deployed on **Cloudflare Pages**.

## Develop

```bash
pnpm install
pnpm dev          # http://localhost:4321
```

## Build

```bash
pnpm build        # outputs dist/ with the Pagefind index pre-built
```

## Test

```bash
pnpm test         # Vitest (helpers + utilities)
pnpm check        # Astro/TS type checks
```

## AI-assisted development

This repo includes shared setup for GitHub Copilot in VS Code and Claude Code: project instructions, scoped custom agents, reusable prompts, MCP server config, and editor tasks.

See [`docs/ai-development.md`](docs/ai-development.md) for setup, security rules, and recommended agent workflows.

## Asset pipeline

```bash
pnpm assets:sync  # re-fetch AOE2 icons from aoe2techtree (pinned SHA)
pnpm icons:map    # regenerate src/data/icon-map.json from aoc-reference-data
```

## Adding content

```bash
# 1. Drop a raw guide into the staging directory
$EDITOR guide-sources/build-orders/hera-new-build.md

# 2. Promote it into a scaffolded EN content entry
pnpm import:md guide-sources/build-orders/hera-new-build.md

# 3. (After EN is filled in) scaffold the TR version
pnpm new:guide build-orders <slug>

# 4. Translate the scaffolded files; missing translations fall back to EN
```

See [`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md) for the full workflow.

## Project shape

```
src/
  content/          ← Astro Content Collections (per-language markdown)
  data/             ← Language-agnostic JSON (stats, icon map)
  components/       ← Astro components (layout, content, ui, islands)
  i18n/             ← Locale config, UI strings, helpers
  lib/              ← Content helpers with locale fallback
  pages/            ← Routes — [lang]/<section>/...
  styles/globals.css← Tailwind v4 + theme tokens (no JS config)

md/                 ← Raw source material (not built; promoted via scripts)
public/images/aoe2/ ← Synced AOE2 icons (committed for offline builds)
scripts/            ← sync-assets, build-icon-map, import-from-md, new-guide
docs/               ← Specs, plans, contributor docs
```

## Attribution

> Age of Empires II © Microsoft Corporation. This site was created under Microsoft's "Game Content Usage Rules" using assets from Age of Empires II, and it is not endorsed by or affiliated with Microsoft.

Icons sourced from [SiegeEngineers/aoe2techtree](https://github.com/SiegeEngineers/aoe2techtree) (MIT code; assets under the Game Content Usage Rules) and [SiegeEngineers/aoc-reference-data](https://github.com/SiegeEngineers/aoc-reference-data) (used as a factual ID→name lookup at build time, not redistributed).

## License

Code under MIT. Site copy authored by contributors; AOE2 names/icons remain Microsoft IP under the Game Content Usage Rules.
