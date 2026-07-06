{{ config(materialized='view') }}

-- Analyzer benchmark overlay: median villagers trained through Castle Age among WINNERS, by
-- civ x map x elo_bucket x mode — feeds `benchmark.json` cells' optional `vils_castle` field (see
-- `pipeline/crates/export/src/benchmark.rs`'s doc: present only on the grains this view actually
-- covers, `null` elsewhere — mirrors `build-benchmark-vils.mjs`'s own "merge into existing cells,
-- never invent" contract). Mirrors `build-benchmark-vils.sql`'s three (not four — this view never
-- rolls up to a bare `(civ_slug)` grain; `build-benchmark-vils.sql` doesn't either) GROUPING SETS.
--
-- `match_ages.uptime_ms >= 0` filters the `-1` "not reached" sentinel (see that migration's doc) —
-- required per the task brief. "Trained through Castle Age" = the `dark` + `feudal` window
-- `villagers` counts SUMMED per player (verified non-cumulative per-window counts, not running
-- totals — see `build-benchmark-vils.sql`'s doc); a player needs BOTH windows present (`
-- n_early_windows = 2`) and a real (sentinel-filtered) `castle` row to count as "reached Castle".
-- `bool_or(won)` (not a bare `won` column) matches the same defensive aggregation
-- `build-benchmark-vils.sql` uses, in case a player's per-age rows ever carry an inconsistent
-- `won` value.

with valid_ages as (

    select match_id, profile_id, civ_id, won, age, villagers
    from match_ages
    where age in ('dark', 'feudal', 'castle')
      and uptime_ms >= 0

),

per_player as (

    select
        match_id,
        profile_id,
        civ_id,
        bool_or(won) as won,
        sum(villagers) filter (where age in ('dark', 'feudal')) as vils_by_castle,
        bool_or(age = 'castle') as reached_castle,
        count(*) filter (where age in ('dark', 'feudal')) as n_early_windows
    from valid_ages
    group by 1, 2, 3

),

qualifying as (

    select match_id, profile_id, civ_id, vils_by_castle
    from per_player
    where won
      and reached_castle
      and n_early_windows = 2
      and vils_by_castle is not null

),

joined as (

    select
        c.slug as civ_slug,
        mo.slug as map_slug,
        mp.elo_bucket,
        m.ladder::text as mode,
        q.vils_by_castle
    from qualifying q
    inner join matches m on m.match_id = q.match_id
    inner join maps mo on mo.map_id = m.map_id
    inner join civs c on c.civ_id = q.civ_id
    inner join match_players mp on mp.match_id = q.match_id and mp.profile_id = q.profile_id
    where c.civ_id <> 0
      and mp.elo_bucket is not null

)

select
    civ_slug,
    coalesce(map_slug, 'all') as map_slug,
    coalesce(elo_bucket, 'all') as elo_bucket,
    coalesce(mode, 'all') as mode,
    percentile_cont(0.5) within group (order by vils_by_castle) as vils_median,
    count(*) as n
from joined
group by grouping sets (
    (civ_slug, map_slug, elo_bucket, mode),
    (civ_slug, map_slug, mode),
    (civ_slug, map_slug)
)
having count(*) >= 50
