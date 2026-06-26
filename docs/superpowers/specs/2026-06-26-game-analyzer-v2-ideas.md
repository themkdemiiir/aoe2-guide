# Game Analyzer — v2 ideas (from public-analyzer research, 2026-06-26)

> Researched CaptureAge, aoe2insights, the mgz/aoestats open-source ecosystem (dj0wns, AgeAlyser), and coaching tools.
> Filtered for feasibility against our command-stream-only data. Not yet implemented — a backlog.

# Replay Macro-Coach — v2 Ideas (command-stream-only)

> Scope reminder: our Rust CLI parses **only the `.aoe2record` command stream** — age-up clicks (Research 101/102/103), `DeQueue` (train), `Research`, `Build`+coords, `Move`/`Interact`+coords, and `PostGame` ELO. We have **no** resource banks, no alive-unit/pop snapshots, no kills, no per-unit gather state. That's the feasibility filter for everything below. Because selections/camera/jump-to are never logged in the rec, our raw command stream **is already the "effective" layer** ([AoEZone geAPM](https://aoezone.net/threads/what-kind-of-apm-does-capture-age-show.178138/), [AoE2 Insights eAPM rules](https://forums.ageofempires.com/t/eapm-effective-actions-per-minute-elo-list/109200)).

## 1. What the field does that we don't (gap list)

- **Real eAPM/geAPM** as the headline macro number, divided by game-minutes, with avg/current/peak binning — CaptureAge ([wiki](https://ageofempires.fandom.com/wiki/CaptureAge_(tool)), [docs](https://captureage.com/cade/docs/cade-latest/match-statistics)). We currently don't surface this; it's almost free for us.
- **Opening / build-order classification** into named labels (Drush, Scouts, Archers, FC, Drush-FC, Tower Rush) — dj0wns `guess_strategy` bitmask, AgeAlyser `extract_opening_strategy`, AoE2Meta signature+confidence ([dj0wns](https://github.com/dj0wns/AoE_Rec_Opening_Analysis/blob/main/aoe_opening_data/aoe_replay_stats.py), [AgeAlyser](https://github.com/byrnesy924/AgeAlyser_2/blob/main/src/agealyser/main.py), [AoE2Meta](https://aoe2meta.com/analyze)).
- **Idle-TC as a localized busy-timeline** with the *when*, not a scalar — santolucito/AoE2IdleTime 25s-block reconstruction ([gist](https://gist.github.com/santolucito/a01927be45a2a7a8e02ce9a50ddd8e75), [repo](https://github.com/BoZenKhaa/AoE2IdleTime)).
- **Villager-count benchmark** from age-up click time (ideal never-idle count) — AgeAlyser `extract_feudal_uptime_info`.
- **Age-up "up-times"** showing both click time and landing time vs benchmark — CaptureAge Technology tab; AgeAlyser TC model.
- **Population-relative benchmarking** ("your Castle is +2:10 vs the median at your ELO") — aoestats model ([aoestats.io](https://aoestats.io/)).
- **Striped-slice idle diagnostic** (idle drawn as a visible fraction) — CaptureAge pie charts ([docs](https://captureage.com/cade/docs/cade-latest/pie-chart)). We can borrow the *presentation* even though we can't measure true worker efficiency.
- **State metrics we structurally cannot match:** resources-over-time, alive-unit/Military Value, kills, market totals, true worker-efficiency/IDL-ECO — all require full simulation.

## 2. Borrowable ideas — prioritized (highest payoff-to-effort first)

| # | Idea | Concrete method / threshold | Feasible (our data) | Effort | Payoff |
|---|------|------------------------------|----------|--------|--------|
| 1 | **Real eAPM** (headline) | `eAPM = effective_cmd_count / game_minutes`. Effective set = Move, Interact/Attack, Build, Research, DeQueue, age-click, formation, town-bell, stance, rally. Selects/camera aren't in the rec so the stream is effective by construction. Anchors: ~25=1200, ~45=1500, ~60+=pro ([insights](https://www.aoe2insights.com/stats/mastering-eapms-strategic-depth-in-age-of-empires/), [forum ELO list](https://forums.ageofempires.com/t/eapm-effective-actions-per-minute-elo-list/109200)). | yes | **S** | **High** |
| 2 | **Age-up report: click + landing + delta** | Click = timestamp of Research 101/102/103. Landing = click + research duration (Feudal 130s/Castle 160s/Imp 190s, civ-modified). Emit delta vs reference + letter grade. (AgeAlyser landing model.) | yes | **S** | **High** |
| 3 | **Build-order timeline** | Merge DeQueue + Build(+coords) + Research + age-clicks, sort by timestamp, annotate (`7:42 Barracks@(112,88)`). Straight sort+label — dj0wns/AgeAlyser event list. | yes | **S** | **High** |
| 4 | **Opening classifier (bitmask + confidence)** | One linear pass tracking current_age; OR-in flags on first military DeQueue per age; Castle-unit techs set Castle* flags; meta-labels = flag combos. FC = no military Build before Castle click; Drush = Barracks+militia in Dark; PreMillDrush = barracks_before_mill. Confidence = how cleanly one signature dominates ([dj0wns](https://github.com/dj0wns/AoE_Rec_Opening_Analysis), [AoE2Meta](https://aoe2meta.com/analyze)). | yes | **M** | **High** |
| 5 | **Phase-segmented eAPM** | Split stream at 101/102/103; report Dark/Feudal/Castle eAPM separately (BO execution vs micro). | yes | **S** | Med-High |
| 6 | **Fast-Castle flag** | Castle click `< ~15:20` (920000 ms) → FastCastle; tune threshold per ELO band (dj0wns constant). | yes | **S** | Med |
| 7 | **Idle-TC via gap method** (better than scalar) | Place each 25s production block *ending* at its DeQueue timestamp; union with Loom(25s)/Town Watch(25s)/Wheelbarrow(75s)/age-up(130s) busy intervals; `idle = window − union(busy)` up to first Castle click. Report **intervals with timestamps** ("idle 0:18 @ 7:42"), not just a total. Localizes idle, auto-handles overlap + production-bonus civs. Label ESTIMATE. ([santolucito](https://gist.github.com/santolucito/a01927be45a2a7a8e02ce9a50ddd8e75)). | partial | **M** | **High** |
| 8 | **Ideal villager-count benchmark** | `ideal_vils = floor(feudal_click_uptime / 25) + starting`, subtract Loom if pre-Feudal; civ bonuses (Chinese +. etc). It's the max you *should* have had — a coaching target, not a measurement (AgeAlyser). | partial | **S** | Med-High |
| 9 | **ELO-relative benchmarking** | Bucket Feudal/Castle click, ideal vils, first-mil-building time by PostGame ELO band; compare to medians from our **own DuckDB crawl** (single-game analog of aoestats). | yes (needs elo-bucketed corpus) | **M** | **High** (once corpus exists) |
| 10 | **Eco vs Mil eAPM split** | Classify Research by tech table, Build by building-id, DeQueue by producing building. Route coord-only Move/Interact to "unclassified" or by proximity to mil-building clusters. Report eco%/mil% of *classified* cmds. | partial | **M** | Med |
| 11 | **Coaching flags (rule-based)** | Thresholds over the timeline: est. idle-TC > 30s pre-Feudal → warn; Feudal click > bench+Xs → slow Feudal; Loom/Wheelbarrow missing/late; zero-DeQueue streak at a TC; no military Build before Castle on aggressive timing. Each flag cites timestamp + benchmark. The human-coach layer CaptureAge omits. | yes | **M** | **High** |
| 12 | **Forward/tower-rush detection from Build coords** | Cluster Build coords vs own/enemy start (inferred from earliest commands); mil building/tower within N tiles of enemy in Feudal → forward/tower rush; palisade/stone Build → walling. Differentiator — public auto-classifiers use unit signatures, not coords. | partial | **M** | Med |
| 13 | **Light spam de-dup for eAPM** | Collapse consecutive identical cmds same-type+target within ~0.05–0.1s (CaptureAge multiqueue proposal); dedupe Move to ~same coord <100ms apart. Only delta between raw count and polished community eAPM ([nolt/164](https://captureage.nolt.io/164)). | yes | **S** | Low-Med |
| 14 | **Queue-simulated "actual" vil/army counts** | Replay DeQueue(amount) into per-building 25s queues to estimate spawn times/running counts; first-army timing. "Produced/queued," never "alive." Confounded by deep-queuing and cancels — estimate-grade (AgeAlyser TC factory). | partial | **L** | Med |

## 3. Output / UX upgrades worth doing

- **JSON export** mirroring the per-player struct: `eapm`, `eapm_by_phase`, `ages[{age,clickTime,reachedTime,deltaVsBench}]`, `buildOrder[]`, `opening{label,bitmask,confidence}`, `idleTc{seconds,intervals[]}`, `flags[]`, `villagers{ideal,estimated}`, `elo`. Keep an `"estimated": true` boolean on every approximated field so the site/DuckDB pipeline ingests it honestly.
- **Per-player report card** echoing CaptureAge columns we *can* fill: opening label, age click+landing+delta, eAPM (avg/peak, + phase split), eco%/mil%, est. idle-TC, ideal-vs-est vils — each with a **letter grade (A–F)** vs ELO-band median.
- **Build-order timeline ledger** (monospaced): `M:SS vil#12 | M:SS Barracks@(112,88) | M:SS Feudal click | M:SS Loom` — Hera-style printable step list.
- **Per-age breakdown** (Dark/Feudal/Castle columns): uptime, idle-TC, eAPM, opening+confidence per phase.
- **Idle-TC strip** (ASCII/Unicode): busy 25s blocks vs hatched idle gaps with timestamps — borrow CaptureAge's "striped slice = non-productive" idea, **labeled ESTIMATE**.
- **eAPM sparkline** per minute (`▁▂▅▇█`) + "peak minute @ M:SS" — matches CaptureAge avg/current/peak.
- **Side-by-side P1 vs P2 diff** for 1v1: age-up race, eAPM gap, who idled more — mirrors the dual scoreboard.
- **MEASURED / ESTIMATED / UNAVAILABLE tag on every metric** — non-negotiable for the project's source-derived ethos; never imply we have resource/alive data we don't.

## 4. Skip / not feasible (and why)

- **Resources-over-time, alive-unit counts, Military Value** — pure state metrics; need resource banks + alive tracking we don't have. No reconstruction possible.
- **Kills / deaths / conversions / Military tab** — require combat events; absent from the command stream.
- **Market totals (bought/sold per resource), tribute balance, relic gold** — need transaction state; we see at most the click, not running totals.
- **True worker efficiency / IDL ECO / WE-LM (striped pie measurement)** — needs per-unit gather state (idle/moving/gathering). Borrow the *visual* (idle-fraction) only; never claim the number.
- **OCR/screenshot idle detection** (idletowncenter.com) — pixel-based, irrelevant to a rec parser.
- **Exact villager spawn moments / exact idle-TC** — DeQueue is a *queue click* with an amount, not a spawn event; deep-queue + cancels + 2-TC + megarandom break it. Ship only as ESTIMATE via the gap method (#7), never as measured.
- **Raw (keylogger) APM ~250** — only geAPM/eAPM (~130 pro) is derivable from the rec; raw input APM is not in the stream.

---
*Sources inline above. The "test" input topic was junk and ignored.*
