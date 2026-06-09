# AOE2 Guide Agent Contract

This file gives project-level guidance to AI coding agents that read `AGENTS.md`. Claude Code should also read [CLAUDE.md](CLAUDE.md); GitHub Copilot should also read [.github/copilot-instructions.md](.github/copilot-instructions.md).

## Mission

Build and maintain a bilingual EN/TR static guide site for Age of Empires II, deployed to Cloudflare Pages. Keep the site fast, static, schema-validated, and safe to build offline from committed assets and data.

## Non-Negotiable

- Astro static output only. No SSR adapters, API routes, Pages Functions, or Cloudflare runtime bindings unless explicitly requested.
- TailwindCSS v4 is CSS-first via `@tailwindcss/vite`; do not add a Tailwind JS config or `@astrojs/tailwind`.
- Use pnpm only.
- Use Astro Content Layer API with `render(entry)`, never legacy `entry.render()`.
- Keep English-canonical slugs across locales.
- When adding new content, provide the English version first. Create a TR stub file with a `TODO: translate` marker rather than leaving the locale file absent, and run `pnpm check:translations` before committing.
- Do not remove Microsoft Game Content Usage Rules attribution from the footer.
- Do not commit generated output or secrets.

## Preferred Workflow

1. Read the nearest source of truth before editing: schemas in [src/content/config.ts](src/content/config.ts), commands in [package.json](package.json), and deployment rules in [docs/deployment.md](docs/deployment.md).
2. Make focused edits that preserve existing structure and naming.
3. Run `pnpm build` when changes touch routing, i18n slugs, build config, or Pagefind indexing. For all other changes, `pnpm check` and `pnpm test` are sufficient. If `pnpm build` fails, stop and report the full error output to the user. Do not attempt to auto-fix build errors that involve changes outside the immediate scope of the current task.
4. For data or icon changes, run the relevant generator/validator scripts instead of guessing mappings by hand. If a validator script reports a schema violation, report the specific violation to the user before making any changes. Do not modify the schema in `src/content/config.ts` to accommodate invalid data without explicit approval.

## Useful Commands

- `pnpm dev` - local Astro server.
- `pnpm validate:icons` - content/icon reference validation.
- `pnpm check` - Astro and TypeScript checks.
- `pnpm test` - Vitest unit tests.
- `pnpm build` - production static build with Pagefind.
- `pnpm check:translations` - locale coverage audit.
- `pnpm icons:map` - regenerate the icon map after asset changes.

## Agent Boundaries

- Content agents should not modify JSON/YAML data files or schema definitions in `src/content/config.ts` unless the task explicitly requires it.
- Data agents should not localize prose or change page layout.
- Launch agents should not suggest SSR Cloudflare adapters for this static site.
- Browser agents should use Playwright only for visual, accessibility, route, or preview smoke checks.
- If a task spans multiple agent roles, default to the most restrictive applicable boundary and flag the cross-role dependency to the user before proceeding.
