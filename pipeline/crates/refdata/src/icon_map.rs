//! Builds `icon-map.json` from the committed [`crate::icon_items`] slice crossed against a live
//! inventory of the committed `public/images/aoe2/{Unit,Tech,Building,Civs}/` PNGs. Mirrors
//! `scripts/build-icon-map.mjs` exactly:
//!
//! * items are walked in the committed slice's order (base-game civs first, then chronicles —
//!   see [`crate::icon_items`]'s doc); the FIRST item for a given `(useType, slug)` wins
//!   (`!units[slug]` in the JS) — never overwritten by a later duplicate;
//! * `picture_index` (NOT the game object id) is the disk-file lookup key — the image folders are
//!   generated from aoe2techtree's OWN `Picture Index`, so keying by game id can silently show
//!   wrong art (e.g. Crossbowman `node_id` 24, `picture_index` 18);
//! * Buildings ARE emitted — despite `build-icon-map.mjs`'s own module-doc comment claiming
//!   otherwise, the actual code populates `buildings` identically to `units`/`techs` (verified:
//!   the committed file carries 45 building entries). Code is truth here, not that stale comment;
//! * the `trebuchet-packed` -> `trebuchet` fallback (a no-op today — the real committed slice
//!   resolves `trebuchet` directly too — but kept for fidelity with the source script);
//! * civs come straight from `Civs/*.png` filenames (already kebab-case), sorted alphabetically.
//!
//! [`AssetInventory`] is the ONE genuinely-external input this module needs (which `.png` files
//! actually exist on disk) — unlike every other builder in this crate, that can't be
//! `include_str!`'d (thousands of binary PNGs). Per this crate's "functional core, imperative
//! shell" split, the actual `fs::read_dir` scan lives in `main.rs` (the imperative shell); this
//! module stays a pure function of its two inputs (the committed slice + the inventory), so tests
//! can drive it with small inline fixtures instead of a real `public/images/aoe2/` checkout.

use std::collections::{BTreeMap, HashSet};
use std::path::{Path, PathBuf};

use crate::error::Result;
use crate::icon_items::{self, IconItem};
use crate::model::IconMapDoc;

/// The repo's `public/images/aoe2/` directory, computed from this crate's own compile-time
/// location — the ONE input [`AssetInventory`] needs that can't be `include_str!`'d (see that
/// type's doc). `CARGO_MANIFEST_DIR` is `pipeline/crates/refdata` (this crate's own root); three
/// `..`s reaches the repo root, mirroring the `include_str!("../../../../reference-data/...")`
/// convention used everywhere else in this crate (that one has an extra `..` because it's
/// relative to a file inside `src/`, one level below the crate root `CARGO_MANIFEST_DIR` already
/// is). No CLI flag for this (unlike `--out`) — the assets always live in the SAME repo checkout
/// this binary was built from, so there's no path to get wrong.
pub fn default_assets_dir() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join("../../../public/images/aoe2")
}

/// A picture-index/civ-filename disk inventory of `public/images/aoe2/{Unit,Tech,Building,Civs}/`
/// — the live filesystem input [`build`] crosses against the committed [`icon_items`] slice.
/// `unit_ids`/`tech_ids`/`building_ids` are `picture_index` strings (a PNG's filename stem, e.g.
/// `"17"` for `Unit/17.png`); `civs` is already the FINAL slug -> path map (civ resolution is
/// filename-only — see the module doc — so there's nothing left for [`build`] to cross-reference).
#[derive(Debug, Clone, Default)]
pub struct AssetInventory {
    pub unit_ids: HashSet<String>,
    pub tech_ids: HashSet<String>,
    pub building_ids: HashSet<String>,
    pub civs: BTreeMap<String, String>,
}

/// Builds the whole `icon-map.json` document from the committed slice (loaded internally, same
/// `include_str!`'d-precedent as every other `build_from_committed_reference_data` in this crate)
/// crossed against `inventory`. Pure given `inventory` — see the module doc.
pub fn build(inventory: &AssetInventory) -> Result<IconMapDoc> {
    let items = icon_items::load()?;
    Ok(build_from_items(&items, inventory))
}

/// The actual cross-reference logic, taking an already-parsed item list — split out from [`build`]
/// so tests can drive it with small inline [`IconItem`] fixtures instead of the real 8746-item
/// committed slice.
pub fn build_from_items(items: &[IconItem], inventory: &AssetInventory) -> IconMapDoc {
    let mut units = BTreeMap::new();
    let mut techs = BTreeMap::new();
    let mut buildings = BTreeMap::new();

    for IconItem(name, use_type, picture_index) in items {
        let slug = slugify(name);
        if slug.is_empty() {
            continue;
        }
        let picture_index = picture_index.to_string();

        match use_type.as_str() {
            "Unit" if inventory.unit_ids.contains(&picture_index) => {
                units
                    .entry(slug)
                    .or_insert_with(|| format!("/images/aoe2/Unit/{picture_index}.png"));
            }
            "Tech" if inventory.tech_ids.contains(&picture_index) => {
                techs
                    .entry(slug)
                    .or_insert_with(|| format!("/images/aoe2/Tech/{picture_index}.png"));
            }
            "Building" if inventory.building_ids.contains(&picture_index) => {
                buildings
                    .entry(slug)
                    .or_insert_with(|| format!("/images/aoe2/Building/{picture_index}.png"));
            }
            _ => {}
        }
    }

    // `trebuchet-packed` -> `trebuchet` fallback (see module doc): a no-op today since the real
    // committed slice already resolves "trebuchet" directly, kept for source-script fidelity.
    if let Some(packed) = units.get("trebuchet-packed").cloned() {
        units.entry("trebuchet".to_string()).or_insert(packed);
    }

    IconMapDoc {
        units,
        techs,
        buildings,
        civs: inventory.civs.clone(),
    }
}

/// Ports `scripts/build-icon-map.mjs`'s `slugify`: lowercase, strip everything that isn't a
/// word-char/whitespace/hyphen, trim, then collapse runs of whitespace to a single hyphen. The JS
/// regexes are ASCII-only and its preceding `normalize('NFKD')` + combining-mark strip only matters
/// for a name carrying a diacritic; every one of the committed `aoe2techtree-icon-items.json`'s 559
/// distinct names is plain ASCII, so that step is a no-op for this dataset and is intentionally not
/// ported (no Unicode-normalization dependency). A future diacritic-bearing display name would just
/// get a coarser slug via [`build`], never corrupt any other output.
fn slugify(name: &str) -> String {
    let lower = name.to_lowercase();
    let filtered: String = lower
        .chars()
        .filter(|&c| is_word_char(c) || c.is_whitespace() || c == '-')
        .collect();

    let mut out = String::with_capacity(filtered.len());
    let mut in_run = false;
    for c in filtered.trim().chars() {
        if c.is_whitespace() {
            if !in_run {
                out.push('-');
                in_run = true;
            }
        } else {
            out.push(c);
            in_run = false;
        }
    }
    out
}

fn is_word_char(c: char) -> bool {
    c.is_ascii_alphanumeric() || c == '_'
}

#[cfg(test)]
mod tests {
    use super::*;

    fn inventory() -> AssetInventory {
        AssetInventory {
            unit_ids: ["17", "18"].into_iter().map(String::from).collect(),
            tech_ids: ["22"].into_iter().map(String::from).collect(),
            building_ids: ["0"].into_iter().map(String::from).collect(),
            civs: BTreeMap::from([(
                "britons".to_string(),
                "/images/aoe2/Civs/britons.png".to_string(),
            )]),
        }
    }

    #[test]
    fn resolves_a_unit_tech_and_building_by_picture_index() {
        let items = vec![
            IconItem("Archer".to_string(), "Unit".to_string(), 17),
            IconItem("Loom".to_string(), "Tech".to_string(), 22),
            IconItem("Archery Range".to_string(), "Building".to_string(), 0),
        ];
        let doc = build_from_items(&items, &inventory());
        assert_eq!(doc.units.get("archer"), Some(&"/images/aoe2/Unit/17.png".to_string()));
        assert_eq!(doc.techs.get("loom"), Some(&"/images/aoe2/Tech/22.png".to_string()));
        assert_eq!(
            doc.buildings.get("archery-range"),
            Some(&"/images/aoe2/Building/0.png".to_string())
        );
        assert_eq!(doc.civs.get("britons"), Some(&"/images/aoe2/Civs/britons.png".to_string()));
    }

    #[test]
    fn a_picture_index_missing_on_disk_is_skipped() {
        let items = vec![IconItem("Ghost".to_string(), "Unit".to_string(), 999)];
        let doc = build_from_items(&items, &inventory());
        assert!(doc.units.get("ghost").is_none());
    }

    #[test]
    fn the_first_item_for_a_slug_wins_over_a_later_duplicate() {
        let items = vec![
            IconItem("Archer".to_string(), "Unit".to_string(), 17),
            IconItem("Archer".to_string(), "Unit".to_string(), 18),
        ];
        let doc = build_from_items(&items, &inventory());
        assert_eq!(
            doc.units.get("archer"),
            Some(&"/images/aoe2/Unit/17.png".to_string()),
            "the FIRST resolvable item must win, never a later duplicate"
        );
    }

    #[test]
    fn trebuchet_packed_falls_back_to_trebuchet_when_unresolved_directly() {
        let items = vec![IconItem("Trebuchet Packed".to_string(), "Unit".to_string(), 17)];
        let doc = build_from_items(&items, &inventory());
        assert_eq!(doc.units.get("trebuchet"), Some(&"/images/aoe2/Unit/17.png".to_string()));
    }

    #[test]
    fn slugify_lowercases_and_hyphenates() {
        assert_eq!(slugify("Archery Range"), "archery-range");
        assert_eq!(slugify("  Two-Handed Swordsman  "), "two-handed-swordsman");
        assert_eq!(slugify("Chu Ko Nu"), "chu-ko-nu");
    }

    #[test]
    fn real_reference_data_produces_the_committed_counts() {
        // Matches the committed src/data/icon-map.json exactly (see tests/shape_parity.rs for the
        // shape-parity side); the real `AssetInventory` here is built the same way `main.rs` would
        // from the real `public/images/aoe2/` checkout (via [`default_assets_dir`]).
        use std::fs;
        let assets_dir = default_assets_dir();
        let scan = |sub: &str| -> HashSet<String> {
            fs::read_dir(assets_dir.join(sub))
                .expect("assets dir must exist in this checkout")
                .flatten()
                .filter_map(|e| e.file_name().into_string().ok())
                .filter_map(|f| f.strip_suffix(".png").map(str::to_string))
                .collect()
        };
        let civs: BTreeMap<String, String> = fs::read_dir(assets_dir.join("Civs"))
            .expect("Civs dir must exist")
            .flatten()
            .filter_map(|e| e.file_name().into_string().ok())
            .filter(|f| f.ends_with(".png"))
            .map(|f| {
                let slug = f.trim_end_matches(".png").to_lowercase();
                (slug, format!("/images/aoe2/Civs/{f}"))
            })
            .collect();
        let inventory = AssetInventory {
            unit_ids: scan("Unit"),
            tech_ids: scan("Tech"),
            building_ids: scan("Building"),
            civs,
        };
        let doc = build(&inventory).expect("build must succeed against the real committed slice");
        assert_eq!(doc.units.len(), 278);
        assert_eq!(doc.techs.len(), 242);
        assert_eq!(doc.buildings.len(), 45);
        assert_eq!(doc.civs.len(), 59);
        assert_eq!(doc.units.get("archer"), Some(&"/images/aoe2/Unit/17.png".to_string()));
    }
}
