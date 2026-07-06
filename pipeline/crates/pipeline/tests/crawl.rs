//! Run-loop integration tests for [`pipeline::crawl`] — the brief's gate 1
//! ("happy path with faked fetch, bad-replay-skip, idempotent re-run"), plus coverage for
//! `--dry-run`, graceful shutdown, the rate-limit courtesy pause, and the "seed persistence gap"
//! documented on [`pipeline::crawl`]'s module doc.
//!
//! NEVER touches the live Relic API or a real Postgres: [`FakeSource`] is a hand-rolled
//! [`pipeline::ReplaySource`] fed canned [`pipeline::ReplayFetch`] outcomes (no HTTP, no binary
//! `.aoe2record` bytes — see `pipeline::source`'s module doc for why the seam is drawn at this
//! level), [`RecordingSink`] is a hand-rolled [`pipeline::IngestSink`] that just records what it
//! was handed, and [`InMemoryManifest`] is a `HashMap`-backed [`fetch::ReplayManifest`] (the same
//! shape as `fetch`'s own private test fake — reimplemented here since that one isn't `pub`).

use std::collections::HashMap;
use std::sync::atomic::{AtomicUsize, Ordering};
use std::sync::Arc;

use bytes::Bytes;
use chrono::{TimeZone, Utc};
use fetch::{
    retry_ready, DiscoverySeed, MatchStatus, PendingMatch, RelicMatchType, ReplayManifest, SeedRow,
    StatusUpdate,
};
use ingest::{IngestStats, ReplayBatch};
use pipeline::{
    crawl, CrawlConfig, IngestSink, RawArchive, ReplayFetch, ReplaySource, SaveOutcome,
};
use pipeline_core::{GameCivId, MatchId, ProfileId};
use replay::{ParsedReplay, ReplayPlayer};
use tokio_util::sync::CancellationToken;

// --- fakes -------------------------------------------------------------------------------------

/// A `HashMap`-backed [`ReplayManifest`], mirroring `fetch::manifest`'s own private test fake
/// (not `pub`, so reimplemented here) — exercises the SAME scheduling policy
/// ([`fetch::retry_ready`]) with no SQLite.
#[derive(Default)]
struct InMemoryManifest {
    rows: HashMap<i64, Row>,
}

#[derive(Clone)]
struct Row {
    played_at: Option<i64>,
    profile_ids: Option<String>,
    status: MatchStatus,
    attempts: i64,
    updated_at: i64,
}

impl ReplayManifest for InMemoryManifest {
    fn seed(&mut self, rows: &[SeedRow]) -> fetch::Result<usize> {
        let mut inserted = 0;
        for r in rows {
            match self.rows.get_mut(&r.match_id.0) {
                None => {
                    self.rows.insert(
                        r.match_id.0,
                        Row {
                            played_at: r.played_at,
                            profile_ids: r.profile_ids.clone(),
                            status: MatchStatus::Pending,
                            attempts: 0,
                            updated_at: 0,
                        },
                    );
                    inserted += 1;
                }
                Some(existing) => {
                    if existing.profile_ids.is_none() {
                        if let Some(pids) = &r.profile_ids {
                            existing.profile_ids = Some(pids.clone());
                            if existing.status == MatchStatus::Expired {
                                existing.status = MatchStatus::Pending;
                            }
                        }
                    }
                }
            }
        }
        Ok(inserted)
    }

    fn take_ready(&self, limit: usize, now: i64) -> fetch::Result<Vec<PendingMatch>> {
        let mut eligible: Vec<(&i64, &Row)> = self
            .rows
            .iter()
            .filter(|(_, row)| match row.status {
                MatchStatus::Pending => true,
                MatchStatus::Error => retry_ready(row.attempts, row.updated_at, now),
                _ => false,
            })
            .collect();
        eligible.sort_by_key(|(_, row)| std::cmp::Reverse(row.played_at));
        Ok(eligible
            .into_iter()
            .take(limit)
            .map(|(id, row)| PendingMatch {
                match_id: MatchId(*id),
                played_at: row.played_at,
                profile_ids: row.profile_ids.clone(),
                attempts: row.attempts,
            })
            .collect())
    }

    fn record(&mut self, updates: &[StatusUpdate]) -> fetch::Result<()> {
        for u in updates {
            if let Some(row) = self.rows.get_mut(&u.match_id.0) {
                row.status = u.status;
                if u.attempts_inc {
                    row.attempts += 1;
                }
            }
        }
        Ok(())
    }

    fn counts(&self) -> fetch::Result<Vec<(MatchStatus, i64)>> {
        let mut by: HashMap<&'static str, (MatchStatus, i64)> = HashMap::new();
        for row in self.rows.values() {
            let e = by.entry(row.status.as_db_str()).or_insert((row.status, 0));
            e.1 += 1;
        }
        Ok(by.into_values().collect())
    }
}

impl InMemoryManifest {
    fn status_of(&self, match_id: MatchId) -> Option<MatchStatus> {
        self.rows.get(&match_id.0).map(|r| r.status)
    }
}

/// A [`ReplaySource`] fed entirely canned outcomes — no HTTP, no live Relic API, ever.
#[derive(Default)]
struct FakeSource {
    seeds: Vec<DiscoverySeed>,
    replays: HashMap<MatchId, ReplayFetch>,
    fetch_calls: AtomicUsize,
}

impl ReplaySource for FakeSource {
    async fn discover(&self, _profile_id: ProfileId) -> fetch::Result<Vec<DiscoverySeed>> {
        Ok(self.seeds.clone())
    }

    async fn fetch_replay(&self, match_id: MatchId) -> ReplayFetch {
        self.fetch_calls.fetch_add(1, Ordering::SeqCst);
        self.replays
            .get(&match_id)
            .cloned()
            .unwrap_or(ReplayFetch::NoReplay)
    }
}

/// An [`IngestSink`] that just records what it was handed.
#[derive(Default)]
struct RecordingSink {
    batches: Vec<ReplayBatch>,
}

impl IngestSink for RecordingSink {
    async fn ingest(&mut self, batch: &ReplayBatch) -> anyhow::Result<IngestStats> {
        self.batches.push(batch.clone());
        Ok(IngestStats {
            matches_inserted: 1,
            matches_skipped: 0,
            players: batch.players.len() as u64,
            events: batch.events.len() as u64,
            ages: batch.ages.len() as u64,
        })
    }
}

// --- fixtures ------------------------------------------------------------------------------

fn seed(match_id: i64) -> DiscoverySeed {
    DiscoverySeed {
        match_id: MatchId(match_id),
        match_type: RelicMatchType::SoloRmRanked,
        played_at: Utc.with_ymd_and_hms(2026, 7, 1, 12, 0, 0).unwrap(),
        new_rating: Some(1600),
    }
}

fn parsed_replay(match_id: i64) -> ParsedReplay {
    ParsedReplay {
        match_id: MatchId(match_id),
        build: Some(101_101),
        map_id: Some(9),
        duration_ms: 1_800_000,
        n_players: 2,
        players: vec![
            ReplayPlayer {
                player_number: 1,
                profile_id: ProfileId(5001),
                civ_id: GameCivId(1),
                name: "P1".to_owned(),
                team: 1,
                color: 1,
                won: Some(true),
                elo: Some(1650),
            },
            ReplayPlayer {
                player_number: 2,
                profile_id: ProfileId(5002),
                civ_id: GameCivId(2),
                name: "P2".to_owned(),
                team: 2,
                color: 2,
                won: Some(false),
                elo: Some(1590),
            },
        ],
        events: vec![],
        ages: vec![],
    }
}

/// Distinct, deterministic bytes standing in for "whatever `download_replay` returned" — the raw
/// archive persists these verbatim, and never needs them to actually be `.aoe2record`-shaped (see
/// `pipeline::source`'s module doc for why the seam is drawn above the real binary format).
fn raw_bytes(match_id: i64) -> Bytes {
    Bytes::from(
        format!("raw-bytes-for-match-{match_id}")
            .repeat(64)
            .into_bytes(),
    )
}

fn config(profile_id: Option<i64>, limit: usize, dry_run: bool) -> CrawlConfig {
    CrawlConfig {
        profile_id: profile_id.map(ProfileId),
        limit,
        concurrency: 4,
        dry_run,
        raw_dir: None,
    }
}

// --- gate 1: happy path / bad-replay-skip / idempotent re-run ----------------------------------

#[tokio::test]
async fn happy_path_with_faked_fetch_processes_and_marks_parsed() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(100)],
        replays: HashMap::from([(
            MatchId(100),
            ReplayFetch::Parsed {
                raw: raw_bytes(100),
                parsed: parsed_replay(100),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let cfg = config(Some(1), 10, false);
    let cancel = CancellationToken::new();

    let (manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("crawl must not fail");

    assert_eq!(summary.seeded, 1);
    assert_eq!(summary.planned, 1);
    assert_eq!(summary.attempted, 1);
    assert_eq!(summary.succeeded, 1);
    assert_eq!(summary.failed, 0);

    assert_eq!(
        sink.batches.len(),
        1,
        "the ingest sink received exactly one batch"
    );
    assert_eq!(sink.batches[0].matches[0].match_id, MatchId(100));

    assert_eq!(manifest.status_of(MatchId(100)), Some(MatchStatus::Parsed));
}

#[tokio::test]
async fn bad_replay_is_skipped_and_the_rest_still_process() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(200), seed(201)],
        replays: HashMap::from([
            (
                MatchId(200),
                ReplayFetch::ParseFailed {
                    raw: raw_bytes(200),
                    message: "corrupt record".to_owned(),
                },
            ),
            (
                MatchId(201),
                ReplayFetch::Parsed {
                    raw: raw_bytes(201),
                    parsed: parsed_replay(201),
                },
            ),
        ]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let cfg = config(Some(1), 10, false);
    let cancel = CancellationToken::new();

    let (manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("one bad replay must not fail the whole crawl");

    assert_eq!(summary.attempted, 2);
    assert_eq!(summary.succeeded, 1, "the good match still gets ingested");
    assert_eq!(
        summary.failed, 1,
        "the bad match is counted as failed, not a crash"
    );

    assert_eq!(sink.batches.len(), 1);
    assert_eq!(sink.batches[0].matches[0].match_id, MatchId(201));

    assert_eq!(
        manifest.status_of(MatchId(200)),
        Some(MatchStatus::ParseFailed),
        "a deterministic parse failure is terminal, never silently dropped"
    );
    assert_eq!(manifest.status_of(MatchId(201)), Some(MatchStatus::Parsed));
}

#[tokio::test]
async fn second_run_is_idempotent_and_reprocesses_nothing() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(300)],
        replays: HashMap::from([(
            MatchId(300),
            ReplayFetch::Parsed {
                raw: raw_bytes(300),
                parsed: parsed_replay(300),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let cfg = config(Some(1), 10, false);
    let cancel = CancellationToken::new();

    let (manifest, first) = crawl(
        Arc::clone(&source),
        manifest,
        Some(&mut sink),
        &cfg,
        &cancel,
    )
    .await
    .expect("first run must succeed");
    assert_eq!(first.succeeded, 1);
    assert_eq!(sink.batches.len(), 1);

    // Re-run against the SAME manifest (now `Parsed` — terminal) and the SAME rediscovered seed —
    // idempotent: nothing new is planned or ingested.
    let (manifest, second) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("second run must succeed");

    assert_eq!(
        second.planned, 0,
        "a Parsed match is terminal — take_ready must not return it"
    );
    assert_eq!(second.attempted, 0);
    assert_eq!(second.succeeded, 0);
    assert_eq!(
        sink.batches.len(),
        1,
        "no new batch was ingested on the idempotent re-run"
    );
    assert_eq!(manifest.status_of(MatchId(300)), Some(MatchStatus::Parsed));
}

// --- dry-run: gate 2's "mocked discover" path ---------------------------------------------------

#[tokio::test]
async fn dry_run_plans_via_a_mocked_discover_without_downloading_or_ingesting() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(400)],
        replays: HashMap::from([(
            MatchId(400),
            ReplayFetch::Parsed {
                raw: raw_bytes(400),
                parsed: parsed_replay(400),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let cfg = config(Some(1), 10, true);
    let cancel = CancellationToken::new();

    let sink: Option<&mut RecordingSink> = None;
    let (manifest, summary) = crawl(Arc::clone(&source), manifest, sink, &cfg, &cancel)
        .await
        .expect("dry-run must not fail");

    // Discover + seed DID happen (this is what proves the CLI/manifest wiring end to end) — but
    // nothing past that.
    assert_eq!(
        summary.seeded, 1,
        "dry-run still discovers + seeds (harmless local bookkeeping)"
    );
    assert_eq!(summary.planned, 1);
    assert_eq!(summary.attempted, 0, "dry-run never spawns a worker");
    assert_eq!(
        source.fetch_calls.load(Ordering::SeqCst),
        0,
        "dry-run must never call fetch_replay (no download, no parse)"
    );
    assert_eq!(
        manifest.status_of(MatchId(400)),
        Some(MatchStatus::Pending),
        "dry-run never writes a terminal/error status"
    );
}

// --- graceful shutdown ---------------------------------------------------------------------------

#[tokio::test]
async fn no_new_work_is_started_once_cancelled() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(500), seed(501)],
        replays: HashMap::from([
            (
                MatchId(500),
                ReplayFetch::Parsed {
                    raw: raw_bytes(500),
                    parsed: parsed_replay(500),
                },
            ),
            (
                MatchId(501),
                ReplayFetch::Parsed {
                    raw: raw_bytes(501),
                    parsed: parsed_replay(501),
                },
            ),
        ]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let cfg = config(Some(1), 10, false);
    let cancel = CancellationToken::new();
    cancel.cancel(); // shutdown already requested before the run even starts

    let (manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("a pre-cancelled run must still return cleanly, not error");

    assert_eq!(summary.planned, 2);
    assert_eq!(
        summary.attempted, 0,
        "no new worker is started once cancelled"
    );
    assert_eq!(summary.cancelled_before_start, 2);
    assert!(sink.batches.is_empty());
    assert_eq!(manifest.status_of(MatchId(500)), Some(MatchStatus::Pending));
    assert_eq!(manifest.status_of(MatchId(501)), Some(MatchStatus::Pending));
}

// --- rate-limit resilience -----------------------------------------------------------------------

#[tokio::test]
async fn a_rate_limited_failure_is_recorded_as_a_retryable_error() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(600)],
        replays: HashMap::from([(
            MatchId(600),
            ReplayFetch::FetchFailed {
                message: "relic api rate-limited".to_owned(),
                // `Some(0)` keeps the courtesy pause instant, so this test stays fast.
                retry_after: Some(0),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let cfg = config(Some(1), 10, false);
    let cancel = CancellationToken::new();

    let (manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("a rate-limited match must not fail the whole crawl");

    assert_eq!(summary.failed, 1);
    assert!(sink.batches.is_empty());
    assert_eq!(
        manifest.status_of(MatchId(600)),
        Some(MatchStatus::Error),
        "rate-limited is transient/retryable, not terminal"
    );
}

// --- seed persistence gap (module-doc'd scope limit) --------------------------------------------

#[tokio::test]
async fn a_ready_retry_with_no_fresh_seed_this_run_is_skipped() {
    // Seed the manifest directly (simulating a match already known from an EARLIER run), with NO
    // discovery this run (`profile_id: None`) — so `crawl` never learns this match's
    // `DiscoverySeed` (match_type/played_at) and cannot compose it.
    let mut manifest = InMemoryManifest::default();
    manifest
        .seed(&[SeedRow {
            match_id: MatchId(700),
            played_at: Some(0),
            profile_ids: None,
        }])
        .unwrap();

    let source = Arc::new(FakeSource::default());
    let mut sink = RecordingSink::default();
    let cfg = config(None, 10, false);
    let cancel = CancellationToken::new();

    let (manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("crawl must not fail");

    assert_eq!(
        summary.seeded, 0,
        "profile_id: None skips discovery entirely"
    );
    assert_eq!(summary.planned, 1, "the manifest still reports it as ready");
    assert_eq!(summary.skipped_no_seed, 1);
    assert_eq!(summary.attempted, 0);
    assert!(sink.batches.is_empty());
    assert_eq!(
        manifest.status_of(MatchId(700)),
        Some(MatchStatus::Pending),
        "left untouched for a future run that rediscovers it"
    );
}

// --- raw-replay archiving (task-rawkeep brief gate 1) -------------------------------------------
//
// Exercises the SAME fake-fetch seam as every test above — `FakeSource`'s `ReplayFetch::Parsed`/
// `ParseFailed` variants now carry arbitrary raw bytes (`raw_bytes`, NOT a real `.aoe2record`; see
// `pipeline::source`'s module doc for why that is fine for this seam) — proving `crawl` itself
// archives them, with NO live Relic API and NO real replay fixture involved.

#[tokio::test]
async fn a_successful_download_is_archived_and_decompresses_back_to_the_original_bytes() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let raw = raw_bytes(800);
    let source = Arc::new(FakeSource {
        seeds: vec![seed(800)],
        replays: HashMap::from([(
            MatchId(800),
            ReplayFetch::Parsed {
                raw: raw.clone(),
                parsed: parsed_replay(800),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let mut cfg = config(Some(1), 10, false);
    cfg.raw_dir = Some(dir.path().to_path_buf());
    let cancel = CancellationToken::new();

    let (_manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("crawl must not fail");

    assert_eq!(summary.raw_saved, 1);
    assert_eq!(summary.raw_failed, 0);
    assert!(summary.raw_bytes_written > 0);

    let archive = RawArchive::new(dir.path());
    let final_path = archive.path_for(MatchId(800));
    assert!(
        final_path.exists(),
        "the raw archive file must exist at the documented sharded path"
    );
    let restored = zstd::stream::decode_all(std::fs::File::open(&final_path).unwrap())
        .expect("the archived file must decompress cleanly");
    assert_eq!(
        restored,
        raw.to_vec(),
        "decompressing the archive must reproduce the exact downloaded bytes"
    );
}

#[tokio::test]
async fn a_parse_failure_still_leaves_its_raw_bytes_archived() {
    // The whole point of the feature: a replay that downloads fine but fails to parse must NOT
    // lose its raw bytes.
    let dir = tempfile::tempdir().expect("create temp dir");
    let raw = raw_bytes(801);
    let source = Arc::new(FakeSource {
        seeds: vec![seed(801)],
        replays: HashMap::from([(
            MatchId(801),
            ReplayFetch::ParseFailed {
                raw: raw.clone(),
                message: "corrupt record".to_owned(),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let mut cfg = config(Some(1), 10, false);
    cfg.raw_dir = Some(dir.path().to_path_buf());
    let cancel = CancellationToken::new();

    let (manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("a parse failure must not fail the whole crawl");

    assert_eq!(
        manifest.status_of(MatchId(801)),
        Some(MatchStatus::ParseFailed),
        "the match itself is still terminal-failed for ingest purposes"
    );
    assert_eq!(
        summary.raw_saved, 1,
        "the raw bytes must be archived EVEN THOUGH the replay failed to parse"
    );

    let archive = RawArchive::new(dir.path());
    let restored =
        zstd::stream::decode_all(std::fs::File::open(archive.path_for(MatchId(801))).unwrap())
            .unwrap();
    assert_eq!(restored, raw.to_vec());
}

#[tokio::test]
async fn dry_run_writes_no_raw_files_even_with_a_raw_dir_configured() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let source = Arc::new(FakeSource {
        seeds: vec![seed(802)],
        replays: HashMap::from([(
            MatchId(802),
            ReplayFetch::Parsed {
                raw: raw_bytes(802),
                parsed: parsed_replay(802),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    // `raw_dir` is deliberately set to `Some(..)` here (NOT the `--no-raw` case) — this proves
    // `crawl` itself never reaches the "Process" step in a dry-run (and so never saves) purely
    // structurally, regardless of what `raw_dir` says (see `CrawlConfig::raw_dir`'s doc).
    let mut cfg = config(Some(1), 10, true);
    cfg.raw_dir = Some(dir.path().to_path_buf());
    let cancel = CancellationToken::new();

    let sink: Option<&mut RecordingSink> = None;
    let (_manifest, summary) = crawl(source, manifest, sink, &cfg, &cancel)
        .await
        .expect("dry-run must not fail");

    assert_eq!(summary.raw_saved, 0);
    assert_eq!(
        std::fs::read_dir(dir.path()).unwrap().count(),
        0,
        "a dry-run must write NOTHING into the raw archive directory"
    );
}

#[tokio::test]
async fn raw_dir_none_the_no_raw_flags_effect_at_the_config_level_writes_nothing() {
    let source = Arc::new(FakeSource {
        seeds: vec![seed(803)],
        replays: HashMap::from([(
            MatchId(803),
            ReplayFetch::Parsed {
                raw: raw_bytes(803),
                parsed: parsed_replay(803),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    // `raw_dir: None` is exactly what `main.rs`'s `--no-raw` flag produces (see
    // `crawl_config`'s doc) — a live (non-dry-run) crawl that still archives nothing, even though
    // every other stage (fetch/parse/ingest) runs normally.
    let cfg = config(Some(1), 10, false);
    assert_eq!(
        cfg.raw_dir, None,
        "the `config()` fixture's default (== --no-raw)"
    );
    let cancel = CancellationToken::new();

    let (_manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("crawl must not fail");

    assert_eq!(summary.raw_saved, 0);
    assert_eq!(summary.raw_already_present, 0);
    assert_eq!(summary.raw_failed, 0);
    assert_eq!(summary.succeeded, 1, "the match itself still ingests fine");
}

#[tokio::test]
async fn a_resumed_crawl_skips_an_already_present_raw_archive_entry() {
    let dir = tempfile::tempdir().expect("create temp dir");
    let archive = RawArchive::new(dir.path());
    let original_raw = raw_bytes(804);
    // Simulate an EARLIER run having already archived this match's raw bytes (e.g. its ingest
    // failed afterwards, leaving it `Error`/retryable — see `crate::raw`'s "Idempotency" doc).
    let pre_existing = archive
        .save(MatchId(804), &original_raw)
        .expect("pre-seed the archive");
    assert!(matches!(pre_existing, SaveOutcome::Saved { .. }));

    // This run's FakeSource returns DIFFERENT bytes — if `crawl` ever re-wrote the archive, the
    // on-disk content would change; it must not.
    let different_raw = raw_bytes(999_999);
    let source = Arc::new(FakeSource {
        seeds: vec![seed(804)],
        replays: HashMap::from([(
            MatchId(804),
            ReplayFetch::Parsed {
                raw: different_raw,
                parsed: parsed_replay(804),
            },
        )]),
        ..Default::default()
    });
    let manifest = InMemoryManifest::default();
    let mut sink = RecordingSink::default();
    let mut cfg = config(Some(1), 10, false);
    cfg.raw_dir = Some(dir.path().to_path_buf());
    let cancel = CancellationToken::new();

    let (_manifest, summary) = crawl(source, manifest, Some(&mut sink), &cfg, &cancel)
        .await
        .expect("crawl must not fail");

    assert_eq!(
        summary.raw_already_present, 1,
        "an already-archived match must be counted as skipped, not re-saved"
    );
    assert_eq!(summary.raw_saved, 0);

    let restored =
        zstd::stream::decode_all(std::fs::File::open(archive.path_for(MatchId(804))).unwrap())
            .unwrap();
    assert_eq!(
        restored,
        original_raw.to_vec(),
        "the ORIGINAL archived bytes must survive untouched, never overwritten by this run"
    );
}
