# AOE2 Guide — Design Spec

**Date:** 2026-05-23
**Status:** Approved
**Owner:** themkdemiiir

A multi-language static guide site for Age of Empires II covering civilizations, build orders, units, maps, matchups, a beginner curriculum, and a glossary. Deployable to Cloudflare Pages with no server-side runtime.

---

## 1. Goals & Constraints

### Goals
- Single, well-curated AOE2 reference site in **four languages** (Turkish, English, Spanish, German).
- Coverage: civilizations, build orders, units, maps, matchups, beginner path, glossary.
- Welcoming to all skill levels.
- Medieval visual identity that fits the subject without sacrificing readability.
- Built so a future contributor community can add content via git PRs.
- Future-proof for one specific interactive feature: a **civilization comparator** widget.

### Hard constraints
- **No server-side JavaScript at runtime.** Pure static HTML/CSS/JS served by CDN.
- **No SSR, no API routes, no edge functions, no ISR.**
- Build-time Node tooling (Vite / Astro) is acceptable; the build artifact must be static.
- Host on **Cloudflare Pages**.
- Content must remain easy to author in Markdown for non-developers.

### Non-goals
- User accounts, comments, forums.
- Live game data (ladder ELO, match history) — out of scope.
- Tier lists are deferred until a stable curation rhythm exists.
- Mobile app, desktop app, Discord bot.

---

## 2. Stack

| Layer | Choice | Rationale |
|---|---|---|
| Site generator | **Astro 4+** (static output mode) | Native i18n, content collections + Zod, islands architecture for the future civ comparator, MDX support. Built on Vite (user already knows Vite). |
| Styling | **TailwindCSS** with custom medieval tokens | Fast iteration, design tokens map cleanly to CSS vars, no runtime cost. |
| Content authoring | **Markdown / MDX** in Astro Content Collections | Low barrier for community PRs, schema-validated via Zod. |
| Search | **Pagefind** | Client-side, no backend, multi-language. Indexes built HTML. |
| Interactivity | **Astro Islands** (zero by default, opt-in per component) | Civ comparator hydrates only its own component. |
| Deployment | **Cloudflare Pages** (git integration) | Free, global CDN, deploy-on-push. |
| CI | **GitHub Actions** | Build check + schema validation on PR. |
| Package manager | **pnpm** | Faster, stricter than npm. |

### Stack alternatives evaluated and rejected
- **Next.js (static export):** overkill for static content, ships more JS by default, App Router i18n is more setup than Astro's built-in.
- **Hugo:** mature i18n and instant builds, but Go templates are awkward for the component-driven medieval UI; harder to add the future civ-comparator widget.
- **Eleventy:** lighter than Astro but less batteries-included for content schemas and i18n.

---

## 3. Languages & Audience

- **Languages at launch:** Turkish (TR), English (EN), Spanish (ES), German (DE). EN is the source-of-truth language; others fall back to EN when a translation is missing.
- **Audience:** all skill levels. The beginner path serves new players; deeper content (matchups, advanced build orders) serves competitive players.

---

## 4. Repository Structure

```
aoe2-guide/
├── README.md
├── CLAUDE.md                   ← project-level (stack, workflow, invariants)
├── LICENSE
├── astro.config.mjs            ← Astro config (i18n, integrations)
├── tailwind.config.mjs         ← medieval theme tokens
├── tsconfig.json
├── package.json
├── pnpm-lock.yaml
├── .nvmrc · .gitignore · .editorconfig · .prettierrc
│
├── .github/
│   ├── workflows/ci.yml        ← build check + schema validation on PR
│   ├── PULL_REQUEST_TEMPLATE.md
│   └── ISSUE_TEMPLATE/
│
├── docs/
│   ├── CONTRIBUTING.md         ← how to add a guide / translation
│   ├── content-schemas.md      ← field reference for every content type
│   ├── translation-workflow.md
│   └── superpowers/specs/
│       └── 2026-05-23-aoe2-guide-design.md
│
├── md/                         ← raw source guides (user inputs, not built)
│   ├── README.md
│   ├── CLAUDE.md               ← marks dir as input-only, not site content
│   └── build-orders/           ← user drops build-order source material here
│
├── public/                     ← static assets served as-is
│   ├── favicon.svg
│   ├── images/
│   │   ├── aoe2/{civs,units,techs,buildings,ages,resources}/  ← synced from aoe2techtree
│   │   └── ui/                 ← logo, ornaments, custom SVGs
│   ├── fonts/                  ← self-hosted Cinzel, Inter, JetBrains Mono
│   └── robots.txt
│
├── src/
│   ├── content/                ← Astro Content Collections
│   │   ├── CLAUDE.md           ← schemas + how to add an entry
│   │   ├── config.ts           ← Zod schemas for every collection
│   │   ├── civilizations/{en,tr,es,de}/*.md
│   │   ├── build-orders/{en,tr,es,de}/*.md
│   │   ├── units/{en,tr,es,de}/*.md
│   │   ├── maps/{en,tr,es,de}/*.md
│   │   ├── matchups/{en,tr,es,de}/*.md
│   │   ├── beginner/{en,tr,es,de}/*.md
│   │   └── glossary/{en,tr,es,de}/*.md
│   │
│   ├── data/                   ← language-agnostic structured data
│   │   ├── civilizations.json  ← slug, era, region, specialty, tier, patch, unique-unit slugs
│   │   ├── unit-stats.json     ← HP, attack, range, cost, train_time
│   │   ├── counters.json       ← unit→strong-against/weak-against
│   │   ├── tech-tree.json      ← per-civ tech availability matrix
│   │   └── icon-map.json       ← slug → /images/aoe2/.../N.png
│   │
│   ├── i18n/
│   │   ├── ui.ts               ← UI strings (nav, buttons) for 4 locales
│   │   ├── locales.ts          ← locale metadata
│   │   └── utils.ts            ← getLocale(), t(), getLocalizedPath()
│   │
│   ├── components/
│   │   ├── CLAUDE.md
│   │   ├── layout/             ← BaseLayout, Header, Footer, Sidebar, LanguageSwitcher, ThemeToggle
│   │   ├── content/            ← CivCard, UnitStatTable, BuildOrderSteps, CounterMatrix, TechTree, MapPreview
│   │   ├── ui/                 ← Card, Badge, Scroll, Ornament, Icon, SearchModal
│   │   └── islands/            ← future interactive widgets (CivComparator)
│   │
│   ├── pages/
│   │   ├── index.astro         ← / → locale chooser landing
│   │   ├── 404.astro
│   │   └── [lang]/
│   │       ├── index.astro
│   │       ├── civs/{index, [civ]}.astro
│   │       ├── builds/{index, [build]}.astro
│   │       ├── units/{index, [unit]}.astro
│   │       ├── maps/{index, [map]}.astro
│   │       ├── matchups/[matchup].astro
│   │       ├── learn/{index, [chapter]}.astro
│   │       ├── glossary/index.astro
│   │       ├── search.astro
│   │       └── about.astro
│   │
│   ├── styles/                 ← globals.css, tokens.css, prose.css
│   ├── lib/                    ← content helpers, route builders
│   ├── env.d.ts
│   └── consts.ts
│
└── scripts/
    ├── sync-assets.mjs         ← pull aoe2 icons from aoe2techtree (pinned SHA)
    ├── build-icon-map.mjs      ← generate icon-map.json from aoc-reference-data
    ├── new-guide.mjs           ← scaffold one entry in all 4 langs
    ├── check-translations.mjs  ← find missing translations
    └── import-from-md.mjs      ← promote /md file → src/content/
```

### CLAUDE.md files (4, hierarchical)

| Path | Purpose |
|---|---|
| `/CLAUDE.md` | Project overview, stack, static-only invariant, branch/PR workflow, commands |
| `/src/content/CLAUDE.md` | Content schemas, how to add an entry, multilang naming rules |
| `/src/components/CLAUDE.md` | Component conventions, Astro-vs-island decision tree, theme tokens |
| `/md/CLAUDE.md` | Marks dir as raw input material, describes curation pipeline `md/ → src/content/` |

---

## 5. Content Model

### Split strategy

| Lives in `src/data/*.json` (lang-agnostic, single source of truth) | Lives in `src/content/<type>/<lang>/<slug>.md` (per-language) |
|---|---|
| numeric stats (HP, attack, cost) | `name`, `tagline`, prose body |
| slug→slug references (counters, unique units, recommended builds) | localized bullet lists (bonus text, tips) |
| era / patch / tier (lang-neutral facts) | section headings, captions |

**Rationale:** patch updates touch one JSON file rather than 400 markdown files. Translators only touch text. The future civ-comparator reads only from `src/data/` and works without re-loading per-language copies.

### Schemas (Zod, `src/content/config.ts`)

```ts
import { z, defineCollection } from "astro:content";

const civilizations = defineCollection({
  schema: z.object({
    slug: z.string(),
    name: z.string(),               // localized
    tagline: z.string(),            // localized
    bonuses: z.array(z.string()),   // localized
    teamBonus: z.string(),          // localized
    uniqueTechs: z.object({
      castle: z.object({ name: z.string(), effect: z.string() }),
      imperial: z.object({ name: z.string(), effect: z.string() }),
    }),
  }),
});

const buildOrders = defineCollection({
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    difficulty: z.enum(["beginner", "intermediate", "advanced"]),
    targetAge: z.enum(["feudal", "castle", "imperial"]),
    durationMin: z.number(),
    civsRecommended: z.array(z.string()),
    steps: z.array(z.object({
      villagers: z.number(),
      time: z.string(),             // "0:00", "4:30"
      assign: z.string(),           // localized
      note: z.string().optional(),  // localized
    })),
    source: z.object({
      author: z.string(),
      url: z.string().url().optional(),
    }),
  }),
});

const units = defineCollection({
  schema: z.object({
    slug: z.string(),
    name: z.string(),               // localized
    role: z.string(),               // localized
    civ: z.string().optional(),     // civ slug if unique
  }),
});

const maps = defineCollection({
  schema: z.object({
    slug: z.string(),
    name: z.string(),
    type: z.enum(["open", "closed", "hybrid", "water", "nomad"]),
    size: z.enum(["tiny", "small", "medium", "large"]).optional(),
    recommendedCivs: z.array(z.string()),
  }),
});

const matchups = defineCollection({
  schema: z.object({
    slug: z.string(),                                          // "britons-vs-franks"
    civA: z.string(),
    civB: z.string(),
    difficulty: z.enum(["even", "favored", "unfavored"]),
  }),
});

const beginner = defineCollection({
  schema: z.object({
    slug: z.string(),
    title: z.string(),              // localized
    order: z.number(),              // chapter sequence
    prereq: z.array(z.string()).optional(),
  }),
});

const glossary = defineCollection({
  schema: z.object({
    slug: z.string(),
    term: z.string(),               // localized
    letter: z.string(),             // alphabetical index
  }),
});

export const collections = {
  civilizations,
  "build-orders": buildOrders,
  units,
  maps,
  matchups,
  beginner,
  glossary,
};
```

### Shared structured data (`src/data/*.json`)

- `civilizations.json` — array of `{ slug, era, region, specialty, tier, patch, uniqueUnits: [], strongAgainst: [], weakAgainst: [], recommendedBuilds: [] }`.
- `unit-stats.json` — array of `{ slug, hp, attack, range, cost: { wood, food, gold, stone }, trainTime }`.
- `counters.json` — `{ slug: { strongAgainst: [], weakAgainst: [] } }`.
- `tech-tree.json` — per-civ tech availability matrix.
- `icon-map.json` — slug → asset path mapping (generated by `scripts/build-icon-map.mjs`).

---

## 6. Routing & i18n

### URL structure

```
/                                  → locale chooser landing
/<lang>/                           → locale home (en, tr, es, de)
/<lang>/civs/                      → civ index
/<lang>/civs/<slug>/               → civ detail
/<lang>/builds/                    → build orders index
/<lang>/builds/<slug>/             → build detail
/<lang>/units/                     → unit index (filterable)
/<lang>/units/<slug>/              → unit detail
/<lang>/maps/                      → map index
/<lang>/maps/<slug>/               → map detail
/<lang>/matchups/<slugA>-vs-<slugB>/  → matchup
/<lang>/learn/                     → beginner index
/<lang>/learn/<chapter>/           → chapter
/<lang>/glossary/                  → alphabetical single-page index
/<lang>/search/                    → search page
/<lang>/about/                     → about + attribution + contributors
```

### Slug policy: English-canonical across all locales

`/tr/civs/britons/` not `/tr/uygarliklar/britonlar/`. Page titles are localized; URLs are not. AOE2 community conventions are English-first, so this matches expectations and keeps cross-language linking deterministic.

### Astro i18n config

```js
i18n: {
  defaultLocale: "en",
  locales: ["en", "tr", "es", "de"],
  routing: {
    prefixDefaultLocale: true,
    redirectToDefaultLocale: false,
  },
  fallback: { tr: "en", es: "en", de: "en" },
}
```

Missing translations auto-serve EN content under the localized URL with a "translation in progress" banner. Translators see exactly what needs work.

### Root `/` behavior

A locale-chooser landing page (hero + 4 language buttons), plus an inline `<head>` script that reads `navigator.language` and `localStorage.preferredLang` to auto-redirect on subsequent visits. First-time visitors see the brand; returning visitors land in their language.

### Language switcher

Always in the header. Switching language on a guide page lands on the same guide in the new language (path rewrite, since slugs match). Falls back to localized home if no translation exists.

### SEO

Every page emits `<link rel="alternate" hreflang="...">` for all 4 locales + `x-default` → EN. Per-page canonical points to that page's own language. Auto-generated `sitemap.xml` via `@astrojs/sitemap` lists every page in every language.

### Search

Single Pagefind index, scoped per-language via `data-pagefind-filter="lang:tr"` attributes on indexed content. `/<lang>/search/` is the dedicated page; a search modal in the header is available globally. Switching language preserves the query.

---

## 7. Visual System

### Palette (CSS custom properties in `src/styles/tokens.css`)

**Light (parchment, default):**
```
--parchment    #f5ecd7    page background
--ink          #1f1812    primary text
--stone-700    #3d3530    secondary text, borders
--gold-500     #c9a44c    links, primary CTAs
--royal-red    #8b1e1e    warnings, important callouts
--forest-700   #3d6936    eco/food indicators
--steel-600    #4a6079    counter/info indicators
```

**Dark (candlelit stone):**
```
--stone-bg     #1c1611    page background
--ember        #f0d99a    primary text
--parchment-60 #5a4f3f    secondary text
--gold-400     #d9b760    accents
```

**Resource colors (locked to game convention):**
```
--res-food   #d44d4d    --res-wood   #8b7355
--res-gold   #dcbe4c    --res-stone  #888888
```

### Typography (self-hosted in `public/fonts/`)

- **Display (h1, h2):** Cinzel — Roman/medieval feel, free, readable.
- **Body:** Inter — strong coverage for TR/ES/DE special characters, highly readable.
- **Mono (build-step timings):** JetBrains Mono.
- No blackletter/Gothic fonts — readability over thematic fidelity.

### Component visual character

- Cards: subtle parchment SVG background (~3% opacity), 2px stone border, 4px radius (sharp, not modern-rounded).
- Section dividers: short SVG ornament between major sections.
- Buttons: filled gold-on-parchment (primary) or outlined stone (secondary). Subtle inset shadow on hover, never glow.
- Build-order step rows: monospace time column + villager count + assignment text, alternating row tint.
- Civ cards: framed icon (stone border SVG) + name + tagline + top 3 bonuses; hover reveals strong-against badges.

### Reading layout

- Prose max-width: 75ch.
- Sidebar on `lg+`, collapsible drawer on mobile.
- Sticky header: logo (left) · primary nav (center) · search + language switcher + theme toggle (right).
- Footer: attribution block + nav + repo link.

### Theme switching

`html[data-theme="dark"]` toggle persisted in localStorage. Three states: `auto` (system), `light`, `dark`. Inline `<head>` script reads localStorage before paint to prevent FOUC.

### Icon strategy

- Content icons (units, civs, techs, buildings): sourced from `SiegeEngineers/aoe2techtree`, wrapped in an `<Icon>` component that adds a stone-border SVG frame.
- UI icons (search, menu, theme toggle): Lucide, stroke-1.5, inherits text color.

---

## 8. Asset Pipeline

### Sources (only two)

| Repo | License | Used for |
|---|---|---|
| **SiegeEngineers/aoe2techtree** | MIT (code) + Microsoft Game Content Usage Rules (assets) | Civ icons, unit icons, tech icons, building icons, age icons, resource icons |
| **SiegeEngineers/aoc-reference-data** | **No license declared** (default copyright). The data is purely factual (ID→name pairs), which is generally not copyrightable. We use it as a reference only, regenerate our own `icon-map.json` from it, and do not redistribute the source file. | Slug-to-numeric-ID mapping (`data/datasets/100.json`) |

### Why these two only

There is no permissively-licensed, MS-IP-clean AOE2 icon set anywhere on GitHub. Every viable source ultimately leans on Microsoft's "Game Content Usage Rules" (fan use OK with attribution). aoe2techtree is the community-standard source every AOE2 tool builds on. GPL-3 alternatives (`recanalyst`, `RTS_Overlay`) would force the entire site to GPL. No-license repos (`qwyt`, `Civbuilder`) offer the same source material with weaker legal footing.

### Sync pipeline

- `scripts/sync-assets.mjs` — pins `aoe2techtree` to a specific commit SHA, fetches the `img/` tree, drops it into `public/images/aoe2/{civs,units,techs,buildings,ages,resources}/`. Pinning means asset changes are deliberate (a PR bumps the SHA), not surprise updates.
- `scripts/build-icon-map.mjs` — reads `aoc-reference-data/data/datasets/100.json`, produces `src/data/icon-map.json` mapping slugs like `"knight"` → `/images/aoe2/units/12.png`.
- Components reference icons via slug; the icon-map indirection means a future swap to redrawn icons is a single PR.

### Attribution

Required footer text on every page:

> Age of Empires II © Microsoft Corporation. This site was created under Microsoft's "Game Content Usage Rules" using assets from Age of Empires II, and it is not endorsed by or affiliated with Microsoft.

Plus an `/about/` (or `/attribution/`) section crediting aoe2techtree and aoc-reference-data with links.

---

## 9. Content Workflow

### Authoring flow (solo, current state)

1. Drop raw guide into `md/build-orders/<source>-<topic>.md` (or another `md/` subfolder).
2. Run `pnpm run import:from-md <file>` to scaffold an entry in `src/content/<type>/en/<slug>.md` with frontmatter pre-filled from the raw source.
3. Edit the scaffolded EN file: fill schema fields, write strategy notes.
4. Run `pnpm run new:translation <type> <slug>` to create empty TR/ES/DE versions for translation later.
5. `pnpm dev` for local preview, commit and push.

### Translation flow

- TR/ES/DE files are scaffolded with `name`, `tagline`, etc. left blank and a top-of-file marker.
- `pnpm run check:translations` reports per-language coverage (which slugs exist in EN but not TR/ES/DE).
- Missing translations auto-serve EN fallback with a banner.

### Community contributions (later)

- `docs/CONTRIBUTING.md` explains the markdown + schema workflow.
- GitHub Action runs `astro build` on every PR to catch schema errors before merge.
- PR template requires source attribution and language indicator.
- No CMS for now — markdown + git is the lowest-barrier path.

---

## 10. Deployment & CI

### Cloudflare Pages

- Git integration: push to `main` deploys to production, every other branch creates a preview deploy.
- Build command: `pnpm install --frozen-lockfile && pnpm build`.
- Output directory: `dist/`.
- Node version pinned via `.nvmrc`.

### GitHub Actions (`/.github/workflows/ci.yml`)

On every PR:
1. `pnpm install --frozen-lockfile`
2. `pnpm build` (catches schema errors and broken links)
3. `pnpm run check:translations` (reports coverage; does not block merge)
4. `pnpm run lint` (Prettier check, optional ESLint)

### No deploy workflow

Cloudflare Pages handles deploy directly from git — no GitHub Action needed for that.

---

## 11. Future Work (out of scope for first pass)

- **Civ comparator widget** — Astro island that reads `src/data/civilizations.json` + `unit-stats.json` and renders side-by-side comparison. Hydrates only on its own page.
- **Tier lists** — versioned per patch, deferred until curation rhythm exists.
- **Replay analyzer integration** — out of scope; would require runtime processing.
- **CMS migration** — possible if community contributions outgrow the markdown+git workflow.
- **Replacing icon source** — if a permissively-licensed redrawn set ever exists, swap via `icon-map.json` indirection.

---

## 12. Decision Log

| Decision | Chosen | Rejected | Reason |
|---|---|---|---|
| Site generator | Astro | Next.js, Hugo, Eleventy | Native i18n + content collections + islands match the use case |
| Runtime model | Static only | SSR / API routes | Explicit user constraint |
| Slug policy | English-canonical | Localized slugs per lang | AOE2 community is English-first; deterministic cross-lang links |
| Data split | JSON for facts, MD for prose | All in markdown frontmatter | Patch updates surgical; translations bounded to text |
| Root URL | Locale chooser landing | Auto-redirect to `/en/` | First-visit branding; static-friendly auto-redirect on return |
| Icon source | aoe2techtree + aoc-reference-data | recanalyst (GPL), qwyt (no license) | Permissive code license, community-standard, MS attribution path |
| CSS | Tailwind with custom tokens | shadcn/ui, plain CSS | Fast iteration + design tokens, no runtime cost |
| Package manager | pnpm | npm, yarn | Faster, stricter |

---

## 13. Open Questions

- **aoc-reference-data fallback path.** Currently no license is declared. We treat the file as factual data (non-copyrightable) and generate our own `icon-map.json` rather than redistributing the source. If this becomes a concern, we can build the ID→slug map by hand from the aoe2techtree filenames + community knowledge — slow but unambiguous.
- **Whether to add a Tier List content type later.** Deferred from initial scope; if the user wants patch-versioned tier lists, a new collection schema gets added.

All other design decisions confirmed by the user during brainstorming on 2026-05-23.
