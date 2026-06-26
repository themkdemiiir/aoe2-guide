# Build Orders — Bilingual Single-File Refactor

> **For agentic workers:** Use superpowers:executing-plans to implement this plan task-by-task.

**Goal:** Collapse each build order from 2 locale MD files (92 total) into 1 bilingual MD file (46 total) using `{ en, tr }` YAML objects for translated fields.

**Architecture:** Single `src/content/build-orders/slug.md` per build. Frontmatter holds all data including `name: { en, tr }`, `assign: { en, tr }` per step, `intro: { en, tr }`, `strategy: { en: string[], tr: string[] }`. MD body removed (content is in frontmatter). Zod validates at build time.

**Why not JSON:** YAML frontmatter is already the native format here — multiline strings don't need escaping, comments allowed, no trailing commas. Same `glob()` loader, no new parser.

**Tech Stack:** Node.js migration script (js-yaml for parsing), Astro Content Layer glob loader, Zod, TypeScript strict.

---

### Task 1: Install js-yaml dev dep

**Files:**
- Modify: `package.json` (devDependencies)

- [ ] **Step 1: Install js-yaml**

```bash
pnpm add -D js-yaml @types/js-yaml
```

- [ ] **Step 2: Verify**

```bash
node -e "import('js-yaml').then(m => console.log('ok', m.default.dump({ a: 1 })))"
```

Expected: `ok a: 1\n`

---

### Task 2: Write migration script

**Files:**
- Create: `scripts/migrate-builds-to-bilingual.mjs`

- [ ] **Step 1: Create the script**

```js
#!/usr/bin/env node
// One-time migration: merge EN+TR build order MD pairs → single bilingual MD.
// Run: node scripts/migrate-builds-to-bilingual.mjs
// After verifying output, delete src/content/build-orders/en/ and tr/.

import { readdirSync, readFileSync, writeFileSync, existsSync, mkdirSync } from "node:fs";
import { join } from "node:path";
import yaml from "js-yaml";

const EN_DIR = "src/content/build-orders/en";
const TR_DIR = "src/content/build-orders/tr";
const OUT_DIR = "src/content/build-orders";

function splitFrontmatter(raw) {
  const m = raw.match(/^---\n([\s\S]*?)\n---\n([\s\S]*)$/);
  if (!m) throw new Error("No frontmatter found");
  return { fm: yaml.load(m[1]), body: m[2].trim() };
}

function parseBody(body) {
  const lines = body.split("\n");
  // Drop the H1 title line
  const content = lines.filter((l) => !l.startsWith("# ")).join("\n").trim();
  const h2 = content.indexOf("\n## ");
  const introPart = h2 !== -1 ? content.slice(0, h2).trim() : content;
  const stratPart = h2 !== -1 ? content.slice(h2 + 4) : "";
  // Extract intro: first non-empty paragraph
  const intro = introPart.split(/\n\n/)[0]?.replace(/\n/g, " ").trim() ?? "";
  // Extract strategy bullets
  const strategy = stratPart
    .split("\n")
    .filter((l) => l.startsWith("- "))
    .map((l) => l.slice(2).trim());
  return { intro, strategy };
}

function mergeSteps(enSteps, trSteps) {
  return enSteps.map((en, i) => {
    const tr = trSteps[i] ?? en;
    const step = {};
    if (en.villagers != null) step.villagers = en.villagers;
    if (en.phase) step.phase = en.phase;
    if (en.time) step.time = en.time;
    step.assign = { en: en.assign, tr: tr.assign ?? en.assign };
    if (en.note || tr.note) step.note = { en: en.note ?? "", tr: tr.note ?? en.note ?? "" };
    if (en.icons?.length) step.icons = en.icons;
    return step;
  });
}

const enFiles = readdirSync(EN_DIR).filter((f) => f.endsWith(".md"));
let count = 0;

for (const fname of enFiles) {
  const enRaw = readFileSync(join(EN_DIR, fname), "utf8");
  const trPath = join(TR_DIR, fname);
  const trRaw = existsSync(trPath) ? readFileSync(trPath, "utf8") : enRaw;

  const { fm: en, body: enBody } = splitFrontmatter(enRaw);
  const { fm: tr, body: trBody } = splitFrontmatter(trRaw);

  const enProse = parseBody(enBody);
  const trProse = parseBody(trBody);

  const merged = {
    slug: en.slug,
    name: { en: en.name, tr: tr.name ?? en.name },
    difficulty: en.difficulty,
    targetAge: en.targetAge,
    durationMin: en.durationMin,
    civsRecommended: en.civsRecommended,
    ...(en.source ? { source: en.source } : {}),
    steps: mergeSteps(en.steps ?? [], tr.steps ?? []),
    ...(enProse.intro
      ? { intro: { en: enProse.intro, tr: trProse.intro || enProse.intro } }
      : {}),
    ...(enProse.strategy.length
      ? {
          strategy: {
            en: enProse.strategy,
            tr: trProse.strategy.length ? trProse.strategy : enProse.strategy,
          },
        }
      : {}),
  };

  const fmYaml = yaml.dump(merged, { lineWidth: 120, quotingType: '"', forceQuotes: false });
  const out = `---\n${fmYaml}---\n`;
  writeFileSync(join(OUT_DIR, fname), out, "utf8");
  count++;
  console.log(`  ✓  ${fname}`);
}
console.log(`\nMigrated ${count} build orders → ${OUT_DIR}/`);
```

- [ ] **Step 2: Run migration (dry-run first — inspect output)**

```bash
node scripts/migrate-builds-to-bilingual.mjs
```

Spot-check output:
```bash
head -40 src/content/build-orders/18pop-scouts.md
```

Expected: single file with `name: {en: ..., tr: ...}` and steps with bilingual assign/note.

---

### Task 3: Update config.ts schema

**Files:**
- Modify: `src/content/config.ts`

- [ ] **Step 1: Add localizedString helper and update buildOrders schema**

Replace the entire `buildOrders` collection definition:

```typescript
const localizedString = z.object({ en: z.string(), tr: z.string() });

const buildOrders = defineCollection({
  loader: glob({
    pattern: "*.{md,mdx}",
    base: "./src/content/build-orders",
    generateId: ({ entry }) => entry.replace(/\.(md|mdx)$/i, ""),
  }),
  schema: z.object({
    slug: z.string(),
    name: localizedString,
    difficulty: z.enum(["beginner", "intermediate", "advanced"]),
    targetAge: z.enum(["feudal", "castle", "imperial"]),
    durationMin: z.number(),
    civsRecommended: z.array(z.string()),
    steps: z.array(
      z.object({
        villagers: z.number().optional(),
        phase: z.enum(["feudal", "castle", "imperial"]).optional(),
        time: z.string().optional(),
        assign: localizedString,
        note: localizedString.optional(),
        icons: z.array(z.string()).optional(),
      }),
    ),
    intro: localizedString.optional(),
    strategy: z
      .object({ en: z.array(z.string()), tr: z.array(z.string()) })
      .optional(),
    source: z
      .object({ author: z.string(), url: z.string().url().optional() })
      .optional(),
  }),
});
```

- [ ] **Step 2: Verify types compile**

```bash
pnpm check
```

Expected: Errors only from callers not yet updated (Step interfaces). That's expected — continue.

---

### Task 4: Update lib/content.ts

**Files:**
- Modify: `src/lib/content.ts`

- [ ] **Step 1: Remove "build-orders" from ContentType + fix canonicalSlug**

1. Remove `"build-orders"` from the `ContentType` union.
2. Update `canonicalSlug` to handle flat IDs (no locale prefix):

```typescript
export function canonicalSlug(id: string): string {
  const segs = id.split("/");
  if (segs[0] === "en" || segs[0] === "tr") segs.shift();
  return segs.join("/").replace(/\.(md|mdx)$/i, "");
}
```

---

### Task 5: Update BuildOrderSteps.astro

**Files:**
- Modify: `src/components/content/BuildOrderSteps.astro`

- [ ] **Step 1: Update Step interface and all assign/note usages**

Change the `Step` interface:
```typescript
interface Step {
  villagers?: number;
  phase?: "feudal" | "castle" | "imperial";
  time?: string;
  assign: { en: string; tr: string };
  note?: { en: string; tr: string };
  icons?: string[];
}
```

Update every use of `step.assign` and `step.note` to include `[lang]`:
- `phaseFromAssign(step.assign)` → `phaseFromAssign(step.assign[lang])`
- `isTransitionAssign(step.assign)` → `isTransitionAssign(step.assign[lang])`
- `{s.assign}` → `{s.assign[lang]}`
- `{s.note && ...{s.note}` → `{s.note?.[lang] && ...{s.note[lang]}`
- `data-assign={s.assign}` → `data-assign={s.assign[lang]}`

---

### Task 6: Update builds/[build].astro

**Files:**
- Modify: `src/pages/[lang]/builds/[build].astro`

- [ ] **Step 1: Replace localized helper with direct getCollection**

Replace the `getStaticPaths` and entry loading:

```typescript
import { getCollection } from "astro:content";

export async function getStaticPaths() {
  const paths: Array<{ params: { lang: string; build: string } }> = [];
  for (const lang of locales) {
    const builds = await getCollection("build-orders");
    for (const b of builds) paths.push({ params: { lang, build: b.id } });
  }
  return paths;
}

const lang = Astro.params.lang as Locale;
const slug = Astro.params.build as string;
const entry = (await getCollection("build-orders")).find((b) => b.id === slug);
if (!entry) return Astro.redirect("/404");
```

- [ ] **Step 2: Update all data accesses**

- `entry.data.name` → `entry.data.name[lang]`
- Remove `render(entry)` and `const { Content } = ...`
- Remove `render` from the import
- In `howToLd.step`: `step.assign` → `step.assign[lang]`, `step.note` → `step.note?.[lang]`
- The `description` string: `entry.data.name[lang]`

- [ ] **Step 3: Replace `<Content />` with structured rendering**

Replace the strategy section:
```astro
<section class="prose prose-stone mt-10 max-w-none rounded border border-stone-700/30 bg-parchment px-5 py-6">
  <h2 class="font-display text-3xl text-ink">{t(lang, "builds.strategyNotes")}</h2>
  {entry.data.intro?.[lang] && (
    <p class="mt-2 text-stone-700">{entry.data.intro[lang]}</p>
  )}
  {entry.data.strategy?.[lang] && (
    <ul class="mt-4 list-inside list-disc space-y-2 text-ink">
      {entry.data.strategy[lang].map((point) => <li>{point}</li>)}
    </ul>
  )}
</section>
```

---

### Task 7: Update builds/index.astro

**Files:**
- Modify: `src/pages/[lang]/builds/index.astro`

- [ ] **Step 1: Replace getLocalizedEntries with getCollection**

```typescript
import { getCollection } from "astro:content";
// Remove: import { getLocalizedEntries, canonicalSlug } from "@/lib/content";

const builds = await getCollection("build-orders");
```

- [ ] **Step 2: Update template data accesses**

- `b.data.name` → `b.data.name[lang]`
- `canonicalSlug(b.id)` → `b.id` (IDs are now already the slug)
- `data-search` attribute: `b.data.name[lang]`

---

### Task 8: Update index.astro (homepage)

**Files:**
- Modify: `src/pages/[lang]/index.astro`

- [ ] **Step 1: Swap getLocalizedEntries for getCollection for builds only**

```typescript
import { getCollection } from "astro:content";

const builds = await getCollection("build-orders");
```

- [ ] **Step 2: Update all build name and link references**

Grep for all `b.data.name` and `canonicalSlug(b.id)` in the builds section and update to `b.data.name[lang]` and `b.id` respectively.

---

### Task 9: Update i18n-coverage.mjs

**Files:**
- Modify: `scripts/i18n-coverage.mjs`

- [ ] **Step 1: Exclude build-orders from the file-pair check**

After the `const enFiles = ...` line, add an exclusion for build-orders (which no longer have en/tr subdirs):

```js
const enFiles = walk(CONTENT_ROOT)
  .filter((f) => f.includes(EN_SEG))
  .filter((f) => !f.includes(`${sep}build-orders${sep}`));
```

---

### Task 10: Build and verify

- [ ] **Step 1: Run full build**

```bash
pnpm build
```

Expected: 0 errors. 92 routes for build pages (46 slugs × 2 locales).

- [ ] **Step 2: Run i18n coverage check**

```bash
pnpm i18n:coverage
```

Expected: passes (build-orders excluded from pair check).

- [ ] **Step 3: Smoke test in browser**

```bash
pnpm preview
```

Open `/en/builds/18pop-scouts` and `/tr/builds/18pop-scouts`. Confirm:
- EN page shows EN step assigns
- TR page shows TR step assigns
- Strategy notes section renders
- Trainer (timer, copy, focus) still works

---

### Task 11: Delete old locale dirs and commit

- [ ] **Step 1: Remove old EN/TR build order files**

```bash
rm -rf src/content/build-orders/en src/content/build-orders/tr
```

- [ ] **Step 2: Verify build still passes**

```bash
pnpm build
```

- [ ] **Step 3: Commit**

```bash
git add -A
git commit -m "refactor(build-orders): collapse EN/TR MD pairs into single bilingual files

46 YAML frontmatter files replace 92 locale-split files.
Translated fields use { en, tr } objects; structural data (villagers,
icons, timing) lives once. Aligns with the counters.json pattern
already used in the codebase."
```
