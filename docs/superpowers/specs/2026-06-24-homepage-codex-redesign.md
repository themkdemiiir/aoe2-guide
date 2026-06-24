# Homepage redesign — The Illuminated Meta Codex

**Date:** 2026-06-24
**Scope:** `src/pages/[lang]/index.astro` + `src/i18n/ui.ts` only (bucket 1 of 3).
**Status:** approved direction; ready for implementation plan.

## Goal

Replace the current homepage — a vertical stack of six interchangeable sections
that reads as *assembled* — with a single composed broadsheet that reads as
*designed*. Keep the medieval parchment/maroon/gold + Cinzel/JetBrains-Mono
theme, keep every number wired to the live crawl data, keep it static +
bilingual (EN/TR via `t()`).

## Non-goals (explicitly out of scope for this spec)

- Map content gaps, matchups work (bucket 2) — separate spec later.
- Cloudflare deploy verification, team-data crawl, no-JS for the *interactive*
  tier-list/map pages, crawl automation (bucket 3) — separate later.
- Renaming the site. Brand stays "AOE2 Guide"; "codex" is a homepage conceit,
  not a rename.
- No new shared component files. Per `src/components/CLAUDE.md`, single-page
  repeated markup is mapped **inline** in `index.astro`, not extracted.

## Design concept — a two-act codex

The page has a spine: **Act I, a dark gilded cover** (masthead + live meta,
gold-on-dark like leaf on vellum) that you "open" into **Act II, the parchment
interior** (chapters + featured content). The dark→parchment transition is the
single structural move that turns a stack into a document.

```
╔═════════ ACT I — GILDED COVER  (one continuous bg-stone-bg section) ═════════╗
║  · A G E   O F   E M P I R E S   II ·            (gold smallcaps + ◆)        ║
║  ┏━┓                                                                          ║
║  ┃A┃OE2 Guide          ← illuminated initial: gold Cinzel cap in a maroon    ║
║  ┗━┛                       (--color-royal) block with a thin gold frame       ║
║       The living codex of the current meta.                                  ║
║       740K+ ranked 1v1 matches · current patch · updated Jun 2026   (mono)   ║
║       [ ⛨ Browse Civilizations ]   [ ▤ View Build Orders ]                    ║
║  ──────────────────────────── ◆ ────────────────────────────                 ║
║  STATE OF THE META                                view full tier list →      ║
║  ┌────────────┐  ┌────────────┐  ┌────────────┐                              ║
║  │  [emblem]  │  │  [emblem]  │  │  [emblem]  │   ← 3 illuminated crest cards ║
║  │ Khitans  S │  │ Lithuan… S │  │ Jurchens S │     (top 3 by live win rate) ║
║  │ 57.2% · …  │  │ 53.6% · …  │  │ 53.6% · …  │     WR (mono) + tier + spec   ║
║  └────────────┘  └────────────┘  └────────────┘                              ║
║  53 civs:  ▆S3 ▆▆▆▆A13 ▆▆▆▆▆▆▆▆B27 ▆▆▆C10        (slim stacked tier bar)     ║
╚══════════════════════════════════════════════════════════════════════════════╝
┌──────────── ACT II — PARCHMENT INTERIOR  (parchment bg) ─────────────────────┐
│  WHERE DO YOU WANT TO START?                                                 │
│  ┌─ I. ───────────────┐  ┌─ II. ──────────────┐  ← chapter cards: illuminated │
│  │ ⛨ Civilizations 53 │  │ ▤ Build Orders  43 │     roman numeral + icon +    │
│  └────────────────────┘  └────────────────────┘     desc + live count + →     │
│  ┌─ III. ─────────────┐  ┌─ IV. ──────────────┐                              │
│  │ ▦ Maps          13 │  │ ⚖ Matchups         │                              │
│  └────────────────────┘  └────────────────────┘                              │
├──────────────────────────────────────────────────────────────────────────────┤
│  FEATURED BUILD ORDERS                                          view all →    │
│  [ BuildCard ] [ BuildCard ] [ BuildCard ]            (beginner-first, ×3)    │
├──────────────────────────────────────────────────────────────────────────────┤
│  MAPS TO MASTER                                                view all →     │
│  [ Arabia ] [ Arena ] [ Black Forest ] [ Nomad ]                  (×4)        │
├──────────────────────────────────────────────────────────────────────────────┤
│  ── More to explore ──   Units · Counters · Learn · Blog · Glossary          │
└──────────────────────────────────────────────────────────────────────────────┘
```

## Visual kit ("illuminated but disciplined")

- **Illuminated initial** — oversized Cinzel capital, `text-gold-400`, sitting
  in a `bg-royal` block with a `border border-gold-500/50` frame and `--radius`.
  Used twice only: the masthead "A", and the four chapter numerals (I·II·III·IV,
  small variant). This is the signature; using it sparingly keeps it from kitsch.
- **Gold hairlines + `lucide:diamond` dividers** — already in theme; used for the
  act-divider (between masthead and State-of-the-Meta) and section rules.
- **Crest motif** — the live civ emblem via `<Icon slug={id} kind="civ" />`
  (responsive `-24/-64/-108.webp` srcset, already built). Star of the three
  spotlight cards and used nowhere else heavy, so it stays special.
- **Vellum grain** — `public/images/aoe2/Backgrounds/bg_aoe2_hd_paper.jpg` as a
  low-opacity (`~0.06`) repeating background layer on the dark cover only.
- **Type voices** — Cinzel = headings/illumination; **JetBrains-Mono = the data
  voice** (win rates, match counts, provenance, counts); Inter = body. Data reads
  in a visibly different "hand," selling the codex idea.
- **Dark-band token rule** (from existing code + `components/CLAUDE.md`): on
  `bg-stone-bg` use only `gold-*`/`ember` text tokens (they stay light in both
  themes); never `text-ink`/`text-stone-700` (they swap and vanish). Keep the
  load-bearing `border-y border-gold-500/40` so the band's extent stays visible
  in dark mode (where `--color-stone-bg` ≈ dark `--color-parchment`). No `dark:`
  prefixes anywhere — tokens do the work.

## Data wiring — all real, build-time, no-JS-safe

Everything renders from `civ-meta.json` + the content collections in Astro
frontmatter → **static HTML, works with JS off**. No hardcoded numbers; a crawl
refresh updates the home automatically. (Respects source-derived-only /
no-defaults.)

| Element | Source | Notes |
|---|---|---|
| Provenance match count | `civMeta.appearances["1v1"] / 2`, floored to 10K | 1,483,860 → **740K+**. Formatted `{n}K+`. |
| "updated {month year}" | `civMeta.generated` (`"2026-06-24"`) | parse `YYYY-MM` → localized month-year (EN/TR month arrays). |
| 3 spotlight crests | top 3 of `civs` by `1v1.winRate`, joined to `civilizations` collection for localized name + `civ-data` for `specialty` | currently Khitans/Lithuanians/Jurchens, all tier S. Robust if the S set is 2 or 4 — always shows 3. Reuses the existing join logic. |
| Crest tier badge + WR | each civ's real `1v1.tier` / `1v1.winRate` | tier color via existing `TIER_COLOR` map (data-driven inline style — the one sanctioned exception to "classes only"). |
| Tier-distribution bar | count `1v1.tier` across `civMeta.civs` → `{S,A,B,C,D}` | segment widths ∝ counts of total (53). Labels `S3·A13·B27·C10`. Pure CSS. |
| Chapter counts 53 / 43 / 13 | `civs.length` / `builds.length` / `maps.length` | collection lengths = source of truth. |
| Featured builds (×3) | `builds` sorted beginner-first, `slice(0,3)` | existing logic + `BuildCard`. |
| Featured maps (×4) | `maps.slice(0,4)` | existing logic. |

## i18n changes (`src/i18n/ui.ts`, EN + TR)

**New keys**

| Key | EN | TR |
|---|---|---|
| `home.codex.lead` | The living codex of the current meta. | Güncel metanın yaşayan kodeksi. |
| `home.codex.matches` | ranked 1v1 matches | sıralı 1v1 maç |
| `home.codex.currentPatch` | current patch | güncel yama |
| `home.codex.updated` | updated | güncelleme |
| `home.stateOfMeta.title` | State of the Meta | Metanın Durumu |
| `home.stateOfMeta.subtitle` | The civilizations leading the current ladder. | Güncel sıralamada öne çıkan uygarlıklar. |
| `home.tierList` | View full tier list | Tüm sınıf listesini gör |
| `home.tierBar.label` | tier distribution of {n} civilizations | {n} uygarlığın sınıf dağılımı |

(Final TR wording verified against existing translations for tone/diacritics
during implementation.)

**Fix (stale):** `home.topCivs.subtitle` currently reads *"Tier snapshot from the
December 2025 ladder (patch 162286)."* / *"Aralık 2025 …"* — contradicts the live
June-2026 data. The `home.topCivs.*` keys are replaced by `home.stateOfMeta.*`;
remove the now-unused `home.topCivs.title`/`subtitle` (confirm no other usage
first via grep).

## Files touched

- `src/pages/[lang]/index.astro` — full rewrite of the template; reuse the
  existing frontmatter join logic (civ-meta ↔ civilizations ↔ civ-data),
  collection-length counts, featured-build sort, featured-map slice. Add: meta
  provenance computation, tier-count computation, localized month-year helper.
- `src/i18n/ui.ts` — add the keys above (EN + TR); remove stale `home.topCivs.*`.

No new component files. No schema/data/script changes.

## Implementation steps

1. i18n: add new keys (EN+TR), remove stale `home.topCivs.*` (grep first).
2. Frontmatter: keep the join; add `matchesLabel` (from appearances), tier-count
   object, localized `updatedLabel` month-year, top-3 spotlight array (name +
   tier + winRate + specialty + id for the crest).
3. Act I — dark cover section: masthead (illuminated initial + wordmark + lead +
   provenance line + two CTAs) → diamond divider → State of the Meta (label +
   `view full tier list →` + 3 crest cards + tier bar). One `bg-stone-bg`
   `border-y` section; vellum grain layer.
4. Act II — parchment: chapters (4 illuminated-numeral cards) → featured builds
   (×3 `BuildCard`) → maps to master (×4) → "more to explore" appendix chips.
5. Verify (below). Then `frontend-design` polish pass + code review.

## Verification

- `pnpm check` (TS) and `pnpm build` (runs the `audit-yaml-translations` gate +
  schema validation) both pass.
- **No-JS:** view-source / JS-disabled render shows the meta, crests, counts —
  homepage is fully build-time.
- **Bilingual:** `/en/` and `/tr/` both render with no raw English enum leaking
  onto `/tr/`; all visible strings via `t()`.
- **Visual:** headless-chromium screenshot of `/en/` and `/tr/`, light + dark,
  desktop + mobile widths, for review.
- Numbers on the page match the JSON (740K+, S3·A13·B27·C10, 53/43/13).

## Risks / mitigations

- *Spotlight needs ≥3 civs with win rates* — dataset has 53; safe. Guard with
  `.slice(0,3)` and render the section only if ≥1.
- *Above-the-fold crest images* use `Icon`'s `loading="lazy"`; 3×~108px webp is
  negligible — accept (no eager-loading change to the shared component).
- *Dark-band text tokens* — easy to regress by using `text-ink` on
  `bg-stone-bg`; the screenshot in dark mode catches it.
