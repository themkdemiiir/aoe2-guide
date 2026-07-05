//! [`Age`] — the closed, four-value age vocabulary shared by `replay_ages`/`match_ages.age`
//! (Postgres `TEXT`).
//!
//! The column is populated from two different producers that do NOT agree on how many ages they
//! emit: the replay extractor only ever writes three (`config::AGES` in
//! `scripts/data-pipeline/replay-rs/src/config.rs`) because a replay never records a player still
//! in the Dark Age at the point it summarizes ages — but the aoestats summariser writes all four,
//! `dark` included. [`Age`] must therefore cover all four so a future `replay`/`fetch` producer
//! can share one enum with `ingest` instead of each guessing its own subset.
//!
//! Deliberately closed (no `#[non_exhaustive]`, no catch-all variant): these four strings are the
//! entire real vocabulary of the column today, verified against both producers above. An unknown
//! age string is a data-corruption signal, not a new legitimate value — [`Age::try_from`] fails
//! loud on it rather than silently coercing or dropping the row (see the crate-level "no
//! defaults" rule).

use std::fmt;

use serde::{Deserialize, Serialize};

/// One of the four AOE2 ages, as written to `replay_ages`/`match_ages.age`. See the module doc
/// for why all four (not just the replay extractor's three) must be represented here.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Serialize, Deserialize)]
#[serde(rename_all = "lowercase")]
pub enum Age {
    Dark,
    Feudal,
    Castle,
    Imperial,
}

impl Age {
    /// The exact lowercase string this variant reads/writes as in the `age` TEXT column — MUST
    /// match both the `#[serde(rename_all = "lowercase")]` wire form and the DB value.
    pub const fn as_db_str(self) -> &'static str {
        match self {
            Age::Dark => "dark",
            Age::Feudal => "feudal",
            Age::Castle => "castle",
            Age::Imperial => "imperial",
        }
    }
}

impl fmt::Display for Age {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.as_db_str())
    }
}

/// An age string outside the closed four-value vocabulary (see the module doc for why this must
/// fail loud rather than silently default/drop).
#[derive(Debug, Clone, PartialEq, Eq, thiserror::Error)]
#[error("unknown age {0:?} (must be one of: dark, feudal, castle, imperial)")]
pub struct UnknownAge(pub String);

impl TryFrom<&str> for Age {
    type Error = UnknownAge;

    fn try_from(value: &str) -> Result<Self, Self::Error> {
        match value {
            "dark" => Ok(Age::Dark),
            "feudal" => Ok(Age::Feudal),
            "castle" => Ok(Age::Castle),
            "imperial" => Ok(Age::Imperial),
            other => Err(UnknownAge(other.to_owned())),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn round_trips_all_four_ages() {
        for (age, expected) in [
            (Age::Dark, "dark"),
            (Age::Feudal, "feudal"),
            (Age::Castle, "castle"),
            (Age::Imperial, "imperial"),
        ] {
            assert_eq!(age.as_db_str(), expected);
            assert_eq!(age.to_string(), expected);
            assert_eq!(Age::try_from(expected), Ok(age));
            assert_eq!(
                serde_json::to_string(&age).unwrap(),
                format!("\"{expected}\"")
            );
            assert_eq!(serde_json::from_str::<Age>(&format!("\"{expected}\"")).unwrap(), age);
        }
    }

    #[test]
    fn try_from_fails_loud_on_unknown_age() {
        assert_eq!(
            Age::try_from("fuedal"),
            Err(UnknownAge("fuedal".to_owned()))
        );
        assert_eq!(Age::try_from(""), Err(UnknownAge(String::new())));
        assert_eq!(
            Age::try_from("Feudal"),
            Err(UnknownAge("Feudal".to_owned())),
            "must be case-sensitive lowercase, matching the DB values exactly"
        );
    }
}
