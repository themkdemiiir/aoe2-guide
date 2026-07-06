{{ config(materialized='view') }}

-- Median feudal/castle/imperial age-up time (seconds) per (civ, ladder) — feeds civ-meta.json's
-- `ageUp`. Mirrors `aggregate-rich.mjs`'s DuckDB `median(feudal_age_uptime)` etc: a civ/ladder
-- with no non-null `feudal_t` sample simply gets no row here (never a fabricated zero) — the
-- exporter's `Option<AgeUp>` stays `None` for it.

select
    c.slug as civ_slug,
    m.ladder::text as ladder,
    percentile_cont(0.5) within group (order by mp.feudal_t) as feudal_median,
    percentile_cont(0.5) within group (order by mp.castle_t) as castle_median,
    percentile_cont(0.5) within group (order by mp.imperial_t) as imperial_median
from match_players mp
inner join matches m on m.match_id = mp.match_id
inner join civs c on c.civ_id = mp.civ_id
where c.civ_id <> 0
  and mp.feudal_t is not null
group by 1, 2
