{{ config(materialized='view') }}

-- Per (civ, ladder, opening) games + its rank within that civ/ladder — feeds civ-meta.json's
-- `openings` top-3 list (mirrors `aggregate-rich.mjs`'s `row_number() ... rn<=3` over DuckDB).
-- pipeline/crates/export selects `WHERE opening_rank <= 3` itself; the rank column is exposed
-- here (not pre-filtered) so a future consumer could ask for a different N without a model
-- change. `share` (games / that civ+ladder's overall games) is computed by the exporter, which
-- already has the overall row in hand — recomputing it here would mean querying `civ_meta` again.

select
    civ_slug,
    ladder,
    opening,
    games,
    row_number() over (partition by civ_slug, ladder order by games desc) as opening_rank
from (

    select
        c.slug as civ_slug,
        m.ladder::text as ladder,
        mp.opening,
        count(*) as games
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join civs c on c.civ_id = mp.civ_id
    where c.civ_id <> 0
      and mp.opening is not null
      and mp.opening <> ''
    group by 1, 2, 3

) opening_counts
