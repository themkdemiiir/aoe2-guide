{{ config(materialized='view') }}

-- Per (civ, ladder, map) games/winrate — feeds civ-meta.json's `byMap`. No min-games gate here:
-- `refresh-civ-current.mjs`'s MIN_MAP threshold is applied once, in
-- pipeline/crates/export/src/civ_meta.rs (MIN_MAP_GAMES), so the threshold lives in a single
-- place rather than duplicated into SQL too.

select
    c.slug as civ_slug,
    m.ladder::text as ladder,
    mo.slug as map_slug,
    count(*) as games,
    -- Cast to `double precision` — see `civ_meta.sql`'s doc on the same line.
    round(100.0 * sum(case when mp.won then 1 else 0 end) / nullif(count(*), 0), 2)::double precision as winrate
from match_players mp
inner join matches m on m.match_id = mp.match_id
inner join civs c on c.civ_id = mp.civ_id
inner join maps mo on mo.map_id = m.map_id
where c.civ_id <> 0
  and mp.won is not null
group by 1, 2, 3
