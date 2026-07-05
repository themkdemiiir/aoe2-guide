use sea_orm_migration::prelude::*;

/// `matches` — one row per game. `build`/`patch` are plain columns (no FK to `patch_index`
/// yet); the only enforced FK here is `map_id -> maps(map_id)`.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .create_table(
                Table::create()
                    .table(Alias::new("matches"))
                    .col(
                        ColumnDef::new(Alias::new("match_id"))
                            .big_integer()
                            .primary_key(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("source"))
                            .custom(Alias::new("source_kind"))
                            .not_null(),
                    )
                    .col(
                        ColumnDef::new(Alias::new("ladder"))
                            .custom(Alias::new("ladder_kind"))
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("map_id")).integer().not_null())
                    .col(ColumnDef::new(Alias::new("build")).integer())
                    .col(ColumnDef::new(Alias::new("patch")).text())
                    .col(
                        ColumnDef::new(Alias::new("played_at"))
                            .timestamp_with_time_zone()
                            .not_null(),
                    )
                    .col(ColumnDef::new(Alias::new("duration_ms")).integer())
                    .col(
                        ColumnDef::new(Alias::new("n_players"))
                            .small_integer()
                            .not_null(),
                    )
                    .foreign_key(
                        ForeignKey::create()
                            .name("fk-matches-map_id")
                            .from(Alias::new("matches"), Alias::new("map_id"))
                            .to(Alias::new("maps"), Alias::new("map_id")),
                    )
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-matches-played_at")
                    .table(Alias::new("matches"))
                    .col(Alias::new("played_at"))
                    .to_owned(),
            )
            .await?;

        manager
            .create_index(
                Index::create()
                    .name("idx-matches-map_id-ladder")
                    .table(Alias::new("matches"))
                    .col(Alias::new("map_id"))
                    .col(Alias::new("ladder"))
                    .to_owned(),
            )
            .await
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        manager
            .drop_table(Table::drop().table(Alias::new("matches")).to_owned())
            .await
    }
}
