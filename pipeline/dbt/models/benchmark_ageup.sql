{{ config(materialized='view') }}

-- Analyzer benchmark: median age-up COMPLETION seconds by civ x map x elo_bucket x mode, with
-- `'all'`-keyed rollups — feeds `scripts/data-pipeline/replay-rs/data/benchmark.json`'s
-- `civs.<slug>.<mapSlug>.<bucket>.<mode> = {feudal_s,castle_s,imperial_s}`. Mirrors
-- `build-benchmark.sql`'s GROUPING SETS shape exactly: the most granular cell
-- (civ,map,bucket,mode), a per-map/mode rollup (civ,map,mode), a per-map rollup (civ,map), and a
-- per-civ rollup (civ) — `civ` itself is NEVER rolled up (every real row names its own civ).
-- `pipeline/crates/export`'s `Benchmark::slice`-style fallback (exact -> map rollup -> civ rollup)
-- reads whichever of these four grains it finds.
--
-- `mp.feudal_t`/`castle_t`/`imperial_t` are the SAME age-completion-second columns
-- `civ_meta_ageup.sql` already medians per (civ, ladder) — this view adds the map/elo_bucket/mode
-- dimensions build-benchmark.sql's `games` table carried that our leaner `civ_meta_ageup` grain
-- does not.
--
-- `elo_bucket IS NOT NULL` is required (not just filtered downstream): GROUPING SETS treats a
-- real NULL `elo_bucket` (an unrated player) as its own group, and the outer
-- `coalesce(elo_bucket, 'all')` would otherwise merge that "actually unknown" group into the SAME
-- output row as the deliberate elo-rollup group from the `(civ, map, mode)` grouping set — a silent
-- double-count. Excluding unrated players up front (same posture as `civ_meta.sql`'s `by_elo` CTE)
-- avoids that collision entirely.
--
-- The old `build-benchmark.sql`'s `HAVING count(*) >= 50` stays IN this model (not the Rust
-- exporter): GROUPING SETS' four grains are only meaningfully comparable post-aggregation, and the
-- old SQL this mirrors already applied the threshold the same way — unlike `civ_meta`'s
-- thresholds, which the OLD `refresh-civ-current.mjs` applied in JS, after the fact.

with base as (

    select
        c.slug as civ_slug,
        mo.slug as map_slug,
        mp.elo_bucket,
        m.ladder::text as mode,
        mp.feudal_t,
        mp.castle_t,
        mp.imperial_t
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join maps mo on mo.map_id = m.map_id
    inner join civs c on c.civ_id = mp.civ_id
    where c.civ_id <> 0
      and mp.feudal_t is not null
      and mp.elo_bucket is not null

)

select
    civ_slug,
    coalesce(map_slug, 'all') as map_slug,
    coalesce(elo_bucket, 'all') as elo_bucket,
    coalesce(mode, 'all') as mode,
    -- `percentile_cont` over a `real`/`float4` column already returns `double precision` (unlike
    -- `round(numeric, int)`/`sum(bigint)`, no extra cast needed — see `civ_meta_ageup.sql`, which
    -- reads the same two columns the same way).
    percentile_cont(0.5) within group (order by feudal_t) as feudal_median,
    percentile_cont(0.5) within group (order by castle_t) as castle_median,
    percentile_cont(0.5) within group (order by imperial_t) as imperial_median,
    count(*) as n
from base
group by grouping sets (
    (civ_slug, map_slug, elo_bucket, mode),
    (civ_slug, map_slug, mode),
    (civ_slug, map_slug),
    (civ_slug)
)
having count(*) >= 50
