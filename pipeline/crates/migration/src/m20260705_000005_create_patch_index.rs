use sea_orm_migration::prelude::*;

/// `patch_index` — dimension table mapping a game build number to its human label and
/// (when known) release date. `released` is nullable: not every historical build's release
/// date is known.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("patch_index"))
                    .col(ColumnDef::new(Alias::new("build")).integer().primary_key())
                    .col(ColumnDef::new(Alias::new("label")).text().not_null())
                    .col(ColumnDef::new(Alias::new("released")).date())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("patch_index")).to_owned())
            .await
    }
}
