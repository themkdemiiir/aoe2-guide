# Content Collections

Astro Content Layer API — schemas in `config.ts`, validated by Zod at build time.

## File layout

```
src/content/<type>/<lang>/<slug>.md
```

- `<type>`: civilizations · build-orders · units · maps · matchups · beginner · glossary
- `<lang>`: en · tr
- `<slug>`: English-canonical, kebab-case (e.g. `britons`, `21pop-archer`, `britons-vs-franks`)

## Critical patterns

### Use Content Layer API, not legacy `type: "content"`

Each collection uses `loader: glob({ pattern, base, generateId: pathId })`. The `generateId: pathId` override is **required** — without it, frontmatter `slug` values collide across the 2 language directories (all `britons.md` files would claim ID `britons`).

### Render bodies via `render(entry)`

```ts
import { render } from "astro:content";
const { Content, headings } = await render(entry);
```

The legacy `entry.render()` method does **not exist** on Content Layer entries.

## Adding a new entry

1. `pnpm import:md md/<type>/<source>.md` (from a raw source), or `pnpm new:guide <type> <slug>` (scaffold blank).
2. Fill the EN file's frontmatter — schema in `config.ts` is the source of truth for required fields.
3. Translate to TR by editing the scaffolded file. Missing TR translations fall back to EN automatically.
4. Schema-breaking changes will fail `pnpm build` and CI.

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
