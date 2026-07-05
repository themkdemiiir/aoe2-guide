-- Plan C: winner MEDIANS from the replay EVENT corpus (the only source with per-event
-- timestamps) — first-military + eco-tech research times among ranked-1v1 WINNERS,
-- per elo bucket. Feeds public/winner-refs.json (medsByBucket), which the analyzer
-- reads to replace the "—" on the first-military + eco-upgrade rows.
--
-- Read-only + lock-safe: reads the shards inline via read_json and ATTACHes the main
-- DB read-only for `rating`, so it never needs the DuckDB write-lock (duckui) and
-- streams the events instead of materialising them. The elo CASE mirrors
-- lib/buckets.mjs boundaries exactly.
--
--   ~/bin/duckdb -c ".read scripts/data-pipeline/build-winner-meds.sql"
--
-- NOTE (2026-07-05): the live shards had corrupt trailing gzip members from an
-- interrupted backfill. Re-run the backfill for clean shards, OR point the two
-- read_json() paths below at recovered `*-clean.ndjson` files (plain, no gzip).

SET temp_directory='/tmp/duck_tmp';
ATTACH 'data-cache/aoe2.duckdb' AS gdb (READ_ONLY);

COPY (
  WITH players AS (
    SELECT * FROM read_json('data-cache/replays/shards/players.ndjson.gz',
      format='newline_delimited', ignore_errors=true,
      columns={'match_id':'BIGINT','player_number':'INTEGER','profile_id':'BIGINT',
               'civ_id':'INTEGER','name':'VARCHAR','team':'INTEGER','color':'INTEGER','won':'BOOLEAN'})
  ),
  onev1 AS (
    SELECT match_id FROM players GROUP BY match_id HAVING count(*) = 2
  ),
  winners AS (
    SELECT p.match_id, p.player_number, g.rating, g.map
    FROM players p
    JOIN onev1 USING (match_id)
    JOIN gdb.games g ON g.match_id = p.match_id AND g.profile_id = p.profile_id
    WHERE p.won = TRUE AND g.rating IS NOT NULL AND g.map IS NOT NULL
  ),
  bucketed AS (
    SELECT match_id, player_number, map, CASE
      WHEN rating < 1000 THEN '<1000'      WHEN rating < 1200 THEN '1000-1199'
      WHEN rating < 1400 THEN '1200-1399'  WHEN rating < 1650 THEN '1400-1649'
      WHEN rating < 1800 THEN '1650-1799'  WHEN rating < 2000 THEN '1800-1999'
      WHEN rating < 2200 THEN '2000-2199'  WHEN rating < 2500 THEN '2200-2499'
      ELSE '2500+' END AS bucket
    FROM winners
  ),
  -- eco unit ids [83,13,128,354,17] = villager/fishing-ship/trade-cart/etc (config.rs);
  -- first_military = first non-eco train. Eco techs: Loom 22, DBA 202, Wheelbarrow 213,
  -- Horse-Collar 14 (metrics.rs WATCHED_TECHS).
  events AS (
    SELECT match_id, player_number, t_ms, kind, target_id
    FROM read_json('data-cache/replays/shards/events.ndjson.gz',
      format='newline_delimited', ignore_errors=true,
      columns={'match_id':'BIGINT','profile_id':'BIGINT','player_number':'INTEGER',
               't_ms':'BIGINT','kind':'VARCHAR','target_id':'INTEGER','amount':'INTEGER','detail':'VARCHAR'})
    WHERE (kind = 'train'    AND target_id NOT IN (83,13,128,354,17))
       OR (kind = 'research' AND target_id IN (22,202,213,14))
  ),
  per_player AS (
    SELECT b.bucket, b.map, b.match_id, b.player_number,
      min(CASE WHEN e.kind='train'                        THEN e.t_ms END) AS first_military_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=22  THEN e.t_ms END) AS loom_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=202 THEN e.t_ms END) AS dba_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=213 THEN e.t_ms END) AS wheelbarrow_ms,
      min(CASE WHEN e.kind='research' AND e.target_id=14  THEN e.t_ms END) AS horse_collar_ms
    FROM bucketed b
    JOIN events e USING (match_id, player_number)
    GROUP BY 1,2,3,4
  )
  -- GROUPING SETS emits BOTH per-map cells (map, bucket) AND the all-maps rollup
  -- (bucket, map=NULL→'all'). The analyzer picks the map cell, falls back to 'all'.
  SELECT coalesce(map, 'all') AS map, bucket, count(*) AS winners_n,
    CAST(median(first_military_ms) AS BIGINT) AS first_military_ms,
    CAST(median(loom_ms) AS BIGINT)           AS loom_ms,
    CAST(median(dba_ms) AS BIGINT)            AS dba_ms,
    CAST(median(wheelbarrow_ms) AS BIGINT)    AS wheelbarrow_ms,
    CAST(median(horse_collar_ms) AS BIGINT)   AS horse_collar_ms
  FROM per_player
  GROUP BY GROUPING SETS ((map, bucket), (bucket))
  HAVING count(*) >= 50   -- honest omission: a (map,bucket) cell with < 50 winners is dropped
  ORDER BY 1, 2
) TO '/tmp/winner-meds.csv' (HEADER, DELIMITER ',');
