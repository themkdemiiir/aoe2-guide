# AOE2 Guide — Design Language

A reference export of the visual design system for the bilingual (EN/TR) Age of
Empires II guide. **This is a token + design-language reference, not a component
bundle** — the site is built with Astro (compile-time `.astro` templates), so there
are no runtime React components to render. Use this to theme on-brand work; build
actual UI with the source `.astro` components in `src/components/`.

Source of truth: `src/styles/globals.css` (`@theme {}`). Portable copies:
`tokens.css` (plain CSS custom properties) and `tokens.json` (structured).

## Brand

Medieval / illuminated-manuscript feel for a competitive AOE2 reference. Warm
**parchment** ground, dark **ink** text, **gold** for links and accents, engraved
**Cinzel** display type over humanist **Inter** body. Restrained: gold is an accent,
never a fill; one accent per surface. Dark mode is a true theme swap, not a tint.

## Color

Tokens are CSS custom properties (`var(--color-*)`), surfaced in the app as Tailwind
utility classes. Values are oklch (see `tokens.css`).

| Token | Tailwind class | Role |
|---|---|---|
| `--color-parchment` | `bg-parchment` | Page background |
| `--color-ink` | `text-ink` | Primary text |
| `--color-stone-700` | `text-stone-700`, `border-stone-700/40` | Secondary text, borders |
| `--color-gold-500` | `text-gold-500`, `border-gold-500` | Links, primary accent |
| `--color-gold-700` | `text-gold-700` | Gold **text on parchment** (WCAG AA ≥4.5:1) — use this, not gold-500, for small gold text on light |
| `--color-royal` | `text-royal`, `border-royal/40` | Warnings, important callouts |
| `--color-forest` / `--color-steel` / `--color-ember` | `text-forest` … | Secondary accents; `ember` = highlight / "due" state |
| `--color-res-food/wood/gold/stone` | `text-res-food` … | AOE2 resource indicators (red / brown / yellow / grey) |

**Accessibility gotcha:** on parchment, small gold text must use `gold-700` (gold-500
fails AA at small sizes). This is a real, shipped constraint.

## Typography

| Token | Class | Use |
|---|---|---|
| `--font-display` (Cinzel 600, serif) | `font-display` | h1 / h2 headings only — h3/h4 stay on body font for readability |
| `--font-body` (Inter, sans) | `font-body` | All body text (weights 400/500/700) |
| `--font-mono` (JetBrains Mono) | `font-mono` | Build-step timings, code |

Self-hosted critical faces (`./fonts/*.woff2`): Inter 400 + Cinzel 600, latin +
latin-ext (covers Turkish `ş ğ ı İ`). Other weights via `@fontsource` at runtime.

## Dark mode — token swap, never `dark:` overrides

`<html data-theme="dark">` swaps four tokens (`parchment`, `ink`, `stone-700`,
`gold-500`); everything else inherits. **Components never write `dark:bg-*`
overrides** — they use the token classes above and the theme does the work. Body
text in dark is a warm neutral light, NOT gold (gold stays on links/accents).

## Spacing & shape

- Radii: `--radius: 4px` (default), `--radius-sm: 2px`. Understated corners.
- Spacing/sizing: Tailwind's default scale (no custom spacing tokens).

## Component vocabulary (source `.astro`, for reference)

Build with these; they already encode the idiom. Not React — Astro components.

- **ui/**: `Alert`, `Badge`, `Card`, `Dialog`, `FilterBar`, `Icon`, `Separator`,
  `Skeleton`, `Stat`, `Tabs`, `Tooltip`
- **content/**: `CivCard`, `BuildOrderSteps`, `UniqueUnitBlock`, `TableOfContents`
- **layout/**: `BaseLayout`, `Header`, `Footer`, `LanguageSwitcher`, `ThemeToggle`

Domain icons (units/civs/techs/buildings): `<Icon slug="..." kind="unit|civ|tech|building" />`
from `components/ui/Icon.astro`, backed by `src/data/icon-map.json`.

## Idiomatic snippet

```astro
---
import Card from "@/components/ui/Card.astro";
import Badge from "@/components/ui/Badge.astro";
---
<Card>
  <h2 class="font-display text-lg text-ink">Britons</h2>
  <p class="text-stone-700">Foot Archer civilization from Western Europe.</p>
  <Badge variant="outline" class="text-gold-700">Archer</Badge>
</Card>
```

The control comes from the library (`Card`, `Badge`); your own layout glue uses the
token classes (`font-display`, `text-ink`, `text-stone-700`, `text-gold-700`). Never
hard-code colors or fonts — always go through the token classes so dark mode and the
medieval palette hold.
