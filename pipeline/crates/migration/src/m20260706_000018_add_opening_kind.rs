use sea_orm_migration::prelude::*;

/// `match_players.opening_kind` — closes final-review finding #1: `match_players.opening` (kept,
/// unchanged) is free TEXT written by two producers with un-reconciled vocabularies (aoestats'
/// lowercase archive labels vs. `replay::derive::classify_opening`'s title-case tags/composites),
/// so `GROUP BY opening` (`civ_meta_openings.sql`) silently splinters the SAME real strategy across
/// rows the moment both sources coexist. This migration adds a second, CLOSED `opening_kind`
/// column both producers cast onto — see `pipeline_core::opening`'s module doc for the full
/// vocabulary/reconciliation trail (the verified live-DB aoestats label set, the aoestats-verbatim
/// renames, and the deliberately-NOT-asserted `trash`/`spears`/`skirms` equivalence).
///
/// Same raw-SQL convention as `m20260705_000001_create_enums.rs`/
/// `m20260706_000011_create_age_kind_enum.rs`: SeaORM has no cleaner enum-define-then-alter flow
/// than raw SQL. Twelve values: the six `replay`-only unit-line tags (`scouts`/`archers`/
/// `skirms`/`eagles`/`man_at_arms`/`spears`), `fast_castle`/`drush` (real strategies BOTH
/// producers name, once aoestats' own spelling is reused), and the four `aoestats`-only labels
/// (`trash`/`fires`/`galleys`/`towers`) that have no replay-side equivalent yet.
///
/// **Backfill:** verified against the live DB before this migration was written — 108,302,508
/// `match_players` rows, all `source = 'aoestats'` today (`replay` is empty); 93,291,310 have
/// `opening IS NULL` (untouched, stay `opening_kind IS NULL` too); of the 15,011,198 non-NULL
/// rows, exactly 9 distinct labels appear, matching this migration's `CASE` arms one-for-one
/// (`SELECT opening, count(*) FROM match_players WHERE opening IS NOT NULL GROUP BY opening`):
/// `fast_castle` 4,200,019 · `unknown` 3,589,570 · `scouts` 2,117,242 · `archers` 1,974,620 ·
/// `trash` 1,099,506 · `man_at_arms` 1,092,875 · `fires` 360,897 · `drush` 238,166 · `galleys`
/// 179,940 · `towers` 158,363. `unknown` (aoestats' own "tried, couldn't classify" bucket — the
/// SAME honest non-classification `classify_opening` represents as `None`) has NO `CASE` arm on
/// purpose, so it falls through to `NULL` — an explicit, documented choice, not a missed label
/// (see `pipeline_core::opening`'s module doc). `skirms`/`eagles`/`spears` have no `CASE` arm
/// either — aoestats never emits them, only the (currently empty) `replay` source will.
///
/// Reversible: `down` drops the column then the type. Also DROPS (never recreates) the dbt-owned
/// `civ_meta_openings` view around the `down`'s `ALTER` — same posture, and for the identical
/// rollback-chain reason, as `m20260706_000011_create_age_kind_enum.rs`'s `benchmark_vils` guard:
/// once a later `dbt run` has repointed that view at `mp.opening_kind`, a bare `DROP COLUMN` here
/// would abort with "cannot drop column ... other objects depend on it" mid-rollback. `up` needs
/// no such guard — adding a nullable column never breaks a dependent view.
#[derive(DeriveMigrationName)]
pub struct Migration;

const CREATE_TYPE_SQL: &str = "CREATE TYPE opening_kind AS ENUM (\
     'scouts', 'archers', 'skirms', 'eagles', 'man_at_arms', 'spears', \
     'fast_castle', 'drush', 'trash', 'fires', 'galleys', 'towers')";

const ADD_COLUMN_SQL: &str = "ALTER TABLE match_players ADD COLUMN opening_kind opening_kind";

/// Every arm's `WHEN`/`THEN` literal is identical (aoestats' label IS the enum value — see the
/// module doc for why that's mechanical, not a guess) — kept as an explicit `CASE`, not a bare
/// `::opening_kind` cast, so a genuinely new/unexpected future label fails SAFE into `NULL`
/// (never a runtime cast error aborting the whole backfill) instead of merely happening to match.
const BACKFILL_SQL: &str = r#"
UPDATE match_players
SET opening_kind = (
    CASE opening
        WHEN 'scouts' THEN 'scouts'
        WHEN 'archers' THEN 'archers'
        WHEN 'man_at_arms' THEN 'man_at_arms'
        WHEN 'fast_castle' THEN 'fast_castle'
        WHEN 'drush' THEN 'drush'
        WHEN 'trash' THEN 'trash'
        WHEN 'fires' THEN 'fires'
        WHEN 'galleys' THEN 'galleys'
        WHEN 'towers' THEN 'towers'
        -- 'unknown' (aoestats' own non-classification) and any unrecognized future label both
        -- fall through here to an honest NULL — never a guessed enum member.
        ELSE NULL
    END
)::opening_kind
WHERE opening IS NOT NULL
"#;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared(CREATE_TYPE_SQL).await?;
        db.execute_unprepared(ADD_COLUMN_SQL).await?;
        db.execute_unprepared(BACKFILL_SQL).await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP VIEW IF EXISTS civ_meta_openings")
            .await?;
        db.execute_unprepared("ALTER TABLE match_players DROP COLUMN opening_kind")
            .await?;
        db.execute_unprepared("DROP TYPE opening_kind").await?;
        Ok(())
    }
}
