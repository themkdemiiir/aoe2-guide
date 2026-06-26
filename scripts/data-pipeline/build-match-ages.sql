-- scripts/data-pipeline/build-match-ages.sql
-- Load the extracted per-player-per-age replay detail into DuckDB and build a
-- civ × age timing/economy summary. Run AFTER build-match-ages.sh produces
-- data-cache/match_ages.parquet, and only when the DB is unlocked (UI/ingest off).
--   ~/bin/duckdb ~/aoe2-guide/data-cache/aoe2.duckdb -f scripts/data-pipeline/build-match-ages.sql

CREATE OR REPLACE TABLE match_ages AS
SELECT
  CAST(game_id AS BIGINT)    AS match_id,   -- aoestats game_id == Relic match_id (join key to games)
  CAST(profile_id AS BIGINT) AS profile_id,
  civ, won, age, uptime, reached,
  villagers, fishing_ships, military, n_buildings, n_research,
  units, buildings, research
FROM read_parquet('data-cache/match_ages.parquet');

-- Per-civ age timing + economy snapshot (the headline "how fast / how many vils"
-- numbers). win rate here = among players who reached that age.
CREATE OR REPLACE TABLE civ_age_stats AS
SELECT
  civ, age,
  count(*)                          AS players,
  round(avg(uptime), 1)             AS avg_uptime_s,
  round(median(uptime), 1)          AS median_uptime_s,
  round(avg(villagers), 1)          AS avg_villagers,
  round(avg(military), 1)           AS avg_military,
  round(avg(n_buildings), 1)        AS avg_buildings,
  round(avg(won::INT) * 100, 2)     AS win_pct
FROM match_ages
WHERE reached
GROUP BY 1, 2
ORDER BY civ, array_position(['dark','feudal','castle','imperial'], age);

SELECT 'match_ages'   AS tbl, count(*) AS rows, count(DISTINCT match_id) AS matches FROM match_ages
UNION ALL
SELECT 'civ_age_stats', count(*), NULL FROM civ_age_stats;
