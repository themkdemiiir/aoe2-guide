use sea_orm_migration::prelude::*;

/// `match_players` — one row per (match, player). `elo_bucket` is a `GENERATED ALWAYS AS
/// (...) STORED` column computed from `elo`; SeaORM's table-builder has no generated-column
/// primitive, so it's added via raw SQL after the plain columns land. The CASE below MUST
/// stay byte-for-byte identical to `pipeline/crates/core/src/elo.rs::elo_bucket` (and
/// `scripts/data-pipeline/lib/buckets.mjs::eloCaseSql`, NULL-passthrough variant): a NULL
/// elo (no post-game block) yields a NULL bucket, never a fabricated band.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("match_players"))
                    .col(ColumnDef::new(Alias::new("match_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("profile_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("civ_id")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("elo")).integer())
                    .col(ColumnDef::new(Alias::new("won")).boolean())
                    .col(ColumnDef::new(Alias::new("opening")).text())
                    .col(ColumnDef::new(Alias::new("feudal_t")).float())
                    .col(ColumnDef::new(Alias::new("castle_t")).float())
                    .col(ColumnDef::new(Alias::new("imperial_t")).float())
                    .primary_key(
                        Index::create()
                            .name("pk-match_players")
                            .col(Alias::new("match_id"))
                            .col(Alias::new("profile_id")),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-match_players-match_id")
                            .from(Alias::new("match_players"), Alias::new("match_id"))
                            .to(Alias::new("matches"), Alias::new("match_id"))
                            .on_delete(ForeignKeyAction::Cascade),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-match_players-civ_id")
                            .from(Alias::new("match_players"), Alias::new("civ_id"))
                            .to(Alias::new("civs"), Alias::new("civ_id")),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                r#"
                ALTER TABLE match_players ADD COLUMN elo_bucket TEXT GENERATED ALWAYS AS (
                    CASE
                        WHEN elo IS NULL THEN NULL
                        WHEN elo < 1000 THEN '<1000'
                        WHEN elo < 1200 THEN '1000-1199'
                        WHEN elo < 1400 THEN '1200-1399'
                        WHEN elo < 1650 THEN '1400-1649'
                        WHEN elo < 1800 THEN '1650-1799'
                        WHEN elo < 2000 THEN '1800-1999'
                        WHEN elo < 2200 THEN '2000-2199'
                        WHEN elo < 2500 THEN '2200-2499'
                        ELSE '2500+'
                    END
                ) STORED
                "#,
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-match_players-civ_id-elo_bucket")
                    .table(Alias::new("match_players"))
                    .col(Alias::new("civ_id"))
                    .col(Alias::new("elo_bucket"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Dropping the table takes the generated column, both FKs, the composite PK, and
        // the (civ_id, elo_bucket) index with it.
        manager
            .drop_table(Table::drop().table(Alias::new("match_players")).to_owned())
            .await
    }
}
