# Components

## Folders

- `layout/` — page chrome (BaseLayout, Header, Footer, LanguageSwitcher, ThemeToggle)
- `content/` — domain-specific content rendering (CivCard, BuildOrderSteps, UnitStatTable, …)
- `ui/` — generic primitives (Card, Badge, Icon, future SearchModal)
- `islands/` — interactive React/Svelte/Vue widgets that hydrate client-side (e.g. future CivComparator)

## Astro component vs island — decision tree

- Static rendering, no JS needed → `.astro` component, no `client:*` directive. Default.
- Needs JS interactivity (state, event handlers, browser APIs) → framework component with `client:visible` (lazy) or `client:idle`.

Don't reach for islands unless interactivity actually requires JS. Most of this site stays static.

## Theme tokens — use Tailwind classes, not raw values

Use the Tailwind utility classes that map to our `@theme {}` tokens:

| Token | Class | Use case |
|---|---|---|
| `--color-parchment` | `bg-parchment`, `text-parchment` | Page background |
| `--color-ink` | `text-ink` | Primary text |
| `--color-stone-700` | `text-stone-700`, `border-stone-700/40` | Secondary text, borders |
| `--color-gold-500` | `text-gold-500`, `border-gold-500` | Links, primary accents |
| `--color-royal` | `text-royal`, `border-royal/40` | Warnings, important callouts |
| `--color-res-{food,wood,gold,stone}` | `text-res-food` etc. | Resource indicators |
| `--font-display` | `font-display` | Cinzel headings |
| `--font-body` | `font-body` | Inter body |
| `--font-mono` | `font-mono` | JetBrains Mono (build-step timings) |

Dark mode swaps the same tokens via `[data-theme="dark"]` — never write `dark:bg-foo` overrides; let the tokens do the work.

## Icons

- Domain icons (units, civs, techs): `<Icon slug="..." kind="unit|civ|tech|building" />` from `components/ui/Icon.astro`. Pulls from `src/data/icon-map.json`.
- UI icons (search, menu, theme toggle): inline SVG or `lucide` (when needed).

## Naming

- PascalCase filenames matching the component name (`CivCard.astro`).
- One component per file.
- Props typed inline via `interface Props { … }` at the top of frontmatter.

## When to create a new component vs reuse

Create a new component when:
- Two or more pages need the same visual unit.
- The page frontmatter has grown beyond ~40 lines of layout glue.
- A piece of UI needs its own state or lifecycle (island).

Don't create a component for one-off rendering that only one page uses — inline it.
