{{ config(materialized='view') }}

-- Winners' first-military-training + eco-tech-research CLICK-time medians, per elo bucket and per
-- elo bucket x map (GROUPING SETS all-maps rollup + per-map cells) -- feeds winner-refs.json's
-- `medsByBucket`/`medsByMap` (pipeline/crates/export/src/winner_refs.rs), the analyzer's
-- coaching-panel reference row for "first military" + the four watched eco upgrades.
--
-- **Replaces `scripts/data-pipeline/build-winner-meds.sql`'s DuckDB pass over the replay event
-- NDJSON shards ("Plan C") -- read that script's doc first if this view's shape looks surprising.
-- Same corpus (REPLAY-SOURCE ONLY -- `replay_events` is populated exclusively by the replay
-- parser; see that table's own migration doc, which explicitly names THIS exact winners-by-elo
-- aggregation as its intended "winner-meds hot path" and adds a partial index for it), same
-- event-kind filter, same eco-unit/tech id lists, same GROUPING SETS shape, same >=50-winners
-- floor. Only the storage engine changed (DuckDB NDJSON shards -> Postgres `replay_events`/
-- `match_players`/`matches`/`maps`).**
--
-- `first_military_ms` = the min `t_ms` of a `train` event whose `target_id` is NOT one of the five
-- eco/utility unit ids (villager 83, fishing ship 13, trade cart 128, trade cog 354, 17 -- the SAME
-- ids `crates/replay/src/config.rs::ECO_UNIT_IDS`/`winner_comps.sql` classify as non-combat).
-- `loom_ms`/`dba_ms`/`wheelbarrow_ms`/`horse_collar_ms` = the min `t_ms` of a `research` event for
-- tech ids 22/202/213/14 (`replay::config::WATCHED_TECHS`, the same four `winner_refs_ecotech.sql`
-- tracks -- Gold Mining is NOT part of this view's shape, matching the old script's
-- `medsByBucket`/`medsByMap`, which never carried a `goldMiningMs` field).
--
-- WINNERS of ranked 1v1 RM only (`mp.won = true`, `m.ladder = '1v1'`). The `replay_events` INNER
-- JOIN alone restricts this view's whole corpus to replay-sourced matches -- a player who never
-- triggered a qualifying train/research event simply has no `per_player` row (same "the join
-- itself restricts the corpus, no separate `source` filter needed" posture `winner_comps.sql`
-- established for `match_player_units`; contrast `winner_refs_ecotech.sql`, whose denominator is a
-- SEPARATE CTE and therefore DOES need an explicit `source = 'replay'` filter -- see that view's
-- doc).
--
-- GROUPING SETS emits both the per-map cell `(map_slug, elo_bucket)` and the all-maps rollup
-- `(elo_bucket)` -> `map_slug` coalesced to `'all'` -- mirrors `benchmark_ageup.sql`'s own
-- `'all'`-keyed rollup convention. `winners_n` is the distinct-player count behind the cell
-- (`count(*)` over `per_player`, one row per player); each of the five `*_ms` columns is its OWN
-- `percentile_cont` and independently NULL if none of that cell's winners triggered the
-- corresponding event. Values are left as raw milliseconds (`double precision`, NOT rounded/cast
-- to `bigint` here) -- rounding happens in Rust (`winner_refs.rs`), the SAME "SQL emits raw
-- percentiles, Rust rounds for display" split `eco_benchmark.rs`'s `round1` already established
-- for `benchmark_ecotech`, rather than introducing a new SQL-side rounding idiom.

with winners as (

    select mp.match_id, mp.profile_id, mp.elo_bucket, mo.slug as map_slug
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join maps mo on mo.map_id = m.map_id
    where m.ladder = '1v1'
      and mp.won = true
      and mp.elo_bucket is not null

),

qualifying_events as (

    select e.match_id, e.profile_id, e.t_ms, e.kind, e.target_id
    from replay_events e
    where (e.kind = 'train' and e.target_id not in (83, 13, 128, 354, 17))
       or (e.kind = 'research' and e.target_id in (22, 202, 213, 14))

),

per_player as (

    select
        w.map_slug,
        w.elo_bucket,
        w.match_id,
        w.profile_id,
        min(case when qe.kind = 'train' then qe.t_ms end) as first_military_ms,
        min(case when qe.kind = 'research' and qe.target_id = 22  then qe.t_ms end) as loom_ms,
        min(case when qe.kind = 'research' and qe.target_id = 202 then qe.t_ms end) as dba_ms,
        min(case when qe.kind = 'research' and qe.target_id = 213 then qe.t_ms end) as wheelbarrow_ms,
        min(case when qe.kind = 'research' and qe.target_id = 14  then qe.t_ms end) as horse_collar_ms
    from winners w
    inner join qualifying_events qe
        on qe.match_id = w.match_id and qe.profile_id = w.profile_id
    group by 1, 2, 3, 4

)

select
    coalesce(map_slug, 'all') as map_slug,
    elo_bucket,
    count(*) as winners_n,
    -- `percentile_cont` over the integer `t_ms` already returns `double precision` (same rule
    -- `winner_comps.sql`'s own doc documents for `match_player_units.trained`) -- no cast needed.
    percentile_cont(0.5) within group (order by first_military_ms) as first_military_ms,
    percentile_cont(0.5) within group (order by loom_ms)           as loom_ms,
    percentile_cont(0.5) within group (order by dba_ms)            as dba_ms,
    percentile_cont(0.5) within group (order by wheelbarrow_ms)    as wheelbarrow_ms,
    percentile_cont(0.5) within group (order by horse_collar_ms)   as horse_collar_ms
from per_player
group by grouping sets ((map_slug, elo_bucket), (elo_bucket))
having count(*) >= 50
order by 1, 2
