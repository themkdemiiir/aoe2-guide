//! Patch `build` -> `{ label, released }` lookup, parsed from the committed
//! `src/data/patch-index.json` (SINGLE SOURCE OF TRUTH — read here, never copied or modified).
//!
//! Only `builds[].{build,label,released}` feed the `patch_index` dimension table; `note`/`source`
//! are documentation for humans, and the top-level `gamemodToBuild`/`ignoreGamemods` keys belong
//! to a different consumer (the Relic gamemod->build join, not implemented in this crate) — none
//! of those are read here.

use serde::Deserialize;

/// One `patch-index.json` `builds[]` entry that feeds `patch_index`.
#[derive(Debug, Clone, PartialEq, Eq, Deserialize)]
pub struct PatchBuild {
    pub build: i32,
    pub label: String,
    /// ISO `YYYY-MM-DD`, or absent for a build whose release date isn't known — mirrors
    /// `patch_index.released`'s nullability. Kept as a raw string, not a date type, so this
    /// module — like [`crate::civs`]/[`crate::maps`] — stays free of a `chrono` dependency; a
    /// caller that needs a real date parses this at the point of use.
    pub released: Option<String>,
}

#[derive(Debug, Deserialize)]
struct PatchDoc {
    builds: Vec<PatchBuild>,
}

/// Parses `patch-index.json`-shaped text into its `builds` list.
pub fn parse(json: &str) -> serde_json::Result<Vec<PatchBuild>> {
    let doc: PatchDoc = serde_json::from_str(json)?;
    Ok(doc.builds)
}

/// Loads the real, committed `src/data/patch-index.json`, baked into the binary at compile time.
/// `Err` (never a panic — playbook rule 8: no `unwrap`/`expect`/`panic` in a `pub` lib fn) if that
/// committed file is somehow malformed; the caller (`dims::load_dims`) `.context()`s it.
pub fn load() -> serde_json::Result<Vec<PatchBuild>> {
    parse(include_str!("../../../../src/data/patch-index.json"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_a_minimal_fixture_with_and_without_a_release_date() {
        let json = r#"{
            "builds": [
                { "build": 2, "label": "b", "released": "2026-01-01", "note": "ignored", "source": "ignored" },
                { "build": 1, "label": "a" }
            ]
        }"#;
        let builds = parse(json).expect("valid fixture must parse");
        assert_eq!(builds.len(), 2);
        assert_eq!(builds[0].build, 2);
        assert_eq!(builds[0].released.as_deref(), Some("2026-01-01"));
        assert_eq!(builds[1].build, 1);
        assert_eq!(
            builds[1].released, None,
            "a build with no released date must stay None, never fabricated"
        );
    }

    #[test]
    fn real_file_has_the_current_live_build() {
        let builds = load().expect("patch-index.json must parse");
        let build = builds
            .iter()
            .find(|b| b.build == 179_158)
            .expect("build 179158 present in the real patch-index.json");
        assert_eq!(build.label, "Update 179158");
        assert_eq!(build.released.as_deref(), Some("2026-06-16"));
    }

    #[test]
    fn real_file_has_more_than_a_handful_of_builds() {
        assert!(
            load().expect("patch-index.json must parse").len() > 10,
            "patch-index.json has ~20 builds"
        );
    }
}
