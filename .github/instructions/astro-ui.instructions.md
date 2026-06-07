---
name: "AOE2 Astro UI Rules"
description: "Use when editing Astro components, layouts, UI primitives, Tailwind v4 CSS, styling, or route templates."
applyTo: ["src/**/*.astro", "src/styles/**/*.css"]
---

# Astro UI Rules

- Keep Astro pages static. Do not add server endpoints, middleware, SSR adapters, or Cloudflare runtime APIs.
- Use TailwindCSS v4 theme tokens from [src/styles/globals.css](../../src/styles/globals.css). Do not add Tailwind config files.
- Prefer existing components in `src/components/ui/`, `src/components/content/`, and `src/components/layout/` before adding new primitives.
- Use `render(entry)` from `astro:content` for entry bodies. Do not use legacy `entry.render()`.
- Keep UI changes content-focused and readable for repeat guide usage; avoid marketing-style landing sections unless explicitly requested.
- Do not remove the footer attribution for Microsoft Game Content Usage Rules.
- Validate route/layout changes with `pnpm check` and `pnpm build`. Use Playwright for route or visual smoke checks when UI behavior changes.