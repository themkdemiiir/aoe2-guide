{{ config(materialized='view') }}

-- Civ-vs-civ 1v1 win rates PER ELO BUCKET, plus an `'all'` rollup row — feeds
-- `civ-matchups-by-elo.json`'s `civs.<slug>.<opp> = {<bucket>: [winRate, games], ..., all: [...]}`.
-- Same `overall`-union-`by_elo` pattern as `civ_meta.sql` (see its doc): the `'all'` row sums only
-- over PAIRS with a known `elo_bucket` (never a null-elo player) — matching
-- `refresh-matchups-current.mjs`'s own `beAcc`/`ag`/`aw` accumulation, which only ever iterates
-- rows the (elo-bucketed) `be` map already carries. `mp1.elo_bucket` is the reads-elo player's OWN
-- generated bucket column (`match_players`'s `elo_bucket`, see that migration's doc) — never the
-- opponent's.

with base as (

    select
        c1.slug as civ_slug,
        c2.slug as opp_slug,
        mp1.elo_bucket,
        mp1.won
    from match_players mp1
    inner join match_players mp2
        on mp2.match_id = mp1.match_id
       and mp2.profile_id <> mp1.profile_id
    inner join matches m on m.match_id = mp1.match_id
    inner join civs c1 on c1.civ_id = mp1.civ_id
    inner join civs c2 on c2.civ_id = mp2.civ_id
    where m.ladder = '1v1'
      and c1.civ_id <> 0
      and c2.civ_id <> 0
      and mp1.civ_id <> mp2.civ_id
      and mp1.won is not null
      and mp1.elo_bucket is not null

),

overall as (

    select civ_slug, opp_slug, 'all' as elo_bucket, count(*) as games,
           sum(case when won then 1 else 0 end) as wins
    from base
    group by 1, 2

),

by_elo as (

    select civ_slug, opp_slug, elo_bucket, count(*) as games,
           sum(case when won then 1 else 0 end) as wins
    from base
    group by 1, 2, 3

)

select
    civ_slug,
    opp_slug,
    elo_bucket,
    games,
    round(100.0 * wins / nullif(games, 0), 2)::double precision as winrate
from (select * from overall union all select * from by_elo) combined
