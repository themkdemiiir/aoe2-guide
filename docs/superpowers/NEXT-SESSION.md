# AOE2 Guide — next-session handoff

Paste the prompt below into a fresh session. The project memory
(`MEMORY.md` + memory files) auto-loads and carries the deep context; this doc
is the task list + the gotchas that matter for the remaining work.

---

## State as of this handoff (all pushed to `origin/main`, latest `b96d362`)

The stats are now **current** (June 2026), sourced from a self-collected World's
Edge crawl (756k 1v1 matches, on the VM `mkd@192.168.111.7` + backed up at
`data-cache/relic-patched/matches.ndjson`, ~142M, gitignored):

- **civ-meta.json** — 1v1 overall/tier/winRate/byElo/byMap/byPatch all **current**
  (refresh-civ-current.mjs). `byPatch` is a 16-month dated axis (aggregate-patches.mjs,
  `gamemod_id`→month). **team ladder is still frozen aoestats** (crawl is 1v1 only).
- **map-meta.json** — 1v1 "best civs on this map" rankings **current** for 45 maps
  (refresh-map-current.mjs). **team rankings still frozen aoestats.**
- Pipeline scripts in `scripts/data-pipeline/`. The crawl→key mapping is canonical
  (`canon()`), done once at ingestion — keep it there, don't scatter it (see
  [[feedback_consistent_names]]).
- Verify client-rendered pages with headless chromium: `playwright` is a LOCAL
  devDep (uncommitted). Write a `*.mjs` in the project root and
  `import { chromium } from "playwright"`.

---

## PROMPT FOR THE NEW SESSION

> I'm continuing work on the AOE2 guide (Astro static site, the memory has full
> context). The stats are now current from a live crawl. Three buckets of work
> remain, in priority order. Read `docs/superpowers/NEXT-SESSION.md` first.
>
> **1 — Homepage redesign (highest priority).** The homepage
> (`src/pages/[lang]/index.astro`) doesn't look good — I want a fresh, striking,
> genuinely well-designed home. Use the `frontend-design` skill. Keep the real
> data wired in (live tiers/win rates from civ-meta, the 53·43·13 counts), keep
> the medieval parchment/maroon/gold theme + Cinzel/JetBrains-Mono, keep it
> static + bilingual (EN/TR via `t()`), but rethink the layout/hierarchy/visual
> impact from scratch — it should feel designed, not assembled. Current sections:
> hero, "53 Civilizations" tier strip, featured TEMPO build cards, nav tiles,
> featured maps, more-to-explore. Show me a design direction before building.
>
> **2 — Content gaps.**
> - Add the **current ranked maps** the crawl has but map-meta/site lacks
>   (em_runestones, vulpine, border_dispute, karsts, fourlakes-variants, …). They
>   need map-meta entries (extend aggregate-maps or refresh-map-current to emit
>   crawl-only maps) and either data-only pages (already supported via
>   `[map].astro` route extension) or curated content.
> - More **curated map content** — only 13 of ~90 maps have recommendedCivs /
>   teamComps / body; the rest are data-only. Don't fabricate (source-derived
>   only, [[feedback_source_derived_only]]).
> - **Matchups team** is overall-only + confounded; consider per-map/per-elo if
>   worth it.
>
> **3 — Polish / infra.**
> - **Verify the Cloudflare Pages deploy** — is aoe2guide.com auto-deploying on
>   push to main? Confirm the live site reflects the recent commits.
> - **Team data is still frozen aoestats** (civ team + map team). To make it
>   current, run a TEAM crawl: collect-relic with the team RM leaderboard id
>   (current is `leaderboard_id=3` = 1v1 RM; find the team RM id), capturing
>   gamemod_id, then a refresh for team like the 1v1 ones.
> - **High-elo buckets (2500+)** are thin in the crawl — some civs lose the top
>   byElo bucket. Decide: accept, or backfill the 2500+ bucket from aoestats.
> - **No-JS fallback** — the civs tier list + map rankings are client-rendered
>   (empty without JS). Add SSR fallback or accept (it's a data tool).
> - **Name consistency** — minimize the canon/mapping; ideally one canonical map
>   key everywhere ([[feedback_consistent_names]]).
> - **Automate the crawl/refresh** (currently manual on the VM).
> - Decide whether to commit `playwright` as a devDep (postinstall downloads
>   browsers — may break CI).
>
> Plan it, then execute with reviews. Keep it light where you can — it's a hobby
> project ([[feedback_hobby_keep_it_light]]).

---

## Gotchas the new session will hit

- **VM SSH** uses the agent at `/run/user/1000/ssh-agent.socket`; if `ssh-add -l`
  is empty, ask the user to `ssh-add ~/.ssh/id_ed25519` (passphrase-locked) — the
  GitHub push needs it too.
- **`pnpm build`** runs the translation audit (any `en===tr` fails) + schema
  validation as a prebuild gate.
- Civ YAML is generated (`pnpm build:civilizations`) — don't hand-edit.
- `data-cache/` + `*.zip` are gitignored.
- Heavy data files are build-time imports (not shipped wholesale); interactive
  pages (matchups ~400KB, map rankings) inline/embed their data and render rows
  client-side to stay light.
