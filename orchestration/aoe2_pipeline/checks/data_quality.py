"""The plan's data-quality gate: no gameplay match may be missing its map, and post-game elo
coverage must stay high. `matches.source_kind` is `ENUM ('replay', 'aoestats')` and
`matches.ladder_kind` is `ENUM ('1v1', 'team')` (see
`pipeline/crates/migration/src/m20260705_000001_create_enums.rs`) — there is no
unranked/custom-game source in this schema, so every row is a ranked-ladder gameplay match and
the invariant applies to the whole `matches`/`match_players` tables, not a filtered subset.

Attached to `aoestats_import` because that's the CURRENT sole writer of these tables in the live
DB (see `assets/ingest_assets.py`); the check itself queries the tables directly, so it validates
the same invariant regardless of which asset produced the rows — it will keep meaning the same
thing once `replay_backfill` starts writing `source = 'replay'` rows too.
"""

from dagster import AssetCheckExecutionContext, AssetCheckResult, asset_check

from ..assets.ingest_assets import aoestats_import
from ..resources import PostgresResource

# `pipeline_core::Elo`'s smart constructor keeps `None` as a legitimate "unknown post-game block"
# state (see pipeline/docs/rust-playbook.md) — a hard 0-tolerance elo check would be too brittle
# against that. 99% leaves headroom for genuinely-unknown rows while still catching a real
# ingestion regression; the live DB is 100% covered today (see task-M6-dagster-report.md).
ELO_COVERAGE_MIN_RATIO = 0.99


@asset_check(
    asset=aoestats_import,
    name="no_gameplay_match_missing_map_or_elo",
    description=(
        "matches.map_id must never be NULL (also FK/NOT NULL-enforced at the schema level — this "
        "check re-asserts the invariant at the data-quality layer); match_players.elo coverage "
        "must stay >= 99%."
    ),
)
def no_gameplay_match_missing_map_or_elo(
    context: AssetCheckExecutionContext, postgres: PostgresResource
) -> AssetCheckResult:
    with postgres.get_connection() as conn, conn.cursor() as cur:
        cur.execute("SELECT count(*) FROM matches WHERE map_id IS NULL")
        (missing_map,) = cur.fetchone()

        cur.execute("SELECT count(*) FILTER (WHERE elo IS NULL), count(*) FROM match_players")
        missing_elo, total_players = cur.fetchone()

    elo_coverage = 1.0 if total_players == 0 else 1 - (missing_elo / total_players)
    passed = missing_map == 0 and elo_coverage >= ELO_COVERAGE_MIN_RATIO

    return AssetCheckResult(
        passed=passed,
        metadata={
            "matches_missing_map_id": missing_map,
            "match_players_missing_elo": missing_elo,
            "match_players_total": total_players,
            "elo_coverage_ratio": round(elo_coverage, 4),
            "elo_coverage_min_ratio": ELO_COVERAGE_MIN_RATIO,
        },
    )
