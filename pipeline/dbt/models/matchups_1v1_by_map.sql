{{ config(materialized='view') }}

-- Civ-vs-civ 1v1 win rates PER MAP — feeds `civ-matchups-by-map.json`'s `civs.<slug>.<mapSlug> =
-- [{opp,games,winRate}]`. Same self-join shape as `matchups_1v1.sql` (see its doc), plus a `maps`
-- JOIN on `matches.map_id` (the pipeline's own replay/aoestats-derived map truth — no separate
-- "map truth" reconciliation step needed here, unlike the old crawl-based generator, since
-- `matches.map_id` already IS the resolved map). Min-games threshold (`MIN_MAP = 50`) applied in
-- `pipeline/crates/export/src/matchups.rs`.

select
    c1.slug as civ_slug,
    c2.slug as opp_slug,
    mo.slug as map_slug,
    count(*) as games,
    round(100.0 * sum(case when mp1.won then 1 else 0 end) / nullif(count(*), 0), 2)::double precision as winrate
from match_players mp1
inner join match_players mp2
    on mp2.match_id = mp1.match_id
   and mp2.profile_id <> mp1.profile_id
inner join matches m on m.match_id = mp1.match_id
inner join maps mo on mo.map_id = m.map_id
inner join civs c1 on c1.civ_id = mp1.civ_id
inner join civs c2 on c2.civ_id = mp2.civ_id
where m.ladder = '1v1'
  and c1.civ_id <> 0
  and c2.civ_id <> 0
  and mp1.civ_id <> mp2.civ_id
  and mp1.won is not null
group by 1, 2, 3
