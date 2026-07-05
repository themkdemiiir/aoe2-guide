use sea_orm_migration::prelude::*;

/// `civs` — dimension table in the GAME/replay `civ_id` space (see
/// `src/data/civ-id-map.json` on the Astro side; never confuse with `civs_relic`, which is
/// the Relic API's own id space).
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("civs"))
                    .col(ColumnDef::new(Alias::new("civ_id")).integer().primary_key())
                    .col(
                        ColumnDef::new(Alias::new("slug"))
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("civs")).to_owned())
            .await
    }
}
