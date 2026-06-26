#!/usr/bin/env bash
# scripts/data-pipeline/sweep.sh
#
# One continuous-capture cycle, run on the VM from cron (~every 3h):
#   1) release the DuckDB lock (stop the browser UI)
#   2) sweep the full 1v1 + team ranked ladders → fresh dated shards
#   3) ingest new shards into DuckDB, deduped by match_id
#   4) bring the UI back (always, even on failure)
#
# This captures the live ~25-28k ranked games/day going forward. The FIRST run
# also recovers whatever recent games are still in players' match histories.
#
#   bash scripts/data-pipeline/sweep.sh            # full sweep
#   PLAYERS=300 bash scripts/data-pipeline/sweep.sh  # smoke test (cap per ladder)

set -uo pipefail
ROOT="$HOME/aoe2-guide"
DB="$ROOT/data-cache/aoe2.duckdb"
cd "$ROOT" || exit 1

log() { echo "[$(date '+%F %T')] $*"; }
restore_ui() {
  tmux has-session -t duckui 2>/dev/null || \
    tmux new-session -d -s duckui "$HOME/bin/duckdb $DB -cmd \"CALL start_ui_server();\""
}
trap restore_ui EXIT  # UI always comes back, even if a step dies

PLAYERS_ARG=""
[ -n "${PLAYERS:-}" ] && PLAYERS_ARG="--players ${PLAYERS}"

log "sweep start ${PLAYERS:+(smoke: ${PLAYERS} players/ladder)}"
tmux kill-session -t duckui 2>/dev/null || true   # free the write lock
sleep 1

node scripts/data-pipeline/stream-relic.mjs        ${PLAYERS_ARG} || log "WARN: 1v1 sweep exited $?"
node scripts/data-pipeline/stream-relic.mjs --team ${PLAYERS_ARG} || log "WARN: team sweep exited $?"
node scripts/data-pipeline/ingest-stream.mjs                      || log "WARN: ingest exited $?"

log "sweep done"
