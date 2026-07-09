{{ config(materialized='view') }}

-- Analyzer ECO-UPGRADE benchmark: WINNER-focused p25/p50/p75 of each watched eco upgrade's
-- first-research CLICK time, by tech_id x map_slug x elo_bucket x mode — feeds `benchmark-eco.json`
-- (pipeline/crates/export/src/eco_benchmark.rs), which the WASM analyzer overlays onto a player's
-- own eco-upgrade timings to show "your Loom vs winners at your elo, on this map (p25–p75 band)".
--
-- WHY MAP-SPECIFIC: eco-upgrade pace is map-driven — a Fast-Castle boom on Arena researches
-- Wheelbarrow/Horse Collar far earlier than a fighting Arabia game. Comparing a player only to a
-- map-agnostic median would be misleading, so this mirrors `benchmark_ageup`'s per-map medians
-- (which already encode FC-on-Arena etc.). The comparison is meaningful only against winners at the
-- player's OWN elo AND map.
--
-- WINNERS only (`mp.won = true`) — "how fast do the players who WON get these," mirroring
-- `benchmark_vils`' winners-only villagers-by-Castle. Percentiles (not a bare median) so the
-- analyzer can draw the p25–p75 band.
--
-- `match_player_techs.t_ms` is the first-research CLICK time (min t_ms per watched tech, recorded by
-- the replay parser for the SAME 10 tech ids the analyzer's `metrics::WATCHED_TECHS` watches). This
-- table is REPLAY-sourced only (aoestats aggregate rows carry no per-tech clicks), so the corpus is
-- the ~194k-and-growing replay set, not the full 30M — thin (map, elo) cells fall out via `HAVING`.
-- No sentinel/negative guard: t_ms is a real click offset (min observed 416ms, never negative).
--
-- `elo_bucket IS NOT NULL` up front (same reason as `benchmark_ageup`): GROUPING SETS would treat a
-- real NULL bucket as its own group and the outer `coalesce(elo_bucket, 'all')` would silently merge
-- that "actually unknown" group into the deliberate elo-rollup row. `HAVING count(*) >= 50` stays IN
-- this model, exactly like `benchmark_ageup`/`benchmark_vils`.
--
-- Four GROUPING SETS give the analyzer's resolution chain — exact -> same-map/all-elo ->
-- all-map/same-elo -> all-map/all-elo, ALWAYS keeping the known mode: the exact
-- `(tech, map, bucket, mode)` cell, a same-map all-elo rollup `(tech, map, mode)` -> bucket `'all'`,
-- an all-map same-elo rollup `(tech, bucket, mode)` -> map `'all'`, and an all-map all-elo rollup
-- `(tech, mode)` -> map `'all'` + bucket `'all'`. Mode is never rolled up (the analyzer always knows
-- 1v1 vs team); `map_slug`/`elo_bucket` real values are never literally `'all'`, so no grain collides.

with base as (

    select
        t.tech_id,
        mo.slug as map_slug,
        mp.elo_bucket,
        m.ladder::text as mode,
        t.t_ms
    from match_player_techs t
    inner join match_players mp
        on mp.match_id = t.match_id and mp.profile_id = t.profile_id
    inner join matches m
        on m.match_id = t.match_id
    inner join maps mo
        on mo.map_id = m.map_id
    where mp.won = true
      and mp.elo_bucket is not null

)

select
    tech_id,
    coalesce(map_slug, 'all') as map_slug,
    coalesce(elo_bucket, 'all') as elo_bucket,
    coalesce(mode, 'all') as mode,
    -- `percentile_cont` over the integer `t_ms` already returns `double precision`; the exporter
    -- converts ms -> seconds and rounds to 1 decimal (see `eco_benchmark.rs`).
    percentile_cont(0.25) within group (order by t_ms) as p25_ms,
    percentile_cont(0.50) within group (order by t_ms) as p50_ms,
    percentile_cont(0.75) within group (order by t_ms) as p75_ms,
    count(*) as n
from base
group by grouping sets (
    (tech_id, map_slug, elo_bucket, mode),
    (tech_id, map_slug, mode),
    (tech_id, elo_bucket, mode),
    (tech_id, mode)
)
having count(*) >= 50
