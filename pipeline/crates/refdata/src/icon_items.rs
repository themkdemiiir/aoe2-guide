//! Loads the committed `reference-data/aoe2techtree-icon-items.json` — icon-eligible tree items
//! (`{name, use_type, picture_index}`) across BOTH base-game AND chronicles civ trees, in the
//! EXACT order `scripts/build-icon-map.mjs` collects them (load-bearing — see the slice's own
//! `provenance.note` and [`crate::icon_map`], the sole consumer).

use serde::Deserialize;

use crate::error::{RefdataError, Result};

/// One `[name, use_type, picture_index]` icon item — `use_type` is `"Unit"`, `"Tech"`, or
/// `"Building"` (matches aoe2techtree's own vocabulary; NOT modeled as a closed enum here, same
/// rationale as [`crate::unit_tree`]'s age strings — [`crate::icon_map::build`] just compares it
/// against the three literals).
#[derive(Debug, Deserialize)]
pub struct IconItem(pub String, pub String, pub i32);

#[derive(Debug, Deserialize)]
struct IconItemsDoc {
    items: Vec<IconItem>,
}

/// Parses `aoe2techtree-icon-items.json`-shaped text. Exposed standalone (not only via [`load`])
/// so tests can drive it with a small inline fixture.
pub fn parse(json: &str) -> Result<Vec<IconItem>> {
    let doc: IconItemsDoc = serde_json::from_str(json).map_err(|source| {
        RefdataError::ParseSource { file: "aoe2techtree-icon-items.json", source }
    })?;
    Ok(doc.items)
}

/// Loads the real committed `aoe2techtree-icon-items.json`, baked into the binary at compile time.
pub fn load() -> Result<Vec<IconItem>> {
    parse(include_str!(
        "../../../../reference-data/aoe2techtree-icon-items.json"
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_fixture_preserving_order() {
        let json = r#"{"items":[
            ["Archery Range","Building",0],
            ["Archer","Unit",17]
        ]}"#;
        let items = parse(json).unwrap();
        assert_eq!(items.len(), 2);
        assert_eq!(items[0].0, "Archery Range");
        assert_eq!(items[1].2, 17);
    }

    #[test]
    fn real_slice_loads_and_has_the_expected_item_count() {
        let items = load().expect("committed slice must parse");
        assert_eq!(items.len(), 8746);
    }
}
