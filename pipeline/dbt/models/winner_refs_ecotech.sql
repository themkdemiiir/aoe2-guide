{{ config(materialized='view') }}

-- Winners' eco-upgrade-researched-BEFORE-CASTLE-AGE rates, by elo bucket only (no civ/map split --
-- matches the old `build-winner-refs.sql`'s `winner-ecotech.csv` shape) -- feeds
-- winner-refs.json's `ecoTechByCastle` (pipeline/crates/export/src/winner_refs.rs), the analyzer's
-- coaching-panel fallback row for eco-tech pacing when a player has no own-elo/map benchmark cell.
--
-- Reuses the SAME `match_player_techs` source `benchmark_ecotech.sql` already established (read
-- that view's doc first for the corpus/basis notes this one inherits verbatim: REPLAY-SOURCE ONLY,
-- `t_ms` is the first-research CLICK time), but answers a different question and pivots the
-- aggregation differently: `benchmark_ecotech` buckets by tech_id x map x elo x mode and reports
-- p25/p50/p75 CLICK time; this view fixes `elo_bucket` as the ONLY grouping dimension and reports,
-- per watched tech, the SHARE of winners who clicked it BEFORE their OWN Castle Age-up completed.
--
-- "Researched before Castle Age" = the tech's first-research CLICK time
-- (`match_player_techs.t_ms`, milliseconds) occurred before that SAME player's Castle Age-up
-- COMPLETED (`match_players.castle_t`, seconds -- a DIFFERENT time basis than `t_ms`, per that
-- column's own migration doc, hence the `* 1000` conversion below). A winner who never reached
-- Castle Age at all (`castle_t IS NULL`) is excluded from the denominator entirely -- mirrors the
-- old DuckDB script's `reached_castle` guard.
--
-- Five watched techs, by id (`replay::config::WATCHED_TECHS`, the SAME ids `winner_refs_meds.sql`/
-- `winner_comps.sql` use elsewhere in this pipeline): Wheelbarrow 213, Loom 22, Double-Bit Axe 202,
-- Horse Collar 14, Gold Mining 55 -- this view is the ONLY one of the three `winner_refs_*` views
-- that reads Gold Mining's id (`winner_refs_meds.sql` tracks only the first four, matching the old
-- `medsByBucket`/`medsByMap` shape, which never carried a `goldMiningMs` field).
--
-- **Deliberate denominator scoping, not a copy of `winner_comps.sql`'s posture:** `winner_comps.sql`
-- computes its `winners`/`totals` CTE from `match_players`/`matches` with NO `source` filter (the
-- FULL corpus, both `aoestats` and `replay` matches), then INNER JOINs a replay-only table
-- (`match_player_units`) to compute its numerator -- since that denominator is a genuinely separate
-- CTE (not derived from the same join), an unrestricted `winners` count there is diluted by every
-- aoestats-archive winner that has no possible corresponding unit row at all. This view instead
-- restricts the WHOLE cell -- both the winners-who-reached-Castle denominator and the
-- before-Castle numerator -- to `m.source = 'replay'`, so `winners_n` honestly reports the (thin)
-- replay-sourced sample size the percentages are actually computed over, and the percentages
-- themselves stay meaningful (comparable in scale to the old full-corpus `ecoTechByCastle`, not
-- collapsed toward zero by an unrelated denominator). See the task report for the observed effect
-- on `winner_comps.sql`'s own thin pass-rate (documented there, not fixed here -- out of scope).
--
-- `HAVING count(*) >= 50` is a NEW floor this view adds that the old (full-corpus) DuckDB query
-- never needed -- the old query's denominator routinely ran in the hundreds of thousands per
-- bucket, so an unthresholded `GROUP BY` was always safe; this view's replay-only denominator can
-- be far thinner, so the floor guards against a noisy rate off a handful of winners (same posture
-- `benchmark_ecotech.sql`/`benchmark_ageup.sql` already apply to their own replay-derived cells).

with winners as (

    select mp.match_id, mp.profile_id, mp.elo_bucket, mp.castle_t
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    where m.source = 'replay'
      and m.ladder = '1v1'
      and mp.won = true
      and mp.castle_t is not null
      and mp.elo_bucket is not null

),

before_castle as (

    select w.match_id, w.profile_id, w.elo_bucket,
        bool_or(t.tech_id = 213) as wheelbarrow,
        bool_or(t.tech_id = 22)  as loom,
        bool_or(t.tech_id = 202) as dba,
        bool_or(t.tech_id = 14)  as horse_collar,
        bool_or(t.tech_id = 55)  as gold_mining
    from winners w
    inner join match_player_techs t
        on t.match_id = w.match_id
       and t.profile_id = w.profile_id
       and t.tech_id in (213, 22, 202, 14, 55)
       and t.t_ms < w.castle_t * 1000
    group by 1, 2, 3

)

select
    w.elo_bucket,
    count(*) as winners_n,
    -- `count(*) FILTER (WHERE <nullable bool>)` treats a NULL (a winner with no `before_castle`
    -- row at all -- none of the five techs, before Castle) the same as `false`: excluded from the
    -- numerator, still counted in the `count(*)` denominator. Same `round(numeric, int)::double
    -- precision` cast convention as `winner_comps.sql`'s `producer_pct`.
    round(100.0 * count(*) filter (where bc.wheelbarrow)  / count(*), 1)::double precision as wheelbarrow_pct,
    round(100.0 * count(*) filter (where bc.loom)         / count(*), 1)::double precision as loom_pct,
    round(100.0 * count(*) filter (where bc.dba)           / count(*), 1)::double precision as dba_pct,
    round(100.0 * count(*) filter (where bc.horse_collar)  / count(*), 1)::double precision as horse_collar_pct,
    round(100.0 * count(*) filter (where bc.gold_mining)   / count(*), 1)::double precision as gold_mining_pct
from winners w
left join before_castle bc using (match_id, profile_id, elo_bucket)
group by 1
having count(*) >= 50
order by 1
