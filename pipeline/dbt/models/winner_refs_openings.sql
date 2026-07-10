{{ config(materialized='view') }}

-- Winners' OPENING distribution per (civ, elo_bucket), ranked -- feeds winner-refs.json's
-- `openings` (pipeline/crates/export/src/winner_refs.rs), the analyzer's coaching-panel "openings
-- winners at your elo play with this civ" reference list.
--
-- **Replaces `scripts/data-pipeline/build-winner-refs.mjs`/`.sql`'s first COPY block (the aoestats
-- `p_*.parquet`/free-text `opening` column) -- read that script's doc first if this view's shape
-- looks surprising, then this comment for exactly what changed and why:**
--
-- 1. **Closed `opening_kind` vocabulary, not the free-text `opening` column.** Same reconciliation
--    `civ_meta_openings.sql` already made (final-review finding #1 -- see
--    `pipeline_core::opening`'s module doc): `opening` is free TEXT written by two producers with
--    un-reconciled casing/vocabularies, so `GROUP BY opening` would silently splinter the SAME real
--    strategy the moment both sources coexist. This view groups by `opening_kind` instead.
-- 2. **No `"unknown"` bucket.** The old DuckDB query's free-text `opening` column carried aoestats'
--    own "tried, couldn't classify" `'unknown'` label as a real, rankable value (it shows up in the
--    committed file's real cells, e.g. armenians/1000-1199 at 13.5%). `opening_kind` has NO
--    `'unknown'` member by design (see `pipeline_core::opening`'s module doc, and the
--    `validate_opening` doc in `export::query`) -- a genuinely unclassified match is honestly
--    absent from this view (`WHERE opening_kind IS NOT NULL`), never a rankable pseudo-strategy. A
--    documented, deliberate scope change, not an oversight.
-- 3. **FULL corpus, same as `civ_meta_openings.sql` -- NOT replay-restricted, unlike
--    `winner_refs_ecotech.sql`.** `opening_kind` is populated for BOTH `aoestats`- and
--    `replay`-sourced matches at their own ingest boundary (see that column's migration doc), so
--    this view applies no `source` filter. Coverage is still bounded by how much of the aoestats
--    archive has been backfilled with `opening_kind` so far (that migration's doc: ~15M of ~108M
--    `opening`-non-null rows backfilled at write time, a real, growing-over-time sample) -- but the
--    grain itself is full-corpus, not replay-only.
--
-- Thresholds (>=100 winners in the cell, an opening's share >=8% of them) mirror the old script's
-- own `HAVING t.n >= 100 AND count(*) >= 0.08 * t.n` exactly. `opening_rank` exposes the
-- share-descending ranking WITHIN each qualifying (civ, elo_bucket) cell (ties broken by `opening`
-- alphabetically for determinism, matching `winner_comps.sql`'s own tiebreak posture) --
-- `pipeline/crates/export/src/query.rs::fetch_winner_openings` selects `WHERE opening_rank <= 5`
-- (the old JS generator's own "first 5 pushed" cap), mirroring `winner_comps.sql`'s
-- rank-in-view/filter-in-Rust split.

with winners as (

    select mp.match_id, c.slug as civ_slug, mp.elo_bucket, mp.opening_kind::text as opening
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join civs c on c.civ_id = mp.civ_id
    where m.ladder = '1v1'
      and mp.won = true
      and mp.opening_kind is not null
      and mp.elo_bucket is not null
      and c.civ_id <> 0

),

totals as (

    select civ_slug, elo_bucket, count(*) as winners_n
    from winners
    group by 1, 2

),

per_opening as (

    select civ_slug, elo_bucket, opening, count(*) as games
    from winners
    group by 1, 2, 3

),

qualifying as (

    select
        po.civ_slug,
        po.elo_bucket,
        po.opening,
        t.winners_n,
        -- `round(numeric, int)` returns `numeric` -- cast to `double precision` so
        -- `pipeline/crates/export`'s plain `f64` column read works, same reasoning as
        -- `winner_comps.sql`'s `producer_pct` cast.
        round(100.0 * po.games / nullif(t.winners_n, 0), 1)::double precision as share_pct
    from per_opening po
    inner join totals t using (civ_slug, elo_bucket)
    where t.winners_n >= 100
      and po.games >= 0.08 * t.winners_n

)

select
    civ_slug,
    elo_bucket,
    opening,
    winners_n,
    share_pct,
    row_number() over (
        partition by civ_slug, elo_bucket
        order by share_pct desc, opening
    ) as opening_rank
from qualifying
