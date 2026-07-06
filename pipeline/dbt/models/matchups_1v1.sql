{{ config(materialized='view') }}

-- Civ-vs-civ 1v1 overall win rates — feeds `civ-matchups.json`'s `civs.<slug> = [{opp,games,
-- winRate}]`. Grain: (civ_slug, opp_slug). Self-join `match_players` on `match_id` (every 1v1
-- match has exactly two player rows, so the join naturally yields BOTH directed pairs — (a,b) and
-- (b,a) — one per perspective, matching `refresh-matchups-current.mjs`'s own `for (const [x, y] of
-- [[a, b], [b, a]])` loop). Mirror matchups (civ_id = civ_id on both sides) are excluded, same as
-- that generator's `if (a.civ === b.civ) continue`. `civ_id = 0` ("random") excluded on both sides
-- per the `civs` migration's own sentinel doc (see `civ_meta.sql`).
--
-- The min-games threshold (`refresh-matchups-current.mjs`'s `MIN = 50`) is applied in
-- `pipeline/crates/export/src/matchups.rs`, not here — same single-place-for-thresholds posture
-- `civ_meta_by_map.sql`/`civ_meta_by_patch.sql` already established.

select
    c1.slug as civ_slug,
    c2.slug as opp_slug,
    count(*) as games,
    -- Cast to `double precision` — `tokio_postgres` has no `f64` FromSql for Postgres `numeric`,
    -- the type `round(numeric, int)` returns (see `civ_meta.sql`'s doc on the same cast).
    round(100.0 * sum(case when mp1.won then 1 else 0 end) / nullif(count(*), 0), 2)::double precision as winrate
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
group by 1, 2
