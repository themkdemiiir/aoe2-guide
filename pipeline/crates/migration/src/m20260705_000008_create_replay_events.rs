use sea_orm_migration::prelude::*;

/// `replay_events` — the raw per-event log parsed out of a replay. Deliberately no primary
/// key and no foreign keys yet: this is the highest-volume table (headed for ~100M rows) and
/// isn't partitioned yet either — partitioning is a separate step done right before that
/// load, not here. The one index is a partial index on `match_id` covering only
/// `kind IN ('train', 'research')`, the winner-meds hot path; SeaORM's `IndexCreateStatement`
/// has no partial-index (WHERE-predicate) primitive, so it's raw SQL.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("replay_events"))
                    .col(ColumnDef::new(Alias::new("match_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("profile_id")).big_integer())
                    .col(
                        ColumnDef::new(Alias::new("player_number"))
                            .small_integer()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("t_ms")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("kind")).text().not_null())
                    .col(ColumnDef::new(Alias::new("target_id")).big_integer())
                    .col(ColumnDef::new(Alias::new("amount")).big_integer())
                    .col(ColumnDef::new(Alias::new("detail")).text())
                    .to_owned(),
            )
            .await?;

        manager
            .get_connection()
            .execute_unprepared(
                r#"
                CREATE INDEX "idx-replay_events-match_id-train_research"
                    ON replay_events (match_id)
                    WHERE kind IN ('train', 'research')
                "#,
            )
            .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        // Dropping the table takes the partial index with it.
        manager
            .drop_table(Table::drop().table(Alias::new("replay_events")).to_owned())
            .await
    }
}
