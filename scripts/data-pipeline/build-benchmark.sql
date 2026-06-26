-- Benchmark medians, dimensioned by civ × map × elo_bucket × mode, with rollups.
-- Run on the VM (DB must be UNLOCKED — stop the duckui tmux session first):
--   ~/bin/duckdb -readonly ~/aoe2-guide/data-cache/aoe2.duckdb -csv -c ".read build-benchmark.sql" > bench.csv
--   node scripts/data-pipeline/build-benchmark.mjs bench.csv   # -> replay-rs/data/benchmark.json
--
-- Source: `games.{feudal,castle,imperial}_t` directly (age COMPLETION seconds; ~13.9M matches
-- with age times — 3× the match_ages subset, and it carries elo_bucket/map/team_size). The
-- analyzer measures age-up CLICK ms, so the compare step adds the research duration
-- (Feudal 130 / Castle 160 / Imperial 190 s) to convert click->completion before comparing.
--
-- The age-time corpus is almost entirely TEAM games, so mode resolves to 'team' or the 'all'
-- rollup (1v1 falls back to the map rollup). `map` is the key dimension — Arena's Fast-Castle
-- Castle (~15 min) vs Arabia's (~21 min) means a single all-map median mis-coaches.
--
-- GROUPING SETS emit, per civ: the specific (civ,map,bucket,mode) cell, a per-map rollup
-- (civ,map,'all','all'), and a per-civ rollup (civ,'all','all','all'); the analyzer's
-- Benchmark::slice falls back exact -> (civ,map,'all','all') -> None (never cross-map).
WITH g AS (
  SELECT civ, "map" AS mp, elo_bucket AS bucket,
         CASE WHEN team_size = 1 THEN '1v1' ELSE 'team' END AS mode,
         feudal_t, castle_t, imperial_t
  FROM games
  WHERE feudal_t IS NOT NULL
)
SELECT civ,
  COALESCE(mp, 'all')     AS mapname,
  COALESCE(bucket, 'all') AS bucket,
  COALESCE(mode, 'all')   AS mode,
  round(median(feudal_t), 1)   AS feudal_s,
  round(median(castle_t), 1)   AS castle_s,
  round(median(imperial_t), 1) AS imperial_s,
  count(*) AS n
FROM g
GROUP BY GROUPING SETS ((civ, mp, bucket, mode), (civ, mp), (civ))
HAVING count(*) >= 50
ORDER BY 1, 2, 3, 4;
