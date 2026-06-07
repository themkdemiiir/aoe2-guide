# AOE2 Guide Agent Contract

This file gives project-level guidance to AI coding agents that read `AGENTS.md`. Claude Code should also read [CLAUDE.md](CLAUDE.md); GitHub Copilot should also read [.github/copilot-instructions.md](.github/copilot-instructions.md).

## Mission

Build and maintain a bilingual EN/TR static guide site for Age of Empires II, deployed to Cloudflare Pages. Keep the site fast, static, schema-validated, and safe to build offline from committed assets and data.

## Non-Negotiables

- Astro static output only. No SSR adapters, API routes, Pages Functions, or Cloudflare runtime bindings unless explicitly requested.
- TailwindCSS v4 is CSS-first via `@tailwindcss/vite`; do not add a Tailwind JS config or `@astrojs/tailwind`.
- Use pnpm only.
- Use Astro Content Layer API with `render(entry)`, never legacy `entry.render()`.
- Keep English-canonical slugs across locales.
- Do not remove Microsoft Game Content Usage Rules attribution from the footer.
- Do not commit generated output or secrets.

## Preferred Workflow

1. Read the nearest source of truth before editing: schemas in [src/content/config.ts](src/content/config.ts), commands in [package.json](package.json), and deployment rules in [docs/deployment.md](docs/deployment.md).
2. Make focused edits that preserve existing structure and naming.
3. Run the narrowest useful validation first, then broaden to `pnpm build` for launch-sensitive work.
4. For data or icon changes, run the relevant generator/validator scripts instead of guessing mappings by hand.

## Useful Commands

- `pnpm dev` - local Astro server.
- `pnpm validate:icons` - content/icon reference validation.
- `pnpm check` - Astro and TypeScript checks.
- `pnpm test` - Vitest unit tests.
- `pnpm build` - production static build with Pagefind.
- `pnpm check:translations` - locale coverage audit.
- `pnpm icons:map` - regenerate the icon map after asset changes.

## Agent Boundaries

- Content agents should not rewrite structured data unless the task explicitly requires it.
- Data agents should not localize prose or change page layout.
- Launch agents should not suggest SSR Cloudflare adapters for this static site.
- Browser agents should use Playwright only for visual, accessibility, route, or preview smoke checks.