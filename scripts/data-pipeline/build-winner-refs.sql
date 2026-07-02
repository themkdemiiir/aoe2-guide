-- Reference-column derivations for the head-to-head coaching view.
-- 1) winners' OPENING distribution per civ x elo (aoestats p.opening, 1v1 RM)
-- 2) winners' eco-upgrade-by-Castle shares per elo (match_ages research JSON)
-- Parquet-only; writes two CSVs to /tmp.
SET temp_directory='/tmp/duckspill-refs';

COPY (
WITH p AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id, profile_id, winner, opening,
         lower(regexp_replace(civ, '[^A-Za-z0-9]', '', 'g')) AS civ,
         CASE WHEN new_rating IS NULL THEN 'unknown'
              WHEN new_rating<1000 THEN '<1000' WHEN new_rating<1200 THEN '1000-1199'
              WHEN new_rating<1400 THEN '1200-1399' WHEN new_rating<1650 THEN '1400-1649'
              WHEN new_rating<1800 THEN '1650-1799' WHEN new_rating<2000 THEN '1800-1999'
              WHEN new_rating<2200 THEN '2000-2199' WHEN new_rating<2500 THEN '2200-2499'
              ELSE '2500+' END AS bucket
  FROM read_parquet('/home/mkd/aoestats/p_*.parquet')
  WHERE winner AND opening IS NOT NULL AND opening <> ''
),
m AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id FROM read_parquet('/home/mkd/aoestats/m_*.parquet')
  WHERE leaderboard = 'random_map'
),
j AS (SELECT p.civ, p.bucket, p.opening FROM p JOIN m USING (game_id) WHERE p.bucket <> 'unknown'),
tot AS (SELECT civ, bucket, count(*) AS n FROM j GROUP BY 1,2)
SELECT j.civ, j.bucket, j.opening, t.n AS winners_n, count(*) AS cnt,
       round(100.0*count(*)/t.n, 1) AS share_pct
FROM j JOIN tot t USING (civ, bucket)
GROUP BY 1,2,3,t.n
HAVING t.n >= 100 AND count(*) >= 0.08 * t.n
ORDER BY 1,2,cnt DESC
) TO '/tmp/winner-openings.csv' (FORMAT CSV, HEADER);

COPY (
WITH ma AS (
  SELECT a.game_id, a.profile_id, bool_or(a.won) AS won,
         max(CASE WHEN a.age='castle' AND a.reached THEN 1 ELSE 0 END) = 1 AS reached_castle,
         -- researched during dark or feudal window = "by Castle Age"
         -- research is a JSON ARRAY of names; quoted-name LIKE avoids substring collisions
         bool_or(a.age IN ('dark','feudal') AND a.research::VARCHAR LIKE '%"wheelbarrow"%') AS wheelbarrow,
         bool_or(a.age IN ('dark','feudal') AND a.research::VARCHAR LIKE '%"loom"%') AS loom,
         bool_or(a.age IN ('dark','feudal') AND a.research::VARCHAR LIKE '%"double-bit axe"%') AS dba,
         bool_or(a.age IN ('dark','feudal') AND a.research::VARCHAR LIKE '%"horse collar"%') AS horse_collar,
         bool_or(a.age IN ('dark','feudal') AND a.research::VARCHAR LIKE '%"gold mining"%') AS gold_mining
  FROM read_parquet('/home/mkd/aoe2-guide/data-cache/match_ages.parquet') a
  WHERE a.age IN ('dark','feudal','castle')
  GROUP BY 1,2
),
p AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id, profile_id,
         CASE WHEN new_rating IS NULL THEN 'unknown'
              WHEN new_rating<1000 THEN '<1000' WHEN new_rating<1200 THEN '1000-1199'
              WHEN new_rating<1400 THEN '1200-1399' WHEN new_rating<1650 THEN '1400-1649'
              WHEN new_rating<1800 THEN '1650-1799' WHEN new_rating<2000 THEN '1800-1999'
              WHEN new_rating<2200 THEN '2000-2199' WHEN new_rating<2500 THEN '2200-2499'
              ELSE '2500+' END AS bucket
  FROM read_parquet('/home/mkd/aoestats/p_*.parquet')
),
m AS (
  SELECT CAST(game_id AS VARCHAR) AS game_id FROM read_parquet('/home/mkd/aoestats/m_*.parquet')
  WHERE leaderboard = 'random_map'
),
j AS (
  SELECT p.bucket, ma.wheelbarrow, ma.loom, ma.dba, ma.horse_collar, ma.gold_mining
  FROM ma
  JOIN m ON m.game_id = ma.game_id
  JOIN p ON p.game_id = ma.game_id AND p.profile_id = CAST(ma.profile_id AS BIGINT)
  WHERE ma.won AND ma.reached_castle AND p.bucket <> 'unknown'
)
SELECT bucket, count(*) AS winners_n,
       round(100.0*sum(wheelbarrow::int)/count(*),1) AS wheelbarrow_pct,
       round(100.0*sum(loom::int)/count(*),1) AS loom_pct,
       round(100.0*sum(dba::int)/count(*),1) AS dba_pct,
       round(100.0*sum(horse_collar::int)/count(*),1) AS horse_collar_pct,
       round(100.0*sum(gold_mining::int)/count(*),1) AS gold_mining_pct
FROM j GROUP BY 1 ORDER BY 1
) TO '/tmp/winner-ecotech.csv' (FORMAT CSV, HEADER);
