//! [`ReplaySource`] — the seam between the crawl loop ([`crate::crawl`]) and `fetch`'s Relic
//! adapter, abstracting the two IO operations the loop needs: discover a profile's recent
//! matches, and fetch+parse one match's replay. Existing purely so `crawl`'s own tests
//! (`tests/crawl.rs`) can inject a FAKE and never touch the live Relic API or a real
//! `.aoe2record` binary — see the brief's "NO live Relic API call in any automated test/gate"
//! rule.
//!
//! The seam is drawn at "fetch bytes + parse them" (not raw HTTP verbs) for a concrete reason:
//! this repo commits no real `.aoe2record` fixture (`replay/tests/fixture_smoke.rs`'s own doc —
//! replay files are large binary data with unclear redistribution rights), so a test double
//! below this line would need to hand-roll aoe2rec's binary format, which is impractical. A test
//! double at the [`ParsedReplay`] level needs no such fixture: [`replay::ParsedReplay`]'s fields
//! are all `pub`, exactly like `pipeline::compose`'s own existing tests already construct them.
//!
//! [`FetchSource`] is the ONE production implementation — it calls the REAL
//! `fetch::discover_recent` / `fetch::get_replay_files` / `fetch::best_file` /
//! `fetch::download_replay` / `replay::parse`, in that order, exactly per the task brief's
//! discover -> download -> parse sequence. It deliberately does NOT attempt the age-archive
//! fallback (`fetch::download_archive_replay`): that endpoint's harsh, match-specific rate limit
//! is its own orchestration-level pacing decision (see `fetch::config`'s doc), scoped to a later
//! milestone. A match that ages out of the fast path here is reported as [`ReplayFetch::NoReplay`]
//! (-> `MatchStatus::Expired`), which a later milestone's archive branch can still retry.

use std::future::Future;

use bytes::Bytes;
use fetch::{
    best_file, discover_recent, download_replay, get_replay_files, DiscoverySeed, FetchClient,
};
use pipeline_core::{MatchId, ProfileId};
use replay::ParsedReplay;

/// One match's fetch+parse outcome. A closed, small vocabulary distinct from `fetch::Result`
/// because it spans two crates' error surfaces (`fetch::Error` for the download, `replay::Error`
/// for the parse) plus a genuinely-just-missing case — see [`crate::crawl`]'s `MatchOutcome` for
/// how each variant maps to a manifest status.
#[derive(Debug, Clone)]
pub enum ReplayFetch {
    /// Downloaded and parsed successfully.
    Parsed(ParsedReplay),
    /// No usable replay file at the fast-path source (aged out of `getReplayFiles`, or no file
    /// was ever uploaded) — terminal until a later milestone's archive fallback retries it.
    NoReplay,
    /// The download itself failed — transient (network, rate limit). `retry_after` is populated
    /// only for a genuine [`fetch::Error::RateLimited`] (seconds the server asked us to wait).
    FetchFailed {
        message: String,
        retry_after: Option<u64>,
    },
    /// The bytes downloaded but `replay::parse` rejected them — deterministic (corrupt/unsupported
    /// record), so terminal: retrying without new bytes would fail identically.
    ParseFailed(String),
}

/// The IO the crawl loop needs from a Relic replay source, abstracted for testability. See the
/// module doc for why this trait lives in `pipeline` (not `fetch`) and where the seam is drawn.
///
/// The methods return `impl Future<..> + Send` (rather than plain `async fn`) because
/// [`crate::crawl::crawl`] hands the returned futures to `tokio::spawn` for bounded-concurrency
/// worker tasks — that requires a `Send` bound the generic caller can actually name, which plain
/// `async fn`-in-traits sugar cannot express.
pub trait ReplaySource: Send + Sync {
    /// Recent ranked matches for one profile (see [`fetch::discover_recent`]).
    fn discover(
        &self,
        profile_id: ProfileId,
    ) -> impl Future<Output = fetch::Result<Vec<DiscoverySeed>>> + Send + '_;

    /// Fetch + parse one match. See [`ReplayFetch`] for the outcome vocabulary.
    fn fetch_replay(&self, match_id: MatchId) -> impl Future<Output = ReplayFetch> + Send + '_;
}

/// Production [`ReplaySource`]: composes `fetch`'s public discovery + fast-path download
/// functions and `replay::parse`, against the real Relic API via a shared, rate-limited
/// [`FetchClient`].
pub struct FetchSource {
    client: FetchClient,
}

impl FetchSource {
    pub fn new(client: FetchClient) -> Self {
        FetchSource { client }
    }
}

/// Maps a [`fetch::Error`] to [`ReplayFetch::FetchFailed`], preserving `retry_after` only for a
/// genuine rate-limit signal — see [`crate::crawl`]'s "sustained-429 pauses politely" handling.
fn fetch_failed(err: fetch::Error) -> ReplayFetch {
    match err {
        fetch::Error::RateLimited { retry_after } => ReplayFetch::FetchFailed {
            message: "relic api rate-limited".to_owned(),
            retry_after,
        },
        other => ReplayFetch::FetchFailed {
            message: other.to_string(),
            retry_after: None,
        },
    }
}

impl ReplaySource for FetchSource {
    // Implemented via plain `async fn` sugar even though the trait declares the explicit
    // `-> impl Future<..> + Send + '_` RPITIT form — rustc accepts an `async fn` impl of an
    // RPITIT-declared trait method (the desugared opaque type is compatible), and clippy's
    // `manual_async_fn` lint requires this form whenever the body is just one async block.
    async fn discover(&self, profile_id: ProfileId) -> fetch::Result<Vec<DiscoverySeed>> {
        discover_recent(&self.client, profile_id).await
    }

    async fn fetch_replay(&self, match_id: MatchId) -> ReplayFetch {
        let files = match get_replay_files(&self.client, &[match_id]).await {
            Ok(files) => files,
            Err(err) => return fetch_failed(err),
        };
        let Some(list) = files.get(&match_id) else {
            return ReplayFetch::NoReplay;
        };
        let Some(best) = best_file(list) else {
            return ReplayFetch::NoReplay;
        };
        // `best_file`'s own invariant guarantees `url.is_some()` for anything it returns, but
        // this crate never `.expect()`s that — an unexpectedly missing URL is treated the same as
        // "nothing usable" rather than panicking (playbook rule 8).
        let Some(url) = best.url.as_deref() else {
            return ReplayFetch::NoReplay;
        };
        let bytes: Bytes = match download_replay(&self.client, match_id, url).await {
            Ok(bytes) => bytes,
            Err(err) => return fetch_failed(err),
        };

        // CPU-bound decode off the async runtime thread (playbook: "spawn_blocking for ...
        // CPU-bound replay parse").
        match tokio::task::spawn_blocking(move || replay::parse(match_id, bytes)).await {
            Ok(Ok(parsed)) => ReplayFetch::Parsed(parsed),
            Ok(Err(err)) => ReplayFetch::ParseFailed(err.to_string()),
            Err(join_err) => {
                ReplayFetch::ParseFailed(format!("parse task did not complete: {join_err}"))
            }
        }
    }
}
