use sea_orm_migration::prelude::*;

/// `match_players.apm` — Phase C of the replay-analytics enrichment (`task-enrichC`): one
/// commands-per-minute scalar per player, derived from a replay's own event count
/// (`replay::derive::apm`). **Replay-source ONLY** — aoestats' archive carries no per-action data,
/// so its `match_players` rows never populate this column and it stays `NULL` there (honest
/// absence, not a fabricated value). A replay player who issued zero commands still gets
/// `Some(0.0)`, never `NULL` — see `replay::derive`'s module doc.
///
/// `ALTER TABLE ... ADD COLUMN` (nullable, no default) is metadata-only on Postgres — no table
/// rewrite, instant even on the 108M-row `match_players` table. `REAL` (f32) matches
/// `feudal_t`/`castle_t`/`imperial_t`, all added the same way and all `.float()` here.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("match_players"))
                    .add_column(ColumnDef::new(Alias::new("apm")).float())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .alter_table(
                Table::alter()
                    .table(Alias::new("match_players"))
                    .drop_column(Alias::new("apm"))
                    .to_owned(),
            )
            .await
    }
}
