//! The ONE canonical map/name slug: lowercase ASCII-alphanumeric only, everything else dropped.
//!
//! MUST match `replay-rs/src/analyze/maps.rs::slug` exactly (the `games.map` format depends on
//! it). The JS mirror is `scripts/data-pipeline/lib/buckets.mjs::canonMap`, which additionally
//! strips a trailing file extension (`.replace(/\.[a-z0-9]+$/i, "")`) before dropping
//! non-alphanumerics — that extra step is intentionally NOT ported here: `canonMap` is sometimes
//! applied to raw map *filenames*, but this crate only ever slugs map *names* (as read from
//! `maps.tsv`), which never carry a file extension, so the strip would be a no-op at best.
//!
//! No regex: ASCII-filter + lowercase is sufficient and keeps this crate regex-free by design.

/// Lowercases and strips every non-ASCII-alphanumeric character from `name`.
///
/// ```
/// assert_eq!(pipeline_core::slug::slug("Black Forest"), "blackforest");
/// ```
pub fn slug(name: &str) -> String {
    name.chars()
        .filter(|c| c.is_ascii_alphanumeric())
        .map(|c| c.to_ascii_lowercase())
        .collect()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::*;

    #[test]
    fn slugs_known_map_names() {
        assert_eq!(slug("Black Forest"), "blackforest");
        assert_eq!(slug("African Clearing"), "africanclearing");
        assert_eq!(slug("Arabia"), "arabia");
    }

    proptest! {
        /// `slug` only ever drops characters and lowercases the rest, so re-slugging an
        /// already-slugged string is a no-op.
        #[test]
        fn slug_is_idempotent(s in ".*") {
            let once = slug(&s);
            let twice = slug(&once);
            prop_assert_eq!(once, twice);
        }

        /// The only characters `slug` ever keeps are lowercase ASCII alphanumerics — it never
        /// introduces a hyphen or any other separator (unlike the JS `canonMap` mirror's
        /// extension-stripping step, deliberately not ported here — see the module doc).
        #[test]
        fn slug_output_is_lowercase_ascii_alphanumeric(s in ".*") {
            let out = slug(&s);
            prop_assert!(out.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit()));
        }
    }
}
