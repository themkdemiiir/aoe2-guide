# Translation Workflow

English (EN) is the source of truth. TR, ES, and DE are translated from EN.

## How fallback works

The Astro i18n config uses `fallbackType: "rewrite"` with `{ tr: "en", es: "en", de: "en" }`. When a request hits `/tr/civs/portuguese/` but the TR file doesn't exist, Astro serves the EN content at the TR URL. The page shows a "translation in progress" banner driven by the `ui.translation.missing` UI string.

## Adding a translation

```bash
# Suppose en/britons.md exists. Scaffold tr/, es/, de/ versions:
pnpm new:guide civilizations britons
```

This creates empty frontmatter scaffolds in `src/content/civilizations/{tr,es,de}/britons.md`. Open each and fill in:

1. **Localized fields only** — name, tagline, bonuses text, teamBonus, uniqueTechs.\*.name/effect, prose body.
2. **Keep the slug identical** — `slug: britons` in every language.
3. **Numeric/structured fields stay the same** as EN — they're not translated (era, tier, unique unit references, etc. live in `src/data/civilizations.json`).

## Checking coverage

```bash
pnpm check:translations
```

Reports per-language coverage: which slugs exist in EN but not TR/ES/DE, broken down by content type.

## What to translate vs leave in English

| Translate | Don't translate |
|---|---|
| Civilization names ("Britons" → "Britanos") | Slug fields (`britons` stays `britons`) |
| Unit role descriptions | Unit costs, HP, attack values |
| Bonus text, team bonus | Tier letter, era |
| Build-order step `assign` and `note` | `time`, `villagers`, `difficulty` enum values |
| Prose bodies | Source author/URL |
| Map playstyle / opening prose | Map `type` and `size` enum values |

## Common AOE2 terms — translation conventions

Some terms are universally retained in English (the AOE2 community uses them across all locales): Loom, Bloodlines, Fletching, Wheelbarrow, Hand Cart, Feudal/Castle/Imperial Age (often kept in English even in Turkish content), Town Center (or "Şehir Merkezi" in TR). When in doubt, keep the English term in parentheses.

## Build-step formatting

The `assign` text for steps is the most important translated field. Keep it short — 4-8 words. Examples:

| EN | TR | ES | DE |
|---|---|---|---|
| `6 → sheep under TC` | `6 köylü koyunda` | `6 → ovejas bajo el TC` | `6 → Schafe unter dem DZ` |
| `+2 → wood, build lumber camp` | `+2 köylü oduna, Lumber Camp kur` | `+2 → madera, construir aserradero` | `+2 → Holz, Holzfällerlager bauen` |
| `Research Loom; click Feudal` | `Loom araştır; Feudal'e tıkla` | `Investigar Tejido; subir a Feudal` | `Loom erforschen; Feudalzeit klicken` |

## Names: when to translate the noun vs keep English

Game-canonical English names are retained in TR content frequently because the game itself uses English unit names in many locales. For example, "Longbowman" might stay as "Longbowman" in a Turkish build description, or be translated to "Uzunyay Okçusu" — both are acceptable. The audience reads both.

For prose descriptions, translate fully. For named units inside `name:` fields, follow community norms.
