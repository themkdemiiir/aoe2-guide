//! The full `data.Tech` id space (194 ids), from the committed
//! `reference-data/aoe2techtree-techs.json` — a PARED `{ID, internal_name}` slice of aoe2techtree's
//! `data.Tech` map. Mirrors [`crate::techtree`]'s role for `data.Unit`, but minimal: unlike
//! `aoe2techtree-units.json` (a verbatim `data.Unit` mirror carrying every stat field
//! [`crate::unit_stats`]/[`crate::game_facts`] read), no `refdata` consumer reads a tech's
//! `Cost`/`Repeatable`/`ResearchTime`, so those fields are omitted from the slice itself rather
//! than carried as unread struct fields (which `-D warnings` dead-code lint would flag).
//!
//! [`crate::unit_tech_names`] is the sole consumer: it walks every id here (the authoritative tech
//! DAT id space, exactly `scripts/build-unit-tech-names.mjs`'s `idInfo` loop) and resolves each
//! one's English display name via [`crate::tree_nodes`], mirroring that script's `buildDim`.

use std::collections::HashMap;

use serde::Deserialize;

use crate::error::{RefdataError, Result};

/// One `data.Tech` entry, pared to the two fields [`crate::unit_tech_names`] needs. `internal_name`
/// is documentation/debugging only downstream (mirrors `pipeline_core::techs::TechInfo`'s doc) —
/// `#[serde(default)]` so an id lacking one still deserializes rather than failing the whole slice.
#[derive(Debug, Clone, PartialEq, Deserialize)]
pub struct RawTech {
    #[serde(rename = "ID")]
    pub id: i32,
    #[serde(rename = "internal_name", default)]
    pub internal_name: Option<String>,
}

#[derive(Debug, Deserialize)]
struct TechsDoc {
    techs: HashMap<String, RawTech>,
}

/// Parses `aoe2techtree-techs.json`-shaped text into the id-keyed tech table. Exposed standalone
/// (not only via [`load_tech_ids`]) so tests can drive it with a small inline fixture.
pub fn parse(json: &str) -> Result<HashMap<i32, RawTech>> {
    let doc: TechsDoc = serde_json::from_str(json)
        .map_err(|source| RefdataError::ParseSource { file: "aoe2techtree-techs.json", source })?;
    Ok(doc.techs.into_values().map(|t| (t.id, t)).collect())
}

/// Loads the real committed `aoe2techtree-techs.json`, baked into the binary at compile time —
/// the full 194-id tech DAT id space.
pub fn load_tech_ids() -> Result<HashMap<i32, RawTech>> {
    parse(include_str!(
        "../../../../reference-data/aoe2techtree-techs.json"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    const TECHS: &str = r#"{"provenance":{},"techs":{
        "22":{"ID":22,"internal_name":"Loom"},
        "213":{"ID":213,"internal_name":"Wheelbarrow"}
    }}"#;

    #[test]
    fn parses_a_fixture() {
        let table = parse(TECHS).unwrap();
        assert_eq!(table.len(), 2);
        assert_eq!(table[&22].internal_name.as_deref(), Some("Loom"));
    }

    #[test]
    fn real_slice_has_the_full_tech_id_space() {
        let table = load_tech_ids().expect("committed slice must parse");
        assert_eq!(table.len(), 194);
        assert_eq!(table[&22].internal_name.as_deref(), Some("Loom"));
    }
}
