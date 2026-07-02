#!/usr/bin/env bash
# scripts/data-pipeline/sweep.sh
#
# One continuous-capture cycle, run on the VM from cron (~every 3h):
#   1) release the DuckDB lock (stop the browser UI)
#   2) sweep the full 1v1 + team ranked ladders → fresh dated shards
#   3) ingest new shards into DuckDB, deduped by match_id
#   4) refresh replay-parsed truth: seed the freshest ranked matches into the
#      replay manifest, download+parse a capped batch (replays expire in weeks;
#      map slices are built ONLY from replay truth — the API mapname is junk)
#   5) bring the UI back (always, even on failure)
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

# --- replay truth refresh (capped; the manifest is resumable, dupes no-op) ---
REPLAY_BIN="$ROOT/scripts/data-pipeline/replay-rs/target/release/replay-rs"
REPLAY_DIR="$ROOT/data-cache/replays"
if [ -x "$REPLAY_BIN" ]; then
  "$HOME/bin/duckdb" "$DB" -noheader -list \
    -c "SELECT DISTINCT match_id FROM games WHERE source='crawl' AND played_at >= now() - INTERVAL 2 DAY" \
    > /tmp/replay-seed.txt \
    && "$REPLAY_BIN" seed /tmp/replay-seed.txt --db "$REPLAY_DIR/manifest.sqlite" \
    || log "WARN: replay seed exited $?"
  "$REPLAY_BIN" run --db "$REPLAY_DIR/manifest.sqlite" --out "$REPLAY_DIR/shards" \
    --limit "${REPLAY_LIMIT:-1500}" || log "WARN: replay run exited $?"
else
  log "WARN: replay-rs binary missing — skipping replay-truth refresh"
fi

log "sweep done"
