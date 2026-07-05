//! ELO -> display bucket. Thresholds are the historical `games.elo_bucket` bands and MUST stay in
//! sync with the two other implementations of this exact table:
//! `replay-rs/src/analyze/data.rs::elo_bucket` and
//! `scripts/data-pipeline/lib/buckets.mjs::eloBucket`/`eloCaseSql`.

/// The nine display buckets, in ascending order. Mirrors JS `ELO_BUCKETS`.
pub const ELO_BUCKETS: [&str; 9] = [
    "<1000",
    "1000-1199",
    "1200-1399",
    "1400-1649",
    "1650-1799",
    "1800-1999",
    "2000-2199",
    "2200-2499",
    "2500+",
];

/// Buckets a known (non-null) ELO rating.
///
/// A missing rating is NOT this function's concern: the JS mirror (`eloCaseSql`) always carries
/// an explicit `IS NULL -> 'unknown'` branch ahead of these thresholds, and callers here must
/// decide the same way — drop the row or label it "unknown" — rather than have this function
/// fabricate a band for absent data.
pub fn elo_bucket(elo: i32) -> &'static str {
    match elo {
        i32::MIN..=999 => "<1000",
        1000..=1199 => "1000-1199",
        1200..=1399 => "1200-1399",
        1400..=1649 => "1400-1649",
        1650..=1799 => "1650-1799",
        1800..=1999 => "1800-1999",
        2000..=2199 => "2000-2199",
        2200..=2499 => "2200-2499",
        _ => "2500+",
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn buckets_every_boundary() {
        assert_eq!(elo_bucket(999), "<1000");
        assert_eq!(elo_bucket(1000), "1000-1199");
        assert_eq!(elo_bucket(1399), "1200-1399");
        assert_eq!(elo_bucket(1400), "1400-1649");
        assert_eq!(elo_bucket(1649), "1400-1649");
        assert_eq!(elo_bucket(1650), "1650-1799");
        assert_eq!(elo_bucket(2499), "2200-2499");
        assert_eq!(elo_bucket(2500), "2500+");
    }

    #[test]
    fn bucket_list_matches_bucket_fn_output() {
        // Every string elo_bucket can return must be a member of ELO_BUCKETS.
        for probe in [999, 1000, 1399, 1400, 1649, 1650, 2499, 2500] {
            assert!(ELO_BUCKETS.contains(&elo_bucket(probe)));
        }
    }
}
