-- LEGACY crawl-only bootstrap (the production merged aoestats+crawl `games`
-- table lives in data-cache/aoe2.duckdb and is maintained by ingest-stream.mjs;
-- see scripts/migrations/2026-07-02-rebuild-crawl-rows.mjs for the full rebuild).
--
-- civmap = the GAME/replay civ-id space (paired SQL face of src/data/civ-id-map.json).
-- It must NEVER be applied to crawl records: the Relic API uses its own id space —
-- that's relic_civmap, generated from src/data/relic-civ-id-map.json by
-- ingest-stream.mjs / the rebuild migration. Run one of those first.
CREATE OR REPLACE TABLE civmap(civ_id INTEGER, civ_slug VARCHAR);
INSERT INTO civmap VALUES (0,'random'),(1,'britons'),(2,'franks'),(3,'goths'),(4,'teutons'),(5,'japanese'),(6,'chinese'),(7,'byzantines'),(8,'persians'),(9,'saracens'),(10,'turks'),(11,'vikings'),(12,'mongols'),(13,'celts'),(14,'spanish'),(15,'aztecs'),(16,'mayans'),(17,'huns'),(18,'koreans'),(19,'italians'),(20,'hindustanis'),(21,'incas'),(22,'magyars'),(23,'slavs'),(24,'portuguese'),(25,'ethiopians'),(26,'malians'),(27,'berbers'),(28,'khmer'),(29,'malay'),(30,'burmese'),(31,'vietnamese'),(32,'bulgarians'),(33,'tatars'),(34,'cumans'),(35,'lithuanians'),(36,'burgundians'),(37,'sicilians'),(38,'poles'),(39,'bohemians'),(40,'dravidians'),(41,'bengalis'),(42,'gurjaras'),(43,'romans'),(44,'armenians'),(45,'georgians'),(46,'achaemenids'),(47,'athenians'),(48,'spartans'),(49,'shu'),(50,'wu'),(51,'wei'),(52,'jurchens'),(53,'khitans'),(54,'macedonians'),(55,'thracians'),(56,'puru'),(57,'muisca'),(58,'mapuche'),(59,'tupi');

CREATE OR REPLACE TABLE games AS
WITH raw AS (
  SELECT match_id, completed, gamemod_id, map_raw, ladder AS matchtype, team_size, players
  FROM read_json(['data-cache/relic-patched/matches.ndjson',
                  'data-cache/relic-team/matches.ndjson'],
                 format='newline_delimited', union_by_name=true)
),
fresh AS (
  SELECT * FROM raw
  WHERE matchtype IN (6, 7, 8, 9)   -- ranked RM only (6=1v1, 7/8/9=team)
    AND completed >= 1756684800     -- 2025-09-01: current Relic civ-id-space era (relic-civ-id-map.json validFrom)
  QUALIFY row_number() OVER (PARTITION BY match_id ORDER BY completed) = 1
),
flat AS (SELECT matchtype, match_id, completed, gamemod_id, map_raw, team_size, UNNEST(players) AS p FROM fresh)
SELECT
  CASE WHEN f.matchtype = 6 THEN '1v1' ELSE 'team' END AS ladder,
  f.match_id, f.completed,
  strftime(to_timestamp(f.completed), '%Y-%m') AS month,
  f.gamemod_id,
  f.map_raw,  -- RAW API mapname: wrong for ~57% of matches; replay parsing is the only map truth
  COALESCE(f.team_size, 2) AS team_size,
  f.p.profile_id AS profile_id, f.p.civ_id AS civ_id,
  -- fail-loud on unmapped ids, same rule as ingest-stream.mjs / the migration
  CASE WHEN cm.civ_slug IS NULL AND f.p.civ_id IS NOT NULL
       THEN error('build-duckdb: unmapped Relic civ id ' || CAST(f.p.civ_id AS VARCHAR))
       ELSE cm.civ_slug END AS civ_slug,
  f.p.rating AS rating,
  -- paired SQL face of scripts/data-pipeline/lib/buckets.mjs (eloCaseSql) — keep thresholds in sync
  CASE WHEN f.p.rating IS NULL THEN 'unknown' WHEN f.p.rating<1000 THEN '<1000' WHEN f.p.rating<1200 THEN '1000-1199' WHEN f.p.rating<1400 THEN '1200-1399' WHEN f.p.rating<1650 THEN '1400-1649' WHEN f.p.rating<1800 THEN '1650-1799' WHEN f.p.rating<2000 THEN '1800-1999' WHEN f.p.rating<2200 THEN '2000-2199' WHEN f.p.rating<2500 THEN '2200-2499' ELSE '2500+' END AS elo_bucket,
  f.p.won AS won
FROM flat f LEFT JOIN relic_civmap cm ON cm.civ_id = f.p.civ_id;
