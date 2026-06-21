# Content Accuracy & Translation Overhaul — Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Make every game fact source-derived (EN + TR from the official aoe2techtree
locales), remove 6 fabricated civilizations, fix the generator so regens preserve
translations, ground build orders in the Hera sources, and restore the translation
guardrail.

**Architecture:** The civ generator re-derives facts from `aoe2techtree` in **both**
languages (parse `strings-en.json` + `strings-tr.json`) and writes bilingual YAML +
`civilizations.json`; it carries over only hand-authored `strategy` prose. Unit roles and
build-order titles are filled by deterministic transforms over the existing YAML. Build
orders are verified/corrected against the Hera transcripts + strategy-guide text. A new
audit script gates `en === tr` in CI.

**Tech Stack:** Node ESM scripts (`.mjs`), `js-yaml`, Astro Content Layer + Zod,
`valibot` (validate-data), Vitest, pnpm.

## Global Constraints

- **Static-only** Astro 5; no SSR/adapters/API routes. TS strict.
- **Source-derived facts only.** Every game value comes from a trusted repo
  (`SiegeEngineers/aoe2techtree` `data.json` + `locales/{en,tr}/strings.json`;
  `aalises/age-of-empires-II-api`; `aoc-reference-data`) or the Hera materials in `md/`.
  Hand/AI authoring only for editorial copy with no source equivalent (build titles,
  strategy prose, unit `role` taxonomy, geographic region labels).
- **Slugs English-canonical**, kebab-case. Proper nouns (unit/tech/civ/map names,
  glossary terms) stay EN in the `tr` slot (allow-listed).
- **Bilingual YAML** is the content shape: each text field is `{ en, tr }`. Numeric/factual
  data lives once in `src/data/*.json`.
- Render via `render(entry)`; never `entry.render()`. No `@astrojs/tailwind`. No
  `tailwind.config.*`. Don't commit `dist/` or `.pagefind/`.
- Package manager **pnpm**. Pinned source SHAs in `scripts/sync-game-data.mjs`.
- Branch: `content-accuracy-translation-overhaul` (already created). Commit per task.

---

## Phase A — Civilizations: source both languages, remove the 6 (WS1+WS2)

### Task 1: Add the Turkish locale to the data sync

**Files:**
- Modify: `scripts/sync-game-data.mjs` (URLS array, after the `strings-en.json` entry)

**Interfaces:**
- Produces: cached file `.cache/aoe2-data/strings-tr.json` (official in-game TR help text,
  same `help_string_id` keys as EN).

- [ ] **Step 1: Add the TR strings URL**

In `scripts/sync-game-data.mjs`, inside the `URLS` array, immediately after the object
that ends with `dest: path.join(CACHE_DIR, "strings-en.json"),`, add:

```js
  {
    // Official in-game Turkish help text — civ bonuses/techs by help_string_id.
    url: `https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/${AOE2TT_SHA}/data/locales/tr/strings.json`,
    dest: path.join(CACHE_DIR, "strings-tr.json"),
  },
```

- [ ] **Step 2: Run the sync and verify both locales cached**

Run: `pnpm assets:sync >/dev/null 2>&1; node scripts/sync-game-data.mjs`
Expected: log lines for civilizations.csv, units.csv, data.json, strings-en.json, **strings-tr.json**.

Run: `node -e "const t=require('./.cache/aoe2-data/strings-tr.json'); console.log('TR keys:', Object.keys(t).length, '| Britons:', !!t['120150'])"`
Expected: `TR keys: 1063 | Britons: true`

- [ ] **Step 3: Commit**

```bash
git add scripts/sync-game-data.mjs
git commit -m "feat(sync): fetch official Turkish locale strings from aoe2techtree"
```

---

### Task 2: Generalized bilingual help-string parser (TDD)

The existing `parseHelpBonuses` only recognizes **English** section markers
(`Unique Unit`, `Unique Tech`, `Team Bonus`, trailing `civilization`). Turkish help text
uses `Özgün Birim`, `Özgün Teknoloji`, `Takım Bonusu`, trailing `medeniyeti`. Generalize it.

**Files:**
- Create: `scripts/lib/parse-help.mjs`
- Test: `tests/parse-help.test.mjs`

**Interfaces:**
- Produces: `export function parseHelp(raw, lang)` where `lang ∈ {"en","tr"}`. Returns
  `null` if no bullet bonuses, else `{ civType, civBonuses, teamBonus, uniqueTechs }`
  where `civType` is the first-line archetype (trailing " civilization"/" medeniyeti"
  stripped), `civBonuses: string[]`, `teamBonus: string`, `uniqueTechs: Array<{name, effect}>`.

- [ ] **Step 1: Write the failing test**

Create `tests/parse-help.test.mjs`:

```js
import { describe, it, expect } from "vitest";
import { parseHelp } from "../scripts/lib/parse-help.mjs";

const EN = [
  "Archer civilization", "",
  "• Shepherds work +25% faster",
  "• Town Centers cost -50% wood starting in the Castle Age", "",
  "Unique Unit:", "Longbowman (Archer)", "",
  "Unique Tech:",
  "• Yeomen (Foot archers +1 range; Towers +2 attack)",
  "• Warwolf (Trebuchets deal blast damage and are more accurate)", "",
  "Team Bonus:", "Archery Ranges work +10% faster",
].join("<br>");

const TR = [
  "Yaya Okçu medeniyeti", "",
  "• Çobanlar %25 daha hızlı çalışır",
  "• Şehir Merkezleri, Kale Çağı'ndan itibaren %50 daha az odun gerektirir", "",
  "Özgün Birim:", "Uzun Yay Okçusu (Yaya Okçu)", "",
  "Özgün Teknoloji:",
  "• Levazımcı (Yaya Okçulara 1 menzil; Gözcü Kulesi türlerine 2 saldırı)",
  "• Savaş Kurdu (Katapultlar patlama hasarı verir ve daha isabetlidir)", "",
  "Takım Bonusu:", "Okçuluk Binası %10 daha hızlı çalışır",
].join("<br>");

describe("parseHelp", () => {
  it("parses English help text", () => {
    const r = parseHelp(EN, "en");
    expect(r.civType).toBe("Archer");
    expect(r.civBonuses).toEqual([
      "Shepherds work +25% faster",
      "Town Centers cost -50% wood starting in the Castle Age",
    ]);
    expect(r.teamBonus).toBe("Archery Ranges work +10% faster");
    expect(r.uniqueTechs).toEqual([
      { name: "Yeomen", effect: "Foot archers +1 range; Towers +2 attack" },
      { name: "Warwolf", effect: "Trebuchets deal blast damage and are more accurate" },
    ]);
  });

  it("parses Turkish help text with localized section markers", () => {
    const r = parseHelp(TR, "tr");
    expect(r.civType).toBe("Yaya Okçu");
    expect(r.civBonuses[0]).toBe("Çobanlar %25 daha hızlı çalışır");
    expect(r.teamBonus).toBe("Okçuluk Binası %10 daha hızlı çalışır");
    expect(r.uniqueTechs.map((t) => t.effect)).toEqual([
      "Yaya Okçulara 1 menzil; Gözcü Kulesi türlerine 2 saldırı",
      "Katapultlar patlama hasarı verir ve daha isabetlidir",
    ]);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test parse-help`
Expected: FAIL — `Cannot find module '../scripts/lib/parse-help.mjs'`.

- [ ] **Step 3: Implement the parser**

Create `scripts/lib/parse-help.mjs`:

```js
// Generalized parser for aoe2techtree locale help strings (EN + TR).
// The civ help string lists bonuses, unique unit(s), unique techs, and team bonus,
// separated by <br>. Section headers differ per language; bullets use "•".

const MARKERS = {
  en: { civ: /civilization$/i, unit: /^Unique Unit/i, tech: /^Unique Tech/i, team: /^Team Bonus/i },
  tr: { civ: /medeniyeti$/i, unit: /^Özgün Birim/i, tech: /^Özgün Teknoloji/i, team: /^Takım Bonusu/i },
};

export function parseHelp(raw, lang) {
  if (typeof raw !== "string") return null;
  const M = MARKERS[lang];
  if (!M) throw new Error(`parseHelp: unknown lang "${lang}"`);

  const lines = raw.split(/<br\s*\/?>/i).map((l) => l.replace(/<\/?[a-z]+>/gi, "").trim());
  const out = { civType: "", civBonuses: [], teamBonus: "", uniqueTechs: [] };
  let section = "bonuses";

  for (const l of lines) {
    if (!l) continue;
    if (!out.civType && M.civ.test(l)) {
      out.civType = l.replace(M.civ, "").trim();
      continue;
    }
    if (M.unit.test(l)) { section = "skip"; continue; }
    if (M.tech.test(l)) { section = "techs"; continue; }
    if (M.team.test(l)) { section = "team"; continue; }

    const text = l.replace(/^•\s*/, "").trim();
    if (section === "bonuses" && l.startsWith("•")) {
      out.civBonuses.push(text);
    } else if (section === "team") {
      out.teamBonus = out.teamBonus ? `${out.teamBonus} ${text}` : text;
    } else if (section === "techs" && l.startsWith("•")) {
      const m = text.match(/^(.+?)\s*\(([^)]+)\)\s*$/);
      out.uniqueTechs.push(m ? { name: m[1].trim(), effect: m[2].trim() } : { name: text, effect: "" });
    }
  }
  return out.civBonuses.length ? out : null;
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm test parse-help`
Expected: PASS (2 tests).

- [ ] **Step 5: Sanity-check against real cached strings**

Run:
```bash
node --input-type=module -e "
import { parseHelp } from './scripts/lib/parse-help.mjs';
import { readFileSync } from 'node:fs';
const tr = JSON.parse(readFileSync('.cache/aoe2-data/strings-tr.json','utf8'));
const r = parseHelp(tr['120150'], 'tr');
console.log(r.civType, '|', r.civBonuses.length, 'bonuses |', r.uniqueTechs.map(t=>t.name).join(', '));
"
```
Expected: `Yaya Okçu | 3 bonuses | Levazımcı, Savaş Kurdu` (or similar real TR values).

- [ ] **Step 6: Commit**

```bash
git add scripts/lib/parse-help.mjs tests/parse-help.test.mjs
git commit -m "feat(civs): generalized bilingual help-string parser (EN+TR)"
```

---

### Task 3: Rewrite the civ generator — source TR, drop hand-coded data, fix carryover

Make `parseHelp` the sole source for civ bonuses/team/techs in **both** languages, delete
the fabricated `SUPPLEMENTAL` data and the 6 extra civs' wiring, add a TR tagline, and
read carryover from the **bilingual YAML** (not the deleted `en/`,`tr/` dirs) preserving
only `strategy`.

**Files:**
- Modify: `scripts/build-civilizations.mjs`

**Interfaces:**
- Consumes: `parseHelp` from `scripts/lib/parse-help.mjs`; `.cache/aoe2-data/strings-{en,tr}.json`.
- Produces: regenerated `src/content/civilizations/<slug>.yaml` (53) with sourced
  `bonuses/teamBonus/uniqueTechs.*.effect` in EN+TR, localized `tagline`, preserved
  `strategy`; and `src/data/civilizations.json` (53 civs).

- [ ] **Step 1: Import the parser; delete the old `parseHelpBonuses`**

At the top of `build-civilizations.mjs`, after the existing imports, add:

```js
import { parseHelp } from "./lib/parse-help.mjs";
```

Delete the entire local `parseHelpBonuses` function (the block from
`// Parse a civ's bonuses + team bonus …` through its closing `}` — currently ≈ lines 35–62).

- [ ] **Step 2: Delete all hand-coded civ data**

Remove these blocks entirely (they are the fabricated/redundant hand-coded data):
- `IMPERIAL_TECH_OVERRIDES` const (≈ lines 192–215)
- `CASTLE_TECH_OVERRIDES` const (≈ lines 217–221)
- `SUPPLEMENTAL` const (≈ lines 227–709)
- the `SUPPLEMENTAL.mapuche = { … }` reassignment (≈ lines 711–730)

In `REGION_OVERRIDE`, delete the 6 fabricated-civ keys: `romans` stays (it is a real
civ in `data.json`), but delete `achaemenids`, `athenians`, `macedonians`, `spartans`,
`thracians`, `puru` (the lines under `// newer DLC`).

- [ ] **Step 3: Drop the icon-map "supplemental" civ-injection loop**

In `run()`, delete the loop that adds civs from the icon-map when `SUPPLEMENTAL[slug]`
exists (the block beginning `// Also include any civ slug in the icon-map that has
supplemental data` and its `for (const slug of Object.keys(iconMap.civs || {}))`). The
civ list now comes purely from `data.json` (+ aalises), which is the 53.

- [ ] **Step 4: Load the TR strings and build a TR help map**

In `run()`, where `stringsText` (EN) is read, also read TR. Replace the
`readFile(path.join(CACHE_DIR, "strings-en.json"), …)` line in the `Promise.all` with two
reads, and after parsing add the TR map:

```js
  const [iconMapText, aalisesMap, aoe2ttData, stringsEnText, stringsTrText] = await Promise.all([
    readFile(ICON_MAP, "utf8"),
    loadAalises(),
    loadAoe2TT(),
    readFile(path.join(CACHE_DIR, "strings-en.json"), "utf8").catch(() => "{}"),
    readFile(path.join(CACHE_DIR, "strings-tr.json"), "utf8").catch(() => "{}"),
  ]);
  const stringsEn = JSON.parse(stringsEnText);
  const stringsTr = JSON.parse(stringsTrText);
```

(Replace later uses of the old `strings` variable accordingly.)

- [ ] **Step 5: Source bonuses/team/techs in both languages per civ**

Replace the existing single-language re-source block (the part that does
`const parsed = helpId != null ? parseHelpBonuses(...)`) with a both-language version that
attaches localized values to the entry. The entry now carries localized fields:

```js
    const helpId = helpIdBySlug[entry.slug];
    const en = helpId != null ? parseHelp(stringsEn[helpId] ?? stringsEn[String(helpId)], "en") : null;
    const tr = helpId != null ? parseHelp(stringsTr[helpId] ?? stringsTr[String(helpId)], "tr") : null;
    if (!en) {
      console.warn(`[WARN] no EN help for ${entry.slug} — skipping`);
      continue;
    }
    // EN civilizations.json fields stay EN (validate-data expects EN strings).
    entry.civBonuses = en.civBonuses;
    entry.teamBonus = en.teamBonus;
    entry.specialty = fixSpecialty(en.civType || entry.specialty);
    entry.uniqueTechs = {
      castle: { name: en.uniqueTechs[0]?.name ?? "", effect: en.uniqueTechs[0]?.effect ?? "" },
      imperial: { name: en.uniqueTechs[1]?.name ?? "", effect: en.uniqueTechs[1]?.effect ?? "" },
    };
    // Localized payload for buildMarkdown (YAML output).
    entry.tr = {
      civBonuses: tr?.civBonuses ?? en.civBonuses,
      teamBonus: tr?.teamBonus ?? en.teamBonus,
      civType: tr?.civType ?? en.civType,
      uniqueTechs: {
        castle: tr?.uniqueTechs[0]?.effect ?? en.uniqueTechs[0]?.effect ?? "",
        imperial: tr?.uniqueTechs[1]?.effect ?? en.uniqueTechs[1]?.effect ?? "",
      },
    };
    entry.regionNoun = REGION_NOUN[entry.region] ?? entry.region;
```

Remove the now-dead earlier assignment of `entry.specialty`/`entry.regionNoun` and the
old `parsed`-based override so this block is the single source of truth.

- [ ] **Step 6: Add the Turkish region map and tagline**

After the existing `REGION_NOUN` const, add the Turkish ablative region labels (editorial
geographic copy — the only hand-written civ strings that remain):

```js
const REGION_NOUN_TR = {
  "Ancient Mediterranean": "Antik Akdeniz'den",
  "Ancient Middle Eastern": "Antik Orta Doğu'dan",
  Caucasian: "Kafkasya'dan",
  "Central Asian": "Orta Asya'dan",
  "Central European": "Orta Avrupa'dan",
  "East African": "Doğu Afrika'dan",
  "East Asian": "Doğu Asya'dan",
  "Eastern European": "Doğu Avrupa'dan",
  "Eastern Mediterranean": "Doğu Akdeniz'den",
  Mesoamerican: "Mezoamerika'dan",
  "Middle Eastern": "Orta Doğu'dan",
  "North African": "Kuzey Afrika'dan",
  "Northern European": "Kuzey Avrupa'dan",
  "South American": "Güney Amerika'dan",
  "South Asian": "Güney Asya'dan",
  "Southeast Asian": "Güneydoğu Asya'dan",
  "Southern European": "Güney Avrupa'dan",
  "West African": "Batı Afrika'dan",
  "Western European": "Batı Avrupa'dan",
};
```

- [ ] **Step 7: Rewrite `buildMarkdown` to emit sourced TR + localized tagline + strategy-only carryover**

Replace the body of `buildMarkdown(entry, displayName, trData, enData)` so that
bonuses/team/tech-effects come from `entry`/`entry.tr` (sourced), the TR tagline is
generated, and only `strategy` is carried from the existing YAML:

```js
function buildMarkdown(entry, displayName, existing) {
  const { slug, region, regionNoun, specialty, civBonuses, teamBonus, uniqueTechs, tr } = entry;
  const place = regionNoun || region;
  const art = /^[aeiou]/i.test(specialty) ? "an" : "a";
  const taglineEn = `${displayName} — ${art} ${specialty} civilization from ${place}.`;
  const placeTr = REGION_NOUN_TR[region] ?? place;
  const taglineTr = `${displayName}, ${placeTr} bir ${tr.civType} uygarlığıdır.`;

  const ex = existing?.fm ?? {};
  const strategy = ex.strategy && (ex.strategy.en || ex.strategy.tr) ? ex.strategy : null;

  const fm = {
    slug,
    name: { en: displayName, tr: String(ex.name?.tr ?? displayName) },
    tagline: { en: taglineEn, tr: taglineTr },
    bonuses: { en: civBonuses, tr: tr.civBonuses },
    teamBonus: { en: teamBonus, tr: tr.teamBonus },
    uniqueTechs: {
      castle: {
        name: { en: uniqueTechs.castle.name, tr: uniqueTechs.castle.name },
        effect: { en: uniqueTechs.castle.effect, tr: tr.uniqueTechs.castle },
      },
      imperial: {
        name: { en: uniqueTechs.imperial.name, tr: uniqueTechs.imperial.name },
        effect: { en: uniqueTechs.imperial.effect, tr: tr.uniqueTechs.imperial },
      },
    },
    ...(strategy ? { strategy } : {}),
  };
  return yaml.dump(fm, { lineWidth: 120 });
}
```

- [ ] **Step 8: Update the carryover read + the `buildMarkdown` call site**

Replace `readExistingCivFile` to read the **bilingual YAML** and the call site in `run()`:

```js
async function readExistingCivYaml(slug) {
  try {
    const text = await readFile(path.join(CONTENT_CIVS, `${slug}.yaml`), "utf8");
    return { fm: yaml.load(text) };
  } catch (_) {
    return { fm: null };
  }
}
```

In `run()`, replace the two `readExistingCivFile(...EN_DIR...)` / `...TR_DIR...` reads and
the `buildMarkdown(entry, displayName, trData, enData)` call with:

```js
    const existing = await readExistingCivYaml(slug);
    const mdPath = path.join(CONTENT_CIVS, `${slug}.yaml`);
    const md = buildMarkdown(entry, displayName, existing);
    await writeFile(mdPath, md, "utf8");
```

Delete the now-unused `CONTENT_EN_DIR` / `CONTENT_TR_DIR` constants and the old
`readExistingCivFile` function.

- [ ] **Step 9: Run the generator**

Run: `node scripts/build-civilizations.mjs`
Expected: `Civs in JSON: 53` and 53 `[WRITE]` lines; no `[WARN] No data` lines.

- [ ] **Step 10: Verify EN unchanged for the 53, TR sourced, strategy preserved**

Run:
```bash
node -e "const d=require('./src/data/civilizations.json'); console.log('civs:', d.civs.length)"
node -e '
const fs=require("fs"),yaml=require("js-yaml");
const b=yaml.load(fs.readFileSync("src/content/civilizations/britons.yaml","utf8"));
console.log("tagline.tr:", b.tagline.tr);
console.log("bonus.tr[0]:", b.bonuses.tr[0]);
console.log("castle effect tr:", b.uniqueTechs.castle.effect.tr);
const f=yaml.load(fs.readFileSync("src/content/civilizations/franks.yaml","utf8"));
console.log("franks castle effect tr (placeholder gone?):", f.uniqueTechs.castle.effect.tr);
'
```
Expected: `civs: 53`; Britons tagline.tr Turkish (`… bir Yaya Okçu uygarlığıdır.`),
bonus.tr Turkish, castle effect tr Turkish; franks effect is real TR, **not**
`"Oyun içi teknoloji ağacına bak"`.

- [ ] **Step 11: Run validate-data and the audit (expect civ fields clean)**

Run: `node scripts/validate-data.mjs`
Expected: `validate-data: 53 civs OK ✓`.

- [ ] **Step 12: Commit**

```bash
git add scripts/build-civilizations.mjs src/content/civilizations src/data/civilizations.json
git commit -m "refactor(civs): source EN+TR facts from aoe2techtree locales; drop hand-coded data"
```

---

### Task 4: Delete the 6 fabricated civ files and the tech-tree stub

The generator no longer emits the 6, but their old YAML files remain on disk.

**Files:**
- Delete: `src/content/civilizations/{achaemenids,athenians,macedonians,spartans,thracians,puru}.yaml`
- Delete: `src/data/tech-tree.json`

- [ ] **Step 1: Confirm no source references the 6 slugs or tech-tree.json**

Run:
```bash
grep -rn -E "achaemenids|athenians|macedonians|spartans|thracians|puru" src/ scripts/ | grep -v node_modules
grep -rn "tech-tree" src/ scripts/ astro.config.mjs | grep -v node_modules
```
Expected: no matches (the `tech-tree.json` file is an orphan; `config.ts` has no
`tech-tree` collection).

- [ ] **Step 2: Delete the files**

```bash
git rm src/content/civilizations/achaemenids.yaml src/content/civilizations/athenians.yaml \
       src/content/civilizations/macedonians.yaml src/content/civilizations/spartans.yaml \
       src/content/civilizations/thracians.yaml src/content/civilizations/puru.yaml \
       src/data/tech-tree.json
```

- [ ] **Step 3: Verify civ YAML count is 53**

Run: `ls src/content/civilizations/*.yaml | wc -l`
Expected: `53`

- [ ] **Step 4: Commit**

```bash
git commit -m "fix(civs): remove 6 fabricated civilizations + unused tech-tree stub"
```

---

### Task 5: Remove the 6 civs' icons and regenerate the icon map

**Files:**
- Delete: `public/images/aoe2/Civs/{achaemenids,athenians,macedonians,spartans,thracians,puru}*`
- Modify (regenerated): `src/data/icon-map.json`

- [ ] **Step 1: Delete the 24 icon files**

```bash
git rm public/images/aoe2/Civs/achaemenids* public/images/aoe2/Civs/athenians* \
       public/images/aoe2/Civs/macedonians* public/images/aoe2/Civs/spartans* \
       public/images/aoe2/Civs/thracians* public/images/aoe2/Civs/puru*
```

- [ ] **Step 2: Regenerate the icon map**

Run: `pnpm icons:map`
Expected: completes; `src/data/icon-map.json` rewritten.

- [ ] **Step 3: Verify the 6 are gone from icon-map**

Run: `node -e "const m=require('./src/data/icon-map.json'); console.log(['achaemenids','athenians','macedonians','spartans','thracians','puru'].filter(s=>m.civs&&m.civs[s]).join(',')||'none')"`
Expected: `none`

- [ ] **Step 4: Commit**

```bash
git add public/images/aoe2/Civs src/data/icon-map.json
git commit -m "fix(assets): drop icons for removed civs; regenerate icon-map"
```

---

## Phase B — Units: localize the `role` taxonomy (WS3)

### Task 6: Fill `role.tr` from a controlled taxonomy (TDD)

`build-units.mjs` writes the legacy `units/en/*.md` and is not used for the bilingual
YAML. Fill `role.tr` in the existing `src/content/units/*.yaml` with a deterministic
transform that preserves `description.tr`.

**Files:**
- Create: `scripts/fill-unit-roles.mjs`
- Create: `scripts/lib/role-tr.mjs`
- Test: `tests/role-tr.test.mjs`

**Interfaces:**
- Produces: `export const ROLE_TR` (map of every EN role → TR) and
  `export function roleTr(en)` (throws on an unmapped role).

- [ ] **Step 1: Write the failing test**

Create `tests/role-tr.test.mjs`:

```js
import { describe, it, expect } from "vitest";
import { readdirSync, readFileSync } from "node:fs";
import yaml from "js-yaml";
import { roleTr } from "../scripts/lib/role-tr.mjs";

describe("roleTr", () => {
  it("translates a known role", () => {
    expect(roleTr("ranged infantry")).toBe("menzilli piyade");
    expect(roleTr("heavy cavalry")).toBe("ağır süvari");
  });

  it("covers every role.en present in the unit YAML", () => {
    const dir = "src/content/units";
    const roles = new Set();
    for (const f of readdirSync(dir).filter((x) => x.endsWith(".yaml"))) {
      const d = yaml.load(readFileSync(`${dir}/${f}`, "utf8"));
      if (d.role?.en) roles.add(d.role.en);
    }
    for (const r of roles) expect(() => roleTr(r)).not.toThrow();
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test role-tr`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the role map**

Create `scripts/lib/role-tr.mjs`:

```js
// Controlled EN→TR taxonomy for unit roles (editorial; no source equivalent).
// Proper nouns kept in EN (e.g. "Britons").
export const ROLE_TR = {
  "melee infantry": "yakın dövüş piyadesi",
  "anti-cavalry infantry": "süvari avcısı piyade",
  "light infantry": "hafif piyade",
  "ranged infantry": "menzilli piyade",
  "anti-archer ranged": "okçu avcısı menzilli birim",
  "mounted archer": "atlı okçu",
  "gunpowder ranged": "barutlu menzilli birim",
  "light cavalry": "hafif süvari",
  "heavy cavalry": "ağır süvari",
  "anti-cavalry cavalry": "süvari avcısı süvari",
  "support / conversion": "destek / din değiştirme",
  siege: "kuşatma",
  "siege gunpowder": "barutlu kuşatma",
  "siege long-range": "uzun menzilli kuşatma",
  naval: "deniz birimi",
  "anti-ship naval": "gemi avcısı deniz birimi",
  "naval suicide": "intihar deniz birimi",
  "naval siege": "deniz kuşatma birimi",
  "economic naval": "ekonomik deniz birimi",
  "utility naval": "yardımcı deniz birimi",
  "unique unit": "özgün birim",
  "Unique foot archer (Britons)": "Özgün yaya okçu (Britons)",
};

export function roleTr(en) {
  if (!(en in ROLE_TR)) throw new Error(`roleTr: unmapped role "${en}"`);
  return ROLE_TR[en];
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm test role-tr`
Expected: PASS (2 tests).

- [ ] **Step 5: Implement and run the fill transform**

Create `scripts/fill-unit-roles.mjs`:

```js
import { readdirSync, readFileSync, writeFileSync } from "node:fs";
import path from "node:path";
import yaml from "js-yaml";
import { roleTr } from "./lib/role-tr.mjs";

const DIR = path.resolve("src/content/units");
let changed = 0;
for (const f of readdirSync(DIR).filter((x) => x.endsWith(".yaml"))) {
  const p = path.join(DIR, f);
  const d = yaml.load(readFileSync(p, "utf8"));
  if (!d.role?.en) continue;
  const want = roleTr(d.role.en);
  if (d.role.tr !== want) {
    d.role.tr = want;
    writeFileSync(p, yaml.dump(d, { lineWidth: 120 }), "utf8");
    changed++;
  }
}
console.log(`fill-unit-roles: updated ${changed} files`);
```

Run: `node scripts/fill-unit-roles.mjs`
Expected: `fill-unit-roles: updated 29 files` (the count of untranslated roles).

- [ ] **Step 6: Verify no English role remains and descriptions intact**

Run:
```bash
node -e '
const fs=require("fs"),yaml=require("js-yaml");
let bad=0,descLost=0;
for(const f of fs.readdirSync("src/content/units").filter(x=>x.endsWith(".yaml"))){
  const d=yaml.load(fs.readFileSync("src/content/units/"+f,"utf8"));
  if(d.role&&d.role.en===d.role.tr&&!/Britons/.test(d.role.en))bad++;
  if(d.description&&!d.description.tr)descLost++;
}
console.log("role still EN:",bad,"| descriptions missing tr:",descLost);
'
```
Expected: `role still EN: 0 | descriptions missing tr: 0`

- [ ] **Step 7: Commit**

```bash
git add scripts/lib/role-tr.mjs scripts/fill-unit-roles.mjs tests/role-tr.test.mjs src/content/units
git commit -m "feat(units): localize role taxonomy to Turkish (preserve descriptions)"
```

---

## Phase C — Build orders: verify & correct against the Hera sources (WS4)

This phase is **source-grounded content verification**, best executed as a Workflow
(fan-out per build). Each build is checked against its source section; villager counts,
`m:ss` timings, `targetAge`, and `durationMin` are corrected; a `source` block is added;
the `name.tr` is translated. The schema (`src/content/config.ts` build-orders) and
`verify-build-facts.mjs` are the gates.

### Task 7: Build the source index

**Files:**
- Create: `docs/superpowers/notes/build-order-sources.md` (a scratch map, not shipped content)

**Interfaces:**
- Produces: a table mapping each of the 46 build slugs → source location
  (per-build `md/build-orders/hera-*.md`, or a named section + line range in
  `md/reference/hera-strategy-guide-2025-12-pdftext.txt`, or "needs public source").

- [ ] **Step 1: List builds and candidate sources**

Run:
```bash
ls src/content/build-orders/*.yaml | sed 's#.*/##;s#.yaml##' | sort > /tmp/builds.txt
ls md/build-orders/hera-*.md | sed 's#.*/hera-##;s#.md##' | sort > /tmp/hera.txt
echo "=== builds with a per-build Hera transcript ==="; comm -12 /tmp/builds.txt <(sed 's/^/X/' /tmp/hera.txt | sed 's/^X//') 2>/dev/null || true
grep -nE "^[A-Z].*[0-9]+$" md/reference/hera-strategy-guide-2025-12-pdftext.txt | head -60
```
Expected: the per-build transcript names and the strategy-guide section index (build name → page).

- [ ] **Step 2: Write the index**

Create `docs/superpowers/notes/build-order-sources.md` with a table: for every build slug,
record `source_type` (transcript | guide-section | needs-public-source), the file, and the
section/line range. Mark the 11 with per-build transcripts and map guide-covered builds
(Drush FC, Fast Castle Boom, FC Light Cav Relic, FC into Unique Unit, FC on Fortress,
Knight Rush, Scouts into Archers/Cavalry Archers/Skirms, …) to their guide sections.

- [ ] **Step 3: Commit**

```bash
git add docs/superpowers/notes/build-order-sources.md
git commit -m "docs(builds): source index mapping each build to its Hera source"
```

---

### Task 8: Verify, correct, and attribute every build (Workflow fan-out)

**Files:**
- Modify: each `src/content/build-orders/*.yaml` that needs correction/attribution/translation

**Interfaces:**
- Consumes: the source index (Task 7); the schema in `src/content/config.ts`
  (build-orders: `villagers`, `time`, `phase`/`targetAge`, `durationMin`, `assign`,
  `note`, `icons`, `source{author,url}`, `name`).
- Produces: corrected, attributed, fully-translated build YAMLs.

- [ ] **Step 1: Correction rules (apply per build, from its source)**

For each build, open its source section and reconcile:
- `targetAge` must equal the source's final age (e.g. `cumans-2tc-boom` → `castle`).
- `durationMin` must equal the source's stated duration (e.g. `cumans-2tc-boom` → `20`).
- Each step's `villagers` (pop after that step) and `time` (`m:ss`) must match the source's
  numbers; fix any that drift. Keep 6–12 steps.
- `name.tr`: use the source transcript's own Turkish title when present; else translate the
  editorial title (proper nouns/civ names stay EN). The 18 untranslated names are:
  `anti-lame-dark-age, armenian-spear-fc-relic, chinese-fast-feudal, dark-age-rush-archers,
  drush-fc, eagle-range-feudal, fast-chickens-dark-age, fc-crossbows, fc-knights-pocket,
  fc-light-cav-relic, fire-galleys-and-archers, galley-rush, georgians-healing-scouts,
  jurchens-fc-fire-lancer, korean-spear-skirm, maa-into-skirms, men-at-arms-towers, win-water`.
- `source`: `{ author: "Hera", url: "<video/guide url>" }` for Hera-derived builds; for any
  build verified against another reputable public source, credit that source instead.
- A build with no source in our materials and none found publicly: leave content unchanged
  but add a `# TODO(owner): no source found` note in the index and flag it in the task
  summary (do **not** invent numbers).

- [ ] **Step 2: Execute the per-build pass**

Recommended: run as a Workflow that pipelines each build slug through
(a) read YAML + source section → (b) produce corrected YAML (schema-valid) →
(c) adversarially verify the numbers against the source. Example skeleton (run via the
Workflow tool; do not commit the script):

```js
export const meta = {
  name: "verify-build-orders",
  description: "Verify/correct each build order against its Hera source and attribute it",
  phases: [{ title: "Correct" }, { title: "Verify" }],
};
const builds = args; // array of {slug, sourceFile, section}
const SCHEMA = { /* JSON Schema mirroring config.ts build-orders */ };
const results = await pipeline(
  builds,
  (b) => agent(`Read src/content/build-orders/${b.slug}.yaml and the source
${b.sourceFile} section "${b.section}". Correct targetAge, durationMin, every step's
villagers + m:ss time to match the source. Add source{author,url}. Translate name.tr
(proper nouns stay EN). Write the corrected YAML back. Return the diff + the source
numbers you used.`, { phase: "Correct", schema: SCHEMA, label: `fix:${b.slug}` }),
  (fixed, b) => agent(`Adversarially verify src/content/build-orders/${b.slug}.yaml against
${b.sourceFile}: does every villager count and timing match the source? Report any
remaining mismatch. Default to "mismatch" if unsure.`, { phase: "Verify", label: `chk:${b.slug}` }),
);
return results;
```

(If executing inline instead, do the same per-build reconciliation by hand, one commit per
small batch.)

- [ ] **Step 3: Gate — schema build + unit-age check + no English titles**

Run:
```bash
node scripts/verify-build-facts.mjs
node -e '
const fs=require("fs"),yaml=require("js-yaml");
let en=0,nosrc=0;
for(const f of fs.readdirSync("src/content/build-orders").filter(x=>x.endsWith(".yaml"))){
  const d=yaml.load(fs.readFileSync("src/content/build-orders/"+f,"utf8"));
  if(d.name&&d.name.en===d.name.tr)en++;
  if(!d.source)nosrc++;
}
console.log("untranslated names:",en,"| builds without source:",nosrc);
'
```
Expected: `No unit-age violations. ✓`; `untranslated names: 0`; `builds without source: 0`
(or a small number explicitly flagged in the index with owner TODOs).

- [ ] **Step 4: Commit**

```bash
git add src/content/build-orders docs/superpowers/notes/build-order-sources.md
git commit -m "fix(builds): verify/correct all builds vs Hera sources; attribute + translate"
```

---

## Phase D — Guardrails & docs (WS5)

### Task 9: `audit-yaml-translations.mjs` — gate `en === tr` (TDD)

**Files:**
- Create: `scripts/audit-yaml-translations.mjs`
- Create: `scripts/lib/audit-core.mjs`
- Test: `tests/audit-translations.test.mjs`
- Modify: `package.json` (prebuild wiring)

**Interfaces:**
- Produces: `export function auditEntry(typeDir, fileName, data)` → `string[]` of
  offending field paths where `en === tr` and the path is not allow-listed.

- [ ] **Step 1: Write the failing test**

Create `tests/audit-translations.test.mjs`:

```js
import { describe, it, expect } from "vitest";
import { auditEntry } from "../scripts/lib/audit-core.mjs";

describe("auditEntry", () => {
  it("flags an untranslated descriptive field", () => {
    const issues = auditEntry("civilizations", "x.yaml", {
      tagline: { en: "A civ.", tr: "A civ." },
    });
    expect(issues).toContain("tagline");
  });

  it("allows proper-noun fields where en==tr", () => {
    const issues = auditEntry("civilizations", "x.yaml", {
      name: { en: "Britons", tr: "Britons" },
      uniqueTechs: { castle: { name: { en: "Warwolf", tr: "Warwolf" }, effect: { en: "A", tr: "B" } } },
    });
    expect(issues).toEqual([]);
  });

  it("allows a unit role naming a proper noun (Britons)", () => {
    const issues = auditEntry("units", "longbowman.yaml", {
      role: { en: "Unique foot archer (Britons)", tr: "Unique foot archer (Britons)" },
    });
    expect(issues).toEqual([]);
  });
});
```

- [ ] **Step 2: Run to verify it fails**

Run: `pnpm test audit-translations`
Expected: FAIL — module not found.

- [ ] **Step 3: Implement the audit core**

Create `scripts/lib/audit-core.mjs`:

```js
// Walk a bilingual content object and report field paths where tr === en
// (untranslated), excluding allow-listed proper-noun fields.

// Field-path suffixes that are intentionally EN in the tr slot.
const ALLOW = [
  /(^|\.)name$/,            // civ/unit/build/map names + tech names
  /(^|\.)term$/,            // glossary terms
];
// A tr value that merely repeats a proper noun is allowed if the EN value itself is
// a recognized proper-noun pattern (contains "(SomeProperNoun)").
const PROPER_NOUN_VALUE = /\([A-ZÇĞİÖŞÜ][\wçğıöşü]*\)\s*$/;

function isLocalized(v) {
  return v && typeof v === "object" && "en" in v && "tr" in v &&
    typeof v.en === "string" && typeof v.tr === "string";
}

export function auditEntry(_typeDir, _fileName, data) {
  const issues = [];
  walk(data, "", issues);
  return issues;
}

function walk(node, prefix, issues) {
  if (Array.isArray(node)) {
    // localizedString arrays are { en: [...], tr: [...] } handled below, not here
    node.forEach((v, i) => walk(v, `${prefix}[${i}]`, issues));
    return;
  }
  if (node && typeof node === "object") {
    if (isLocalized(node)) {
      if (node.en === node.tr && !allowed(prefix, node.en)) issues.push(prefix);
      return;
    }
    // localized array field: { en: string[], tr: string[] }
    if (Array.isArray(node.en) && Array.isArray(node.tr)) {
      if (JSON.stringify(node.en) === JSON.stringify(node.tr) && !allowed(prefix, "")) {
        issues.push(prefix);
      }
      return;
    }
    for (const [k, v] of Object.entries(node)) {
      walk(v, prefix ? `${prefix}.${k}` : k, issues);
    }
  }
}

function allowed(path, enValue) {
  if (ALLOW.some((re) => re.test(path))) return true;
  if (PROPER_NOUN_VALUE.test(enValue)) return true;
  return false;
}
```

- [ ] **Step 4: Run to verify it passes**

Run: `pnpm test audit-translations`
Expected: PASS (3 tests).

- [ ] **Step 5: Implement the CLI wrapper**

Create `scripts/audit-yaml-translations.mjs`:

```js
import { readdirSync, readFileSync } from "node:fs";
import path from "node:path";
import yaml from "js-yaml";
import { auditEntry } from "./lib/audit-core.mjs";

const TYPES = ["civilizations", "build-orders", "units", "maps", "glossary"];
let total = 0;
for (const t of TYPES) {
  const dir = path.resolve("src/content", t);
  let files = [];
  try { files = readdirSync(dir).filter((f) => /\.ya?ml$/.test(f)); } catch (_) { continue; }
  for (const f of files) {
    const data = yaml.load(readFileSync(path.join(dir, f), "utf8"));
    const issues = auditEntry(t, f, data);
    for (const p of issues) {
      console.error(`  ✗ ${t}/${f}: "${p}" is identical in en and tr (untranslated)`);
      total++;
    }
  }
}
if (total) {
  console.error(`\naudit-yaml-translations: ${total} untranslated field(s)`);
  process.exit(1);
}
console.log("audit-yaml-translations: all bilingual fields translated ✓");
```

- [ ] **Step 6: Run it against the repo**

Run: `node scripts/audit-yaml-translations.mjs`
Expected: `all bilingual fields translated ✓` (after Phases A–C). If it lists fields,
fix them, then re-run until clean.

- [ ] **Step 7: Wire into prebuild**

In `package.json`, find the `prebuild` script and append the audit so a bad regen fails
the build. If `prebuild` is `node scripts/validate-data.mjs && node scripts/verify-build-facts.mjs`,
change it to:

```json
"prebuild": "node scripts/validate-data.mjs && node scripts/verify-build-facts.mjs && node scripts/audit-yaml-translations.mjs",
```

Run: `pnpm run prebuild`
Expected: all three pass.

- [ ] **Step 8: Commit**

```bash
git add scripts/audit-yaml-translations.mjs scripts/lib/audit-core.mjs tests/audit-translations.test.mjs package.json
git commit -m "feat(i18n): audit-yaml-translations gate (en==tr) wired into prebuild"
```

---

### Task 10: Retire/repair the stale translation scripts

**Files:**
- Modify: `scripts/check-translations.mjs` (or remove it)
- Modify: `scripts/i18n-coverage.mjs`

- [ ] **Step 1: Repoint `check-translations.mjs`**

The current `check-translations.mjs` scans the old `<type>/{en,tr}/` layout and reports
`0/0`. Replace its body with a thin delegation to the new audit so there is one source of
truth. Overwrite the file with:

```js
#!/usr/bin/env node
// Bilingual content lives in single YAML files; per-field en==tr is audited by
// scripts/audit-yaml-translations.mjs. The remaining MD pairs (beginner, articles)
// are covered by scripts/i18n-coverage.mjs. This script delegates to both.
import { execFileSync } from "node:child_process";
for (const s of ["audit-yaml-translations.mjs", "i18n-coverage.mjs"]) {
  try { execFileSync("node", [`scripts/${s}`], { stdio: "inherit" }); }
  catch (_) { process.exitCode = 1; }
}
```

- [ ] **Step 2: Confirm i18n-coverage still passes for MD pairs**

Run: `node scripts/i18n-coverage.mjs`
Expected: `All TR content present …` (unchanged behavior for beginner/articles).

- [ ] **Step 3: Run the combined check**

Run: `node scripts/check-translations.mjs`
Expected: audit `✓` then i18n-coverage `✓`.

- [ ] **Step 4: Commit**

```bash
git add scripts/check-translations.mjs scripts/i18n-coverage.mjs
git commit -m "chore(i18n): retire stale check-translations; delegate to audit + coverage"
```

---

### Task 11: Update the docs to the bilingual-YAML reality

**Files:**
- Modify: `CLAUDE.md`
- Modify: `src/content/CLAUDE.md`

- [ ] **Step 1: Fix `src/content/CLAUDE.md`**

Replace the "File layout" section's `src/content/<type>/<lang>/<slug>.md` description with
the single bilingual YAML shape (`src/content/<type>/<slug>.yaml`, each field `{ en, tr }`),
note that `beginner/` and `articles/` remain MD `en/tr` dirs, and document the
`audit-yaml-translations` gate + the proper-noun allowlist. Update the "Adding a new entry"
steps to match (no more `import:md`/`new:guide` for YAML types).

- [ ] **Step 2: Fix root `CLAUDE.md`**

In the "Layout map" and "Workflow for new content" sections, replace
`src/content/<type>/{en,tr}/*.md` with the bilingual YAML shape, and add a one-line note:
civ facts (EN+TR) are sourced from `aoe2techtree` locales via `build-civilizations.mjs`;
build orders are verified against the Hera materials in `md/`.

- [ ] **Step 3: Verify no stale path remains**

Run: `grep -n "content/<type>/<lang>\|{en,tr}/\*.md\|en/<slug>.md" CLAUDE.md src/content/CLAUDE.md`
Expected: no matches (except where intentionally describing beginner/articles).

- [ ] **Step 4: Commit**

```bash
git add CLAUDE.md src/content/CLAUDE.md
git commit -m "docs: update CLAUDE.md to bilingual-YAML + sourced-civ reality"
```

---

### Task 12: Full build + final verification

**Files:** none (verification only)

- [ ] **Step 1: Full type check + build**

Run: `pnpm check && pnpm build`
Expected: both succeed; prebuild gates (validate-data, verify-build-facts,
audit-yaml-translations) all pass; Pagefind index builds.

- [ ] **Step 2: Final content assertions**

Run:
```bash
node -e "const d=require('./src/data/civilizations.json'); console.log('civs:', d.civs.length)"   # 53
node scripts/audit-yaml-translations.mjs                                                          # ✓
node scripts/verify-build-facts.mjs                                                                # ✓
ls src/content/civilizations/*.yaml | wc -l                                                        # 53
```
Expected: civs 53; audit ✓; no unit-age violations; 53 YAML files.

- [ ] **Step 3: Spot-check rendered TR**

Run: `pnpm preview` and open `/tr/civs/britons/`, `/tr/civs/franks/`, and a corrected build
(e.g. `/tr/build-orders/18pop-cumans-2tc-boom/`). Confirm Turkish bonuses/effects render and
the Cumans build shows Castle Age / ~20 min.

- [ ] **Step 4: Finalize the branch**

Use the `superpowers:finishing-a-development-branch` skill to choose merge/PR/cleanup.

---

## Self-Review

**Spec coverage:**
- WS1 (remove 6 civs) → Tasks 3 (drop SUPPLEMENTAL/wiring), 4 (delete files/stub), 5 (icons). ✓
- WS2 (fix generator + TR source) → Tasks 1, 2, 3. ✓
- WS3 (fill EN-in-TR) → civ facts via Task 3 (sourced), unit roles via Task 6, build names via Task 8, taglines via Task 3 template. ✓
- WS4 (build orders) → Tasks 7, 8. ✓
- WS5 (guardrails + docs) → Tasks 9, 10, 11. ✓
- Verification section → Task 12. ✓

**Placeholder scan:** Build-order per-build numbers are intentionally derived from sources
at execution (Task 8), not pre-written — this is source-derivation, not a placeholder; the
*process* and *gates* are fully specified. No "TBD/handle errors/similar to" placeholders.

**Type consistency:** `parseHelp(raw, lang)` returns `{civType, civBonuses, teamBonus,
uniqueTechs:[{name,effect}]}` — consumed identically in Task 3. `roleTr(en)` / `ROLE_TR`
consistent across Task 6. `auditEntry(typeDir, fileName, data)` consistent across Task 9.
`buildMarkdown(entry, displayName, existing)` signature matches its Task 3 call site.
