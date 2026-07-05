//! Native parity-check CLI (Task 4a). Parses a `.aoe2record` and prints the
//! `analyze::analyze()` `Report` as pretty JSON — the exact same shape the OLD
//! `replay-rs analyze --json` CLI prints (both call `serde_json::to_string_pretty`
//! on the same `Report` type) — so the two outputs can be byte-diffed to prove the
//! ported lib behaves identically to the original.
//!
//! Usage: cargo run --example analyze_cli -- <file.aoe2record> [profile_id]
//! (no profile_id => YouSel::Auto, i.e. the recording player — matches the old
//! CLI's default when neither --you nor --profile-id is given.)

use std::env;
use std::fs;

use analyzer::analyze::{analyze, YouSel};

fn main() {
    let mut args = env::args().skip(1);
    let path = args
        .next()
        .expect("usage: analyze_cli <file.aoe2record> [profile_id]");
    let you = match args.next() {
        Some(p) => YouSel::ProfileId(p.parse().expect("profile_id must be a number")),
        None => YouSel::Auto,
    };

    let data = fs::read(&path).expect("read replay file");
    let game = aoe2rec::Savegame::from_bytes(bytes::Bytes::from(data))
        .unwrap_or_else(|e| panic!("could not parse this replay: {e}"));
    let report = analyze(&game, &you).expect("analyze");
    println!(
        "{}",
        serde_json::to_string_pretty(&report).expect("serialize")
    );
}
