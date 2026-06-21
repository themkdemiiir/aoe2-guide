# Content Accuracy & Translation Overhaul

**Date:** 2026-06-21
**Status:** Approved (design)
**Owner:** themkdemiiir

## Problem

An audit of the bilingual (EN/TR) AOE2 guide found five classes of defect that
violate the project's core rule — *every factual value must be source-derived, never
hand-written* — and the recent "collapse EN/TR into single bilingual YAML" migration
silently removed the guardrails that would have caught them.

### 1. Fabricated game facts (CRITICAL)

The authoritative source (`SiegeEngineers/aoe2techtree` `data.json`, cached) defines
**53 civilizations**. The site ships **59**. The 6 extras —
`achaemenids, athenians, macedonians, spartans, thracians, puru` — have **no backing
in any source**. Their bonuses, team bonuses, and unique techs are hand-invented in the
`SUPPLEMENTAL` block of `scripts/build-civilizations.mjs` (≈ lines 597–708). Four
(achaemenids/athenians/macedonians/spartans) correspond to *Chronicles: Battle for
Greece* — a separate game mode, not the standard roster. Two (thracians/puru) appear
entirely invented. **Decision: remove all 6.** The site covers the standard 53 only.

`src/data/tech-tree.json` is a 43-byte empty stub (`{"patch":…,"matrix":{}}`),
unreferenced anywhere in `src/`.

### 2. English sitting silently in Turkish slots (HIGH)

The migration put each field as `{ en, tr }` in one YAML file. Where translation was
skipped, the EN string was **copied into the `tr` slot** — so it renders under
`lang="tr"` looking like a translation but isn't.

**Key opportunity:** `SiegeEngineers/aoe2techtree` ships a **Turkish locale**
(`data/locales/tr/strings.json`, alongside en/de/es/fr/… 17 langs). It contains the
**official in-game Turkish** help text for every civ, keyed by the same
`help_string_id`. Verified: **all 53 civs have TR help text** (incl. the newest Three
Kingdoms / Chronicles-of-China DLC). So civ bonuses, team bonus, and unique-tech
effects can be **source-derived in Turkish** the same way they already are in English —
no AI/hand translation for those fields. Measured `en === tr` counts (what's wrong today):

| Type | Field | Untranslated (of total) |
|---|---|---|
| civilizations | `uniqueTechs.castle.effect` | 37 / 59 |
| civilizations | `uniqueTechs.imperial.effect` | 37 / 59 |
| civilizations | `tagline` | 8 / 59 |
| civilizations | `bonuses` | 6 / 59 |
| civilizations | `teamBonus` | 6 / 59 |
| units | `role` | 29 / 90 |
| build-orders | `name` | 18 / 46 |

(Removing the 6 fabricated civs eliminates several of these, since those civs are
entirely EN.) Plus one leftover placeholder: `franks.yaml` castle effect TR =
`"Oyun içi teknoloji ağacına bak"`. Long-form prose (map `body`, civ `strategy`,
glossary, beginner, articles) is already fully and fluently translated (grade A).

### 3. The generator will wipe translations on next run (CRITICAL, latent)

`scripts/build-civilizations.mjs` writes the new bilingual `<slug>.yaml` but **reads
carryover from the old `src/content/civilizations/{en,tr}/*.md` directories, which no
longer exist**. So the next `pnpm build-civilizations` (or asset re-sync) silently
resets every `tr` field and `strategy` body to EN. Translating the YAMLs is pointless
until this read-side is repaired.

### 4. Blind tooling & stale docs (root cause)

- `scripts/check-translations.mjs` still scans the old `<type>/{en,tr}/` layout →
  reports `0/0` for all YAML types (false "all good").
- `scripts/i18n-coverage.mjs` only checks the remaining MD pairs → prints "✅" while
  21% of YAML fields are English.
- No per-field `en === tr` audit exists.
- `CLAUDE.md` and `src/content/CLAUDE.md` still document the old `<type>/<lang>/<slug>.md`
  structure and the `import:md` / `new:guide` workflow.

### 5. Build-order sourcing (MEDIUM)

46 builds: 11 derive from Hera transcripts in `md/build-orders/`, 35 are unsourced.
**None** populate the optional `source` field (no attribution shown). Five Hera builds
have duration mismatches vs their source; worst is `cumans-2tc-boom` — a 20-min Castle
boom in the source but tagged `targetAge: feudal`, `durationMin: 9` (wrong). Build
content is strategy (curated meta), not raw game data, so the 35 unsourced builds are
**kept as-is**; only factual errors and attribution are fixed.

## Goals

1. Every civilization fact is source-derived (aoe2techtree / strings) — zero hand-coded
   civ bonuses/techs remain.
2. No EN text remains in a `tr` slot except an explicit, allow-listed set of proper
   nouns (unit/tech/civ/map names, glossary terms).
3. Regenerating civ data preserves TR translations and `strategy` prose.
4. CI fails on any new fabricated civ or untranslated field — the guardrails are real
   again.
5. Build-order factual errors fixed; Hera builds attributed.

Non-goals: rewriting the 35 unsourced build orders; re-sourcing map `recommendedCivs`
(curated meta, acceptable); translating intentional-EN proper nouns; adding new content
types or new civs.

## Guiding principle — maximize source-derivation

> Pull everything available from the trusted open-source repos before any hand/AI
> authoring. (Owner directive: "güvenli open source repolardan ne varsa her şeyi alalım.")

Trusted sources already cached/available:
- `SiegeEngineers/aoe2techtree` `data.json` — civ list, tech tree, unit stats (MIT).
- `SiegeEngineers/aoe2techtree` `data/locales/{en,tr}/strings.json` — **official in-game
  EN + TR** help text, unit/tech names (verified: 53/53 civs covered in TR).
- `aalises/age-of-empires-II-api` `civilizations.csv` / `units.csv` (BSD-3-Clause).
- `aoc-reference-data` (`aoc-reference-100.json`, cached).

Hand/AI authoring is reserved **only** for editorial copy that has no source equivalent:
build-order titles, civ/map strategy prose, the unit `role` taxonomy, and geographic
region labels. Every game *fact* — in both languages — comes from a source.

## Approach

**Chosen: "Re-derive EN *and TR* from source, preserve editorial prose in YAML."** The
generator regenerates civ facts from aoe2techtree in **both languages** — it runs the
existing `parseHelpBonuses` over `strings-en.json` *and* a newly-synced `strings-tr.json`
to populate `bonuses`, `teamBonus`, and `uniqueTechs.*.effect` for EN and TR from the
official locale. It reads the existing bilingual YAML only to carry over the genuinely
hand-authored fields (`strategy`, and TR for any editorial field) when the EN value is
unchanged. Translations of *facts* are therefore source-derived and regeneratable;
only editorial prose lives by hand in the YAML. Rejected: (2) retire the generator →
loses the re-sync path as the game patches; (3) surgical delete-and-translate only →
leaves the regen landmine and blind tooling, so defects recur.

## Workstreams

### WS1 — Remove the 6 fabricated civilizations
- Delete `src/content/civilizations/{achaemenids,athenians,macedonians,spartans,thracians,puru}.yaml`.
- Remove their 6 entries from `src/data/civilizations.json` (→ 53).
- Delete the entire `SUPPLEMENTAL` block and the fabricated `REGION_OVERRIDE` keys in
  `build-civilizations.mjs`. All 53 source civs have a `help_string_id`, so SUPPLEMENTAL
  is fully redundant; removing it makes civ data 100% source-derived. (Keep
  `REGION_OVERRIDE`/`REGION_NOUN` for the 53 — editorial geographic categorization,
  not fabricated game stats.)
- Delete the 24 icon files (`{slug}.png` + 3 `.webp` sizes each) for the 6 civs;
  regenerate `src/data/icon-map.json`.
- **Keep** `units/war-elephant.yaml` — verified `civ: persians`, a real unique unit, not
  Puru's.
- Verify (already confirmed): no dangling refs to the 6 in maps/build-orders/units/counters.
- Remove the unused `src/data/tech-tree.json` stub (and its `tech-tree` collection wiring
  if any — confirm none).

### WS2 — Fix the generator + add the TR source
- Add `data/locales/tr/strings.json` to `scripts/sync-game-data.mjs` URLs (cache as
  `strings-tr.json`), mirroring the existing EN fetch.
- In `build-civilizations.mjs`, run `parseHelpBonuses` over **both** EN and TR strings;
  emit `bonuses`, `teamBonus`, and `uniqueTechs.*.effect` as `{ en, tr }` both sourced
  from the official locale. (`uniqueTechs.*.name` stays EN — allow-listed proper noun.)
- Rewrite the carryover read-side to parse the existing bilingual
  `src/content/civilizations/<slug>.yaml` (not the deleted `en/`,`tr/` dirs) and preserve
  only the **editorial** fields (`strategy.en`/`strategy.tr`, and TR for any field with
  no source equivalent) when the regenerated EN value matches the stored EN.
- Add a regression test (vitest): a regen with unchanged EN must keep `strategy.tr`
  verbatim and must source TR bonuses/effects from `strings-tr.json`.

### WS3 — Fill remaining fields (mostly from source, editorial by hand)
**Source-derived (no hand translation):**
- Civ `bonuses`, `teamBonus`, `uniqueTechs.*.effect` in TR — from `strings-tr.json`
  (WS2). Eliminates the 37+37 untranslated tech effects, the 6 bonuses, the 6 team
  bonuses, and the `franks` placeholder in one deterministic pass.
- Civ archetype/`specialty` — from the help-string first line in EN + TR
  (e.g. `"Süvari medeniyeti"` → `Cavalry` / `Süvari`), replacing aalises' typo-prone
  `army_type`. This also feeds the TR tagline.

**Editorial (hand/controlled, no source equivalent):**
- Civ `tagline` — a localized sentence *template* using sourced specialty (EN+TR) +
  region label (region is an editorial geographic map, kept). Generated, not per-file.
- Unit `role` (29 untranslated) — a single controlled EN→TR taxonomy map in
  `build-units.mjs`, applied to all 90 units for consistency.
- Build-order `name` (18 untranslated) — translate the editorial titles by hand.

**Convention** (mirror the A-grade maps/glossary/beginner style): proper nouns — unit
names, tech names, civ names, map names, glossary terms — stay EN; descriptive text is
natural Turkish (`"Throwing Axemen +2 range"` → `"Throwing Axemen +2 menzil"`).
Independent native-Turkish verification pass over the editorial output only (the sourced
fields are official and need no review beyond a parse sanity-check).

### WS4 — Build-order accuracy + attribution
- Fix the 5 duration mismatches; correct `cumans-2tc-boom` `targetAge` (feudal→castle)
  and `durationMin` to match its Hera source.
- Populate the `source` block (`author: "Hera"`, `url`) on the 11 Hera-derived builds.
- 35 unsourced builds: content unchanged (verified structurally sound by
  `verify-build-facts.mjs`); no provenance note added this round.

### WS5 — Re-instrument guardrails + docs
- New `scripts/audit-yaml-translations.mjs`: walk every bilingual YAML, report fields
  where `en === tr`, with an **allowlist** of intentional-EN fields
  (`name`, `term`, `uniqueTechs.*.name`, civ/map proper-noun names). Exit non-zero on
  any non-allowed `en===tr`. Wire into `prebuild`/CI.
- Repair or retire `check-translations.mjs`; update `i18n-coverage.mjs` so the combined
  signal reflects YAML reality.
- Update `CLAUDE.md` and `src/content/CLAUDE.md` to describe single bilingual YAML, the
  audit script, and the corrected generator behavior.

## Verification

- `pnpm build` passes (schemas + `validate-data` + `verify-build-facts`).
- `civilizations.json` has 53 civs; `node scripts/validate-data.mjs` OK.
- New `audit-yaml-translations.mjs` reports 0 non-allowed `en===tr`.
- Generator regression test passes: a dry regen sources civ TR facts from
  `strings-tr.json`, preserves `strategy.tr` verbatim, and the only change vs the
  current `civilizations.json` is the 6 removals.
- Spot-check 5 civ pages + 5 build pages render correct TR.

## Risks

- **Translation quality**: the AI/hand surface is now small — only build-order titles,
  the tagline template, and the unit `role` taxonomy. Civ facts (bonuses/team/tech
  effects) are official source text, so the risk is confined to editorial copy.
  Mitigation: native-Turkish verification pass over editorial output + the proper-noun
  allowlist keeps technical terms stable.
- **Icon/asset removal**: deleting civ PNGs could break a hard-coded reference.
  Mitigation: grep for the 6 slugs across `src/` before deleting; regen icon-map.
- **Generator rewrite**: must not change EN output for the 53. Mitigation: diff
  `civilizations.json` before/after regen — only the 6 removals should change.
