CREATE OR REPLACE TABLE civmap(civ_id INTEGER, civ_slug VARCHAR);
INSERT INTO civmap VALUES (0,'random'),(1,'britons'),(2,'franks'),(3,'goths'),(4,'teutons'),(5,'japanese'),(6,'chinese'),(7,'byzantines'),(8,'persians'),(9,'saracens'),(10,'turks'),(11,'vikings'),(12,'mongols'),(13,'celts'),(14,'spanish'),(15,'aztecs'),(16,'mayans'),(17,'huns'),(18,'koreans'),(19,'italians'),(20,'hindustanis'),(21,'incas'),(22,'magyars'),(23,'slavs'),(24,'portuguese'),(25,'ethiopians'),(26,'malians'),(27,'berbers'),(28,'khmer'),(29,'malay'),(30,'burmese'),(31,'vietnamese'),(32,'bulgarians'),(33,'tatars'),(34,'cumans'),(35,'lithuanians'),(36,'burgundians'),(37,'sicilians'),(38,'poles'),(39,'bohemians'),(40,'dravidians'),(41,'bengalis'),(42,'gurjaras'),(43,'romans'),(44,'armenians'),(45,'georgians'),(46,'achaemenids'),(47,'athenians'),(48,'spartans'),(49,'shu'),(50,'wu'),(51,'wei'),(52,'jurchens'),(53,'khitans'),(54,'macedonians'),(55,'thracians'),(56,'puru'),(57,'non-guide-57'),(58,'mapuche'),(59,'tupi'),(60,'muisca');

CREATE OR REPLACE TABLE games AS
WITH raw AS (
  SELECT '1v1' AS ladder, match_id, completed, gamemod_id, map_raw, CAST(NULL AS BIGINT) AS team_size, players
  FROM read_json('data-cache/relic-patched/matches.ndjson', format='newline_delimited')
  UNION ALL BY NAME
  SELECT 'team' AS ladder, match_id, completed, gamemod_id, map_raw, team_size, players
  FROM read_json('data-cache/relic-team/matches.ndjson', format='newline_delimited')
),
flat AS (SELECT ladder, match_id, completed, gamemod_id, map_raw, team_size, UNNEST(players) AS p FROM raw)
SELECT
  f.ladder, f.match_id, f.completed,
  strftime(to_timestamp(f.completed), '%Y-%m') AS month,
  f.gamemod_id, f.map_raw,
  regexp_replace(lower(regexp_replace(f.map_raw, '\.[a-z0-9]+$', '')), '[^a-z0-9]', '', 'g') AS map_canon,
  COALESCE(f.team_size, 2) AS team_size,
  f.p.profile_id AS profile_id, f.p.civ_id AS civ_id, cm.civ_slug, f.p.rating AS rating,
  CASE WHEN f.p.rating IS NULL THEN 'unknown' WHEN f.p.rating<1000 THEN '<1000' WHEN f.p.rating<1200 THEN '1000-1199' WHEN f.p.rating<1400 THEN '1200-1399' WHEN f.p.rating<1650 THEN '1400-1649' WHEN f.p.rating<1800 THEN '1650-1799' WHEN f.p.rating<2000 THEN '1800-1999' WHEN f.p.rating<2200 THEN '2000-2199' WHEN f.p.rating<2500 THEN '2200-2499' ELSE '2500+' END AS elo_bucket,
  f.p.won AS won
FROM flat f LEFT JOIN civmap cm ON cm.civ_id = f.p.civ_id;
