//! [`FetchClient`] — the rate-limited async HTTP client every Relic call goes through.
//!
//! Three INDEPENDENT constraints, composed (per the playbook — they solve different problems):
//!   1. **rate** — `governor` (GCRA) paces how OFTEN we send, keeping the steady request rate under
//!      the API cap even when nothing is failing;
//!   2. **concurrency** — a `tokio::Semaphore` caps how MANY requests are in flight at once (a
//!      handful of connections, never a flood);
//!   3. **backoff** — `reqwest-retry`'s stock `RetryTransientMiddleware` reacts to 429/5xx with
//!      exponential backoff + jitter. It does NOT honor `Retry-After` (its policy never sees the
//!      response), so a 429 that survives its retries is surfaced as [`Error::RateLimited`] carrying
//!      the header, letting the caller (M6) defer precisely — see `parse_retry_after`.
//!
//! Signed replay-blob URLs carry a short-lived Azure SAS token, so no URL is ever put in an error
//! message or a log line here (the download path is `#[tracing::instrument]`ed by `match_id`
//! instead — see [`crate::api::download_replay`]).

use std::num::NonZeroU32;
use std::sync::Arc;
use std::time::Duration;

use bytes::Bytes;
use governor::{DefaultDirectRateLimiter, Quota, RateLimiter};
use reqwest::header::{HeaderMap, RETRY_AFTER};
use reqwest::StatusCode;
use reqwest_middleware::{ClientBuilder, ClientWithMiddleware};
use reqwest_retry::policies::ExponentialBackoff;
use reqwest_retry::RetryTransientMiddleware;
use tokio::sync::Semaphore;

use crate::config;
use crate::error::{Error, Result};

/// A shared, rate-limited, retrying HTTP client. Cheap to `clone`-share by reference across tasks.
pub struct FetchClient {
    http: ClientWithMiddleware,
    limiter: Arc<DefaultDirectRateLimiter>,
    concurrency: Arc<Semaphore>,
}

impl FetchClient {
    /// Build a client with the default World's-Edge rate (`REPLAYFILES_PER_MIN`) + concurrency.
    pub fn new() -> Result<Self> {
        Self::with_limits(config::REPLAYFILES_PER_MIN, config::MAX_CONCURRENCY)
    }

    /// Build a client with an explicit steady rate (requests/min) and max in-flight concurrency —
    /// e.g. the age archive wants a far slower rate than getReplayFiles.
    pub fn with_limits(per_min: u32, max_concurrency: usize) -> Result<Self> {
        Self::with_limits_and_retries(per_min, max_concurrency, config::RETRY_MAX_RETRIES)
    }

    /// [`Self::with_limits`] with the retry count also explicit — the seam the wiremock tests use
    /// for a deterministic, fast client (0 retries surfaces a mocked 429/5xx immediately, with no
    /// real backoff sleep). Production code always goes through `new`/`with_limits`, which supply
    /// `config::RETRY_MAX_RETRIES`.
    pub(crate) fn with_limits_and_retries(
        per_min: u32,
        max_concurrency: usize,
        max_retries: u32,
    ) -> Result<Self> {
        let inner = reqwest::Client::builder()
            .user_agent(config::USER_AGENT)
            .timeout(Duration::from_secs(config::REQUEST_TIMEOUT_SECS))
            .build()?;
        let retry_policy = ExponentialBackoff::builder().build_with_max_retries(max_retries);
        let http = ClientBuilder::new(inner)
            .with(RetryTransientMiddleware::new_with_policy(retry_policy))
            .build();

        // `.max(1)` + `unwrap_or(MIN)` keeps this panic-free (a 0 rate would otherwise be invalid);
        // the effective floor is one request per minute.
        let quota = Quota::per_minute(NonZeroU32::new(per_min.max(1)).unwrap_or(NonZeroU32::MIN));
        let limiter = Arc::new(RateLimiter::direct(quota));
        let concurrency = Arc::new(Semaphore::new(max_concurrency.max(1)));

        Ok(FetchClient {
            http,
            limiter,
            concurrency,
        })
    }

    /// GET, gated by BOTH the concurrency semaphore and the rate limiter, returning the raw
    /// [`reqwest::Response`] with NO status check — callers that must branch per-status (the
    /// archive's 200/404/429 loop) use this directly.
    pub(crate) async fn get_response(&self, url: &str) -> Result<reqwest::Response> {
        // Hold a concurrency permit for the whole request. `.ok()` keeps this panic-free: the
        // semaphore is owned here and never closed, so acquire can't actually fail — if it somehow
        // did, we proceed unbounded rather than crash an unattended crawl.
        let _permit = self.concurrency.acquire().await.ok();
        self.limiter.until_ready().await;
        Ok(self.http.get(url).send().await?)
    }

    /// GET expecting a successful body: maps a surviving 429 to [`Error::RateLimited`] and any
    /// other non-2xx to [`Error::Status`] (code only — never the signed URL). `endpoint` is a
    /// static label for the error, safe to log.
    pub(crate) async fn get_bytes(&self, url: &str, endpoint: &'static str) -> Result<Bytes> {
        let resp = self.get_response(url).await?;
        let status = resp.status();
        if status == StatusCode::TOO_MANY_REQUESTS {
            return Err(Error::RateLimited {
                retry_after: parse_retry_after(resp.headers()),
            });
        }
        if !status.is_success() {
            return Err(Error::Status {
                status: status.as_u16(),
                endpoint,
            });
        }
        Ok(resp.bytes().await?)
    }
}

/// The server's `Retry-After` in whole seconds, when present as the numeric form (the only form
/// these endpoints use — the HTTP-date form is deliberately not parsed, matching the old client).
pub(crate) fn parse_retry_after(headers: &HeaderMap) -> Option<u64> {
    headers
        .get(RETRY_AFTER)?
        .to_str()
        .ok()?
        .trim()
        .parse::<u64>()
        .ok()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn client_builds_with_valid_and_degenerate_limits() {
        // A normal client builds.
        assert!(FetchClient::new().is_ok());
        // A 0 rate / 0 concurrency must not panic — it floors to 1 (see `with_limits`).
        assert!(FetchClient::with_limits(0, 0).is_ok());
    }

    #[test]
    fn parse_retry_after_reads_numeric_seconds_only() {
        let mut h = HeaderMap::new();
        h.insert(RETRY_AFTER, "120".parse().unwrap());
        assert_eq!(parse_retry_after(&h), Some(120));

        // HTTP-date form is intentionally not parsed.
        let mut h2 = HeaderMap::new();
        h2.insert(RETRY_AFTER, "Wed, 21 Oct 2026 07:28:00 GMT".parse().unwrap());
        assert_eq!(parse_retry_after(&h2), None);

        assert_eq!(parse_retry_after(&HeaderMap::new()), None);
    }
}
