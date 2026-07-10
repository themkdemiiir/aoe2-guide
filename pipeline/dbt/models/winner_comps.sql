{{ config(materialized='view') }}

-- Winner unit-composition benchmark: among WINNERS of ranked 1v1 RM matches, the share of players
-- (per civ x elo bucket) who trained each military unit at least once, and the median trained
-- count among those producers — feeds `winner-comps.json`, the analyzer's "winners with your civ
-- at your elo typically produce ..." panel (`pipeline/crates/export/src/winner_comps.rs`).
--
-- **Replaces `scripts/data-pipeline/build-winner-comps.sql`'s DuckDB aggregation over aoestats'
-- `match_ages.parquet` per-age `units` JSON — read that script's doc first if this view's shape
-- looks surprising, then this comment for exactly what changed and why:**
--
-- 1. **Source table.** This Postgres schema's `match_ages` (`m20260706_000010_create_match_ages.rs`)
--    carries no per-unit breakdown at all — only `villagers`/`military`/`n_buildings`/`n_research`
--    per-age summary counts. Per-unit totals only exist in `match_player_units`
--    (`m20260706_000012_create_match_player_units.rs`), which that migration's own doc names as
--    the intended "Phase E winner-comps exporter" source — this view IS that Phase E.
-- 2. **REPLAY-SOURCE ONLY, so a smaller corpus.** `match_player_units` is populated exclusively
--    from parsed replays (`replay::derive::player_units`) — aoestats' archive rows carry no
--    per-unit data, so this view's corpus is the replay-derived subset of `1v1` matches (the
--    ~194k-and-growing replay backfill), not the full ~30M-match aoestats archive `civ_meta`/
--    `benchmark_ageup` draw from. Expect materially smaller `winners_n` per cell than the old
--    generator's, and some thin (civ, elo_bucket) cells that used to clear the >=100 threshold to
--    now fall out of it (or vice versa, as the replay corpus keeps growing).
-- 3. **Whole-match totals, not "through Castle Age."** The old script summed only the
--    dark+feudal+castle per-age windows of aoestats' per-age JSON (a player had to have actually
--    reached Castle Age to count at all). `match_player_units.trained` (Σ `amount` over a player's
--    `train` events for one `unit_id`, the WHOLE match, no age dimension at all — see that
--    migration's doc) has no equivalent cutoff available: a late-Imperial siege spam now counts
--    toward `producers`/`med_count` exactly like an early Castle-Age rush would have. This is a
--    documented scope change, not an oversight — there is no age column on this table to filter.
-- 4. **Eco/utility exclusion mirrors the old script's manual list 1:1 by id, not by name**:
--    villager(83), fishing ship(13), trade cart(128), trade cog(354), transport ship(17) — the
--    SAME five ids `crates/replay/src/config.rs::ECO_UNIT_IDS` classifies as non-combat. Kept as a
--    literal id list here (not a Rust-side filter) since the whole aggregation already lives in
--    this view, matching every other dbt model's posture (`export` only ever reads a view's
--    already-aggregated rows — see `query.rs`'s module doc).
-- 5. **`unit` is the `units` dimension's `name`, lower-cased to match the old aoestats extract's
--    casing convention** (e.g. `'skirmisher'`, not `'Skirmisher'`) — see the `units` migration's
--    doc for its aoe2techtree provenance. Its id granularity may not be identical to aoestats' own
--    unit taxonomy (e.g. a base/elite pair aoestats folded under one label could resolve to two
--    distinct ids here); a `train` command's `target_id` should only ever reference a directly
--    trainable (base-tier) unit id in practice, but this is NOT independently re-verified by this
--    view — see the task report for verification notes.
--
-- Thresholds (>=100 winners in the cell, unit produced by >=15% of them) mirror the old
-- `build-winner-comps.sql`'s `HAVING` exactly (`qualifying` below, applied BEFORE ranking so a
-- thin unit never occupies one of the six rank slots). `unit_rank` exposes the top-6 (by producer
-- share) ranking WITHIN each qualifying (civ, elo_bucket) cell — mirrors `civ_meta_openings.sql`'s
-- own "the view assigns rank, the exporter's SQL text filters `<= N`" split
-- (`pipeline/crates/export/src/query.rs::fetch_winner_comps` selects `WHERE unit_rank <= 6`).
-- Ties broken by `unit` (alphabetically) for determinism — the old DuckDB script's plain
-- `ORDER BY ... producers DESC` had no documented tiebreaker.

with winners as (

    select mp.match_id, mp.profile_id, c.slug as civ_slug, mp.elo_bucket
    from match_players mp
    inner join matches m on m.match_id = mp.match_id
    inner join civs c on c.civ_id = mp.civ_id
    where m.ladder = '1v1'
      and mp.won = true
      and mp.elo_bucket is not null
      and c.civ_id <> 0

),

totals as (

    select civ_slug, elo_bucket, count(*) as winners_n
    from winners
    group by 1, 2

),

per_unit as (

    select
        w.civ_slug,
        w.elo_bucket,
        lower(u.name) as unit,
        count(*) as producers,
        -- `trained` is `integer`; `percentile_cont` over an integer-family column already returns
        -- `double precision` (same rule `benchmark_ageup.sql`/`civ_meta_ageup.sql` document for
        -- their own `real` columns) — no extra cast needed.
        percentile_cont(0.5) within group (order by mpu.trained) as med_count
    from winners w
    inner join match_player_units mpu
        on mpu.match_id = w.match_id and mpu.profile_id = w.profile_id
    inner join units u on u.unit_id = mpu.unit_id
    where mpu.unit_id not in (83, 13, 128, 354, 17)
    group by 1, 2, 3

),

qualifying as (

    select
        pu.civ_slug,
        pu.elo_bucket,
        pu.unit,
        t.winners_n,
        pu.producers,
        -- `round(numeric, int)` returns `numeric` — cast to `double precision` so
        -- `pipeline/crates/export`'s plain `f64` column read works, same reasoning as
        -- `civ_meta.sql`'s `winrate` cast.
        round(100.0 * pu.producers / nullif(t.winners_n, 0), 1)::double precision as producer_pct,
        pu.med_count
    from per_unit pu
    inner join totals t using (civ_slug, elo_bucket)
    where t.winners_n >= 100
      and pu.producers >= 0.15 * t.winners_n

)

select
    civ_slug,
    elo_bucket,
    unit,
    winners_n,
    producers,
    producer_pct,
    med_count,
    row_number() over (
        partition by civ_slug, elo_bucket
        order by producers desc, unit
    ) as unit_rank
from qualifying
