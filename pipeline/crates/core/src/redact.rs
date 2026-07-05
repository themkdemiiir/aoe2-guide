//! Redact the `DATABASE_URL` secret (and its password) from error messages before logging.
//!
//! Every crate in this workspace that connects with `DATABASE_URL` (migration, ingest, and
//! future producers/exporters) shares the same connect-error leak risk: the database driver's
//! own URL-parse-failure error echoes the whole connection string verbatim. This logic must
//! never diverge between crates, so it lives here once and every caller re-uses it.

/// Remove the `DATABASE_URL` and its password from an error message before logging, so a
/// malformed/rejected connection string — the database driver's URL-parse error echoes the whole
/// connection string verbatim — can never leak the secret. Redacts the full URL substring
/// (catches the verbatim echo) and, if the URL parses far enough to expose a password, the
/// password substring on its own (catches partial echoes).
pub fn redact_secret(message: &str, database_url: &str) -> String {
    if database_url.is_empty() {
        // An empty needle would make `str::replace` insert `<DATABASE_URL redacted>` between
        // every character of `message`, corrupting it instead of redacting anything.
        return message.to_owned();
    }

    let mut redacted = message.replace(database_url, "<DATABASE_URL redacted>");

    if let Some(password) = url::Url::parse(database_url)
        .ok()
        .and_then(|url| url.password().map(str::to_owned))
        .filter(|password| !password.is_empty())
    {
        redacted = redacted.replace(&password, "<redacted>");
    }

    redacted
}

#[cfg(test)]
mod tests {
    use proptest::prelude::*;

    use super::redact_secret;

    /// Mirrors the reviewer's repro: a malformed `DATABASE_URL` makes sqlx echo the whole
    /// connection string (password included) back into the error message. `redact_secret` must
    /// strip both the password and the full URL so neither ever reaches a log line.
    #[test]
    fn redact_secret_strips_password_and_full_url() {
        let database_url = "postgres://myuser:SUPER_SECRET_MARKER_PW@host/db";
        let message = format!(
            "failed to connect to the database: The connection string '{database_url}' cannot be parsed."
        );

        let redacted = redact_secret(&message, database_url);

        assert!(!redacted.contains("SUPER_SECRET_MARKER_PW"));
        assert!(!redacted.contains(database_url));
    }

    /// The real bug: a malformed connection string (bad IPv6 host) fails `url::Url::parse`
    /// entirely, so the password-specific redaction pass never runs. Only the unconditional
    /// full-string replacement can catch it — this pins that behavior down.
    #[test]
    fn redact_secret_strips_full_url_when_parsing_fails() {
        let database_url = "postgres://myuser:SUPER_SECRET_MARKER_PW@[::1";
        assert!(
            url::Url::parse(database_url).is_err(),
            "test fixture must be unparseable to exercise the parse-failure path"
        );

        let message = format!(
            "failed to connect to the database: The connection string '{database_url}' cannot be parsed."
        );

        let redacted = redact_secret(&message, database_url);

        assert!(!redacted.contains("SUPER_SECRET_MARKER_PW"));
        assert!(!redacted.contains(database_url));
    }

    proptest! {
        /// `redact_secret` must never panic, whatever shape `message`/`database_url` take —
        /// including non-UTF8-adjacent unicode, empty strings, and pathological overlaps.
        #[test]
        fn redact_secret_never_panics(message in ".*", database_url in ".*") {
            let _ = redact_secret(&message, &database_url);
        }

        /// The empty-url guard: an empty needle must return the message unchanged rather than
        /// corrupting it (the bug `redact_secret`'s doc comment calls out by name).
        #[test]
        fn redact_secret_empty_url_is_a_no_op(message in ".*") {
            prop_assert_eq!(redact_secret(&message, ""), message);
        }

        /// For any non-empty url, the redacted output never contains that url as a substring —
        /// the whole point of the function.
        #[test]
        fn redact_secret_strips_non_empty_url(message in ".*", database_url in ".+") {
            let redacted = redact_secret(&message, &database_url);
            prop_assert!(!redacted.contains(database_url.as_str()));
        }
    }
}
