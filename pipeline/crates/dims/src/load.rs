//! The idempotent dims-load algorithm.
//!
//! Each dimension table gets ONE `INSERT ... ON CONFLICT (pk) DO UPDATE`, built from a
//! `UNNEST`-ed multi-row column list rather than a per-row `INSERT` loop or the binary-COPY
//! staging-table machinery `ingest` uses for its 100M-row facts — these tables are tiny (maps
//! ~150 rows, civs/civs_relic ~50, patch_index ~20, units ~238, techs ~192), so one round trip
//! per table is plenty, and `ON CONFLICT DO UPDATE` means re-running this after a refdata change
//! (a new DLC civ, a map rename, a new-DLC unit) refreshes every row's slug/label/family/name in
//! place. All six loads share ONE transaction: a failure partway through (e.g. a malformed
//! refdata date) rolls back everything, never leaving the dims half-loaded.

use anyhow::{Context, Result};
use chrono::NaiveDate;
use pipeline_core::civs::{GameCivMap, RelicCivMap};
use pipeline_core::maps::MapTable;
use pipeline_core::patch::PatchBuild;
use pipeline_core::techs::TechTable;
use pipeline_core::units::UnitTable;
use tokio_postgres::types::ToSql;
use tokio_postgres::{Client, Transaction};

/// Row counts loaded (inserted-or-updated) by [`load_dims`] into each dimension table.
#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct DimsStats {
    pub maps: u64,
    pub civs: u64,
    pub civs_relic: u64,
    pub patch_index: u64,
    pub units: u64,
    pub techs: u64,
}

const UPSERT_MAPS_SQL: &str = r#"
INSERT INTO maps (map_id, name, slug, family)
SELECT map_id, name, slug, family::map_family
FROM UNNEST($1::integer[], $2::text[], $3::text[], $4::text[]) AS t(map_id, name, slug, family)
ON CONFLICT (map_id) DO UPDATE SET
    name = EXCLUDED.name,
    slug = EXCLUDED.slug,
    family = EXCLUDED.family
"#;

const UPSERT_CIVS_SQL: &str = r#"
INSERT INTO civs (civ_id, slug)
SELECT civ_id, slug
FROM UNNEST($1::integer[], $2::text[]) AS t(civ_id, slug)
ON CONFLICT (civ_id) DO UPDATE SET slug = EXCLUDED.slug
"#;

const UPSERT_CIVS_RELIC_SQL: &str = r#"
INSERT INTO civs_relic (civ_id, slug, valid_from)
SELECT civ_id, slug, valid_from
FROM UNNEST($1::integer[], $2::text[], $3::date[]) AS t(civ_id, slug, valid_from)
ON CONFLICT (civ_id) DO UPDATE SET
    slug = EXCLUDED.slug,
    valid_from = EXCLUDED.valid_from
"#;

const UPSERT_PATCH_INDEX_SQL: &str = r#"
INSERT INTO patch_index (build, label, released)
SELECT build, label, released
FROM UNNEST($1::integer[], $2::text[], $3::date[]) AS t(build, label, released)
ON CONFLICT (build) DO UPDATE SET
    label = EXCLUDED.label,
    released = EXCLUDED.released
"#;

const UPSERT_UNITS_SQL: &str = r#"
INSERT INTO units (unit_id, name, internal_name)
SELECT unit_id, name, internal_name
FROM UNNEST($1::integer[], $2::text[], $3::text[]) AS t(unit_id, name, internal_name)
ON CONFLICT (unit_id) DO UPDATE SET
    name = EXCLUDED.name,
    internal_name = EXCLUDED.internal_name
"#;

const UPSERT_TECHS_SQL: &str = r#"
INSERT INTO techs (tech_id, name, internal_name)
SELECT tech_id, name, internal_name
FROM UNNEST($1::integer[], $2::text[], $3::text[]) AS t(tech_id, name, internal_name)
ON CONFLICT (tech_id) DO UPDATE SET
    name = EXCLUDED.name,
    internal_name = EXCLUDED.internal_name
"#;

/// Idempotently loads every reference dimension into `client`'s database from the committed
/// refdata: `maps` from [`pipeline_core::maps::load`], `civs` from
/// [`pipeline_core::civs::load_game_civs`], `civs_relic` from
/// [`pipeline_core::civs::load_relic_civs`], `patch_index` from [`pipeline_core::patch::load`],
/// `units` from [`pipeline_core::units::load_units`], and `techs` from
/// [`pipeline_core::techs::load_techs`]. Re-running with unchanged refdata leaves every table's
/// row count and contents unchanged (`ON CONFLICT DO UPDATE` with the same values is a no-op
/// write).
///
/// # Errors
/// Any DB error, a malformed refdata JSON file (`civs`/`civs_relic`/`patch_index`/`units`/`techs`
/// — `core`'s loaders return `Result`, never panic; see the playbook's "no panic in a pub lib fn"
/// rule), or a malformed refdata date (`civs_relic.valid_from` / `patch_index.released`) — the
/// whole transaction rolls back on drop, so nothing partial is ever committed.
pub async fn load_dims(client: &mut Client) -> Result<DimsStats> {
    let tx = client
        .transaction()
        .await
        .context("failed to begin dims-load transaction")?;

    let maps = upsert_maps(&tx, &pipeline_core::maps::load()).await?;
    let game_civs =
        pipeline_core::civs::load_game_civs().context("failed to load civ-id-map.json")?;
    let civs = upsert_civs(&tx, &game_civs).await?;
    let relic_civs =
        pipeline_core::civs::load_relic_civs().context("failed to load relic-civ-id-map.json")?;
    let civs_relic = upsert_civs_relic(&tx, &relic_civs).await?;
    let patch_builds = pipeline_core::patch::load().context("failed to load patch-index.json")?;
    let patch_index = upsert_patch_index(&tx, &patch_builds).await?;
    let unit_table =
        pipeline_core::units::load_units().context("failed to load unit-names.json")?;
    let units = upsert_units(&tx, &unit_table).await?;
    let tech_table =
        pipeline_core::techs::load_techs().context("failed to load tech-names.json")?;
    let techs = upsert_techs(&tx, &tech_table).await?;

    tx.commit()
        .await
        .context("failed to commit dims-load transaction")?;

    let stats = DimsStats {
        maps,
        civs,
        civs_relic,
        patch_index,
        units,
        techs,
    };
    tracing::info!(?stats, "dims load committed");
    Ok(stats)
}

async fn upsert_maps(tx: &Transaction<'_>, table: &MapTable) -> Result<u64> {
    let mut map_ids = Vec::new();
    let mut names = Vec::new();
    let mut slugs = Vec::new();
    let mut families = Vec::new();
    for (id, info) in table.iter() {
        // Map ids are always small non-negative numbers in practice (< 300 today) — same
        // reasoning `core::ids` uses for the signed id newtypes it hands to Postgres.
        map_ids.push(id as i32);
        names.push(info.name.clone());
        slugs.push(info.slug.clone());
        families.push(info.family.as_db_str().to_owned());
    }

    let params: [&(dyn ToSql + Sync); 4] = [&map_ids, &names, &slugs, &families];
    tx.execute(UPSERT_MAPS_SQL, &params)
        .await
        .context("failed to upsert maps")
}

async fn upsert_civs(tx: &Transaction<'_>, civs: &GameCivMap) -> Result<u64> {
    let mut civ_ids = Vec::new();
    let mut slugs = Vec::new();
    for (id, slug) in civs.entries() {
        civ_ids.push(id);
        slugs.push(slug.to_owned());
    }

    let params: [&(dyn ToSql + Sync); 2] = [&civ_ids, &slugs];
    tx.execute(UPSERT_CIVS_SQL, &params)
        .await
        .context("failed to upsert civs")
}

async fn upsert_civs_relic(tx: &Transaction<'_>, relic: &RelicCivMap) -> Result<u64> {
    let valid_from = parse_iso_date(relic.valid_from()).with_context(|| {
        format!(
            "relic-civ-id-map.json provenance.validFrom {:?} is not a YYYY-MM-DD date",
            relic.valid_from()
        )
    })?;

    let mut civ_ids = Vec::new();
    let mut slugs = Vec::new();
    for (id, slug) in relic.entries() {
        civ_ids.push(id);
        slugs.push(slug.to_owned());
    }
    let valid_froms = vec![valid_from; civ_ids.len()];

    let params: [&(dyn ToSql + Sync); 3] = [&civ_ids, &slugs, &valid_froms];
    tx.execute(UPSERT_CIVS_RELIC_SQL, &params)
        .await
        .context("failed to upsert civs_relic")
}

async fn upsert_patch_index(tx: &Transaction<'_>, builds: &[PatchBuild]) -> Result<u64> {
    let mut build_ids = Vec::new();
    let mut labels = Vec::new();
    let mut releaseds: Vec<Option<NaiveDate>> = Vec::new();
    for b in builds {
        build_ids.push(b.build);
        labels.push(b.label.clone());
        let released = b
            .released
            .as_deref()
            .map(parse_iso_date)
            .transpose()
            .with_context(|| {
                format!(
                    "patch-index.json build {} has a malformed released date",
                    b.build
                )
            })?;
        releaseds.push(released);
    }

    let params: [&(dyn ToSql + Sync); 3] = [&build_ids, &labels, &releaseds];
    tx.execute(UPSERT_PATCH_INDEX_SQL, &params)
        .await
        .context("failed to upsert patch_index")
}

async fn upsert_units(tx: &Transaction<'_>, units: &UnitTable) -> Result<u64> {
    let mut unit_ids = Vec::new();
    let mut names = Vec::new();
    let mut internal_names: Vec<Option<String>> = Vec::new();
    for (id, info) in units.entries() {
        unit_ids.push(id);
        names.push(info.name.clone());
        internal_names.push(info.internal_name.clone());
    }

    let params: [&(dyn ToSql + Sync); 3] = [&unit_ids, &names, &internal_names];
    tx.execute(UPSERT_UNITS_SQL, &params)
        .await
        .context("failed to upsert units")
}

async fn upsert_techs(tx: &Transaction<'_>, techs: &TechTable) -> Result<u64> {
    let mut tech_ids = Vec::new();
    let mut names = Vec::new();
    let mut internal_names: Vec<Option<String>> = Vec::new();
    for (id, info) in techs.entries() {
        tech_ids.push(id);
        names.push(info.name.clone());
        internal_names.push(info.internal_name.clone());
    }

    let params: [&(dyn ToSql + Sync); 3] = [&tech_ids, &names, &internal_names];
    tx.execute(UPSERT_TECHS_SQL, &params)
        .await
        .context("failed to upsert techs")
}

/// Parses a refdata `YYYY-MM-DD` string into a [`NaiveDate`]. Fails loud (never guesses/defaults)
/// on anything else, per the crate's "no defaults, fail loud" rule.
fn parse_iso_date(s: &str) -> Result<NaiveDate> {
    NaiveDate::parse_from_str(s, "%Y-%m-%d")
        .with_context(|| format!("{s:?} is not a YYYY-MM-DD date"))
}
