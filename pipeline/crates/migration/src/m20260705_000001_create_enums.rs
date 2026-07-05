use sea_orm_migration::prelude::*;

/// The three Postgres-native enums backing `matches.source`, `matches.ladder`, and
/// `maps.family`. Plain `CREATE TYPE ... AS ENUM (...)` — SeaORM's schema builder has no
/// first-class "define this enum, then reference it as a column type" flow that's simpler
/// than raw SQL here, so this stays raw SQL for the same reason the generated `elo_bucket`
/// column later in this migrator does.
#[derive(DeriveMigrationName)]
pub struct Migration;

#[async_trait::async_trait]
impl MigrationTrait for Migration {
    async fn up(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("CREATE TYPE source_kind AS ENUM ('replay', 'aoestats')")
            .await?;
        db.execute_unprepared("CREATE TYPE ladder_kind AS ENUM ('1v1', 'team')")
            .await?;
        db.execute_unprepared(
            "CREATE TYPE map_family AS ENUM \
             ('open', 'closed', 'hybrid', 'water', 'nomad', 'special', 'other')",
        )
        .await?;
        Ok(())
    }

    async fn down(&self, manager: &SchemaManager) -> Result<(), DbErr> {
        let db = manager.get_connection();
        db.execute_unprepared("DROP TYPE map_family").await?;
        db.execute_unprepared("DROP TYPE ladder_kind").await?;
        db.execute_unprepared("DROP TYPE source_kind").await?;
        Ok(())
    }
}
