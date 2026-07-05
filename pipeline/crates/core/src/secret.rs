//! [`Secret`] — a compile-time second layer of defense behind [`crate::redact_secret`].
//!
//! `redact_secret` does string-level redaction of whatever text a connect error happens to echo
//! back at us; it cannot stop a value from being accidentally logged/printed/serialized in the
//! first place. `Secret` closes that gap for the raw value itself (`DATABASE_URL` today, API
//! tokens later): its `Debug`/`Display` always print `"<redacted>"`, and it deliberately does NOT
//! implement `Serialize`, `Deref`, or any `Clone`-to-`String` escape hatch, so accidentally
//! logging or serializing a `Secret` is a compile error rather than a leak. The one sanctioned way
//! to read the value back out is [`Secret::expose`], named to make every call site visually stand
//! out as the point where the "no plain-text secrets" rule is deliberately, momentarily lifted.

use std::fmt;

/// A sensitive string that must never be printed, logged, or serialized as itself. See the module
/// doc for the full rationale.
pub struct Secret(String);

impl Secret {
    /// Wraps `value` as a [`Secret`].
    pub fn new(value: String) -> Self {
        Secret(value)
    }

    /// Returns the real value. Named loudly on purpose — every call site is a deliberate,
    /// visible exception to "never print/log/serialize this".
    pub fn expose(&self) -> &str {
        &self.0
    }
}

impl fmt::Debug for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted>")
    }
}

impl fmt::Display for Secret {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "<redacted>")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn debug_and_display_never_print_the_value() {
        let secret = Secret::new("postgres://user:hunter2@host/db".to_owned());
        assert_eq!(format!("{secret:?}"), "<redacted>");
        assert_eq!(format!("{secret}"), "<redacted>");
    }

    #[test]
    fn expose_returns_the_real_value() {
        let secret = Secret::new("postgres://user:hunter2@host/db".to_owned());
        assert_eq!(secret.expose(), "postgres://user:hunter2@host/db");
    }
}
