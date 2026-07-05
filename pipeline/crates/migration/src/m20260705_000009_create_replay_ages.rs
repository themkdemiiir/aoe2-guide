use sea_orm_migration::prelude::*;

/// `replay_ages` — one row per (match, player, age) age-up summary parsed from a replay.
/// Same posture as `replay_events`: no primary key, no foreign keys, not partitioned yet.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("replay_ages"))
                    .col(ColumnDef::new(Alias::new("match_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("profile_id")).big_integer().not_null())
                    .col(ColumnDef::new(Alias::new("civ_id")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("won")).boolean())
                    .col(ColumnDef::new(Alias::new("age")).text().not_null())
                    .col(ColumnDef::new(Alias::new("uptime_ms")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("villagers")).integer())
                    .col(ColumnDef::new(Alias::new("military")).integer())
                    .col(ColumnDef::new(Alias::new("n_buildings")).integer())
                    .col(ColumnDef::new(Alias::new("n_research")).integer())
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("replay_ages")).to_owned())
            .await
    }
}
