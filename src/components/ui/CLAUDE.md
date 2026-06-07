# UI Primitives

Astro-native UI components with zero JS hydration unless explicitly noted.
Inspired by shadcn/ui aesthetic; built from this project's design tokens
(see `src/styles/globals.css` and the project `src/components/CLAUDE.md`).

## Available primitives

| Component | Use case | JS? |
|---|---|---|
| `Card.astro` | Generic content container, optional header/footer slots, optional `href` | none |
| `Badge.astro` | Small inline label, variants: default/secondary/outline/destructive/success | none |
| `Stat.astro` | Label + value pair (HP, attack, cost, etc.) | none |
| `Alert.astro` | Info/warning/error/success callout banner | none |
| `Separator.astro` | Horizontal or vertical divider | none |
| `Skeleton.astro` | Animated loading placeholder | none |
| `Tooltip.astro` | CSS-only :hover/:focus tooltip; for complex positioning use floating-ui island | none |
| `Dialog.astro` | Wraps native `<dialog>`; backdrop styling included | none |
| `Tabs.astro` | Radio-based tab pattern, minimal JS for panel swap | tiny |
| `Icon.astro` | Domain icon (units/civs/techs/buildings) via `src/data/icon-map.json` | none |

## Design rules

- Props typed inline via `interface Props { ... }` per `src/components/CLAUDE.md`.
- Use Tailwind utilities mapped to theme tokens — `bg-parchment`, `text-ink`,
  `border-stone-700/40`, `text-gold-500`, `font-display`, `font-body`.
  No raw hex values.
- Dark mode handled by token swap on `[data-theme="dark"]` — do NOT write
  `dark:` prefixes.
- Avoid `client:*` directives unless interactivity demands it (Tabs has minimal
  inline JS; Tooltip has none).
- Pass a `class` prop for caller-supplied additions. Use `class:list` to merge.
- Each component file < 80 lines including frontmatter.

## UI chrome icons (astro-icon)

For menu/toggle/chevron/search/copy-button — use `astro-icon` with iconify:

```astro
import { Icon as UIIcon } from "astro-icon/components";
<UIIcon name="lucide:menu" class="h-5 w-5" />
<UIIcon name="heroicons:moon" class="h-4 w-4" />
```

This is separate from the domain `Icon.astro` (units/civs/techs/buildings).
Different namespaces; no collision.

## Browser support

`<dialog>` requires Safari 15.4+ / Chrome 37+ / Firefox 98+ — acceptable for
this site's audience. Tooltip CSS uses `:has()` (Baseline 2023) for the
purely-CSS variant; the JS-driven Tooltip in Tabs is a separate code path.

View Transitions API (via `astro:transitions` ClientRouter) works in all
evergreen browsers; degrades to instant navigation in older ones.
