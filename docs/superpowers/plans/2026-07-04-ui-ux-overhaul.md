# AoE2 Guide — Site-wide UI/UX Overhaul Plan

> Generated 2026-07-04 from a 13-specialist UI/UX audit reading the real rendered pages (light/dark/mobile) + source. Companion to [2026-07-04-fixes-and-features.md](./2026-07-04-fixes-and-features.md) — **Plan F (chart unification) there is the home for all chart/dataviz items (theme T1)**; do not duplicate it.

**Session annotations (2026-07-04):**
- **C1 is already FIXED and shipped** (commit `9bc9930`): the civ-matchups inline-script TS-generic bug that killed the ladder/map/view controls on every civ page. Verified error-free. The table below keeps it for the record, struck through.
- **C2, C3, C4 independently re-verified as real** against the source (unlocalized `toLocaleString` at matchups/index.astro:244,419; raw kebab slug in `<code>` at UniqueUnitBlock.astro:82; hardcoded English `<h2>Upgrade Chain</h2>` at units/[unit].astro:64).
- Execution order: **correctness bugs (C2-C5) → Phase 1 quick wins → Phase 2 shared primitives (incl. Plan F) → Phase 3 per-page → Phase 4 net-new.** Phase 2 primitives (table cell, eyebrow, select) are connective tissue — build them before the per-page passes so Phase 3 stays thin.

---

# AoE2 Codex — Consolidated UI/UX Audit → Execution Plan

## 1. State of the UI (3-sentence summary)
The illuminated-codex identity is strong and fully realized on the **home cover** (mono eyebrows + gold hairline ticks, Cinzel headings, ember/gold restraint), but that language **evaporates on every interior page** — Civ, Map, Unit, Matchups, Learn read like a plainer, more default site. The site's core signal — win rate — is **systematically misrendered** (zero/truncated baselines, no 50% reference line, unreadable overlapping axis ticks, three unrelated chart systems), and enumerable data is drawn **four incompatible ways** with no shared numeric-cell convention. Underneath the polish sit a handful of **real correctness bugs** (a dead JS toggle shipping on every civ page, locale-broken number formatting, untranslated headings, slug-leaking empty states) that must be separated from styling work.

---

## 2. CORRECTNESS BUGS — fix first (not UI polish)

| # | Bug | Sev | Eff | File(s) |
|---|-----|-----|-----|---------|
| ~~C1~~ ✅ DONE | `<script data-astro-rerun>` ships raw TS generic `querySelectorAll<HTMLElement>` to the browser → SyntaxError aborts the whole script → **1v1/Team toggle, map `<select>`, and active-state are dead on every civ page** (confirmed in built HTML). Fix: make it a processed module (add an import like CivStats does) or drop the four `<HTMLElement>` annotations; re-verify the generic is gone from `dist/`. | **high** | S | `src/components/content/CivMatchups.astro` |
| C2 | `games.toLocaleString()` takes no locale arg → renders **"2.316 games" on the EN page** (European separator), non-deterministic across visitors. Fix: `toLocaleString(lang === 'tr' ? 'tr-TR' : 'en-US')`. | med | S | `src/pages/[lang]/matchups/index.astro` |
| C3 | Empty state renders **"Stats not yet available forcomposite-bowman"** — collapsed space + raw kebab slug in `<code>` + a duplicate "stats pending" Badge saying the same thing. Fix: interpolate localized display `name`, fix spacing, drop the redundant marker. (Reported by 3 auditors.) | med | S | `src/components/content/UniqueUnitBlock.astro`, `src/i18n/ui.ts` |
| C4 | Hardcoded English `<h2>Upgrade Chain</h2>` + unit description untranslated on TR routes. Fix: add `units.upgradeChain` i18n key. | med | S | `src/pages/[lang]/units/[unit].astro`, `src/i18n/ui.ts` |
| C5 | Patch/cube data fetch has no loading state — stale UI sits frozen then snaps (x-states auditor). Fix: intermediate loading state. | med | S | (CivStats data-slice fetch) |

---

## 3. Deduplicated themes (merged across auditors)

- **T1 — Chart/dataviz honesty (Plan F territory).** Zero/truncated baselines crush the 48–62% real spread into the top third; no 50% reference line despite captions claiming "50% = average"; elo x-axis ticks (exactly ~10 buckets, guard fires at >10) overlap into a smear; values are tooltip-only; dark-mode "bad" bars flip to loud salmon; three unrelated chart vocabularies (Chart.js bars vs map hbar-list vs ECharts). *Flagged by civ, x-tables, cross-cutting, x-a11y, matchups, analyzer auditors.* Files: `CivStats.astro`, `MapCivRankings.astro`, `matchups/index.astro`, `analyzer.astro`, `globals.css`.
- **T2 — Shared table / numeric-cell system.** Four incompatible renderings of enumerable facts; the one real `<table>` (unit page) is the least polished (left-aligned values, no `tabular-nums`, no header, no dividers); build-order values center-aligned; analyzer player table left-aligned. Files: `units/[unit].astro`, `BuildOrderSteps.astro`, `CivMatchups.astro`, `CivStats.astro`, `analyzer.astro`.
- **T3 — Typography / eyebrow hierarchy.** Codex eyebrow+tick motif is home-only; same-rank H2s render at 2–3 different sizes per page (civ, map); h3 collapses into bold body runs (learn); format labels use stray Inter semibold. Files: home + all interior page files, `globals.css`.
- **T4 — Mobile / a11y.** `gold-500` as small text/links ≈1.8:1 on parchment (fails WCAG); sub-44px tap targets (hamburger, search, chart dataZoom handle ~10px); missing `focus-visible` on `<select>`/`<summary>`; floating toolbar overlaps 3rd spotlight card at 390px; build split-columns crush the step text on mobile; chain stats `hidden sm:flex` disappear on mobile. Files: `globals.css`, `Header.astro`, `BuildOrderSteps.astro`, `MapCivRankings.astro`, `BaseLayout.astro`, `units/[unit].astro`.
- **T5 — States / interaction.** No row hover/focus on clickable+focusable build steps; native OS `<select>` breaks identity everywhere; analyzer loading = bare mono text for a 1.4MB WASM fetch; elo filter silently drops the per-map block; empty states leak slugs (see C3). Files: `BuildOrderSteps.astro`, `CivStats.astro`, `MapCivRankings.astro`, `FilterBar.astro`, `matchups/index.astro`, `analyzer.astro`.
- **T6 — Per-page depth gaps.** Matchups is one-pair-at-a-time with no ranked table; comparator panels don't row-align; glossary "Related terms" are plain text not links + no sticky nav/filter on a 5600px page; learn TOC has no scroll-spy + weak pager + hidden course position; analyzer drop zone is timid/iconless + not keyboard-operable + no sample path; map header thinner than data-only pages; unit page reads sparse/unfinished.

---

## 4. Phased roadmap (impact-vs-effort ordered)

### Phase 1 — Quick wins (high-impact / low-effort, mostly S) — do right after correctness
- **[high/S]** Chart baseline honesty: replace `suggestedMin: Math.min(40,…)-1` with hard `min/max` (~40–62) + a 1px 50% reference line on byElo/byPatch/byMap — *implement as the first slice of Plan F, not a throwaway.* `CivStats.astro`
- **[high/S]** Elo axis legibility: drop rotation threshold to `>6`/always, abbreviate buckets (`<1.0k`, `1.0–1.1k`…`2.2k+`); same for byPatch → use stored `patchLabel`. `CivStats.astro`
- **[high/S]** `gold-500` text contrast: route text-sized gold through `gold-700` on the light token block only (prose links, eyebrow labels, "View all →"); dark-mode gold already passes. `globals.css`, `Header.astro`
- **[high/S]** Build-order value legibility: floor unchanged cells to ≈`stone-700/70` (AA), keep changed values bold in resource hue, render empty cells as a hairline dash not a 30%-opacity digit. `BuildOrderSteps.astro`
- **[med/S]** Right-align numeric cells + `font-mono tabular-nums`: build-order (currently center), unit stat table (currently left+Inter), analyzer player table (currently left). Fixes column scanning in three places. `BuildOrderSteps.astro`, `units/[unit].astro`, `analyzer.astro`
- **[med/S]** Build step rows: add themed `hover:bg-gold-500/5` + `focus-visible` gold ring (rows are `cursor-pointer` + `tabindex=0` today with zero feedback). `BuildOrderSteps.astro`
- **[med/S]** `focus-visible` gold ring for `<select>` and `<summary>` in the global rule (currently only `a`/`button`); bump hamburger/search to 44px hit areas. `globals.css`, `Header.astro`
- **[med/S]** Unit page: suppress zero-cost resources (`0f`/`0s`), set stat values in `font-mono tabular-nums`, lift dark-mode label contrast (`dark:text-stone-400`). `units/[unit].astro`
- **[med/S]** BuildCard: relabel "TEMPO" eyebrow to "AGE"/"REACHES" (bar encodes target age, not pace) and move fill off pure green toward the ember/gold ramp. `BuildCard.astro`
- **[low/S]** Hero provenance line: replace "current patch" with the real patch id from civMeta; lift numerals to gold tabular-nums, mute connectives. `[lang]/index.astro`
- **[low/S]** Build "BUIL" header (`RES_LABEL[k].slice(0,4)`) → proper icon or explicit i18n abbreviation (won't localize; reads as a glitch). `BuildOrderSteps.astro`

### Phase 2 — Systemic (the reusable foundations, M)
- **[high/M] Shared numeric-cell / table primitive** — one convention (numeric = `text-right font-mono tabular-nums`, label left, hairline `stone-700/15` dividers, optional `bg-parchment/70` zebra with a *real* step, header-row treatment); retrofit the unit `<table>` first, then build-order, matchups lists, CivStats tiles, analyzer table. Resolves T2 + the "four incompatible tables" finding. `units/[unit].astro`, `BuildOrderSteps.astro`, `CivMatchups.astro`, `CivStats.astro`, `analyzer.astro`
- **[med/M] Chart unification — reuse existing Plan F**, don't re-plan. Bring byElo/byPatch into the "by map" hbar-with-inline-value language (thin marks, 4px rounded data-ends, single hairline grid, value printed at bar end, red reserved for genuinely-below-average). One bar language across all win-rate views + matchup bars + analyzer production chart (add the missing solid=vils / dashed=military legend). `CivStats.astro`, `MapCivRankings.astro`, `matchups/index.astro`, `analyzer.astro`
- **[med/M] Shared section-eyebrow component** — promote the home mono-kicker + gold-tick motif to a component used on every interior section head, encoding something true ("RANKED · 1v1 RM", "TECH TREE", "CHAPTER n"). Unify all page-level H2s to one Cinzel scale + one divider rule. Resolves T3. Interior pages + `globals.css`
- **[med/M] Themed `<select>` component** — parchment fill, hairline border, custom gilded chevron, mono `text-xs`, 44px min-height, gold `focus-visible`; apply to FilterBar, CivMatchups, MapCivRankings, CivStats. Removes the most obvious "default SaaS" element. `FilterBar.astro`, `globals.css`, data-page components
- **[med/M] Dark-mode token discipline** — hand-select burnished dark gold (lower L / higher C) instead of auto-lighten; give the logo tile its own dark fill token; pull `wr-bad` to muted oxblood so charts stay validated on the dark ground. `globals.css`

### Phase 3 — Per-page deeper redesigns
- **[high/L]** Matchups: add a **ranked best/worst matchup table** for the selected civ from `h2hElo[a]` (opponent icon+name, win% right-aligned mono, games) — turns a lookup tool into a coaching tool. `matchups/index.astro`
- **[high/S]** Matchups: replace binary 50% color cut with a **diverging encoding + neutral 48–52% band** (per-map + h2h), add a hairline 50% tick + civ-bound legend to the signature bar, add a low-sample honesty state. `matchups/index.astro`
- **[med/L]** Matchups: restructure the two brochure panels into an **attribute-aligned comparison** (shared row labels, civ A/B columns). `matchups/index.astro`
- **[high/M]** Glossary: make **"Related terms" real anchors** (typed `related: [slug]` in schema, `id={slug}` per entry, gold chips); add a **sticky A–Z nav + type-to-filter**; turn the section letter into an illuminated drop-cap chapter break. `glossary/index.astro`, glossary schema/yaml
- **[high/M]** Learn: **scroll-spy TOC** (IntersectionObserver → `aria-current`), **numbered "Chapter n of 8" eyebrow**, **bordered prev/next pager cards**, distinct h3 signature, one codex flourish (lead drop-cap / fleuron divider). `learn/[chapter].astro`, `TableOfContents.astro`, `globals.css`
- **[high/M]** Analyzer: make the **drop zone the one bold element** (focal glyph + warm gold rest state), promote "Choose a file" to a real ≥44px button, make it **keyboard-operable** (`role="button"`, Enter/Space, visible focus), add a two-phase loading state (engine → replay). `analyzer.astro`
- **[med/M]** Map page: give the curated header the same dignity as data pages (mono eyebrow + larger Cinzel h1 + `type · size · N games`); explain or fold the "Recommended Civilizations" list (currently a redundant, unexplained 5-chip block next to the 53-civ chart); extract one `CivChip` size; widen the dataZoom grab to ≥24px on mobile. `maps/[map].astro`, `MapCivRankings.astro`
- **[med/M]** Home: fix tier-distribution bar palette (B/C/D indistinguishable tans → wider ΔL + hairline separators + inline counts); anchor spotlight win-rates to 50% with sample context; replace the "Counter an opponent" card's duplicated description-as-count; per-type map icons; dock the floating toolbar so it stops overlapping the 3rd spotlight card at 360–430px; extend eyebrows into interior sections. `[lang]/index.astro`, `BaseLayout.astro`

### Phase 4 — Net-new
- **[med/L]** Unit page **counters block** — start with a cheap "Strong vs / Weak vs" pair using existing `counters.strongVs/weakVs/beats` strings, graduate to bonus-damage hbars (Plan F marks) once data is wired. This is the highest-value missing element for a combat unit. `units/[unit].astro`, `i18n/ui.ts`
- **[med/M]** Unit page density: turn the Upgrade Chain into a pure cross-tier comparison (HP 4→35→40), make the stat table the authoritative block that adds what the chain omits (train time, armor classes, ROF, LOS) — kills the duplicate five-number readout; keep a compact stat readout visible on mobile (don't `hidden sm:flex` it). `units/[unit].astro`
- **[med/M]** Analyzer **sample-replay path** — bundle a small `.aoe2record` in `public/` + "Analyze a sample" secondary action so browse-only visitors see the payoff. `analyzer.astro`
- **[med/M]** Counters page: render age relations as a grouped table (winner/loser/reason, hard vs soft tag if source has it) instead of flat pills. `matchups/counters.astro`

---

## 5. Reuse existing planned work
- **Plan F (chart unification) already covers T1.** Phase 1's baseline/axis fixes and Phase 2's one-bar-language item are **slices of Plan F, not new work** — schedule them under it. The "by map" hbar-with-inline-value component is the already-on-theme reference every other win-rate view (byElo, byPatch, matchup bar, analyzer production/matrix) should converge to; do not invent a fresh chart system.
- **The shared numeric-cell primitive (Phase 2) and the shared eyebrow/select components** are the connective tissue that lets Phase 3 per-page redesigns be thin — build those before the per-page passes so the deep redesigns consume them rather than re-styling ad hoc.
- **Dark-mode fixes converge on tokens in `globals.css`** — fix once at the token (gold, `wr-bad`, logo tile) rather than per-component.

Key files by touch frequency: `src/components/content/CivStats.astro`, `src/components/content/BuildOrderSteps.astro`, `src/pages/[lang]/units/[unit].astro`, `src/pages/[lang]/matchups/index.astro`, `src/styles/globals.css`, `src/components/content/MapCivRankings.astro`.