{{ config(materialized='view') }}

-- Per (civ, ladder) overall + per (civ, ladder, elo_bucket) breakdown, one flat grain — the
-- primary civ-meta aggregation task M5a's brief asks for ("GROUP BY civ + elo_bucket"). Feeds the
-- committed src/data/civ-meta.json shape's civs.<slug>.<ladder>.{games,winRate,ci95,playRate,
-- tier} (the `elo_bucket = 'all'` rollup row) and .byElo (the nine real buckets).
--
-- civ_id -> slug is resolved HERE, via the `civs` JOIN — pipeline/crates/export never does its
-- own civ_id->slug lookup (see that crate's lib.rs doc). civ_id = 0 ("random") is excluded, per
-- the `civs` migration's own doc: it is a sentinel, not a real civilization.
--
-- `ladder_total_games` is the SAME value repeated on every row of a given ladder (that ladder's
-- 'all'-row games, summed) — pipeline/crates/export reads it only off the 'all' row, to compute
-- `playRate = games / ladder_total_games`, matching `refresh-civ-current.mjs`'s
-- `pct(c.g / totalApp)`.

with base as (

    select
        c.slug as civ_slug,
        m.ladder::text as ladder,
        mp.elo_bucket,
        mp.won
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join civs c on c.civ_id = mp.civ_id
    where c.civ_id <> 0
      -- A missing result is excluded, never counted as a loss (see the "fail-loud vs bulk-safe"
      -- posture `aoestats::db`'s module doc documents for the same kind of gap).
      and mp.won is not null

),

overall as (

    select
        civ_slug,
        ladder,
        'all' as elo_bucket,
        count(*) as games,
        sum(case when won then 1 else 0 end) as wins
    from base
    group by 1, 2

),

by_elo as (

    select
        civ_slug,
        ladder,
        elo_bucket,
        count(*) as games,
        sum(case when won then 1 else 0 end) as wins
    from base
    where elo_bucket is not null
    group by 1, 2, 3

),

combined as (

    select * from overall
    union all
    select * from by_elo

)

select
    civ_slug,
    ladder,
    elo_bucket,
    games,
    wins,
    -- `round(numeric, int)` returns `numeric`, which `tokio_postgres` has no built-in `f64`
    -- FromSql for — cast to `double precision` so `pipeline/crates/export`'s plain `f64` column
    -- read works without pulling in `rust_decimal` for one column.
    round(100.0 * wins / nullif(games, 0), 2)::double precision as winrate,
    -- `sum(bigint)` returns `numeric` in Postgres (overflow-safe) — cast back to `bigint`, same
    -- reasoning as the `winrate` cast above (`tokio_postgres` has no `i64` FromSql for `numeric`).
    (sum(games) filter (where elo_bucket = 'all') over (partition by ladder))::bigint as ladder_total_games
from combined
