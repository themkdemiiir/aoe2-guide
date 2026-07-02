-- What WINNERS produce, per civ × elo bucket (ranked 1v1 RM): for each military
-- unit line, the share of winners who produced it through Castle Age and the
-- median count among those who did.
--
-- Source: match_ages.parquet per-age `units` JSON (dark+feudal+castle windows
-- summed = produced through Castle Age) joined to aoestats m_/p_ parquet for
-- mode/rating. Unit names come from the aoestats extract verbatim (lowercase,
-- includes unique units like "conquistador").
--
-- Runs on the box (parquet only):
--   ~/bin/duckdb -init /dev/null < scripts/data-pipeline/build-winner-comps.sql
-- writes /tmp/winner-comps.csv, consumed by build-winner-comps.mjs.
SET temp_directory='/tmp/duckspill-comps';
COPY (
WITH ma AS (
  SELECT game_id, profile_id, lower(regexp_replace(any_value(civ), '[^A-Za-z0-9]', '', 'g')) AS civ,
         bool_or(won) AS won
  FROM read_parquet('/home/mkd/aoe2-guide/data-cache/match_ages.parquet')
  WHERE age IN ('dark', 'feudal', 'castle')
  GROUP BY 1, 2
  HAVING max(CASE WHEN age = 'castle' AND reached THEN 1 ELSE 0 END) = 1
),
units_exploded AS (
  SELECT a.game_id, a.profile_id, je.key AS unit, CAST(je.value AS INTEGER) AS n
  FROM read_parquet('/home/mkd/aoe2-guide/data-cache/match_ages.parquet') a,
       json_each(a.units) je
  WHERE a.age IN ('dark', 'feudal', 'castle')
    AND je.key NOT IN ('villager', 'fishing ship', 'transport ship', 'trade cart', 'trade cog')
),
per_player_units AS (
  SELECT game_id, profile_id, unit, sum(n) AS n
  FROM units_exploded GROUP BY 1, 2, 3
),
m AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id
  FROM read_parquet('/home/mkd/aoestats/m_*.parquet')
  WHERE leaderboard = 'random_map'
),
p AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id, profile_id,
         -- paired SQL face of scripts/data-pipeline/lib/buckets.mjs (eloCaseSql)
         CASE WHEN new_rating IS NULL THEN 'unknown'
              WHEN new_rating<1000 THEN '<1000' WHEN new_rating<1200 THEN '1000-1199'
              WHEN new_rating<1400 THEN '1200-1399' WHEN new_rating<1650 THEN '1400-1649'
              WHEN new_rating<1800 THEN '1650-1799' WHEN new_rating<2000 THEN '1800-1999'
              WHEN new_rating<2200 THEN '2000-2199' WHEN new_rating<2500 THEN '2200-2499'
              ELSE '2500+' END AS bucket
  FROM read_parquet('/home/mkd/aoestats/p_*.parquet')
),
winners AS (
  SELECT ma.game_id, ma.profile_id, ma.civ, p.bucket
  FROM ma
  JOIN m ON m.game_id = ma.game_id
  JOIN p ON p.game_id = ma.game_id AND p.profile_id = CAST(ma.profile_id AS BIGINT)
  WHERE ma.won AND p.bucket <> 'unknown'
),
totals AS (
  SELECT civ, bucket, count(*) AS winners_n FROM winners GROUP BY 1, 2
),
per_unit AS (
  SELECT w.civ, w.bucket, u.unit,
         count(*) AS producers,
         median(u.n) AS med_count
  FROM winners w
  JOIN per_player_units u ON u.game_id = w.game_id AND u.profile_id = w.profile_id
  GROUP BY 1, 2, 3
)
SELECT pu.civ, pu.bucket, pu.unit,
       t.winners_n,
       pu.producers,
       round(100.0 * pu.producers / t.winners_n, 1) AS producer_pct,
       pu.med_count
FROM per_unit pu JOIN totals t USING (civ, bucket)
WHERE t.winners_n >= 100 AND pu.producers >= 0.15 * t.winners_n
ORDER BY pu.civ, pu.bucket, pu.producers DESC
) TO '/tmp/winner-comps.csv' (FORMAT CSV, HEADER);
