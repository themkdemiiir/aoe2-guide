{{ config(materialized='view') }}

-- Per (civ, ladder, opening) games + its rank within that civ/ladder — feeds civ-meta.json's
-- `openings` top-3 list (mirrors `aggregate-rich.mjs`'s `row_number() ... rn<=3` over DuckDB).
-- pipeline/crates/export selects `WHERE opening_rank <= 3` itself; the rank column is exposed
-- here (not pre-filtered) so a future consumer could ask for a different N without a model
-- change. `share` (games / that civ+ladder's overall games) is computed by the exporter, which
-- already has the overall row in hand — recomputing it here would mean querying `civ_meta` again.
--
-- Groups by `opening_kind` (the closed enum), NOT the free-text `opening` column — closes
-- final-review finding #1: `opening` is written by two producers (aoestats' lowercase archive
-- labels, `replay::derive`'s title-case tags/composites) with un-reconciled vocabularies, so
-- `GROUP BY opening` would silently splinter the SAME real strategy across rows the moment both
-- sources coexist. `opening_kind` is the shared closed target both producers cast onto at their
-- own ingest boundary (`aoestats::db::INSERT_PLAYERS_SQL`, `ingest::ingest::INSERT_MATCH_PLAYERS_SQL`)
-- — see `pipeline_core::opening`'s module doc for the full vocabulary/reconciliation trail. The
-- exported `opening` label is therefore now an `opening_kind` enum value (e.g. `man_at_arms`), not
-- the rich per-match string.

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
        mp.opening_kind::text as opening,
        count(*) as games
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join civs c on c.civ_id = mp.civ_id
    where c.civ_id <> 0
      and mp.opening_kind is not null
    group by 1, 2, 3

) opening_counts
