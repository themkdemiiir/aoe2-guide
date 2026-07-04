#!/usr/bin/env bash
# scripts/data-pipeline/backfill-seed.sh
#
# Rolling archive backfill seeder, run on the VM from sweep.sh. Walks BACKWARDS
# through the weekly aoestats parquet dumps (~/aoestats/m_YYYY-MM-DD.parquet),
# one week per invocation, seeding a deterministic random sample of that week's
# ranked-RM matches (with participant profile_ids) into the replay manifest.
# `replay-rs run` then drains them via the age-archive fallback
# (api.ageofempires.com — replays for matches years old), paced + capped there.
#
# Honesty note: at the archive's harsh rate limit full coverage is impossible
# (~230k RM matches/week vs ~500 paced requests/day), so this is a SAMPLER —
# the same statistical footing as the live sweep, which parses ~1.5k of ~28k
# daily games. Sampling is hash-ordered: deterministic, not cherry-picked.
#
# Guards (each skips loudly, never silently):
#   - free disk on / must be >= MIN_FREE_GB (replay shards are the only copy)
#   - manifest pending backlog must be < MAX_PENDING (drain before seeding more)
#
# State: $CURSOR holds the week LAST SEEDED; the next run seeds the newest
# archive week strictly older than it. Cursor advances only after a successful
# seed, so a failed run retries the same week.

set -uo pipefail
ROOT="$HOME/aoe2-guide"
ARCHIVE_DIR="$HOME/aoestats"
REPLAY_BIN="$ROOT/scripts/data-pipeline/replay-rs/target/release/replay-rs"
MANIFEST="${MANIFEST:-$ROOT/data-cache/replays/manifest.sqlite}"
CURSOR="${CURSOR:-$ROOT/data-cache/replays/backfill-cursor}"
DUCKDB="$HOME/bin/duckdb"
SAMPLE_PER_WEEK=${SAMPLE_PER_WEEK:-2500}
MIN_FREE_GB=${MIN_FREE_GB:-20}
MAX_PENDING=${MAX_PENDING:-3000}

log() { echo "[$(date '+%F %T')] backfill: $*"; }

[ -x "$REPLAY_BIN" ] || { log "SKIP — replay-rs binary missing"; exit 0; }
[ -d "$ARCHIVE_DIR" ] || { log "SKIP — archive dir $ARCHIVE_DIR missing"; exit 0; }

# --- disk guard ---------------------------------------------------------------
free_gb=$(df --output=avail -BG / | tail -1 | tr -dc '0-9')
if [ "${free_gb:-0}" -lt "$MIN_FREE_GB" ]; then
  log "SKIP — only ${free_gb}G free on / (need ${MIN_FREE_GB}G). Expand the disk."
  exit 0
fi

# --- backlog guard ------------------------------------------------------------
if command -v sqlite3 >/dev/null 2>&1 && [ -f "$MANIFEST" ]; then
  # Count both statuses: rate-limited/deferred archive matches park as 'error'
  # between runs (replay-rs resets them to 'pending' at the start of its next
  # run), so 'error' is undrained backlog too — ignoring it would over-seed.
  pending=$(sqlite3 "$MANIFEST" "SELECT count(*) FROM matches WHERE status IN ('pending','error')" 2>/dev/null || echo 0)
  if [ "${pending:-0}" -ge "$MAX_PENDING" ]; then
    log "SKIP — $pending pending/error in manifest (>= $MAX_PENDING); draining first"
    exit 0
  fi
else
  log "note: sqlite3 CLI or manifest missing — seeding without a backlog check"
fi

# --- pick the next week (newest archive week strictly older than the cursor) ---
last_seeded=$(cat "$CURSOR" 2>/dev/null || echo "9999-12-31")
week=""
for f in $(ls "$ARCHIVE_DIR"/m_*.parquet 2>/dev/null | sort -r); do
  w=$(basename "$f" .parquet); w=${w#m_}
  if [[ "$w" < "$last_seeded" ]]; then week="$w"; break; fi
done
if [ -z "$week" ]; then
  log "DONE — no archive week older than cursor ($last_seeded); nothing to seed"
  exit 0
fi
# A missing players file can't be sampled — advance PAST this week (don't wedge
# on it forever) and let the next run pick the next-older week.
if [ ! -f "$ARCHIVE_DIR/p_$week.parquet" ]; then
  log "skip week $week — p_$week.parquet missing; advancing cursor past it"
  echo "$week" > "$CURSOR"
  exit 0
fi

# --- sample the week's ranked-RM matches with participant ids ------------------
SQL=/tmp/backfill-seed.sql
CSV=/tmp/backfill-seed.csv
cat > "$SQL" <<EOF
SET temp_directory='/tmp/duck_tmp';
COPY (
  WITH m AS (
    SELECT game_id, started_timestamp
    FROM read_parquet('$ARCHIVE_DIR/m_$week.parquet')
    WHERE leaderboard IN ('random_map', 'team_random_map') AND game_id IS NOT NULL
  ),
  sampled AS (
    SELECT game_id, started_timestamp FROM m ORDER BY hash(game_id) LIMIT $SAMPLE_PER_WEEK
  )
  SELECT CAST(s.game_id AS BIGINT) AS match_id,
         CAST(epoch(s.started_timestamp) AS BIGINT) AS played_at,
         string_agg(DISTINCT CAST(CAST(p.profile_id AS BIGINT) AS VARCHAR), ';') AS profile_ids
  FROM sampled s
  JOIN read_parquet('$ARCHIVE_DIR/p_$week.parquet') p USING (game_id)
  WHERE p.profile_id IS NOT NULL
  GROUP BY 1, 2
) TO '$CSV' (HEADER, DELIMITER ',');
EOF
mkdir -p /tmp/duck_tmp
# A duckdb ERROR (bad/corrupt parquet) is retryable — exit WITHOUT advancing so
# a transient failure retries the same week next run.
"$DUCKDB" < "$SQL" || { log "FAIL — duckdb sample query for week $week (will retry)"; exit 1; }
rows=$(($(wc -l < "$CSV") - 1))
# 0 rows is deterministic for this week (no RM matches with profile_ids) — that
# won't change on retry, so advance PAST it rather than wedge.
if [ "$rows" -le 0 ]; then
  log "week $week sampled 0 rows; advancing cursor past it"
  echo "$week" > "$CURSOR"
  exit 0
fi

"$REPLAY_BIN" seed "$CSV" --db "$MANIFEST" || { log "FAIL — replay-rs seed for week $week (will retry)"; exit 1; }
echo "$week" > "$CURSOR"
log "seeded $rows matches from week $week (cursor advanced)"
