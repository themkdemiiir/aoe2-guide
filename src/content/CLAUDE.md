# Content Collections

Astro Content Layer API — schemas in `config.ts`, validated by Zod at build time.

## File layout

Most content types use a **single bilingual YAML file per entry**:

```
src/content/<type>/<slug>.yaml
```

Every translatable text field is a `{ en: "...", tr: "..." }` object (a `localizedString`).

- `<type>`: civilizations · build-orders · units · maps · glossary
- `<slug>`: English-canonical, kebab-case (e.g. `britons`, `21pop-archer`, `arabia`)

**Exceptions — separate locale dirs (MD):**

```
src/content/beginner/{en,tr}/<slug>.md
src/content/articles/{en,tr}/<slug>.md
```

These long-form types remain in per-language MD directories; `pathId` generates IDs like `en/intro`.

## Critical patterns

### Use Content Layer API, not legacy `type: "content"`

YAML collections use `loader: glob({ pattern: "*.{yaml,yml}", base, generateId })` with a custom `generateId` that strips the extension. MD collections (`beginner`, `articles`) use `pathId` to avoid slug collisions across locale dirs.

### Render bodies via `render(entry)`

```ts
import { render } from "astro:content";
const { Content, headings } = await render(entry);
```

The legacy `entry.render()` method does **not exist** on Content Layer entries.

### Bilingual gate: `audit-yaml-translations`

`pnpm build` runs `scripts/audit-yaml-translations.mjs` as a prebuild step. It walks every YAML entry and fails the build if any `localizedString` field has `en === tr` (identical/untranslated). Allow-listed fields (proper nouns that don't need translation): paths matching `name` or `term`, and values ending with a parenthesised proper noun. Fix by providing a real TR translation before building.

## Adding a new entry

### YAML types (civilizations, build-orders, units, maps, glossary)

1. Create `src/content/<type>/<slug>.yaml` — each text field as `{ en: "...", tr: "..." }`.
2. Consult `config.ts` for required fields (it is the authoritative schema).
3. `pnpm build` validates schema **and** runs the translation audit; both must pass.

> **Civilizations:** do not hand-edit the generated civ YAML. Run `pnpm build:civilizations` to regenerate EN+TR text from `aoe2techtree` locale strings (`.cache/aoe2-data/strings-{en,tr}.json`).

### MD types (beginner, articles)

1. Add `src/content/<type>/en/<slug>.md` as the EN source of truth.
2. Add a matching `src/content/<type>/tr/<slug>.md` for the TR translation (or scaffold with `pnpm new:guide <type> <slug>`). Missing TR falls back to EN automatically.
3. Schema-breaking frontmatter will fail `pnpm build` and CI.

## Data split

| Lives here (per language) | Lives in `src/data/*.json` (language-agnostic) |
|---|---|
| `name`, `tagline`, prose body, bullet text | numeric stats (HP, cost, attack) |
| section headings, captions | era, tier, patch |
| localized `assign` / `note` in build steps | slug→slug references (counters, recommended builds) |

Numeric/factual data has ONE source of truth; translations only touch text.

## Step format for build-orders

Each step in `steps[]` has:
- `villagers` (number) — the population count after this step's villagers complete
- `time` (string in `m:ss` format, e.g. `"4:30"`) — the in-game timestamp
- `assign` (localized string) — what those villagers do (e.g. `"6 → sheep under TC"`)
- `note` (optional localized string) — single-line tip (e.g. `"Research Loom"`)

Aim for 6-12 steps per build — enough to be useful, not exhaustive.

## Schema field reference

`config.ts` is authoritative. Read it directly when adding new content — don't trust your memory.
