use sea_orm_migration::prelude::*;

/// `maps` — dimension table: game-space map ids, slugs, and their gameplay family.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("maps"))
                    .col(ColumnDef::new(Alias::new("map_id")).integer().primary_key())
                    .col(ColumnDef::new(Alias::new("name")).text().not_null())
                    .col(
                        ColumnDef::new(Alias::new("slug"))
                            .text()
                            .not_null()
                            .unique_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("family"))
                            .custom(Alias::new("map_family"))
                            .not_null(),
                    )
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("maps")).to_owned())
            .await
    }
}
