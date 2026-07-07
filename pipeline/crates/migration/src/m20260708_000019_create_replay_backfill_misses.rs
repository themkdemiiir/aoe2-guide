use sea_orm_migration::prelude::*;

/// `replay_backfill_misses` — the durable "already tried, can't upgrade" skip-list for the
/// recent→old replay backfill (`pipeline backfill`; see that module's doc). The backfill walks
/// `source='aoestats'` matches newest→oldest, downloads each match's real replay from the age
/// archive, and UPGRADES it to a rich `source='replay'` row. A SUCCESS flips the row's `source`,
/// so it naturally drops out of the backfill's discovery `WHERE source='aoestats'` — no marker
/// needed. A TERMINAL FAILURE is the case this table exists for: the archive has no copy
/// (`not_found` — the rolling ~12-month retention window has aged the replay out, or no
/// participant ever uploaded one), the ZIP is structurally unusable (`bad_payload`), the
/// `.aoe2record` won't parse (`parse_failed`), or the parsed replay can't compose into a batch
/// (`compose_failed`). Those rows STAY `source='aoestats'` (their aggregate data is still valid —
/// we simply couldn't enrich them), so without this skip-list the discovery query would re-select
/// and re-download them on every run forever. TRANSIENT failures (a surviving 429, a network
/// blip) are deliberately NOT recorded here — they must be retried on a later run.
///
/// State lives entirely in Postgres (this table + the `source` flip), NOT in a local SQLite
/// manifest — so the backfill container is stateless and needs no persistent volume for its
/// resume cursor (see `pipeline::backfill`'s module doc).
///
/// `reason` is a CHECK-closed vocabulary rather than a free TEXT column (the pipeline's
/// hard-constrain-where-you-can rule) — a fifth outcome must be added here deliberately, it can't
/// slip in as an unvalidated string. `match_id` FKs to `matches(match_id)` with `ON DELETE
/// CASCADE`: a miss only ever exists for a match we just read out of `matches`, and if that match
/// is ever removed the miss should go with it (never dangle).
#[derive(DeriveMigrationName)]
pub struct Migration;

const CREATE_SQL: &str = "\
CREATE TABLE replay_backfill_misses (\
    match_id BIGINT PRIMARY KEY REFERENCES matches(match_id) ON DELETE CASCADE, \
    reason TEXT NOT NULL CHECK (reason IN ('not_found', 'bad_payload', 'parse_failed', 'compose_failed')), \
    attempted_at TIMESTAMPTZ NOT NULL DEFAULT now()\
)";

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared(CREATE_SQL)
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .get_connection()
            .execute_unprepared("DROP TABLE replay_backfill_misses")
            .await?;
        Ok(())
    }
}
