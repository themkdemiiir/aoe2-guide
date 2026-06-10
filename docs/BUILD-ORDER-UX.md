# Build-Order UX — file-grounded improvement spec

> Verified against repo @ `3bf752b`, 2026-06-09. Everything here is **$0 and zero new runtime dependencies** — vanilla inline `<script>` modules matching the existing `ThemeToggle.astro` pattern. No React, no state library.
>
> Current state (verified): `src/pages/[lang]/builds/[build].astro` (117 lines) renders header/stats/civs + `BuildOrderSteps.astro`, which is **fully static — no client script anywhere on the page**. Step schema is already ideal for interactivity: `{ villagers: number, time?: string, assign: string, note?: string, icons?: string[] }`. All interactivity below is greenfield on top of existing data; **no content restructuring required** except where explicitly marked.

---

## P0 — correctness & i18n (S, ship first)

### 0.1 Untranslated hardcoded strings on `[build].astro`
These bypass `t(lang, …)` even though equivalent keys **already exist** in `src/i18n/ui.ts`:

| Line | Hardcoded | Fix |
|---|---|---|
| `:56` | `Source:` | new key `builds.source` (TR: "Kaynak") — note `blog.sources`/"Kaynaklar" exists but is plural |
| `:64` | `label="Difficulty"` + raw value `{entry.data.difficulty}` ("beginner") | label → `t(lang,"filter.difficulty")` ✅ exists ("Zorluk"); value → `t(lang, \`difficulty.${entry.data.difficulty}\`)` ✅ exists (Başlangıç/Orta/İleri) |
| `:65` | `label="Target"` + raw `{entry.data.targetAge}` ("feudal") | label → `t(lang,"filter.age")` ✅ exists ("Hedef çağ"); value → `t(lang, \`age.${entry.data.targetAge}\`)` ✅ exists |
| `:66` | `label="Duration"` + `~{n} min` | new key `builds.duration`; unit → `t(lang,"home.minutes")` ✅ exists ("dk") |
| `:72` | `Recommended civilizations` | `t(lang,"maps.recommendedCivs")` ✅ exists ("Önerilen Uygarlıklar") — or duplicate as `builds.recommendedCivs` if you want them independent |
| `:80` | `{civSlug.replace(/-/g, " ")}` renders "japanese" lowercase | reuse `titleCase()` (already written in `BuildOrderSteps.astro`, move to a shared util) — civ proper names stay English per glossary, but capitalized |
| `:81` | `Tier {…}` | `t(lang,"matchups.tier")` ✅ exists ("Sınıf") |

### 0.2 Structured phase markers (kills the regex)
`BuildOrderSteps.astro` detects age transitions by **regexing prose**: `isPhase()` matches `/\b(Feudal|Castle|Imperial|Kale|Feodal|Emperyal)\s+(arrives?|click|hit|tıkla|geç|geçiş)/i` and `phaseLabel()` re-matches keywords. This is fragile (a step saying "wall before Castle" can false-positive), hardcodes the deprecated "Emperyal" spelling (see GLOSSARY Conflicts), and silently breaks on rephrased TR.

Fix: add an **optional** `phase: "feudal" | "castle" | "imperial"` field to the step schema in `src/content/config.ts`; component prefers it, regex stays as fallback for unmigrated files. Backfill with a one-shot script over the 36 builds (the regex itself can do the backfill, reviewed once — then it never runs at request of correctness again). Phase section labels then come from the `age.*` i18n keys instead of hardcoded English `"Feudal Age"` strings in `phaseLabel()`.

---

## P1 — interactivity (one inline script, ~150 lines total, no deps)

All of these live in a single `<script>` in `BuildOrderSteps.astro` + `data-` attributes, mirroring how `ThemeToggle.astro` already does vanilla DOM + `localStorage`.

### 1.1 Step tracking
- Each `<li>` gets `id="step-{n}"` and `data-step`.
- Click (or Space on the focused step) toggles done → strikethrough/dim via a class; **j/k or ↑/↓** move the highlighted "current" step; current step gets a gold left border + `scrollIntoView({block:"center"})`.
- Persist `{slug → doneSet, current}` in `localStorage` key `bo-progress:{slug}` (wrap in try/catch — Safari private mode throws on write). "Reset" button clears.
- Deep-linkable: keep `location.hash = "#step-7"` in sync with current.

### 1.2 Focus mode ("play view")
Toggle button that adds one class to the section; CSS does the rest: current step rendered large (`clamp(1.5rem, 4vw, 2.5rem)`), villager count as a big sticky number, next step previewed dim below, everything else hidden. Readable from across the desk mid-game. Zero JS beyond the class toggle (state shares 1.1's "current").

### 1.3 Game-clock timer (accuracy-critical — read the note)
Start/pause/reset stopwatch displayed as a **simulated in-game clock**.

**Verified facts:** AoE2 DE's "Normal" speed — the ranked/tournament standard — runs at **1.7× real time** (the old 1.5× from HD was deliberately changed back), and **the in-game clock shows game-time, not wall-time** ("the ingame clock changes speed accordingly"). Build-order timings like `time: "8:25"` in the frontmatter are therefore **in-game clock values**. So: a wall-clock stopwatch is *wrong* for following along; the widget must advance its displayed clock at `speed ×` wall time, with a selector for the real DE speeds **1.0 / 1.5 / 1.7 / 2.0, defaulting to 1.7**.

- When the simulated clock passes a step's `time:`, mark it "due" (amber) — but **do not auto-advance untimed steps**: in the sampled build only 1 of 16 steps carries `time:`, so the timer is a reference clock, not a step driver. (Auto-pacing every step is the full trainer — FEATURE-IDEAS #2, stage 2.)
- Optional audio ping on a due step: a 2-line WebAudio `OscillatorNode` beep — no audio assets, no library.
- Implementation: `performance.now()` delta × speed; never `setInterval`-accumulate (drifts).

### 1.4 Read-step-aloud (TTS) — stage 1 of the trainer
A 🔊 button per step / on the current step: `speechSynthesis.speak(new SpeechSynthesisUtterance(step.assign))` with `utterance.lang = document.documentElement.lang`.

**Accuracy caveats, stated explicitly:**
- Web Speech *synthesis* is free and on-device/OS-provided — no API, no quota. (Don't confuse with Speech *recognition*, which in Chrome is a network service.)
- **`tr-TR` voice availability varies by platform**: present on Windows (Microsoft Tolga; natural voices in Edge), macOS/iOS (Yelda), Android (Google TTS), Chrome desktop (network "Google Türkçe" voice — requires being online). It can be absent on bare Linux. Therefore: feature-detect via `speechSynthesis.getVoices()` filtered by `lang.startsWith("tr")`; if none on a `/tr/` page, hide the button rather than reading Turkish text with an English voice. `getVoices()` populates async in Chrome — listen for `voiceschanged`.
- English unit names inside TR sentences will be pronounced by the TR voice (acceptable; players do the same).

### 1.5 Copy as text
Button serializing steps to plain text (`6 vill — 6 → sheep under the TC …`) via `navigator.clipboard.writeText` for pasting into Discord. ~15 lines. (Clipboard API requires HTTPS — fine, the site is.)

### 1.6 Print / cheat-sheet stylesheet
`@media print` in `globals.css`: hide header/footer/civ chips, single-column condensed steps, force light theme tokens. CSS only; pairs with a "Print" button (`window.print()`).

---

## P2 — needs a (small) data addition

### 2.1 Eco-allocation bar
A sticky "13 food / 3 wood / 0 gold" bar per phase. The allocation exists only in prose today (`note: "Land with roughly 3 on wood and 13 on food"`) — **do not parse prose for numbers** (accuracy rule). Add an optional `eco: { food?, wood?, gold?, stone? }` field on age-landing steps only; render the bar when present. Backfill from the source guide's source during the Epic 9 translation pass (you're touching every file anyway).

### 2.2 PWA / offline builds (optional library: `@vite-pwa/astro`, MIT, free)
Service worker precaching `/builds/**` + icons so the trainer works offline at LANs. The **only** library this whole document needs, and it's optional. Defer until after 1.1–1.4 prove out.

---

## Explicit non-goals
- No auto-advancing full trainer here (that's FEATURE-IDEAS #2; 1.3 + 1.4 are its staged foundation).
- No prose parsing for any number (villagers/eco/timing) — structured fields or nothing.
- No framework islands; if shared state across components ever becomes real, `nanostores` is the pre-approved escape hatch (checklist Libraries), not React.

## Suggested order
0.1 + 0.2 (one PR, pairs with Epic 1 regeneration) → 1.1 + 1.2 (one script) → 1.5 + 1.6 (trivial) → 1.3 → 1.4 → 2.1 during translation backfill → 2.2 last.
