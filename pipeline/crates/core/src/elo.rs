//! ELO -> display bucket. Thresholds are the historical `games.elo_bucket` bands and MUST stay in
//! sync with the two other implementations of this exact table:
//! `replay-rs/src/analyze/data.rs::elo_bucket` and
//! `scripts/data-pipeline/lib/buckets.mjs::eloBucket`/`eloCaseSql`.

/// The nine display buckets, as a closed domain enum — previously the `ELO_BUCKETS` string array
/// and the `elo_bucket` match arms carried this same nine-way split independently, tied together
/// only by a containment test. See [`EloBucket::of`] for the thresholds and [`EloBucket::as_str`]
/// for the exact display string each variant maps to.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum EloBucket {
    Under1000,
    R1000To1199,
    R1200To1399,
    R1400To1649,
    R1650To1799,
    R1800To1999,
    R2000To2199,
    R2200To2499,
    R2500Plus,
}

impl EloBucket {
    /// Every variant, in ascending order.
    pub const ALL: [EloBucket; 9] = [
        EloBucket::Under1000,
        EloBucket::R1000To1199,
        EloBucket::R1200To1399,
        EloBucket::R1400To1649,
        EloBucket::R1650To1799,
        EloBucket::R1800To1999,
        EloBucket::R2000To2199,
        EloBucket::R2200To2499,
        EloBucket::R2500Plus,
    ];

    /// The exact display string — byte-for-byte identical to the historical `ELO_BUCKETS`/JS
    /// strings that `games.elo_bucket` and downstream consumers depend on.
    pub const fn as_str(self) -> &'static str {
        match self {
            EloBucket::Under1000 => "<1000",
            EloBucket::R1000To1199 => "1000-1199",
            EloBucket::R1200To1399 => "1200-1399",
            EloBucket::R1400To1649 => "1400-1649",
            EloBucket::R1650To1799 => "1650-1799",
            EloBucket::R1800To1999 => "1800-1999",
            EloBucket::R2000To2199 => "2000-2199",
            EloBucket::R2200To2499 => "2200-2499",
            EloBucket::R2500Plus => "2500+",
        }
    }

    /// Buckets a known (non-null) ELO rating. See [`elo_bucket`] for the missing-rating caveat —
    /// it applies here identically. Keeps the `2500+` top band open-ended (a real open bound, not
    /// a lazy catch-all).
    pub const fn of(elo: i32) -> EloBucket {
        match elo {
            i32::MIN..=999 => EloBucket::Under1000,
            1000..=1199 => EloBucket::R1000To1199,
            1200..=1399 => EloBucket::R1200To1399,
            1400..=1649 => EloBucket::R1400To1649,
            1650..=1799 => EloBucket::R1650To1799,
            1800..=1999 => EloBucket::R1800To1999,
            2000..=2199 => EloBucket::R2000To2199,
            2200..=2499 => EloBucket::R2200To2499,
            _ => EloBucket::R2500Plus,
        }
    }
}

/// The nine display buckets, in ascending order. Mirrors JS `ELO_BUCKETS`. Derived from
/// [`EloBucket::ALL`] so the string list and the enum can never drift apart again.
///
/// Written as an explicit const loop rather than `EloBucket::ALL.map(EloBucket::as_str)`: `Iterator`/
/// `array::map` are not yet usable in a `const` context on this toolchain (rust-lang/rust#143874).
pub const ELO_BUCKETS: [&str; 9] = {
    let mut out = [""; 9];
    let mut i = 0;
    while i < EloBucket::ALL.len() {
        out[i] = EloBucket::ALL[i].as_str();
        i += 1;
    }
    out
};

/// Buckets a known (non-null) ELO rating.
///
/// A missing rating is NOT this function's concern: the JS mirror (`eloCaseSql`) always carries
/// an explicit `IS NULL -> 'unknown'` branch ahead of these thresholds, and callers here must
/// decide the same way — drop the row or label it "unknown" — rather than have this function
/// fabricate a band for absent data.
///
/// Thin back-compat wrapper over [`EloBucket::of`] for existing string-based callers; prefer the
/// enum in new code.
pub fn elo_bucket(elo: i32) -> &'static str {
    EloBucket::of(elo).as_str()
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

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

    proptest! {
        /// For every possible i32 (not just the boundary probes above), `elo_bucket` must return
        /// one of the nine known strings, and it must never disagree with `EloBucket::of(..)`.
        #[test]
        fn elo_bucket_is_always_a_known_bucket(elo in any::<i32>()) {
            let bucketed = elo_bucket(elo);
            prop_assert!(ELO_BUCKETS.contains(&bucketed));
            prop_assert_eq!(EloBucket::of(elo).as_str(), bucketed);
        }
    }
}
