# Game Analyzer — replay-based post-game coaching

**Status:** design approved 2026-06-26 · research + validation folded in 2026-06-26 ·
BUILT + hardened 2026-06-26 · map-specific elo benchmark live ·
**Surface:** Rust CLI (v1) · reusable engine for a future backend/site feature.

## WASM direction + repo organization (planned)

**WASM-readiness (do later).** The analyze logic is already pure and isolated under
`src/analyze/` — `metrics`, `position`, `float`, `compare`, `model`, `data`, `maps`,
and `walk` (which parses via `aoe2rec`, a crate that already ships a WASM build,
`aoe2rec-js`). The only IO is in `analyze::mod::run`/`load_game` (file read +
`--match-id` download). To ship the coach **in the browser on the static site**:
1. Extract `analyze` into a `[lib]` exposing a pure `analyze(&Savegame, you) -> Report`
   (a struct: meta + per-player metrics + findings) + a renderer; keep file/network IO
   in the `[[bin]]`. The committed `data/*.{tsv,json}` are `include_str!`-baked, so they
   travel into the WASM bundle with no runtime fetch.
2. Add a `wasm-bindgen` wrapper crate that takes replay bytes → `aoe2rec` parse →
   `analyze()` → **JSON `Report`** (derive `Serialize` on the output types). This also
   gives the CLI a `--json` flag for free.
3. The site loads the `.wasm` + calls it on a drag-dropped `.aoe2record` — no backend,
   preserving the static-only deploy.

**Repo cleanup (low priority, careful — these are live WIP).** Superseded but still
cross-referenced, so archive rather than delete: `build-duckdb.sql` (superseded by
`build-match-ages.sql` + `load-replays.sql`; only referenced in `docs/data-workbench.md`),
and `collect-relic.mjs` (the streaming `stream-relic.mjs`/`ingest-stream.mjs` replaced it,
but `refresh-team-current.mjs`/`aggregate-civmeta.mjs` still import its helpers — untangle
those imports first). `build-civ-cube.mjs` has no inbound references — verify it's unused
before removing. Leave the rest of the pipeline (it generates live site data).

## Purpose

Take a single ranked game (1v1 or team) and tell each player, in plain terms,
**what they did badly, where they fell behind, and what to improve** — a macro/
build-order coach. Built on the existing `replay-rs` parser and the `match_ages`
benchmark (4.33M games).

**Success criteria:** given a replay file or a `match_id`, print a per-player
scorecard + a prioritized "top fixes" list, where every flag is justified by one
of four comparisons. Runs in seconds.

## Decisions log (why this shape)

These came from the design session; they keep the tool **light** (a hobby project):

1. **Core product = a per-player MACRO coach that works on any map** (ages, vils,
   idle-TC, military, eco-techs, float). Position/role coaching is a *secondary,
   light tag*, not the headline.
2. **Flank/pocket role is detected by the COLOR/SLOT rule, not geometry.** Within a
   team, the min & max slot = flank, inner slots = pocket. Color also labels players
   to match the in-game minimap. (We validated that geometry is *more* accurate —
   color mislabeled a real 4v4 — but role is a low-stakes light tag, so the simple
   rule is acceptable. See "Position / role".)
3. **Role coaching is scoped to Arabia + Arena (+ maybe Black Forest) only**, and
   **mainly matters in 4v4** (lighter in 3v3, label-only in 2v2). Every other map
   gets the plain macro coach with no role tag.
4. **The base-position/coordinate code is repurposed for two NEW metrics** the user
   asked for — **enemy proximity** ("was an enemy TC close?") and
   **time-to-find-enemy** ("when did your scout reach the enemy?") — not for the role.
5. **Lean dependencies.** `owo-colors` + `comfy-table` for the report; keep the
   crate's hand-rolled CLI parser; `costs.json` from aoe2techtree (no `genie-rs`).

## Scope / non-goals

- **In:** age-up timing, villager count + production continuity (idle TC), military
  count/timing, building count/timing, key eco-tech timing, APM, an **estimated**
  resource-float signal, plus the two coordinate metrics (enemy proximity, scout
  find-time) and a light flank/pocket tag on Arabia/Arena/BF team games.
- **Out (not reliably in the command stream):** exact resource bank, fights/kills,
  map control, villager resource-assignment, micro. This is a macro coach. Stated
  plainly in the report so users don't over-trust it.

## Architecture

A new **`analyze` subcommand on the existing `replay-rs` crate** (one binary, no new
language). Reuses the parse path; adds an analysis module + a report printer:

```text
replay-rs analyze <file.aoe2record | --match-id N> [--you NAME]
   ├─ parse        (existing: Savegame::from_bytes / from_file)
   ├─ metrics      (new: ops -> per-player MetricSet, incl. coords)
   ├─ position     (new: color/slot role tag, map+size scoped)
   ├─ benchmark    (new: load benchmark.json; pick civ×elo slice + 2500+ slice)
   ├─ compare      (new: 4-way -> Vec<Finding>)
   └─ report       (new: render scorecard + top fixes with owo-colors + comfy-table)
```

`--match-id N` reuses `api.rs` (getReplayFiles → download) before parsing.
`--you NAME` overrides the auto-detected "you" (defaults to `replay.rec_player`).

## Parser facts we rely on (validated 2026-06-26 against real replays)

Empirically confirmed by probing the user's local saves + one downloaded replay
(1v1 / 2v2 / 3v3 / 4v4, current patch build 178524 / version_major 68):

- **`world_time` is milliseconds** (a 40:26 game → `last_t = 2_426_423`).
- **Age-up = `Research` with `technology_type` 101/102/103** = feudal/castle/imp
  (the *click* time; add research duration for completion — see benchmarks).
- **Villager = `DeQueue` `unit_id == 83`**; counts are **cumulative queued** by the
  age-up boundary, not a live snapshot (same basis as the forward `replay_ages`).
- **`Build` action data blob layout: `x: f32 @ [4..8]`, `y: f32 @ [8..12]`,
  `building_id: i32 @ [12..16]`** (confirmed: house id 70, etc.). Build positions are
  the **cleanest base-location signal** (buildings sit at home).
- **`Move`(3) and `Interact`(0) actions carry `x: f32, y: f32`** directly; `Viewlock`
  ops carry coords too but are **recorder-only** (non-POV players have zero) and
  camera-noisy — do **not** use Viewlock for base detection; prefer Build, then
  Move/Interact medians.
- **Per-player ELO is in the replay** via `Operation::PostGame` →
  `PostGameBlock::Leaderboards` → `LeaderboardPlayer { player_number, rank, elo }`.
  - Leaderboard `player_number` is **0-indexed**: game `player_number = lb + 1`.
  - **Leaderboard `id` = ladder:** `3` = 1v1 RM, `4` = team RM. Both can appear; pick
    by mode. A player unranked on a ladder is simply absent → show `-`.
  - This means the "vs your-elo" comparison can use the player's **real ELO** with no
    API call (when the PostGame block exists — i.e. a full/finished replay).
- **`replay.rec_player`** identifies the POV ("you") player for local saves.
- **`GameSettings`** exposes `resolved_map_id`, `map_size`, `team_positions`,
  `random_positions`, and per-player `resolved_team_id` + `color_id`. Team ids seen:
  `2` and `3` for the two teams; **`1` = unaffiliated/FFA**.
- **`zheader.initial.players` is parsed with `count = 1`** (aoe2rec only reads one
  PlayerInit) → header `spawn_location`/`camera` is **not** available per-player. All
  coordinate work uses the command stream, which is also parser-version-robust.
- **FFA gate:** in one 8-player game *all* players were `team_id == 1` → no enemy
  geometry. Any coordinate/role logic must require **≥2 distinct teams with members**
  and otherwise fall back to non-positional coaching.

## Metrics (per player, from the command stream)

Macro (any map):
- **Age-ups:** feudal/castle/imperial click time (Research 101/102/103). Report both
  click and completion (= click + {Feudal 130, Castle 160, Imperial 190}s).
- **Villagers:** cumulative count over time (DeQueue 83); **idle-TC gaps** = stretches
  with no villager queued beyond an age-scaled threshold (lost production).
- **Military:** first-military time; counts by age (DeQueue of non-eco units).
- **Buildings:** counts + timing (Build).
- **Key eco techs:** Loom, Wheelbarrow, Hand Cart, Double-Bit Axe→Bow Saw, Horse
  Collar→Heavy Plow, Gold/Stone Mining — present? when? (Research ids).
- **APM/eapm:** command rate (effort proxy).
- **Estimated float** (see below).

Coordinate-derived (the repurposed geometry; meaningful mainly on open maps):
- **Base centroid** per player = median (x,y) of that player's Build coords in the
  first ~6 min (fallback: median of early Move/Interact).
- **Enemy proximity / rush distance** = distance from a player's base to the nearest
  enemy base → "your flank faced a close enemy (~D tiles) — walling was urgent" vs
  "enemies were far — a greedier opening was safe."
- **Time-to-find-enemy (scouting)** = first time any of the player's unit coords come
  within radius R of an enemy base centroid → "you didn't scout the enemy until
  M:SS — scout earlier." Honest caveat: approximate (camera ≠ unit vision).

## Position / role (light tag — color/slot rule)

- **Gate:** team game with **≥2 distinct teams**, on a map whose **`family` is
  `open` or `closed`** in the committed `data/maps.tsv` (id→name→family→notes table;
  families classified from the AoE2 Fandom wiki by real flank/pocket structure, not
  just terrain) — e.g. open = Arabia, Gold Rush, Mongolia, Golden Pit…; closed =
  Arena, Black Forest, Fortress, Hideout…; user-editable. Primarily surfaced in
  **4v4**. (`hybrid`/`water`/`nomad`/`special`/`other` maps — incl. Nomad(33) and
  **African Clearing(149, scattered)** — get no role tag; just the macro coach.)
- **Role from slot:** within each team, sort the slots present; **min & max slot =
  FLANK, inner slots = POCKET**. → 2v2 = 1 flank + 1 pocket (functional; label-only),
  3v3 = 2 flanks + 1 pocket, 4v4 = 2 flanks + 2 pockets. (Documented ranked "1357 vs
  2468" convention; `color_id` tracks slot.)
- **Labels:** refer to players by their **color** (Blue/Cyan/…) to match the minimap,
  then attach the derived role ("Cyan — pocket"). Never map an absolute color value to
  a role (color 5 is pocket in 4v4 but flank in 3v3).
- **Coaching is map-conditioned** (see appendix): Arabia = open (archer-flank vs
  booming scout-pocket); Arena = closed (FC / monk+siege / relics / boom / unique-unit
  Castle drops). Coach each player *against their seat + map*, not a generic standard.
- Known imperfection (accepted): the slot rule occasionally disagrees with the
  physical spawn (validated on a real 4v4). As a light tag this is fine; the
  coordinate metrics above are the rigorous signal if we ever want a confidence flag.

## The four comparisons → Findings

1. **vs your-elo benchmark** — your `elo_bucket` slice of `benchmark.json` (bucket
   taken from the replay's PostGame ELO when present; else from `--match-id`/games).
2. **vs pro** — the `2500+` slice (best players' medians; no hand-curated BOs).
3. **vs opponent(s)** — the other parsed players in the same replay (head-to-head;
   for team games, the opposite seat where a role tag exists).
4. **Absolute waste flags** — idle TC, stuck-in-age, no/late military, missing eco
   tech, float, housed (bad regardless of elo). Thresholds in the appendix.

Each `Finding` carries: metric, your value, the reference value, the basis (one of
the four), severity, and a one-line coaching note. "Top fixes" sorts by severity
(gap size vs reference, weighted toward early-game fundamentals — idle TC and age
timing compound hardest). Findings are **conditioned on build/strategy and age**
(e.g. "no Feudal military" is correct for a Fast Castle).

## Floating-resources estimator (best-effort)

No resource data exists in the stream, so model it:

- **Income rate(t)** ≈ `villagers(t) × ~0.30 res/s` (effective blended rate, incl.
  walk/deposit/idle discount; `0.40` = optimistic on-resource upper bound). Per-task
  rates in the appendix.
- **Spend(t)** = cumulative cost (from `costs.json`) of everything trained/built/
  researched up to `t`.
- **Float signal** = windows where cumulative income outran cumulative spend, reported
  as a **rate gap** ("17:00–20:00: economy out-produced spending by ~250 res/min ≈
  ~750 banked"), not a fake absolute bank value.

**Printed caveat:** ignores villager assignment, walk distance, civ bonuses, market
trades, upgrades, and deaths — flags *big* floats reliably, not exact amounts. Easy
to disable if noisy.

## `benchmark.json`

Generated once from the workbench DuckDB (`~/aoe2-guide/data-cache/aoe2.duckdb` on the
VM) by a small SQL/mjs. Shape (v1): `civ → elo_bucket → mode(1v1|team) → { feudal_s,
castle_s, imperial_s, vils_castle }` as **medians**, plus an
`elo_bucket = "2500+"` "pro" entry, and (where data allows) a `team_size` split.

- **`games` already has** `elo_bucket`, per-player `rating`, `team_size`, `map`,
  `opening`, and age times `feudal_t/castle_t/imperial_t`. **`match_ages`** has
  per-age `villagers/military/n_buildings` but **no elo** → JOIN `match_ages` to
  `games` on `(match_id, profile_id)` for the bucket. Existing elo buckets:
  `<1000, 1000-1199, 1200-1399, 1400-1649, 1650-1799, 1800-1999, 2000-2199,
  2200-2499, 2500+`.
- **Timing-basis caveat (must reconcile):** the analyzer measures age-up **click**
  time + **cumulative** villagers, while historical `match_ages` uses age
  **completion** time. Compare completion-to-completion (analyzer click + research
  duration). **Preferred future:** rebuild `benchmark.json` from our own forward
  `replay_ages` (identical extraction → apples-to-apples) once those shards are loaded
  into DuckDB (`load-replays.sql`); `replay_ages` is **not loaded yet**.
- Loaded via serde at runtime; small committed JSON; no DB/network dep at runtime.

## `costs.json`

Unit/building/tech resource costs (food/wood/gold/stone), extracted **once** from
**aoe2techtree `data/data.json`** (`Unit`/`Building`/`Tech` objects, each keyed by DE
numeric id with a `Cost` block; 479 cost entries = 245 units + 40 buildings + 194
techs). A one-off `serde_json` script
emits `costs.json` **keyed by category** (id namespaces overlap across Unit/Building/
Tech). Missing resources default to 0; these are **base** costs (no civ discounts).
Used only by the float estimator. Refresh with the techtree pin. (Evaluated and
rejected: `genie-dat` — frozen at 2020, no DE `.dat`; `aoc-reference-data` — id→name
only, no costs.)

## Report format

Terminal report. Per player: identity (**color + role tag where it applies**) + age/
vil/military/tech/idle/float scorecard with ✓/⚠/✗ vs references, then a **Top 3
Fixes** block. **Team games:** one scorecard per player + a "weakest link / who fell
behind" line; the POV/"you" player highlighted. Rendered with `comfy-table`
(`default-features = false`) + `owo-colors` (gate color on `std::io::IsTerminal`).

## Dependencies (new)

| Need | Crate | Note |
|---|---|---|
| Color/styling | `owo-colors = "4.3"` | zero required deps; gate on `IsTerminal` |
| Scorecard table | `comfy-table = { "7.2", default-features = false }` | drops crossterm; unicode-width/-segmentation only |
| CLI | *(keep hand-rolled)* | one subcommand, 2 flags; `lexopt` optional if it grows |
| JSON / costs | `serde_json` *(already present)* | also runs the one-off costs extractor |

Skip: clap, tabled, cli-table, yansi/colored, genie-dat, genie-rec, stats crates,
num-format. Keep `aoe2rec` (parses current patch where mgz/genie-rec do not).

## Testing

- Unit tests on the metrics module with hand-built event vectors (known age-up times,
  villager gaps) → assert computed metrics.
- Unit tests on the float estimator with a synthetic villager/spend curve → rate gap.
- Unit test on the position rule (slot sets → flank/pocket per team size) + the FFA
  gate (all `team_id==1` → no role).
- Golden test: `analyze` on one committed sample replay → assert key report numbers,
  cross-checked against the existing parser/probe output.
- Smoke: run on the user's local saves (1v1/2v2/3v3/4v4) + one `--match-id` download.

---

## Domain research (folded in)

Cited research from a fan-out of web finders (Hera/T90/Survivalist coaching, aoe2/
Fandom wiki, Liquipedia, Spirit of the Law, aoestats) + our 43 Hera-verified internal
build orders, adversarially verified. `[C]` = source-cited, `[I]` = interpolated
consensus estimate (ship as estimate, not fact). Full sources at the end.

### Team positions: flank vs pocket (by game size)

Coach each player **against their seat**, not a generic ladder standard.

| Size | Seat | Must-do | Build / units | Age target | Supports allies by |
|---|---|---|---|---|---|
| **2v2** | Flank | win/deny the early eco trade vs enemy flank; wall open side | archers → xbow | Feudal ~8–10 | front-loads eco damage |
| | Pocket | don't over-boom; harass enemy flank; power-unit | scouts → knights | Feudal ~10, Castle ~16–17 | scout raids relieve flank; late knight flood |
| **3v3** | 2 Flanks | hold the front; archers | archers → xbow | Feudal ~8–11 | trade resources to pocket when gold-low |
| | 1 Pocket | boom **and** scout BOTH flanks at once; greed starves eco | scouts → knights; wall both sides | Feudal ~10, Castle ~16–17 | sends scouts/knights to win flank fights |
| **4v4** | 2 Flanks | as 3v3 flank; lean on the nearer pocket | archers → xbow | as 3v3 | as 3v3 |
| | 2 Pockets | split: one FC-heavy-booms (2–3 TC), one plays support scouts→knights | scouts → knights (+ one greedy FC) | one FC-boom ~16–17 | together cover both flanks |

2v2 note: wiki-strict taxonomy says "pocket" needs allies on *both* sides (3v3/4v4
only), but every coaching source treats 2v2 as 1 flank + 1 pocket — surface the
functional split (it drives the unit-comp advice). Slot/color layout: odds (1,3,5,7)
vs evens (2,4,6,8); **outermost slot numbers per team = flanks, inner = pockets.**

### Per-map role scope (we only tag Arabia / Arena / Black Forest)

- **Arabia (open land, `family:"open"`)** — the canonical case above: archer-flank
  fights early; scout→knight pocket booms behind safety and bails out both flanks.
- **Arena (closed)** — almost always **Fast Castle** (Feudal fighting through stone
  walls is impossible). Flank: forward Castle + **monk+siege** pushes + **relics** →
  trash wars. Pocket: boom harder / FC into multi-TC / unique-unit Castle drop.
- **Black Forest (maybe)** — really **lanes & chokes**, not classic harassment: flank
  walls the choke (palisade in Dark, stone in Feudal) and holds; pocket scouts forward
  then booms and feeds wood/stone to the flank. Treat as optional / lighter.
- **Every other map** (water, nomad, migration, etc.): **no role tag** — plain macro
  coach. (Roles do exist there in theory, but out of scope to keep the tool light.)

### Per-ELO benchmarks (1v1, open-map)

Only `[C]` cells are source-anchored; `[I]` are interpolated targets. Times m:ss.
"Vils@Castle" = villagers when Castle Age *begins*.

| ELO | Feudal up | Castle up | Imperial up | Vils@Castle | First mil | APM | eAPM | Dark idle-TC |
|---|---|---|---|---|---|---|---|---|
| ~1000 | 14:00–16:00 [I] | 23:00–27:00 [I] | 35:00+/NA [I] | ~30–40 [I] | ~14–16m [I] | ~40–60 [I] | ~15–20 [I] | ~60–120s+ [I] |
| ~1200 | 12:30–14:00 [I] | 21:00–24:00 [I] | 32:00–36:00 [I] | ~40–48 [I] | ~12–14m [I] | ~50–70 [I] | ~20–25 [C] | ~45–75s [I] |
| ~1400 | 11:30–12:30 [I] | 19:00–21:00 [I] | 28:00–32:00 [I] | ~45–55 [I] | ~11–12m [I] | ~70–90 [I] | 20–30 [C] | ~30–50s [I] |
| ~1700 | 10:45–11:30 [I] | 17:30–19:00 [I] | 26:00–29:00 [I] | ~50–60 [I] | ~10:45–11:30 [I] | ~90–120 [I] | ~35–45 [C/I] | ~15–30s [I] |
| ~2000 | 10:15–10:50 [I] | 16:00–18:00 [I] | 25:00–27:00 [I] | ~55–65 [I] | ~10–11m [I] | ~120–170 [I] | ~45–55 [I] | ~5–20s [I] |
| **2500+** | **10:00–10:30 [C]** | 15:30–17:30 [C/I] | 24:00–26:00 [I] | ~55–70 [I] | ~9:30–10:30 [I] | ~150–300 [C] | ~90–130+ [C] | ~0–10s [C/I] |

Cited anchors: Feudal research 130s, Castle 160s, Imperial 190s, villager train 25s;
pro Feudal ~10:05–10:10 at 21–22 pop; FC clicks Castle at ~25–30 vils. **No public
source publishes ELO-bucketed *medians*** for Castle/Imp up-time, idle-TC, or
first-military — the only defensible path is our own `match_ages`/`replay_ages` corpus
(see open items). **Team-game numbers differ** (flanks faster, pockets boom) — do not
reuse the 1v1 ladder; derive from our team-RM data.

### Macro mistakes & thresholds (condition on build + age)

| Mistake | Threshold | Sev | Note |
|---|---|---|---|
| Idle TC | Dark >~10–15s; Feudal cum >~25s; Castle sustained >~25–50s | High→Med | each 25s idle ≈ 1 lost villager; distinguish deliberate eco-cap from accidental gap |
| Floating wood | >~300–500 outside an age-up window | Med | "~1000 wood in Feudal" = classic error; queue a farm per ~60 spare wood |
| Floating food | >~600–800 unless <~30s from age/tech | Med | slow float toward an age-up is fine |
| Floating gold | >~400–500 if not saving for Castle/unit | Med | heuristic bands, severity-scaling |
| Slow uptime | >~30–60s slower than build benchmark; 1–2min = major | High | benchmarks: Feudal 17p 8:00 / 23p 10:30 / 32p 14:15; Castle 31p 17:20; FC ~16–17 |
| Late/no military | aggro build, 0 mil ~1–2min post-uptime | High | conditioned on strategy — FC has none in Feudal |
| Missing eco tech | flag when gatherers make it pay within ~5min; also flag *premature* | Med | timing table below |
| Housed | pop==cap AND production queued → sum lost seconds | High | ≥4 houses before Feudal; House = +5 pop / 25 wood |
| Under-walling | vils die with stone/wood but no walls (vs drush ~7:30, vs scouts ~9:20) | Med–High | soft, civ/map-dependent |

Eco upgrades (costs Liquipedia-confirmed): Loom (Dark, 50g, by Feudal vs aggro),
Double-Bit Axe (Feudal, highest ROI ~3.5min@12), Bow Saw (Castle, 2nd ROI),
Wheelbarrow (Feudal, best 16–18 farmers), Horse Collar (Feudal, cheap), Heavy Plow
(Castle, standard), Hand Cart (Castle, ~50–80 vil), Gold/Stone Mining (Feudal, ~12–13
miners). Flag both *late* and *premature* (premature slows uptime).

### Income-estimator constants

Effective res/s (with walk/deposit/idle): Sheep 0.33, Berries 0.31, Boar/hunt 0.41,
Wood 0.39, Gold 0.38, Stone 0.36, Shore fish 0.43, **Farm 0.32–0.37 (source-split)**.
**Blended nominal ~0.35; safe effective default ~0.30; optimistic on-resource 0.40.**
(Raw work-rate coefficients are ~40% higher — exclude walk/deposit — do **not** use
them or you double-count.) Upgrades add +15–40%.

### Internal build-order targets (our source of truth — agrees with the web)

From 43 Hera-verified `src/content/build-orders/*.yaml`:

| Class | Feudal click | Castle click |
|---|---|---|
| **Flank** (20 builds) | **18 vils @ 8:50** (16–21, 7:35–9:15) | transition, time unstated |
| **Pocket** (13 builds) | ~23–25 vils @ ~10:30–11:20 | ~24–29 vils @ ~14:00–17:00 |
| **1v1-generic** (10) | 17–21 vils @ 8:25–9:15; FI/drush-FC 12:35–13:00 | generic FI 32 @ 16:30 |

Pocket builds incl. `fc-knights-pocket` (explicit "support flanks"),
`fast-castle-boom`, `25pop-knight-rush`, `23pop-fast-castle-boom-arena`. Flank builds =
the scouts/archers/skirms/drush/MAA/towers set.

### Sources

aoe2.guide [Teamgame Roles](https://aoe2.guide/teamgame-roles/) ·
[Arena](https://aoe2.guide/arena/) · Liquipedia
[Glossary](https://liquipedia.net/ageofempires/Glossary) · Sitaux
[Team Game Guide](https://sitaux.com/team-game-guide/) · Steam
[color/positions in ranked TG](https://steamcommunity.com/app/813780/discussions/0/2139714324767343284/) ·
Fandom [Player / color order](https://ageofempires.fandom.com/wiki/Player) ·
Official [Player Color Picker](https://support.ageofempires.com/hc/en-us/articles/7742181295892-Player-Color-Picker) ·
Steam [Black Forest guide](https://steamcommunity.com/sharedfiles/filedetails/?id=1307648667) ·
AoE Forums [eAPM & ELO list](https://forums.ageofempires.com/t/eapm-effective-actions-per-minute-elo-list/109200) ·
Steam [age-up times 130/160s, 40-vil rule](https://steamcommunity.com/app/221380/discussions/0/144513670979166692/) ·
[aoe2database gather rates](https://www.aoe2database.com/gathering_rates/en) ·
AOE Library [uptimes](https://aoelibrary.com/uptimes) ·
historich.net [eco-upgrade payoff (SotL)](https://ageofempires.historich.net/guide-to-aoe2-economic-upgrades) ·
aoe2techtree [data.json (costs)](https://raw.githubusercontent.com/SiegeEngineers/aoe2techtree/master/data/data.json) ·
internal: `src/content/build-orders/*.yaml`.

### Open items for main-loop verification

1. **ELO-bucketed up-time / idle-TC / first-military medians** — every `[I]` row is an
   estimate; compute the real medians from our `match_ages`/`games` corpus.
2. **Team-game (flank vs pocket) per-ELO deltas** — undocumented publicly; derive from
   our team-RM data; do not reuse the 1v1 ladder.
3. **Benchmark timing basis** — prefer rebuilding from `replay_ages` (click +
   cumulative, same as the analyzer) once shards are loaded; interim use `match_ages`
   completion + the click/research-duration offset.
4. **Map id → name → family table** — RESOLVED: committed `data/maps.tsv` (gameplay
   map ids; names from aoc-reference-data, families classified from the AoE2 Fandom
   wiki by real flank/pocket structure). Arabia 9 verified vs a real replay. Nomad 33
   + **African Clearing 149 (the user's main team map — scattered, NO role tag)** =
   nomad. User-editable; edit the `family` column to coach more/fewer maps.
5. **ELO leaderboard mapping** — confirm `game pn = lb pn + 1` and the `id 3=1v1 /
   4=team` ladder mapping on a couple more replays before trusting it for benchmarks.
6. **Float bands & gather rates** (wood>300–500, farm 0.32–0.37) — calibrate against
   our corpus; currently heuristic/source-split.
