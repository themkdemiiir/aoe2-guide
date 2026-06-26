#!/usr/bin/env bash
# scripts/data-pipeline/build-match-ages.sh
#
# Mine aoestats' replay_summary_raw into a tidy per-player-per-age parquet, by
# streaming: duckdb (read parquet) -> python (ast parse + flatten) -> duckdb
# (write parquet). No giant NDJSON intermediate (the verbose form would overflow
# the VM disk); the parquet is zstd-compressed. Produces data-cache/match_ages.parquet.
# Loading it into the .duckdb (build-match-ages.sql) is a separate step so this
# never needs the DB write lock and can run alongside a crawl sweep.
#
#   bash scripts/data-pipeline/build-match-ages.sh
set -euo pipefail
ROOT="$HOME/aoe2-guide"
DUCKDB="$HOME/bin/duckdb"
SRC="$HOME/aoestats/p_*.parquet"
OUT="$ROOT/data-cache/match_ages.parquet"
EXTRACT="$ROOT/scripts/data-pipeline/extract-replay-summaries.py"

echo "[$(date '+%F %T')] extracting replay_summary_raw -> $OUT"
"$DUCKDB" -c "COPY (SELECT game_id, profile_id, civ, winner, replay_summary_raw
     FROM read_parquet('$SRC')
     WHERE replay_summary_raw IS NOT NULL AND length(replay_summary_raw) > 50)
   TO '/dev/stdout' (FORMAT JSON)" \
 | python3 "$EXTRACT" \
 | "$DUCKDB" -c "COPY (SELECT * FROM read_json('/dev/stdin', format='newline_delimited',
       columns={
         'game_id':'VARCHAR','profile_id':'DOUBLE','civ':'VARCHAR','won':'BOOLEAN',
         'age':'VARCHAR','uptime':'DOUBLE','reached':'BOOLEAN','villagers':'INTEGER',
         'fishing_ships':'INTEGER','military':'INTEGER','n_buildings':'INTEGER',
         'n_research':'INTEGER','units':'JSON','buildings':'JSON','research':'JSON'}))
     TO '$OUT' (FORMAT PARQUET, COMPRESSION zstd)"

echo "[$(date '+%F %T')] done. rows:"
"$DUCKDB" -c "SELECT count(*) AS age_rows, count(DISTINCT game_id) AS matches FROM read_parquet('$OUT');"
ls -lah "$OUT"
