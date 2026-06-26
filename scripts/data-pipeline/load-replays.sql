-- scripts/data-pipeline/load-replays.sql
-- Load the gzipped replay shards (from replay-rs) into the DuckDB workbench.
-- The Rust pipeline writes ONE append-only .ndjson.gz per table (multi-member
-- gzip), which DuckDB reads natively. `ignore_errors` tolerates a half-written
-- tail line if the run is still going. Tables key to `games` by match_id;
-- `replay_ages` shares the shape of historical `match_ages` so forward + history
-- unify (note: forward uptime = age-up CLICK time + villagers are CUMULATIVE,
-- vs historical = completion time + per-age production — reconcile when merging).
--
--   ~/bin/duckdb aoe2.duckdb -f load-replays.sql   (set `shards` below)

SET VARIABLE shards = 'data-cache/replays/shards';

CREATE OR REPLACE TABLE replay_meta AS
SELECT * FROM read_json(getvariable('shards') || '/meta.ndjson.gz',
  format='newline_delimited', ignore_errors=true,
  columns={'match_id':'BIGINT','build':'INTEGER','version_major':'INTEGER',
           'map_id':'INTEGER','duration_ms':'BIGINT','n_players':'INTEGER'});

CREATE OR REPLACE TABLE replay_players AS
SELECT * FROM read_json(getvariable('shards') || '/players.ndjson.gz',
  format='newline_delimited', ignore_errors=true,
  columns={'match_id':'BIGINT','player_number':'INTEGER','profile_id':'BIGINT',
           'civ_id':'INTEGER','name':'VARCHAR','team':'INTEGER','color':'INTEGER','won':'BOOLEAN'});

-- full event timeline: every train/research/build/move/..., timestamped
CREATE OR REPLACE TABLE replay_events AS
SELECT * FROM read_json(getvariable('shards') || '/events.ndjson.gz',
  format='newline_delimited', ignore_errors=true,
  columns={'match_id':'BIGINT','profile_id':'BIGINT','player_number':'INTEGER',
           't_ms':'BIGINT','kind':'VARCHAR','target_id':'INTEGER','amount':'INTEGER','detail':'VARCHAR'});

-- per-player-per-age composition (same shape as historical match_ages)
CREATE OR REPLACE TABLE replay_ages AS
SELECT * FROM read_json(getvariable('shards') || '/ages.ndjson.gz',
  format='newline_delimited', ignore_errors=true,
  columns={'match_id':'BIGINT','profile_id':'BIGINT','civ_id':'INTEGER','won':'BOOLEAN',
           'age':'VARCHAR','uptime_ms':'BIGINT','villagers':'INTEGER','military':'INTEGER',
           'n_buildings':'INTEGER','n_research':'INTEGER'});

SELECT 'replay_meta' AS tbl, count(*) n FROM replay_meta
UNION ALL SELECT 'replay_players', count(*) FROM replay_players
UNION ALL SELECT 'replay_events', count(*) FROM replay_events
UNION ALL SELECT 'replay_ages', count(*) FROM replay_ages;
