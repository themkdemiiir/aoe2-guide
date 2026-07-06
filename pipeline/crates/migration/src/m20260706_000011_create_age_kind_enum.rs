use sea_orm_migration::prelude::*;

/// Promotes `replay_ages.age`/`match_ages.age` from plain `TEXT` to a native `age_kind` enum —
/// same raw-SQL convention as `m20260705_000001_create_enums.rs` (`source_kind`/`ladder_kind`/
/// `map_family`): SeaORM has no cleaner enum-define-then-alter flow than raw SQL. Verified against
/// the live DB before this migration was written (60,044,792 `match_ages` rows, all four values;
/// `replay_ages` empty) — see `pipeline_core::age::Age::as_db_str` for the vocabulary source of
/// truth. `('dark', 'feudal', 'castle', 'imperial')` — order is game progression, not alphabetical.
///
/// Also DROPS (never recreates) the `benchmark_vils` view around the `ALTER`s: the live DB's
/// `benchmark_vils` view (`pipeline/dbt/models/benchmark_vils.sql`, materialized by `dbt run` —
/// NOT created by any SeaORM migration) reads `match_ages.age` via plain `age IN ('dark',
/// 'feudal', 'castle')`/`age = 'castle'` comparisons, and Postgres refuses `ALTER COLUMN … TYPE`
/// on a column any view depends on ("cannot alter type of a column used by a view or rule") —
/// discovered live when this migration's `up` was first run against the real DB (not anticipated
/// by the task brief, which only listed `migration`/`aoestats`/`ingest` in scope). `DROP VIEW IF
/// EXISTS` (not a bare `DROP VIEW`) so this migration also applies cleanly on a from-scratch DB
/// that hasn't had `dbt run` yet (the testcontainers suites, and a brand-new deploy before its
/// first dbt run) — there, the view simply doesn't exist yet, so the drop is a no-op.
///
/// Neither `up` nor `down` recreates the view — a later `dbt run` (which follows migrations in
/// the deploy sequence, and is re-run after any rollback too) recreates the identical view over
/// whichever column type is live at that point, since a bare string-literal `IN`/`=` comparison
/// resolves the same way against `age_kind` as it does against `TEXT` (Postgres types the literal
/// from the left-hand operand either way). This is deliberate, not an oversight: an EARLIER
/// version of this migration recreated the view in both `up` and `down`, which meant `down`
/// left `benchmark_vils` depending on `match_ages` — so a multi-step rollback crossing below this
/// migration (`refresh`/`reset`/`down -n>=2`) hit `m20260706_000010`'s `DROP TABLE match_ages` and
/// aborted with the same "view depends on table" error this migration exists to avoid. Dropping
/// without recreating breaks that chain: nothing queries `benchmark_vils` between `migrate` and
/// `dbt run` (only the `export` crate reads it, strictly post-dbt), so there is no window where
/// its absence matters. Note the same posture applies to `replay_ages` — harmless either way,
/// since the view reads `match_ages`, never `replay_ages`.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    /// Drops the dbt-owned `benchmark_vils` view (so the `ALTER COLUMN … TYPE` below isn't blocked
    /// by its dependency on `match_ages.age`), promotes both `age` columns to `age_kind`, and
    /// deliberately does NOT recreate the view — see the module doc for why.
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP VIEW IF EXISTS benchmark_vils")
            .await?;
        db.execute_unprepared(
            "CREATE TYPE age_kind AS ENUM ('dark', 'feudal', 'castle', 'imperial')",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE replay_ages ALTER COLUMN age TYPE age_kind USING age::age_kind",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE match_ages ALTER COLUMN age TYPE age_kind USING age::age_kind",
        )
        .await?;
        Ok(())
    }

    /// Drops the dbt-owned `benchmark_vils` view (so the revert `ALTER COLUMN … TYPE` below isn't
    /// blocked by its dependency on `match_ages.age`), reverts both `age` columns to `TEXT`, and
    /// deliberately does NOT recreate the view — see the module doc for why: recreating it here
    /// left it depending on `match_ages`, which broke `m20260706_000010`'s `DROP TABLE match_ages`
    /// on any rollback that continues past this migration (`refresh`/`reset`/`down -n>=2`).
    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP VIEW IF EXISTS benchmark_vils")
            .await?;
        db.execute_unprepared(
            "ALTER TABLE match_ages ALTER COLUMN age TYPE TEXT USING age::text",
        )
        .await?;
        db.execute_unprepared(
            "ALTER TABLE replay_ages ALTER COLUMN age TYPE TEXT USING age::text",
        )
        .await?;
        db.execute_unprepared("DROP TYPE age_kind").await?;
        Ok(())
    }
}
