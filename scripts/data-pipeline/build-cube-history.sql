-- Historical cube cells (civ × elo × map × month) from the aoestats archive —
-- the "back months" of the patch axis. The archive has REAL per-match maps and
-- name-derived civs for every month through 2026-02, so back months get full
-- map slices (the crawl-only era before replay truth could not provide them).
-- Months AFTER the archive freeze come from the crawl (build-civ-cube.mjs).
--
-- Runs on the box (parquet only):
--   ~/bin/duckdb -init /dev/null < scripts/data-pipeline/build-cube-history.sql
-- writes /tmp/cube-history.csv, merged by build-civ-cube.mjs.
SET temp_directory='/tmp/duckspill-cubeh';
COPY (
WITH m AS (
  SELECT game_id, map, strftime(started_timestamp, '%Y-%m') AS month
  FROM read_parquet('/home/mkd/aoestats/m_*.parquet')
  WHERE leaderboard = 'random_map'
    AND started_timestamp >= TIMESTAMP '2024-07-01'  -- more than the 16-month axis will ever need
),
p AS (
  SELECT game_id, civ, winner,
         -- paired SQL face of scripts/data-pipeline/lib/buckets.mjs (eloCaseSql)
         CASE WHEN new_rating IS NULL THEN 'unknown'
              WHEN new_rating<1000 THEN '<1000' WHEN new_rating<1200 THEN '1000-1199'
              WHEN new_rating<1400 THEN '1200-1399' WHEN new_rating<1650 THEN '1400-1649'
              WHEN new_rating<1800 THEN '1650-1799' WHEN new_rating<2000 THEN '1800-1999'
              WHEN new_rating<2200 THEN '2000-2199' WHEN new_rating<2500 THEN '2200-2499'
              ELSE '2500+' END AS bucket
  FROM read_parquet('/home/mkd/aoestats/p_*.parquet')
)
SELECT p.civ, p.bucket, m.map, m.month,
       count(*) AS games, sum(p.winner::int) AS wins
FROM p JOIN m USING (game_id)
WHERE p.bucket <> 'unknown'
GROUP BY 1, 2, 3, 4
HAVING count(*) >= 20
) TO '/tmp/cube-history.csv' (FORMAT CSV, HEADER);
