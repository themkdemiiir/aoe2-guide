-- Winner "villagers trained by Castle Age" medians for the analyzer benchmark
-- (civ × map × elo_bucket × mode + the same 'all' rollups build-benchmark.sql emits).
--
-- Source: match_ages.parquet — per-player-per-AGE-WINDOW counts (verified: dark
-- med 23, feudal med 11, castle med 42 — windows, NOT cumulative snapshots), so
-- "trained by Castle Age" = dark + feudal windows summed per player. This is
-- the SAME basis as the analyzer's vils@Castle (cumulative DeQueue count by the
-- castle-age research completing; starting villagers excluded in both).
-- WINNERS ONLY: the benchmark answers "what do players who WIN at your elo
-- train by Castle Age". Joined to aoestats m_/p_ parquet for map/rating/mode.
--
-- Runs on the box (parquet only — no DuckDB-file lock):
--   ~/bin/duckdb -init /dev/null < scripts/data-pipeline/build-benchmark-vils.sql
-- writes /tmp/bench-vils.csv, consumed by build-benchmark-vils.mjs.
SET temp_directory='/tmp/duckspill-vils';
COPY (
WITH per_player AS (
  SELECT game_id, profile_id,
         lower(regexp_replace(any_value(civ), '[^A-Za-z0-9]', '', 'g')) AS civ,
         bool_or(won) AS won,
         sum(villagers) FILTER (age IN ('dark', 'feudal')) AS vils_by_castle
  FROM read_parquet('/home/mkd/aoe2-guide/data-cache/match_ages.parquet')
  WHERE age IN ('dark', 'feudal', 'castle')
  GROUP BY 1, 2
  -- must actually have reached Castle AND have both earlier windows recorded
  HAVING max(CASE WHEN age = 'castle' AND reached THEN 1 ELSE 0 END) = 1
     AND count(*) FILTER (age IN ('dark', 'feudal')) = 2
),
m AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id,
         regexp_replace(lower(regexp_replace(map, '\.[a-z0-9]+$', '')), '[^a-z0-9]', '', 'g') AS map,
         CASE WHEN leaderboard = 'random_map' THEN '1v1'
              WHEN leaderboard = 'team_random_map' THEN 'team' END AS mode
  FROM read_parquet('/home/mkd/aoestats/m_*.parquet')
  WHERE leaderboard IN ('random_map', 'team_random_map')
),
p AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id, profile_id,
         -- paired SQL face of scripts/data-pipeline/lib/buckets.mjs (eloCaseSql) — keep thresholds in sync
         CASE WHEN new_rating IS NULL THEN 'unknown'
              WHEN new_rating<1000 THEN '<1000' WHEN new_rating<1200 THEN '1000-1199'
              WHEN new_rating<1400 THEN '1200-1399' WHEN new_rating<1650 THEN '1400-1649'
              WHEN new_rating<1800 THEN '1650-1799' WHEN new_rating<2000 THEN '1800-1999'
              WHEN new_rating<2200 THEN '2000-2199' WHEN new_rating<2500 THEN '2200-2499'
              ELSE '2500+' END AS bucket
  FROM read_parquet('/home/mkd/aoestats/p_*.parquet')
),
j AS (
  SELECT pp.civ, m.map, p.bucket, m.mode, pp.vils_by_castle
  FROM per_player pp
  JOIN m ON m.game_id = pp.game_id
  JOIN p ON p.game_id = pp.game_id AND p.profile_id = CAST(pp.profile_id AS BIGINT)
  WHERE pp.won AND p.bucket <> 'unknown'
)
SELECT
  COALESCE(civ, 'all') AS civ,
  COALESCE(map, 'all') AS map,
  COALESCE(bucket, 'all') AS bucket,
  COALESCE(mode, 'all') AS mode,
  count(*) AS n,
  median(vils_by_castle) AS vils_castle
FROM j
GROUP BY GROUPING SETS ((civ, map, bucket, mode), (civ, map, mode), (civ, map))
HAVING count(*) >= 50
) TO '/tmp/bench-vils.csv' (FORMAT CSV, HEADER);
