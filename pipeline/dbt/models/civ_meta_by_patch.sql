{{ config(materialized='view') }}

-- Per (civ, ladder, build) games/winrate — feeds civ-meta.json's `byPatch`. `matches.build` is
-- the numeric game build id (aoestats' own `patch` column, see `aoestats::db`'s
-- `INSERT_MATCHES_SQL`) — `matches.patch` (the free-text label column) has no aoestats source and
-- is always NULL today, so `build` is the only usable patch axis. Thresholds
-- (MIN_CIV_PATCH_GAMES, and the global MIN_PATCH_TOTAL_MATCHES from `patch_axis.sql`) are applied
-- in pipeline/crates/export/src/civ_meta.rs, same single-place-for-thresholds posture as
-- civ_meta_by_map.sql.

select
    c.slug as civ_slug,
    m.ladder::text as ladder,
    m.build::text as build,
    count(*) as games,
    -- Cast to `double precision` — see `civ_meta.sql`'s doc on the same line.
    round(100.0 * sum(case when mp.won then 1 else 0 end) / nullif(count(*), 0), 2)::double precision as winrate
from match_players mp
inner join matches m on m.match_id = mp.match_id
inner join civs c on c.civ_id = mp.civ_id
where c.civ_id <> 0
  and mp.won is not null
  and m.build is not null
group by 1, 2, 3
