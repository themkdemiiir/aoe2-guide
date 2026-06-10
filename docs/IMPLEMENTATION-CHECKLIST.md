# AOE2 Guide — Master Implementation Checklist (v2, repo-verified)

> Verified against `themkdemiiir/aoe2-guide` @ `3bf752b` and live data sources on **2026-06-09**.
> Pair with `aoe2guide-plan-and-audit.md`. Tags: **[OPEN]** confirmed in current code · **[DONE]** already shipped · **[CORRECTED]** fix location/scope differs from v1 · **[NEW]** found in this verification pass.
> **Priority:** P0 = cheap + high impact · P1 · P2 · P3. **Effort:** S/M/L.

## Verification deltas since v1 (read first)

1. **aoestats is confirmed still dead** — every weekly dump through `2026-05-31_2026-06-06` has `num_matches: 0` (identical checksums = empty parquets). The site UI serves frozen **patch 162286 (Dec 2025)** aggregates. `sync-meta-stats.mjs` was already converted to a hardcoded scrape of that UI ("fetched 2026-05-23") — so `metaLastFetched: 2026-05-23` stamps the *scrape* date, not the *data* date. The data is ≥6 months stale.
2. **Patch drift is now a content problem, not just a tier problem.** Current game build is **177723** (June 2026). Since the last data sync: **update 169123 (Feb 2026) = Naval Overhaul** (Hulk line counters Fire Ships, Catapult Galleons, dock-bonus changes) and **The Last Chieftains DLC (2026-02-17)** added Mapuche/Muisca/Tupi — which is exactly why those three (plus aoestats-untracked Jurchens/Khitans) have no tier. → new Epic 4 items.
3. **The a/an fix lives in the build script, not the render layer** (v1 said "grep the render layer"). Tagline is baked into md frontmatter by `scripts/build-civilizations.mjs:1070`. And there's a second grammar bug there v1 missed: every `region` value is an **adjective** ("Eastern European", "North African") so "civilization from {region}." is ungrammatical roster-wide.
4. **The /tr/counters English-body problem is a data-JSON problem, not a missing-file problem.** `src/data/counters.json` `intro`/`why` strings are EN-only and render verbatim on TR pages. Content-collection coverage scripts can't see it.
5. **Epic 8 references corrected:** `aoe2companion`'s public repo no longer contains reliclink parsing (it proxies through their own backend) — use `ryantaylor/relic_link` docs + your own curl. reliclink also blocks robots-respecting fetchers; irrelevant for the server-side CF Function, but the "paste one real `getPersonalStat` response" gate still requires a manual curl.
6. **Measured i18n gap:** 70/232 TR files translated → **162 missing** (89 units, 52 civs, 13 maps, 8 build-orders). `src/content/civilizations/tr/` contains only `britons.md`. Zero byte-identical files currently (the fallback gap is *missing files* + *data-JSON strings*).

## Already good — do NOT touch
`output: static` · ClientRouter + hover prefetch · theme-before-paint inline script · `_headers` (immutable `/_astro/*`, `/images/*` 30d SWR, `/fonts/*` immutable, security headers, CSP report-only) · Cloudflare Web Analytics · Biome · Knip · Vitest · Playwright (local) · `validate-icons` / `verify-build-facts` prebuild · `check:translations` script · `tsconfig` extends `astro/tsconfigs/strict` · `.github/prompts/` convention (`translation-audit.prompt.md` etc.) · 36 build orders re-sourced from a pro player's build guide (EN+TR) · per-civ "How to Play" (53 civs).

---

## Epic 1 — Data & content correctness

- [ ] **P0·S [OPEN]** Fix `Calvary` → `Cavalry` (berbers, magyars) and remove `Civilzation` (khmer). Confirmed in **both** `src/data/civilizations.json` (lines ~102, ~818, ~910) and `src/content/civilizations/en/{berbers,magyars,khmer}.md` (frontmatter tagline + body line 19). Fix the JSON + the generator inputs in `build-civilizations.mjs`, then regenerate — don't hand-patch the md.
- [ ] **P0·S [CORRECTED]** Fix **a/an** in `scripts/build-civilizations.mjs:1070`: `` `${displayName} — a ${specialty} civilization from ${region}.` `` → `/^[aeiou]/i.test(specialty) ? "an" : "a"`. Same fix for the generated body sentence ("Magyars are a Eastern European civilization specializing in…"). Regenerate all civ md.
- [ ] **P0·S [NEW]** Fix the **region grammar** in the same template: all 18 `region` values are adjectives, so "from Eastern European." is wrong for every civ. Either (a) restructure: `{Civ} — a/an {region} {specialty} civilization.` / "…{specialty} civilization from the {region} region", or (b) add a parallel `regionNoun` field ("Eastern Europe", "North Africa"). (a) is one template edit; (b) is cleaner English. Pick one, regenerate.
- [ ] **P1·S [OPEN]** Normalize `specialty` casing: `Archers` → `Archer` (britons); lowercase `naval` in italians ("Archer and naval"), koreans ("Tower and naval"), saracens ("Camel and naval"), vikings ("Infantry and naval") vs capitalized elsewhere. Enforce via the Epic 10 schema.
- [ ] **P1·S [OPEN]** Missing `meta.tier`: **jurchens, khitans, mapuche, muisca, tupi** (verified). Blocked on the Epic 4 tier-source decision; if editorial, hand-set all five.
- [ ] **P2·S [OPEN]** Britons off-template tagline ("Foot archer specialists with the longest-range Longbowman.") — regenerating from the fixed template covers this for free.

## Epic 2 — Page rendering bugs

- [ ] **P0·M [OPEN]** De-duplicate civ pages. `src/pages/[lang]/civs/[civ].astro` renders structured bonuses (:58), `UniqueUnitBlock` (:67), uniqueTechs (:75-76) **and** `<Content/>` (:81), while md bodies still carry `## Civ Bonuses` / `## Team Bonus` / `## Unique Units` / `## Unique Techs`. The "How to Play" commit added narrative without stripping the duplicates. Fix: strip those four sections from md bodies via script (keep "How to Play" + intro), regenerate, keep the structured block as the single source.
- [ ] **P0·S [CORRECTED]** Kill the contradictory unit table. The second table is a **placeholder in the md body**: **89/90** files in `src/content/units/en/` contain `## Stats summary` with `Free`/`Melee` junk ("Standard unit. See the stats table above."). One-shot script: delete the `## Stats summary` section (and the pointless `## Notes` stub) from all unit md; the structured `unit-stats` render is correct (knight = 60f/75g, range 0). Mirror the deletion in `units/tr/` when those exist. Effort drops M→S since it's mechanical.
- [ ] **P1·M [OPEN]** Backfill unique-unit stats: **40 of 61** unique units have no `unit-stats` entry (arambai, boyars, camel-archer, condottiero, coustillier, konnik, legionary, leitis, obuch, organ-gun, ratha, serjeant, …) → all show "stats pending" via `UniqueUnitBlock.astro:78`. Source from `aoe2techtree` `data/data.json` through `build-units.mjs`.
- [ ] **P2·M [OPEN]** Expand rendered stat fields. `unit-stats.json` **already carries** `armorMelee`/`armorPiercing` — `[unit].astro:119-130` just doesn't render them. Add armor now (free); pull attack bonuses/speed/LOS from techtree in the same pass as the backfill.

## Epic 3 — Sorting & lists

- [ ] **P1·S [CORRECTED]** Root cause is shared: `getLocalizedEntries()` (`src/lib/content.ts:27-42`) returns raw `getCollection` glob order; both `civs/index.astro` and `maps/index.astro` render it unsorted (only *facet options* are sorted). Cleanest fix: sort by display name inside `getLocalizedEntries` once — fixes civs, maps, and any future index. If civs should stay tier-grouped, sort tier→`localeCompare(name)` at the page level instead.

## Epic 4 — Meta/tier data & patch currency

- [ ] **P0·S [OPEN]** Stop the homepage claiming "current". `src/i18n/ui.ts:66` `home.topCivs.subtitle: "Ranked by current ladder win rate."` and `:195` TR `"Güncel sıralama…"`. Given delta #1, decide:
  - **(a) Editorial tier** (recommended): hand-set tiers (you already synthesize from a pro player's build guide), reword to "Editorial tier list — updated for patch 177723", drop the winRate ranking on the homepage; **or**
  - **(b) Honest snapshot**: keep the scraped numbers but stamp them "patch 162286 snapshot (Dec 2025)" — and accept that the Naval Overhaul + Last Chieftains make a Dec-2025 winrate ranking actively misleading in June 2026. (a) is the defensible choice.
- [ ] **P0·S [NEW]** Fix the misleading stamp either way: `metaLastFetched: 2026-05-23` in `civilizations.json` is a scrape date on frozen patch-162286 data. Rename/split: `metaPatch: "162286"`, `metaDataThrough: "~2026-02-08"`, `scrapedAt: "2026-05-23"`. Distinguish from the game-data patch (`patch: v100.1.84` from techtree).
- [ ] **P0·M [NEW]** **Patch-currency sweep to build 177723.** Re-run `sync:game-data` / `build:units` / `build:facts` against current `aoe2techtree` data (confirm upstream is updated for 177723 first), then audit **naval content specifically**: update 169123's Naval Overhaul (Hulk line vs Fire Ships, Catapult Galleons, dock bonuses) touches unit lines, counters, and any water build orders the site documents. Add a CI-visible "data synced for patch X" stamp to the footer or About page.
- [ ] **P3·S [DOWNGRADED]** Historical aoestats page from the last good dump (≤2026-02-01): only if you want it; the parquet pipeline shows no sign of returning, so don't build anything live on it.

## Epic 5 — Design polish (`src/styles/globals.css`)

- [ ] **P0·S [OPEN]** Dark-mode body text is gold: `:40` `[data-theme="dark"] --color-ink: oklch(0.86 0.12 75)`. Set a neutral warm-light body (e.g. `oklch(0.90 0.01 80)`); keep gold for headings/links/accents. Biggest "looks weird" fix.
- [ ] **P0·S [OPEN]** Cinzel scope: `:84-85` applies `--font-display` to `.prose :is(h1,h2,h3,h4)`. Restrict to h1/h2 (+ logo), Inter for h3/h4 + UI, add slight `letter-spacing` to Cinzel.
- [ ] **P1·S [OPEN]** Verify gold-link contrast on parchment vs WCAG AA 4.5:1; darken `--color-gold-500` for link use if it fails.
- [ ] **P2·M [OPEN]** Tighten accent palette (gold primary; royal/forest/steel semantic-only; resource colours only on resource icons).
- [ ] **P2·M [OPEN]** Type-scale + spacing-scale tokens shared by index pages and `.prose`.
- [ ] **P3·S [OPEN]** Radius: `--radius: 4px` (`:34`) vs `Card.astro` `rounded-lg` (8px). Unify.
- [ ] **P3·S [NEW]** CSP is currently `Report-Only` in `_headers` — after a quiet period in reports, promote to enforcing.

## Epic 6 — Performance

- [ ] **P0·S [OPEN, sharpened]** Fonts load via `@import "@fontsource/…"` in `globals.css:1-7` (late discovery → FOUT), no preloads in `BaseLayout`. Because @fontsource woff2 URLs are content-hashed, a hardcoded `<link rel="preload">` won't survive builds. Either: (a) migrate the two critical faces (Inter 400, Cinzel 600) to Astro's **Fonts API** (`experimental.fonts`), which emits preloads + fallback metrics; or (b) self-host those two woff2 in `public/fonts/` with explicit `@font-face` + preload (the `/fonts/*` immutable cache rule already exists). Keep the remaining weights on @fontsource.
- [ ] **P1·S [OPEN]** Trim weights: 7 files imported (Cinzel 400/600, Inter 400/500/700, JBMono 400/700). Audit Inter 500 and JBMono 700 usage; drop unused.
- [ ] **P1·M [OPEN]** PNG → WebP for `public/images/aoe2/**` via a `sharp` step in `sync-assets.mjs` / `build-icon-map.mjs` (or move into `src/` + `<Image>`). Biggest byte win.
- [ ] **P2·S [OPEN]** `matchups/index.astro:106,128` builds `<img>` via strings without `width/height`/`loading="lazy"` → add.
- [ ] **P3·M [OPEN]** SVG sprite for tiny counter/matchup icons.
- [ ] **P3·S [OPEN]** Replace `astro.config.mjs:25` `icon({ include: { lucide: ["*"], heroicons: ["*"] } })` with the explicit used-icon list.

## Epic 7 — Cloudflare

- [ ] **P0·S [OPEN — dashboard, unverifiable from repo]** Rocket Loader OFF, Brotli ON, HTTP/3 ON. Skip Polish (plan-gated); WebP at build instead (Epic 6).
- [ ] **P2·S [OPEN]** Optional `preload` hints in `_headers` once Epic 6's font decision lands (stable URLs required → only viable with option b).
- [ ] **P3·S [OPEN]** Early Hints only if resource-hint headers exist.

## Epic 8 — Player stats & team builder

> **Gate unchanged:** curl one real `getPersonalStat?title=age2&profile_ids=[6174996]` response before writing the flattener. Note: reliclink robots-blocks generic fetchers — irrelevant server-side, but it means no third party will have done this for you. **Reference correction:** parse-shape ground truth is `ryantaylor/relic_link` (endpoint docs) + your own curl; `aoe2companion`'s public repo no longer calls reliclink directly.

- [ ] **P1·L [CORRECTED]** Proxy `/api/*` → `aoe-api.reliclink.com`, built as a **single Worker with static assets** (`run_worker_first: ['/api/*']`, Hono) rather than a Pages Function — Workers reached full Pages parity in March 2026 and all new platform features are Workers-only. Caching: use the **Cache API (`caches.default`) as the primary layer, not KV** — verified free-tier caps: KV allows only **1,000 writes/day**, and a 5-min TTL on even ~20 hot endpoints (~288 refills/day each) blows past it; the Cache API has no daily quota (per-PoP locality is acceptable for this traffic). Keep KV optional for cross-PoP needs only. Budget facts (verified): Workers Free = 100k req/day, 10 ms CPU/invocation, 50 subrequests/request, 3 MB compressed — all comfortably sufficient for a JSON proxy. One retry, fail-soft.
- [ ] **P1·M [OPEN]** TS Relic client (`getPersonalStat`, `getRecentMatchHistory`, `getAvailableLeaderboards`) + Valibot validation.
- [ ] **P1·M [OPEN]** Flattener: `statGroups` + `leaderboardStats` → `{profileId, name, country, ladders:{rm1v1, tg, ew}}` (join `member.profile_id → statgroup_id → leaderboardStats[leaderboard_id]`).
- [ ] **P1·M [OPEN]** Personal dashboard (paste profile_id → ratings, recent form, uPlot rating history).
- [ ] **P2·L [OPEN]** Team builder: 2ⁿ partitions (n ≤ 8), minimise `|winProb − 0.5|` (Elo logistic on TG rating), lobby in URL, map-aware civ suggestion per slot.

## Epic 9 — i18n & writing

- [ ] **P0·S [OPEN]** Extend `scripts/check-translations.mjs` (currently filename-only) with byte-identical-body detection — or just adopt the already-written `i18n-coverage.mjs` (drop into `scripts/`, set `CONTENT_ROOT`, wire `pnpm i18n:coverage`) and keep `check:translations` for the per-type table. Add to CI as non-blocking report first; blocking once the backlog is burned down.
- [ ] **P0·M [READY TO LAND]** Add `docs/i18n/GLOSSARY.md` + `docs/i18n/STYLE.md` (drafted; `docs/i18n/` doesn't exist yet). Resolve the `⚠ confirm` cells, then link from `.github/instructions/content.instructions.md` (confirmed path).
- [ ] **P0·M [NEW]** **Localize `src/data/counters.json`.** Its `intro`/`why` strings are EN-only and render verbatim on `/tr/counters` — this is the original "TR counters body is English" finding, and no file-based coverage check will ever see it. Restructure to `{en, tr}` string objects (or key into `ui.ts`), update `counters.astro`, translate via glossary. Audit other data JSONs for embedded prose while at it.
- [ ] **P1·S [READY TO LAND]** Ship the translate command as `.github/prompts/translate.prompt.md` (convention confirmed — `translation-audit.prompt.md` etc. already live there); adapt the drafted `translate.md`, bind to Content Curator, reference glossary + style.
- [ ] **P1·M [OPEN, quantified]** Burn down the 162 missing TR files in value order: **counters-adjacent unit pages → civs (52, only britons exists) → maps (13) → remaining build-orders (8) → long-tail units (89)**. One `/translate` invocation per file, review, commit. Do Epic 2's md-body cleanup **first** so you don't translate placeholder tables.

## Epic 10 — Stack modernization & CI guardrails

- [ ] **P1·M [OPEN]** Build-time data validation (Valibot schema over `civilizations.json`, `unit-stats.json`, `counters.json`): fail CI on missing tier, `Calvary`/`Civilzation`-class typos (regex denylist), casing violations, adjective-region misuse. Automates Epic 1's bug class.
- [ ] **P1·S [OPEN]** Link checker in CI (`lychee` over `dist/`) — would have caught dead librematch/aoestats links.
- [ ] **P1·S [NEW]** CI runs only check/biome/vitest/build (`ci.yml` verified). Add the Playwright suite to CI (it exists locally) — it's the only thing that can verify the §1.5 browser items (comparator, theme toggle, filters, Pagefind).
- [ ] **P2·M [CORRECTED]** TS migration: `tsconfig` already extends `astro/tsconfigs/strict`, so the remaining work is the **13 `.mjs` scripts** (0 `.ts` in `scripts/`) + enabling `noUncheckedIndexedAccess`.
- [ ] **P2·M [OPEN]** Astro 5 Content Layer (loader-based collections) with schema validation.
- [ ] **P2·M [OPEN]** `@axe-core/playwright` + Lighthouse CI budgets.
- [ ] **P2·S [OPEN]** CI assertion that the Pagefind index built and returns hits.
- [ ] **P3·S [OPEN]** Renovate/Dependabot.

## Epic 11 — Build-order UX (NEW — spec in `docs/BUILD-ORDER-UX.md`, zero new runtime deps)
- [ ] **P0·S** i18n the hardcoded EN strings on `[build].astro` :56/:64/:65/:66/:72/:80/:81 — the needed keys (`filter.difficulty`, `difficulty.*`, `filter.age`, `age.*`, `maps.recommendedCivs`, `matchups.tier`, `home.minutes`) **already exist in `ui.ts`**; only `builds.source`/`builds.duration` are new. Capitalize civ chip names via the existing `titleCase()`.
- [ ] **P0·S** Add optional `phase: feudal|castle|imperial` step field; `BuildOrderSteps.astro` prefers it over the fragile prose regex (`isPhase`/`phaseLabel`, which also hardcodes the deprecated "Emperyal"). Backfill the 36 builds once; phase labels then come from `age.*` keys.
- [ ] **P1·M** One vanilla inline script (ThemeToggle pattern): step done-toggling + j/k navigation + current-step highlight, `localStorage` progress per slug (try/catch for private mode), `#step-N` deep links, Reset.
- [ ] **P1·S** Focus mode: class toggle + CSS — current step huge, villager count sticky, rest hidden.
- [ ] **P1·S** Game-clock timer with speed selector **1.0/1.5/1.7/2.0, default 1.7** — DE "Normal" (ranked standard) runs 1.7× real time and the in-game clock shows *game-time*, so frontmatter `time:` values are game-clock; tick via `performance.now()` delta × speed. Marks timed steps "due"; does NOT auto-advance untimed steps. Optional WebAudio beep (no assets).
- [ ] **P1·S** TTS read-step button via `speechSynthesis` (free, on-device): `utterance.lang` from page lang; feature-detect `tr-TR` voices (`voiceschanged` event) and hide on `/tr/` if absent — availability varies by OS/browser.
- [ ] **P1·S** Copy-build-as-text (`navigator.clipboard`) + print cheat-sheet stylesheet (`@media print`).
- [ ] **P2·S** Eco-allocation bar — requires optional `eco:{food,wood,gold,stone}` on age-landing steps; **never parse prose for numbers**. Backfill during the Epic 9 translation pass.
- [ ] **P2·M** Optional PWA offline for `/builds/**` via `@vite-pwa/astro` (MIT) — the only library in this epic, deferred.
- [ ] **P2·M** Build-time OG images for all static pages: `satori` + `@resvg/resvg-js` in the build (Node — no Worker CPU limits apply). See FEATURE-IDEAS §3a; dynamic edge OG is paid-plan-only and out of scope.

---

## Libraries (updated)
`hono`, `valibot`, `uplot`, `sharp`, `msw`, `nanostores` (only if needed), `lychee` (CI), `@axe-core/playwright`, `@lhci/cli`, **`satori` + `@resvg/resvg-js`** (build-time OG, Epic 11), **`@vite-pwa/astro`** (optional offline builds, Epic 11). All MIT/Apache, $0. ~~`hyparquet`/`duckdb`~~ — drop unless the P3 historical page happens. **Avoid:** React, heavy chart libs, a database, GraphQL/ORM, anything with a paid tier dependency.

## Dependencies between epics
- Epic 9's translation burn-down depends on **Epic 2's md cleanup** (don't translate placeholders) and **Epic 1's regeneration** (don't translate typos).
- Epic 1 regeneration and Epic 4's patch sweep share `build-civilizations.mjs` / `sync-game-data` — do them in one PR train.
- Epic 8 gates on the manual curl; Epic 10's validation schema should land before Epic 1's regeneration so the regen is checked by it.

## Suggested first sprint (max impact, min effort)
1. **Epic 5 P0** — dark-mode ink + Cinzel scope (pure CSS, kills most "looks weird").
2. **Epic 1 P0 ×3 + Epic 2 P0 ×2** — one PR: fix `build-civilizations.mjs` template (a/an + region + typos), strip md duplicate sections + placeholder stat tables, regenerate. Land the Epic 10 Valibot schema in the same PR so the regen is validated.
3. **Epic 4 P0** — reword `home.topCivs.subtitle` (EN+TR), restamp meta fields, decide editorial-vs-snapshot.
4. **Epic 6 P0 + Epic 7 P0** — font strategy + Rocket Loader off.
5. **Epic 9 P0** — land `docs/i18n/`, wire `i18n-coverage.mjs`, restructure `counters.json` for TR.
6. **Epic 4 P0 patch sweep** — re-sync game data to 177723, audit naval content.
7. **Epic 11 P0** — build-page i18n strings + structured `phase` field (rides the same PR train as #2; the i18n keys already exist).
