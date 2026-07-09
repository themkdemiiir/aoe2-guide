//! [`DimIds`] + [`filter_to_known_dims`] + [`merge_into`] — the FK-safety and batch-merge helpers
//! shared by the two `source='replay'` bulk producers: [`crate::import_shards`] (the one-time
//! old-corpus migration) and [`crate::backfill`] (the ongoing recent→old archive upgrade). Both
//! derive `match_player_units`/`match_player_techs` rows via `replay::derive`, and
//! `match_player_units.unit_id` FK-references the `units` dim — so a `train` of a unit the live dim
//! doesn't list (verified real in this corpus: game unit ids **37** and **1570**, absent from the
//! 238-unit `aoe2techtree` dim) would abort the WHOLE batch's transaction, taking down every good
//! match batched alongside it. Single-sourced here rather than duplicated per producer so extending
//! the dim (or the filter policy) is a one-place change.

use std::collections::{BTreeSet, HashSet};

use anyhow::{Context, Result};
use ingest::ReplayBatch;
use tokio_postgres::Client;

/// The live `units`/`techs` dimension id sets, loaded once so [`filter_to_known_dims`] can drop
/// FK-unsafe child rows without a per-row round trip.
pub struct DimIds {
    pub units: HashSet<i32>,
    pub techs: HashSet<i32>,
}

/// Loads the `units`/`techs` dim id sets from the live schema (tiny — ~238 units, ~192 techs).
pub async fn load_dim_ids(client: &Client) -> Result<DimIds> {
    let units = client
        .query("SELECT unit_id FROM units", &[])
        .await
        .context("failed to load units dim ids")?
        .into_iter()
        .map(|r| r.get::<_, i32>(0))
        .collect();
    let techs = client
        .query("SELECT tech_id FROM techs", &[])
        .await
        .context("failed to load techs dim ids")?
        .into_iter()
        .map(|r| r.get::<_, i32>(0))
        .collect();
    Ok(DimIds { units, techs })
}

/// What [`filter_to_known_dims`] dropped from one batch — folded into each producer's own summary
/// (`ImportShardsSummary` / `BackfillSummary`). Never fabricated into a row; the DISTINCT unknown
/// ids are surfaced so an operator can decide whether to extend the dim.
#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct DimFilterCounts {
    pub dropped_unit_rows: u64,
    pub dropped_tech_rows: u64,
    pub unknown_unit_ids: BTreeSet<i32>,
    pub unknown_tech_ids: BTreeSet<i32>,
}

/// Drops `player_units`/`player_techs` rows whose id isn't in the live dim, returning the counts +
/// the DISTINCT unknown ids seen. `match_player_units.unit_id` FK-references `units`, so a `train`
/// of a unit the dim doesn't list would abort the whole batch's transaction — taking down every
/// good match batched with it. Rather than fail those matches over unattributable unit ids, this
/// drops ONLY the offending child rows (the match itself, its KNOWN units, and all other enrichment
/// still import) and reports the ids. This is the FK-safe analog of the pipeline's "skip + count,
/// never fabricate" rule at row granularity — NOT a silent drop (every dropped id is surfaced in
/// the caller's summary). Techs are filtered too for symmetry, though `replay::derive` only ever
/// emits the fixed watched-tech set (all standard dim entries), so `unknown_tech_ids` is expected
/// to stay empty.
pub fn filter_to_known_dims(batch: &mut ReplayBatch, dims: &DimIds) -> DimFilterCounts {
    let mut counts = DimFilterCounts::default();

    let before_u = batch.player_units.len();
    batch.player_units.retain(|u| {
        let known = dims.units.contains(&u.unit_id.0);
        if !known {
            counts.unknown_unit_ids.insert(u.unit_id.0);
        }
        known
    });
    counts.dropped_unit_rows = (before_u - batch.player_units.len()) as u64;

    let before_t = batch.player_techs.len();
    batch.player_techs.retain(|t| {
        let known = dims.techs.contains(&t.tech_id.0);
        if !known {
            counts.unknown_tech_ids.insert(t.tech_id.0);
        }
        known
    });
    counts.dropped_tech_rows = (before_t - batch.player_techs.len()) as u64;

    counts
}

/// Appends every row of `batch` onto `pending` — the accumulator both producers use to build up
/// `batch_size` matches before committing one `ingest_batch*` transaction.
pub fn merge_into(pending: &mut ReplayBatch, batch: ReplayBatch) {
    pending.matches.extend(batch.matches);
    pending.players.extend(batch.players);
    pending.events.extend(batch.events);
    pending.ages.extend(batch.ages);
    pending.player_units.extend(batch.player_units);
    pending.player_techs.extend(batch.player_techs);
}

#[cfg(test)]
mod tests {
    use ingest::{NewMatchPlayerTech, NewMatchPlayerUnit};
    use pipeline_core::{GameUnitId, MatchId, ProfileId, TechId};

    use super::*;

    #[test]
    fn filter_to_known_dims_drops_out_of_dim_ids_and_records_them() {
        // unit 83 (militia) is a real dim id; 1570 is a verified out-of-dim id from this corpus.
        // tech 22 (loom) is in-dim; 99999 is not.
        let mut batch = ReplayBatch {
            player_units: vec![
                NewMatchPlayerUnit {
                    match_id: MatchId(1),
                    profile_id: ProfileId(5001),
                    unit_id: GameUnitId(83),
                    trained: 5,
                },
                NewMatchPlayerUnit {
                    match_id: MatchId(1),
                    profile_id: ProfileId(5001),
                    unit_id: GameUnitId(1570),
                    trained: 2,
                },
            ],
            player_techs: vec![
                NewMatchPlayerTech {
                    match_id: MatchId(1),
                    profile_id: ProfileId(5001),
                    tech_id: TechId(22),
                    t_ms: 10_000,
                },
                NewMatchPlayerTech {
                    match_id: MatchId(1),
                    profile_id: ProfileId(5001),
                    tech_id: TechId(99999),
                    t_ms: 20_000,
                },
            ],
            ..ReplayBatch::default()
        };
        let dims = DimIds {
            units: HashSet::from([83]),
            techs: HashSet::from([22]),
        };

        let counts = filter_to_known_dims(&mut batch, &dims);

        assert_eq!(batch.player_units.len(), 1, "the out-of-dim unit is dropped");
        assert_eq!(batch.player_units[0].unit_id, GameUnitId(83));
        assert_eq!(batch.player_techs.len(), 1, "the out-of-dim tech is dropped");
        assert_eq!(batch.player_techs[0].tech_id, TechId(22));
        assert_eq!(counts.dropped_unit_rows, 1);
        assert_eq!(counts.dropped_tech_rows, 1);
        assert_eq!(counts.unknown_unit_ids, BTreeSet::from([1570]));
        assert_eq!(counts.unknown_tech_ids, BTreeSet::from([99999]));
    }
}
