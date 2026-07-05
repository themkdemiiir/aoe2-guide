//! `replay::Error` — the closed failure surface of [`crate::parse`].
//!
//! Deliberately does NOT `#[from]` aoe2rec's own error type: `aoe2rec::Savegame::from_bytes`
//! returns `Result<Savegame, Box<dyn std::error::Error>>` (no `Send + Sync` bound). Boxing that
//! straight into a variant would make THIS enum lose `Send + Sync` too, which breaks the
//! playbook's "thiserror at library edges, anyhow at binary edges" contract — a binary can't
//! `.context()` a non-`Send`/`Sync` error into `anyhow::Result`. So [`Error::Parse`] captures the
//! message via `.to_string()` instead (the OLD extractor did the same thing, via
//! `anyhow!(e.to_string())`), keeping every variant here `Send + Sync + 'static`.

use thiserror::Error;

/// [`crate::parse`]'s result alias.
pub type Result<T> = std::result::Result<T, Error>;

#[derive(Debug, Error)]
pub enum Error {
    /// `aoe2rec::Savegame::from_bytes` rejected the bytes (truncated file, unsupported
    /// version, corrupt header, ...). The message is aoe2rec's own `Display` text.
    #[error("failed to parse .aoe2record: {0}")]
    Parse(String),

    /// A value read from the replay didn't fit the DB-native signed width `parse` narrows it
    /// to (`i32`/`i16`). Real games never hit this — it means a corrupt or adversarial record;
    /// see the crate's "no silent `as` narrowing" rule.
    #[error("integer overflow converting replay field {field}: {source}")]
    Overflow {
        field: &'static str,
        #[source]
        source: std::num::TryFromIntError,
    },

    /// A research target_id decoded to an age-marker id that isn't one of the closed
    /// `dark`/`feudal`/`castle`/`imperial` vocabulary `pipeline_core::Age` covers.
    #[error("unexpected age in replay data: {0}")]
    UnknownAge(#[from] pipeline_core::age::UnknownAge),
}

/// Curries the field name so call sites read as `.map_err(overflow("meta.n_players"))?`.
pub(crate) fn overflow(field: &'static str) -> impl FnOnce(std::num::TryFromIntError) -> Error {
    move |source| Error::Overflow { field, source }
}
