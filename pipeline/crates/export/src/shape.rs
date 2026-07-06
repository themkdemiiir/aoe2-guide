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
//! `null` is treated as a wildcard on either side: an `Option<T>` field legitimately renders as
//! `null` under thin data (see `model.rs`'s doc), and that alone is never a shape mismatch.

use std::collections::BTreeSet;

use serde_json::{Map, Value};

/// JSON keys whose OWN key set is data (not schema) — see the module doc.
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
                let merged_a = merge_map_values(ma);
                let merged_b = merge_map_values(mb);
                diff_at(&format!("{path}.*"), "", &merged_a, &merged_b, out);
                return;
            }
            diff_object(path, ma, mb, out);
        }
        (Value::Array(xa), Value::Array(xb)) => {
            // Arrays here are homogeneous lists of one element shape (`patches`, `openings`,
            // `ci95`) — comparing the first element of each is enough; an empty array on either
            // side (real under the M5a sample's thin data) has no shape to compare, not a diff.
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

/// Unions every key across every entry of a data-keyed map into one representative object, so
/// `diff_at` can compare "the shape of a `civs` entry" instead of "the exact set of civ slugs".
/// Recurses one level deep on nested objects too (e.g. a `civs` entry's own `byElo` map) by
/// picking the first non-null value seen for each key — good enough for a structural merge, since
/// [`diff_at`] itself still recognizes nested dynamic-map fields by name as it walks further in.
fn merge_map_values(map: &Map<String, Value>) -> Value {
    let mut merged = Map::new();
    for value in map.values() {
        if let Value::Object(entry) = value {
            for (k, v) in entry {
                merged.entry(k.clone()).or_insert_with(|| v.clone());
            }
        }
    }
    Value::Object(merged)
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
}
