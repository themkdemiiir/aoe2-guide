{{ config(materialized='view') }}

-- Joint civ stats cube (civ x elo_bucket x map x build) so `/civs` can offer COMBINABLE filters
-- (e.g. "current patch + 1200 elo + Arabia") with no server — feeds `civ-cube.json`'s dict-indexed
-- `rows` via `pipeline/crates/export/src/civ_cube.rs`, and that module's tiny dropdown-only
-- sibling `civ-cube-dims.json`.
--
-- **Replaces `scripts/data-pipeline/build-civ-cube.mjs` + `build-cube-history.sql`'s DuckDB
-- aggregation — read those first if this view's shape looks surprising, then this comment for
-- exactly what changed and why:**
--
-- 1. **FULL CORPUS, one view, no merge.** The old generator stitched together TWO physically
--    separate sources: an archive-side DuckDB pass (`build-cube-history.sql`, months <= 2026-02,
--    `HAVING count(*) >= 20`) written to a CSV, plus a live crawl loop
--    (`build-civ-cube.mjs`, months > 2026-02, UNTHRESHOLDED per cell) — two code paths because
--    aoestats' frozen parquet archive and the live crawl were separate corpora with no shared
--    store. This Postgres schema already unifies BOTH into the SAME `matches`/`match_players`
--    tables (`matches.source` is `'aoestats'` or `'replay'` — see
--    `m20260705_000001_create_enums.rs`), so one `GROUP BY` over the whole corpus replaces the
--    merge entirely: no month cutoff, no CSV hand-off, no asymmetric per-source thresholding. This
--    view applies the archive side's own `>= 20` floor UNIFORMLY to every cell (not just the
--    pre-2026-02 ones), which tightens what the old crawl-side cells allowed (no floor at all) —
--    a documented, deliberate change; see `civ_cube.rs`'s doc for the practical effect.
-- 2. **No `__unknown__` map sentinel.** The old crawl loop needed one because a live-crawled
--    match's map came from replay-parsed "truth" that didn't always resolve — an unresolved match
--    still had to count toward its elo/patch totals, so it landed in a `__unknown__` bucket kept
--    IN the cube's rows but OUT of the dropdown dims. `matches.map_id` here is a `NOT NULL` foreign
--    key to `maps`: a staged row whose source map slug doesn't resolve is excluded at INSERT time
--    by `aoestats::db`/`ingest` (see that module's "fail-loud vs. bulk-safe" doc) and simply never
--    reaches this table — every match already carries a real, resolvable map, so there is nothing
--    left for a sentinel to catch.
-- 3. **Patch-axis membership is NOT filtered here.** Unlike the old generator (which dropped any
--    row whose patch wasn't already in `civ-meta.json`'s kept-patches set BEFORE aggregating into
--    the cube), this view emits every `(civ, elo_bucket, map, build)` cell with `build IS NOT
--    NULL` and `>= 20` games — same "thresholds live in Rust, not SQL" posture
--    `civ_meta_by_patch.sql` already documents. `pipeline/crates/export/src/civ_cube.rs` reuses
--    `civ_meta::build_patches` (the SAME `MIN_PATCH_TOTAL_MATCHES`/`MAX_PATCHES` computation
--    `civ-meta.json`'s own `patches` axis is built from) as the single source of truth for which
--    builds belong on the axis, then intersects it against this view's rows in Rust — so the two
--    documents' patch axes can never independently drift the way two separately-thresholded
--    SQL/JS computations could.
-- 4. **No guide-civs filter.** The old generator restricted to `src/data/civilizations.json`'s
--    53 guide-covered civs (`loadGuideCivs()`). This view, like every other `civ_meta*`/
--    `matchups_*` view in this project, excludes only `civ_id = 0` (the `"random"` sentinel — see
--    the `civs` migration's doc) and otherwise covers the FULL ~59-civ game roster the `civs` dim
--    tracks. A newly-added DLC civ can appear here before its guide page ships — a real, harmless,
--    already-precedented divergence (`civ_meta.sql` made the identical choice first).
-- 5. **`elo_bucket`** is the same `GENERATED ALWAYS AS (...)` column every other view reads (see
--    `match_players`' migration doc) — already NULL-safe (a NULL `elo` yields a NULL bucket),
--    dropped here via `WHERE elo_bucket IS NOT NULL`, mirroring the old generator's
--    `eloBucket(...) == null` skip.
--
-- Threshold (>= 20 games per cell) mirrors `build-cube-history.sql`'s own `HAVING` — see point 1
-- above for why it now applies to the whole corpus, not just the archive months.

select
    c.slug as civ_slug,
    mp.elo_bucket,
    mo.slug as map_slug,
    m.build::text as build,
    count(*) as games,
    sum(case when mp.won then 1 else 0 end) as wins
from match_players mp
inner join matches m on m.match_id = mp.match_id
inner join civs c on c.civ_id = mp.civ_id
inner join maps mo on mo.map_id = m.map_id
where m.ladder = '1v1'
  and mp.won is not null
  and mp.elo_bucket is not null
  and c.civ_id <> 0
  and m.build is not null
group by 1, 2, 3, 4
having count(*) >= 20
