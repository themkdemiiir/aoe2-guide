//! A structural (key-set + type-family) diff between two JSON documents, ignoring VALUES — the
//! human-readable half of the M5a parity gate (the other half, `tests/shape_parity.rs`,
//! deserializes both documents into the SAME [`crate::model::CivMetaDoc`], which is the stronger
//! proof; this module exists to print an actual diff for the task report, and to let `main.rs`
//! expose a standalone `export shape-diff <a> <b>` command for ad-hoc checks).
//!
//! civ-meta.json mixes two VERY different kinds of JSON object:
//! - **fixed-schema structs** (the document root, one `LadderStats`, one `Opening`, ...) — two
//!   files should have the exact same key SET here.
//! - **data-keyed maps** (`civs`, `byElo`, `byMap`, `byPatch`) — their OWN keys are civ slugs /
//!   elo buckets / map slugs / patch builds, which legitimately differ between a frozen archive
//!   and a live sample. Comparing these by key-set would report every non-overlapping civ/bucket
//!   as a "missing key", which is noise, not a real shape problem — so instead this compares the
//!   UNIONED shape of their VALUES (i.e. "does a `civs` entry look like a `CivEntry` in both
//!   files", not "do both files cover the exact same civs").
//!
//! Task M5b's matchup/benchmark docs (`model.rs`'s `CivMatchupsByMapDoc`/`CivMatchupsByEloDoc`/
//! `BenchmarkDoc`) nest data-keyed maps SEVERAL levels deep with no wrapping field name at the
//! inner levels (e.g. `civs.<civSlug>.<mapSlug> = Array<Opponent>` — `<mapSlug>` has no field name
//! to gate on the way `byElo`/`byMap` do). [`collapse_dynamic`] handles this by recursing
//! STRUCTURALLY instead of by name: as long as a level's values are UNIFORMLY objects (or
//! UNIFORMLY arrays), it keeps unioning/pooling one level deeper, stopping the moment a level's
//! values are heterogeneous (that heterogeneity IS the signal that we've reached a real
//! fixed-schema struct, e.g. `LadderStats`'s mix of numbers/strings/arrays/objects) — at which
//! point normal by-name `diff_object` recursion resumes, so already-named children like `byElo`
//! are still re-gated on their own.
//!
//! `null` is treated as a wildcard on either side: an `Option<T>` field legitimately renders as
//! `null` under thin data (see `model.rs`'s doc), and that alone is never a shape mismatch. An
//! EMPTY collapsed container (object or array) on either side is the same kind of wildcard, one
//! level up: a data-keyed map with zero entries (a legitimately thin/empty M5b sample — see
//! `matchups.rs`'s doc) has no representative element/key to compare, so there's nothing to call a
//! mismatch.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

/// JSON keys whose OWN key set is data (not schema) — see the module doc. Only the OUTERMOST
/// dynamic level needs a name here; [`collapse_dynamic`] finds any further nesting structurally.
const DYNAMIC_MAP_FIELDS: &[&str] = &["civs", "byElo", "byMap", "byPatch"];

/// Returns every structural difference found between `a` and `b`. Empty means "same shape".
pub fn shape_diff(a: &Value, b: &Value) -> Vec<String> {
    let mut out = Vec::new();
    diff_at("$", "", a, b, &mut out);
    out
}

fn diff_at(path: &str, field_name: &str, a: &Value, b: &Value, out: &mut Vec<String>) {
    if a.is_null() || b.is_null() {
        return;
    }
    match (a, b) {
        (Value::Object(ma), Value::Object(mb)) => {
            if DYNAMIC_MAP_FIELDS.contains(&field_name) {
                let collapsed_a = collapse_dynamic(a);
                let collapsed_b = collapse_dynamic(b);
                if is_empty_container(&collapsed_a) || is_empty_container(&collapsed_b) {
                    return;
                }
                diff_at(&format!("{path}.*"), "", &collapsed_a, &collapsed_b, out);
                return;
            }
            diff_object(path, ma, mb, out);
        }
        (Value::Array(xa), Value::Array(xb)) => {
            // Arrays here are homogeneous lists of one element shape (`patches`, `openings`,
            // `ci95`) — comparing the first element of each is enough; an empty array on either
            // side (real under a thin sample) has no shape to compare, not a diff.
            if let (Some(fa), Some(fb)) = (xa.first(), xb.first()) {
                diff_at(&format!("{path}[]"), "", fa, fb, out);
            }
        }
        _ => {
            let (ta, tb) = (type_family(a), type_family(b));
            if ta != tb {
                out.push(format!("{path}: type mismatch ({ta} vs {tb})"));
            }
        }
    }
}

fn is_empty_container(v: &Value) -> bool {
    matches!(v, Value::Object(m) if m.is_empty()) || matches!(v, Value::Array(xs) if xs.is_empty())
}

/// Recursively strips arbitrary-depth "data key -> data key -> ... -> leaf" nesting into one
/// representative leaf shape — see the module doc for the "why" and the worked civ-meta example
/// this preserves exactly. Three cases, checked over `value`'s own non-null children:
/// - all children are objects: union their keys (first-value-wins per key, like the old
///   `merge_map_values` this replaces), then keep collapsing the result — handles both a genuine
///   further data-keyed level (map slugs, opponent slugs, ...) AND a fixed-2-key wrapper whose
///   branches share one schema (civ-meta's `{"1v1": LadderStats, "team": LadderStats}`, where
///   unioning is a no-op since both sides declare the same field names).
/// - all children are arrays: pool every element from every array into ONE flat array, then keep
///   collapsing (lets the existing array-first-element comparison in `diff_at` finish the job).
/// - anything else (mixed types, scalars, or no children at all): return `value` unchanged — this
///   is the terminal case, e.g. `LadderStats`'s own mix of numbers/strings/arrays/objects, or an
///   already-empty map/array.
fn collapse_dynamic(value: &Value) -> Value {
    let Value::Object(map) = value else {
        return value.clone();
    };
    if map.is_empty() {
        return value.clone();
    }
    let mut non_null = map.values().filter(|v| !v.is_null());
    let first = match non_null.next() {
        Some(v) => v,
        None => return value.clone(), // every entry is null — nothing further to collapse
    };
    if first.is_object() && map.values().filter(|v| !v.is_null()).all(Value::is_object) {
        let mut merged: Map<String, Value> = Map::new();
        for entry in map.values().filter_map(|v| v.as_object()) {
            for (k, v) in entry {
                // First NON-NULL value wins per key, but the key itself always survives even if
                // EVERY entry's value for it is null (e.g. `vils_castle` when the whole sample
                // has zero rows for that overlay) — a null placeholder must be upgradable by a
                // later non-null value, and a key that's null EVERYWHERE must still end up
                // present-as-null rather than vanish, so the ordinary null-wildcard rule (not a
                // "missing key" diff) is what judges it. Omitting a key here caused a real
                // false-positive gap caught wiring up M5b's benchmark doc against the live DB —
                // see the task report.
                match merged.get(k) {
                    Some(existing) if !existing.is_null() => {}
                    _ => {
                        merged.insert(k.clone(), v.clone());
                    }
                }
            }
        }
        return collapse_dynamic(&Value::Object(merged));
    }
    if first.is_array() && map.values().filter(|v| !v.is_null()).all(Value::is_array) {
        let mut pooled = Vec::new();
        for items in map.values().filter_map(|v| v.as_array()) {
            pooled.extend(items.iter().cloned());
        }
        return collapse_dynamic(&Value::Array(pooled));
    }
    value.clone()
}

fn diff_object(
    path: &str,
    ma: &Map<String, Value>,
    mb: &Map<String, Value>,
    out: &mut Vec<String>,
) {
    let ka: BTreeSet<&String> = ma.keys().collect();
    let kb: BTreeSet<&String> = mb.keys().collect();
    for k in ka.difference(&kb) {
        out.push(format!("{path}.{k}: present in A, missing in B"));
    }
    for k in kb.difference(&ka) {
        out.push(format!("{path}.{k}: present in B, missing in A"));
    }
    for k in ka.intersection(&kb) {
        diff_at(&format!("{path}.{k}"), k, &ma[*k], &mb[*k], out);
    }
}

fn type_family(v: &Value) -> &'static str {
    match v {
        Value::Null => "null",
        Value::Bool(_) => "bool",
        Value::Number(_) => "number",
        Value::String(_) => "string",
        Value::Array(_) => "array",
        Value::Object(_) => "object",
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn identical_documents_have_no_diff() {
        let doc = json!({"a": 1, "b": {"c": "x"}});
        assert_eq!(shape_diff(&doc, &doc), Vec::<String>::new());
    }

    #[test]
    fn dynamic_map_field_ignores_disjoint_keys() {
        let a = json!({"civs": {"franks": {"games": 1, "tier": "S"}}});
        let b = json!({"civs": {"britons": {"games": 2, "tier": "A"}}});
        assert_eq!(shape_diff(&a, &b), Vec::<String>::new());
    }

    #[test]
    fn dynamic_map_field_still_catches_a_missing_sub_key() {
        let a = json!({"civs": {"franks": {"games": 1, "tier": "S"}}});
        let b = json!({"civs": {"britons": {"games": 2}}});
        let diff = shape_diff(&a, &b);
        assert!(diff.iter().any(|d| d.contains("tier")), "diff={diff:?}");
    }

    #[test]
    fn static_struct_key_mismatch_is_reported() {
        let a = json!({"source": "x", "generated": "2026-01-01"});
        let b = json!({"source": "x"});
        let diff = shape_diff(&a, &b);
        assert!(
            diff.iter().any(|d| d.contains("generated")),
            "diff={diff:?}"
        );
    }

    #[test]
    fn type_mismatch_is_reported() {
        let a = json!({"games": 10});
        let b = json!({"games": "ten"});
        let diff = shape_diff(&a, &b);
        assert!(
            diff.iter().any(|d| d.contains("type mismatch")),
            "diff={diff:?}"
        );
    }

    #[test]
    fn null_is_a_wildcard() {
        let a = json!({"tier": null});
        let b = json!({"tier": "S"});
        assert_eq!(shape_diff(&a, &b), Vec::<String>::new());
    }

    // --- task M5b regression coverage for `collapse_dynamic` ---------------------------------

    #[test]
    fn dynamic_field_whose_entries_are_bare_arrays_still_checks_element_shape() {
        // `civ-matchups.json`'s `civs.<slug> = Array<Opponent>` — no wrapping object at all
        // between the civ slug and the array, unlike civ-meta's `CivEntry`.
        let a = json!({"civs": {"franks": [{"opp": "britons", "games": 1, "winRate": 60.0}]}});
        let b = json!({"civs": {"teutons": [{"opp": "goths", "games": 2}]}});
        let diff = shape_diff(&a, &b);
        assert!(diff.iter().any(|d| d.contains("winRate")), "diff={diff:?}");
    }

    #[test]
    fn two_levels_of_unnamed_dynamic_nesting_are_collapsed() {
        // `civ-matchups-by-map.json`'s `civs.<slug>.<mapSlug> = Array<Opponent>`.
        let a = json!({"civs": {"franks": {"arabia": [{"opp": "britons", "games": 1, "winRate": 60.0}]}}});
        let b = json!({"civs": {"teutons": {"arena": [{"opp": "goths", "games": 2}]}}});
        let diff = shape_diff(&a, &b);
        assert!(diff.iter().any(|d| d.contains("winRate")), "diff={diff:?}");
    }

    #[test]
    fn three_levels_of_unnamed_dynamic_nesting_are_collapsed() {
        // `civ-matchups-by-elo.json`'s `civs.<slug>.<opp>.<bucket> = [winRate, games]`.
        let a = json!({"civs": {"franks": {"britons": {"all": [55.0, 40]}}}});
        let b = json!({"civs": {"teutons": {"goths": {"<1000": [45.0, 30]}}}});
        assert_eq!(shape_diff(&a, &b), Vec::<String>::new());
    }

    #[test]
    fn an_entirely_empty_dynamic_map_on_one_side_is_not_a_diff() {
        // A thin M5b sample (see `matchups.rs`'s doc): the produced `civs` map can be genuinely
        // empty while the committed archive's is not — no representative shape to compare either
        // way, so this must NOT be reported as "every key missing".
        let a = json!({"civs": {}});
        let b = json!({"civs": {"franks": [{"opp": "britons", "games": 1, "winRate": 60.0}]}});
        assert_eq!(shape_diff(&a, &b), Vec::<String>::new());
    }

    #[test]
    fn civ_entry_wrapper_with_shared_schema_branches_still_collapses_like_before() {
        // The ORIGINAL civ-meta case this module was built for: `civs.<slug> = {"1v1":
        // LadderStats|null, "team": LadderStats|null}` must keep behaving exactly as it did
        // before `collapse_dynamic` replaced the old one-level-only `merge_map_values`.
        let a = json!({"civs": {"franks": {"1v1": {"games": 1, "tier": "S"}, "team": null}}});
        let b = json!({"civs": {"britons": {"1v1": null, "team": {"games": 2, "tier": "A"}}}});
        assert_eq!(shape_diff(&a, &b), Vec::<String>::new());
    }
}
