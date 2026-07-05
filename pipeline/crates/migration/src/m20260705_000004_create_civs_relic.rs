use sea_orm_migration::prelude::*;

/// `civs_relic` — dimension table in the Relic API's own `civilization_id` space. `slug` is
/// deliberately NOT unique: the Relic id->slug mapping is era-scoped (`valid_from`), so the
/// same slug can legitimately reappear under a different id across eras.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("civs_relic"))
                    .col(ColumnDef::new(Alias::new("civ_id")).integer().primary_key())
                    .col(ColumnDef::new(Alias::new("slug")).text().not_null())
                    .col(ColumnDef::new(Alias::new("valid_from")).date().not_null())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("civs_relic")).to_owned())
            .await
    }
}
