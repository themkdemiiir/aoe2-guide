//! A minimal Dagster Pipes message WRITER for the pipeline binaries, so a Dagster-launched run (the
//! backfill container) reports its outcome as asset-materialization metadata + streams a few logs
//! into the Dagster UI — instead of Dagster's "did not receive any messages from external process"
//! warning and an opaque exit-code-only materialization.
//!
//! There is no official Rust `dagster-pipes` SDK, but the wire protocol is small: Dagster passes
//! `DAGSTER_PIPES_MESSAGES` as a base64-encoded, zlib-compressed JSON that names the message
//! CHANNEL, and the external process writes newline-delimited JSON messages to it, each shaped
//! `{"__dagster_pipes_version": "0.1", "method": ..., "params": ...}`. Two channels are supported —
//! `{"path": "<file>"}` (the `PipesSubprocessClient` default: append to that file) and
//! `{"stdio": "stdout"|"stderr"}` (the `PipesDockerClient` default: write to that stream, which its
//! `PipesDockerLogsMessageReader` parses out of the container's log stream). **The Docker path uses
//! `stdout`**, because a host-side temp file isn't visible inside the container. When the env var is
//! absent (a plain CLI invocation, not under Pipes), every method here is a silent no-op, so the
//! binary behaves identically standalone. Only the WRITE half is implemented (log +
//! report_asset_materialization + the opened/closed lifecycle) — the backfill has no need to READ
//! Dagster's context/extras, so `DAGSTER_PIPES_CONTEXT` is left untouched.
//!
//! A Pipes-side failure (bad env var, unwritable file) must never take down the actual work, so
//! every operation is best-effort: decode failures yield a no-op writer, write failures are ignored.

use std::fs::OpenOptions;
use std::io::{Read, Write};

use base64::Engine as _;
use flate2::read::ZlibDecoder;
use serde_json::{json, Value};

/// The wire-protocol version this writer emits (matches `dagster_pipes`' `PIPES_PROTOCOL_VERSION`).
const PIPES_VERSION: &str = "0.1";

/// Where a `Pipes` writer sends its messages, per the decoded `DAGSTER_PIPES_MESSAGES` channel.
enum Sink {
    /// `{"path": "<file>"}` — append newline-JSON to the file (`PipesSubprocessClient`).
    File(String),
    /// `{"stdio": "stdout"}` — write newline-JSON to this process's stdout, which
    /// `PipesDockerLogsMessageReader` parses out of the container log stream (`PipesDockerClient`).
    Stdout,
    /// `{"stdio": "stderr"}` — same, on stderr.
    Stderr,
}

/// Writes Pipes messages to whichever channel Dagster requested. Holds `None` when not running
/// under Pipes — every method is then a no-op.
pub struct Pipes {
    sink: Option<Sink>,
    /// The step's single asset key, read from the injected context. `report_asset_materialization`
    /// MUST name its asset — Dagster silently drops the reported metadata when the report's
    /// `asset_key` is null and the step's asset can't be inferred.
    asset_key: Option<String>,
}

impl Pipes {
    /// Open from the environment, emitting the required `opened` message when under Pipes.
    pub fn from_env() -> Self {
        let sink = std::env::var("DAGSTER_PIPES_MESSAGES")
            .ok()
            .and_then(|encoded| decode_sink(&encoded));
        let pipes = Pipes {
            sink,
            asset_key: context_asset_key(),
        };
        pipes.write("opened", json!({}));
        pipes
    }

    /// Stream a log line to the Dagster UI. `level` is `"INFO"` / `"WARNING"` / `"ERROR"`.
    pub fn log(&self, level: &str, message: impl Into<String>) {
        self.write("log", json!({ "message": message.into(), "level": level }));
    }

    /// Report the single asset's materialization with `metadata` (each value already in Dagster's
    /// `{"raw_value": .., "type": ".."}` shape). `asset_key: null` lets Dagster infer the one asset
    /// of a single-asset step.
    pub fn report_materialization(&self, metadata: Value) {
        self.write(
            "report_asset_materialization",
            json!({ "asset_key": self.asset_key, "metadata": metadata, "data_version": null }),
        );
    }

    /// Emit the required `closed` message. Call exactly once, last.
    pub fn close(&self) {
        self.write("closed", json!({}));
    }

    fn write(&self, method: &str, params: Value) {
        let Some(sink) = &self.sink else {
            return;
        };
        let line = json!({
            "__dagster_pipes_version": PIPES_VERSION,
            "method": method,
            "params": params,
        })
        .to_string();
        match sink {
            Sink::File(path) => {
                if let Ok(mut f) = OpenOptions::new().create(true).append(true).open(path) {
                    let _ = writeln!(f, "{line}");
                }
            }
            // The logs reader scans every stdout/stderr line; a message line must be flushed on its
            // own so it isn't interleaved with other output mid-line.
            Sink::Stdout => {
                let mut out = std::io::stdout().lock();
                let _ = writeln!(out, "{line}");
                let _ = out.flush();
            }
            Sink::Stderr => {
                let mut err = std::io::stderr().lock();
                let _ = writeln!(err, "{line}");
                let _ = err.flush();
            }
        }
    }
}

/// Read the step's single asset key from `DAGSTER_PIPES_CONTEXT` (base64 → zlib → JSON). The env
/// context injector inlines the context under `"data"`; a file injector points to it via `"path"`.
/// Best-effort: any decode failure → `None` (the report's `asset_key` then stays null).
fn context_asset_key() -> Option<String> {
    let encoded = std::env::var("DAGSTER_PIPES_CONTEXT").ok()?;
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(&encoded)
        .ok()?;
    let mut json_str = String::new();
    ZlibDecoder::new(&compressed[..])
        .read_to_string(&mut json_str)
        .ok()?;
    let outer: Value = serde_json::from_str(&json_str).ok()?;
    let ctx = if let Some(path) = outer.get("path").and_then(Value::as_str) {
        serde_json::from_str::<Value>(&std::fs::read_to_string(path).ok()?).ok()?
    } else {
        outer.get("data")?.clone()
    };
    ctx.get("asset_keys")?
        .as_array()?
        .first()?
        .as_str()
        .map(str::to_owned)
}

/// Decode `DAGSTER_PIPES_MESSAGES` (base64 → zlib-inflate → JSON) into its message [`Sink`] —
/// either a `{"path": ..}` file or a `{"stdio": "stdout"|"stderr"}` stream.
fn decode_sink(encoded: &str) -> Option<Sink> {
    let compressed = base64::engine::general_purpose::STANDARD
        .decode(encoded)
        .ok()?;
    let mut json_str = String::new();
    ZlibDecoder::new(&compressed[..])
        .read_to_string(&mut json_str)
        .ok()?;
    let v: Value = serde_json::from_str(&json_str).ok()?;
    if let Some(path) = v.get("path").and_then(Value::as_str) {
        return Some(Sink::File(path.to_owned()));
    }
    match v.get("stdio").and_then(Value::as_str) {
        Some("stderr") => Some(Sink::Stderr),
        Some("stdout") => Some(Sink::Stdout),
        // An unrecognized/absent channel → no-op rather than guess (never fabricate a sink).
        _ => None,
    }
}
