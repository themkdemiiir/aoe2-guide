# AOE2 Guide — Redesign & data brief

Living plan for the navigation/design/data work. Static-only (Astro 5, Tailwind v4
CSS-first, EN/TR). Source-derived data only (see [SOURCES.md](./SOURCES.md)).

## Decided direction

- **Overall vibe:** *In between* — clean, modern base with tasteful medieval accents (current direction, done better).
- **Focus (all four):** layout & spacing · cards & components · visual identity · typography & readability.

## Done

- ✅ **Navigation bug fixed.** `Card.astro` rendered an inert lowercase `<tag>` (Astro treats it as a literal element, not the `href?"a":"div"` variable) → every civ/unit/featured card was a non-clickable box. Capitalized to `<Tag>`; 53 civ cards + all card links work.
- ✅ **Mobile nav menu.** Header `<nav>` was `hidden md:flex` (no menu on phones). Added a `<details>` hamburger (☰) with all 8 sections, EN/TR, outside-click close.
- ✅ **Prose typography + Table of Contents** (`@tailwindcss/typography` was missing → all markdown was unstyled). Themed to the medieval tokens; TOC on Learn + Blog.
- ✅ **Source-verified unit facts.** `build-game-facts.mjs` → `game-facts.json` (age/cost/building from aalises); `verify-build-facts.mjs` gates builds against real unit ages (caught light-cavalry-in-Feudal). `SOURCES.md` documents provenance. A strict-Rust, aoe2techtree-sourced replacement for the aalises sourcing is built (`reference-data/`, `pipeline/crates/refdata`) and pending cutover — see `docs/rust-migration-plan.md`.
- ✅ **Card design pass (iteration 1).** `rounded-lg`, `shadow-sm`, hover lift + gold border, focus-visible ring, `group/card` title-hover.

## In progress / next

### Design (iterative — review on deploy, then steer)
- [ ] **Iteration 2 — components:** Badge (pill style + variants), Button primitive, Stat, section headers; consistent across pages.
- [ ] **Iteration 3 — layout & spacing:** section rhythm (`py`), grid gaps, container widths; homepage hero + section hierarchy.
- [ ] **Iteration 4 — visual identity:** tasteful medieval accents (subtle parchment texture / gold rules / iconography), dark-mode polish.
- [ ] **Iteration 5 — typography:** type scale + readability sweep across list/detail/prose pages.
- [ ] Accessibility pass (focus states, contrast, aria, keyboard) throughout.

### Data
- [ ] **Merge the facts data** into one source-attributed unit dataset (`game-facts` + `unit-stats` + counters), each field tagged with its origin; update scripts + `SOURCES.md`; keep `verify:facts` green.

### Content
- [ ] **Add a pro player's build guide-sourced builds** (start: *19 Vils Romans 5 MAA Rush* from the source guide's strategy guide PDF) with proper citations + higher-confidence `source`.
- [ ] **Correct all build orders (deferred — "later").** Finish the editorial accuracy pass: the ~80 subjective/strategy claims the sweep flagged (civ recommendations, "best/strongest" wording, timings) need an expert eye. Objective errors already fixed.

## Constraints
Static-only (no SSR) · Tailwind v4 CSS-first (no config) · `render(entry)` · every string via `t()` (EN+TR) · all `steps[].icons` pass `validate-icons` · builds pass `verify:facts` · facts stay source-derived (never hand-written).
