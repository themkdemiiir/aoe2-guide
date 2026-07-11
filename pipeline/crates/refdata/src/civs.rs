//! Reads the committed `reference-data/aoe2techtree-civs.json` — all 53 base-game civs from
//! `data.json`'s `civs` map, each carrying its raw EN+TR `help` string verbatim (civ bonuses,
//! unique unit(s), unique techs, team bonus — see [`crate::civ_help`] for the parser). Mirrors
//! [`crate::techs`]'s role for `data.Tech`: a thin, pared committed-slice reader: the actual
//! bonus/tech/unit-name EXTRACTION lives in `civ_help`, not here.
//!
//! Keyed by `data.json`'s OWN `civs` object key (e.g. `"Hindustanis"` — NOT `"Indians"`, which is
//! only that civ's `internal_name`; the aoe2techtree source itself already uses the display key,
//! so — unlike `scripts/build-civilizations.mjs`'s `aalises` CSV path — no `"Indians" ->
//! "Hindustanis"` rename is needed here).

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::{RefdataError, Result};

/// One civ's committed source data: its `help_string_id` (documentation/debugging — the EN/TR
/// strings it points to are already carried alongside it) plus the raw EN+TR help text itself.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawCiv {
    #[serde(default)]
    pub internal_name: Option<String>,
    pub help_string_id: i64,
    pub help_en: String,
    pub help_tr: String,
}

#[derive(Debug, Deserialize)]
struct CivsDoc {
    civs: HashMap<String, RawCiv>,
}

/// Parses `aoe2techtree-civs.json`-shaped text. Exposed standalone (not only via [`load`]) so
/// tests can drive it with a small inline fixture.
pub fn parse(json: &str) -> Result<HashMap<String, RawCiv>> {
    let doc: CivsDoc = serde_json::from_str(json)
        .map_err(|source| RefdataError::ParseSource { file: "aoe2techtree-civs.json", source })?;
    Ok(doc.civs)
}

/// Loads the real committed `aoe2techtree-civs.json`, baked into the binary at compile time — all
/// 53 base-game civs.
pub fn load() -> Result<HashMap<String, RawCiv>> {
    parse(include_str!("../../../../reference-data/aoe2techtree-civs.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    const CIVS: &str = r#"{"provenance":{},"civs":{
        "Britons":{"internal_name":"Britons","help_string_id":120150,
            "help_en":"Foot Archer civilization<br>\n<br>\n• Shepherds work +25% faster",
            "help_tr":"Yaya Okçu medeniyeti<br>\n<br>\n• Çobanlar %25 daha hızlı çalışır"}
    }}"#;

    #[test]
    fn parses_a_fixture() {
        let civs = parse(CIVS).unwrap();
        assert_eq!(civs.len(), 1);
        let britons = &civs["Britons"];
        assert_eq!(britons.help_string_id, 120150);
        assert!(britons.help_en.starts_with("Foot Archer civilization"));
    }

    #[test]
    fn real_slice_has_all_fifty_three_base_game_civs() {
        let civs = load().expect("committed slice must parse");
        assert_eq!(civs.len(), 53);
        let britons = &civs["Britons"];
        assert!(britons.help_en.contains("Shepherds work +25% faster"));
        assert!(britons.help_tr.contains("medeniyeti"));
        // "Hindustanis" is the aoe2techtree KEY; "Indians" only shows up as internal_name.
        assert_eq!(civs["Hindustanis"].internal_name.as_deref(), Some("Indians"));
    }
}
