{{ config(materialized='view') }}

-- Cross-team civ-vs-civ win rates (team ladder, CONFOUNDED by the other players per side) — feeds
-- `civ-matchups-team.json`'s `civs.<slug> = [{opp,games,winRate}]`. Same self-join as
-- `matchups_1v1.sql`, but restricted to CROSS-TEAM pairs: `mp1.won <> mp2.won` is the team
-- discriminator (teammates always share the same `won` value; opponents never do), matching
-- `refresh-matchups-current.mjs`'s own `if (x.won === y.won || x.civ === y.civ) continue; //
-- cross-team only`. A team match's `n_players` is 4/6/8 (never 2), so this join naturally emits
-- every cross-team ordered pair per match, not just one.

select
    c1.slug as civ_slug,
    c2.slug as opp_slug,
    count(*) as games,
    round(100.0 * sum(case when mp1.won then 1 else 0 end) / nullif(count(*), 0), 2)::double precision as winrate
from match_players mp1
inner join match_players mp2
    on mp2.match_id = mp1.match_id
   and mp2.profile_id <> mp1.profile_id
inner join matches m on m.match_id = mp1.match_id
inner join civs c1 on c1.civ_id = mp1.civ_id
inner join civs c2 on c2.civ_id = mp2.civ_id
where m.ladder = 'team'
  and c1.civ_id <> 0
  and c2.civ_id <> 0
  and mp1.civ_id <> mp2.civ_id
  and mp1.won is not null
  and mp2.won is not null
  and mp1.won <> mp2.won
group by 1, 2
