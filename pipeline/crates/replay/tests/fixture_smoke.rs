//! Opportunistic regression test against a real `.aoe2record`. No fixture is committed to this
//! repo (replay files are large binary data with unclear redistribution rights) — ignored by
//! default, same convention as the OLD crate's `analyze::walk::tests::walks_a_real_replay`.
//!
//! Runs only on demand: `TEST_REPLAY=/path/to.aoe2record cargo test -p replay -- --ignored`
//!
//! See `.superpowers/sdd/task-4b-replay-report.md` for the full row-by-row parity diff run
//! against the OLD `replay-rs` extractor on a real fixture — this test is a lighter-weight,
//! repeatable regression check, not the parity gate itself (which needs the OLD crate's private
//! `extract` module and so can't be expressed as a `cargo test` in this crate).

use bytes::Bytes;
use pipeline_core::MatchId;

#[test]
#[ignore = "set TEST_REPLAY=/path/to.aoe2record to run"]
fn parses_a_real_replay_without_error() {
    let path = std::env::var("TEST_REPLAY").expect("set TEST_REPLAY=/path/to.aoe2record");
    let bytes = Bytes::from(std::fs::read(&path).expect("TEST_REPLAY must be readable"));

    let parsed = replay::parse(MatchId(1), bytes).expect("a real replay must parse");

    assert!(!parsed.players.is_empty());
    assert!(parsed.duration_ms > 0);
    assert!(parsed.build.is_some());
    assert!(parsed.map_id.is_some());
    eprintln!(
        "match_id={} players={} events={} ages={}",
        parsed.match_id,
        parsed.players.len(),
        parsed.events.len(),
        parsed.ages.len()
    );
}
