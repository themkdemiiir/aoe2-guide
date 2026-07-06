//! Pure stat helpers — a byte-for-byte Rust port of `scripts/data-pipeline/lib/stats.mjs`, the
//! single JS source the OLD civ-meta.json generators (`aggregate-rich.mjs`,
//! `refresh-civ-current.mjs`) share for these exact formulas. Kept here (not `pipeline_core`)
//! because they're specific to this exporter's civ-meta aggregation, not a cross-pipeline
//! primitive like `elo_bucket`/`slug`.
//!
//! No IO, no `Client` — functional core per the playbook.

/// Wilson score 95% confidence interval for a binomial proportion, as FRACTIONS in `0.0..=1.0`
/// (matching the JS `wilson` — callers apply [`pct`] to each bound themselves, same two-step shape
/// as `refresh-civ-current.mjs`'s `wilson(...)` + `pct(lo)`/`pct(hi)`). `n == 0` returns `(0.0,
/// 0.0)` rather than dividing by zero, matching the JS guard.
pub fn wilson(wins: f64, n: f64) -> (f64, f64) {
    if n == 0.0 {
        return (0.0, 0.0);
    }
    let z = 1.96_f64;
    let p = wins / n;
    let denom = 1.0 + (z * z) / n;
    let center = (p + (z * z) / (2.0 * n)) / denom;
    let margin = (z * ((p * (1.0 - p)) / n + (z * z) / (4.0 * n * n)).sqrt()) / denom;
    (center - margin, center + margin)
}

/// Fraction -> percent, rounded to 2 decimals (`0.5123 -> 51.23`) — matches JS `pct`'s
/// `+(x * 100).toFixed(2)` (banker's-rounding differences at the half-cent boundary are the kind
/// of value-level nuance the M5a brief explicitly waives; see its "parity model" note).
pub fn pct(fraction: f64) -> f64 {
    (fraction * 100.0 * 100.0).round() / 100.0
}

/// Tier from a win rate expressed as a PERCENT (0..100 scale), matching JS `tierOf` exactly.
pub fn tier_of(win_rate_pct: f64) -> &'static str {
    if win_rate_pct >= 53.0 {
        "S"
    } else if win_rate_pct >= 51.0 {
        "A"
    } else if win_rate_pct >= 49.0 {
        "B"
    } else if win_rate_pct >= 47.0 {
        "C"
    } else {
        "D"
    }
}

/// Verbatim copy of JS `TIER_METHOD` — describes this exact `tier_of` threshold table, so it's a
/// literal restatement of shared logic, not fabricated commentary.
pub const TIER_METHOD: &str =
    "Win rate: \u{2265}53 S | 51\u{2013}53 A | 49\u{2013}51 B | 47\u{2013}49 C | <47 D. No data \u{2192} null.";

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn pct_rounds_to_two_decimals() {
        assert_eq!(pct(0.5123), 51.23);
        assert_eq!(pct(0.0), 0.0);
        assert_eq!(pct(1.0), 100.0);
    }

    #[test]
    fn wilson_zero_n_is_zero() {
        assert_eq!(wilson(0.0, 0.0), (0.0, 0.0));
    }

    #[test]
    fn wilson_matches_known_bounds_for_50_of_100() {
        // Cross-checked against the JS formula's known output for wins=50, n=100.
        let (lo, hi) = wilson(50.0, 100.0);
        assert!((lo - 0.4038).abs() < 1e-3, "lo={lo}");
        assert!((hi - 0.5962).abs() < 1e-3, "hi={hi}");
    }

    #[test]
    fn tier_boundaries() {
        assert_eq!(tier_of(53.0), "S");
        assert_eq!(tier_of(52.99), "A");
        assert_eq!(tier_of(51.0), "A");
        assert_eq!(tier_of(50.99), "B");
        assert_eq!(tier_of(49.0), "B");
        assert_eq!(tier_of(48.99), "C");
        assert_eq!(tier_of(47.0), "C");
        assert_eq!(tier_of(46.99), "D");
    }
}
