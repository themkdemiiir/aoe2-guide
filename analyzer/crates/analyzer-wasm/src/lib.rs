//! In-browser replay analysis: raw `.aoe2record` bytes in, the full coaching
//! `Report` (schema_version 1, same contract as `replay-rs analyze --json`) out
//! as a JSON string. All benchmark/cost/map/civ data is include_str!-baked into
//! analyzer, so this needs no network and no filesystem.
//!
//! Build (from this directory; output is committed so CF Pages needs no Rust):
//!   pnpm build:wasm

use wasm_bindgen::prelude::*;

#[wasm_bindgen(start)]
fn start() {
    console_error_panic_hook::set_once();
}

/// Parse + analyze a replay. `you` picks the report perspective:
/// pass a profile_id as a string, or an empty string for the recording player.
/// Errors (unparseable file, unknown profile) surface as JS exceptions.
#[wasm_bindgen]
pub fn analyze_replay(data: &[u8], you: &str) -> Result<String, JsError> {
    let game = aoe2rec::Savegame::from_bytes(bytes::Bytes::copy_from_slice(data))
        .map_err(|e| JsError::new(&format!("could not parse this replay: {e}")))?;
    let sel = if you.trim().is_empty() {
        analyzer::analyze::YouSel::Auto
    } else {
        let id: i64 = you
            .trim()
            .parse()
            .map_err(|_| JsError::new("profile id must be a number"))?;
        analyzer::analyze::YouSel::ProfileId(id)
    };
    let report = analyzer::analyze::analyze(&game, &sel)
        .map_err(|e| JsError::new(&format!("{e}")))?;
    serde_json::to_string(&report).map_err(|e| JsError::new(&format!("serialize failed: {e}")))
}
