//! [`RawArchive`] — persists every downloaded `.aoe2record`'s raw bytes losslessly, so an
//! improved parser can re-extract more from the SAME game later, and so a parse FAILURE never
//! loses the replay (see [`crate::crawl`]'s module doc for how this composes with the run loop's
//! download -> parse flow, and [`crate::reparse`] for reading the archive back).
//!
//! ## Layout
//! `<raw_dir>/<match_id % 1000>/<match_id>.aoe2record.zst` — sharded into 1000 subdirectories (by
//! `match_id % 1000`) so no single directory accumulates millions of entries (a flat directory of
//! that size makes `readdir`/backup/`ls` pathologically slow on most filesystems). `match_id` is
//! Relic's own globally-unique match identifier, so no date/profile prefix is needed to avoid
//! collisions. Each entry has a matching `<match_id>.aoe2record.zst.sha256` sidecar — see
//! "Integrity" below.
//!
//! ## Compression
//! `zstd` (the `zstd` crate, libzstd bindings) — LIBRARY-FIRST, no hand-rolled framing. Chosen
//! over gzip/flate2 after measuring both against a real 4.3 MB `.aoe2record`: zstd -19 -> 7.4x
//! (590 KB) vs gzip -9 -> 7.0x (620 KB), AND zstd decompresses far faster — the latter matters
//! more than encode time for the eventual bulk [`crate::reparse`] of millions of archived files
//! (this crawl's write side is already network-rate-limited, so the slower high-level encode is
//! free). [`new_encoder`] is the ONE place the encoder is constructed, specifically so a future
//! trained dictionary (a real win once a corpus of a few thousand replays exists to train on — NOT
//! built now) has a single call site to thread through, instead of touching every write site.
//!
//! ## Integrity — write-then-verify (never trust a fresh compress)
//! This archive is meant to eventually be the ONLY surviving copy of these bytes (the whole point
//! of keeping it is that the in-memory copy is transient), so a silently-corrupted `.zst` would be
//! a PERMANENT loss. Two independent layers guard against that:
//! 1. **zstd's own content checksum** ([`Encoder::include_checksum`]) is enabled on every write —
//!    a bit-flipped `.zst` FAILS LOUD on decompress (`Restored data doesn't match checksum`)
//!    instead of silently returning garbage.
//! 2. **Read-back verification**: after the atomic temp-write + rename (see "Atomicity" below),
//!    [`RawArchive::save`] re-opens the FINAL path, fully decompresses it, and compares its
//!    SHA-256 against the original downloaded bytes' SHA-256 (computed once, before compression).
//!    Only on a match is the save considered to have happened — the original hash is written to
//!    a `.sha256` sidecar file (a manifest-schema column would need a cross-crate change to
//!    `fetch::ReplayManifest`, out of this task's scope — see the module doc's "Also" note in the
//!    task brief; a sidecar is the documented, additive-only alternative) so a much later
//!    [`crate::reparse`] run — long after the in-memory bytes are gone — can still detect disk rot
//!    (bitrot, filesystem corruption) independently of re-deriving the hash. On a MISMATCH the bad
//!    `.zst` is deleted (never left on disk masquerading as a good archive) and the error is
//!    returned — the caller logs it and counts the match as an archive failure; it does NOT retry
//!    the raw save on its own within this run (see [`crate::crawl`]'s "Raw-save failures" note for
//!    why that is a deliberate, documented scope limit, not an oversight).
//!
//! This doubles the CPU cost of a fresh save (compress once, decompress once to verify) and adds
//! one extra read — a deliberate trade against "once saved we may discard the original."
//! Already-present files are NOT re-verified on every skip (see "Idempotency") — that would mean
//! re-decompressing the entire archive on every resumed crawl, which is `reparse`'s job, not a
//! per-run cost.
//!
//! ## Atomicity
//! Every write lands at `<final>.tmp-<pid>-<nonce>` first; the encoder's `finish()` + `sync_all()`
//! close out the zstd frame durably, THEN `rename` lands it at the final path — `rename` is atomic
//! on the same filesystem (POSIX), so a crash mid-write can never leave a truncated `.zst` at the
//! final path for [`RawArchive::exists`] (or a later `reparse`) to trip over. The `.sha256`
//! sidecar is written the same way.
//!
//! ## Idempotency
//! [`RawArchive::save`] checks for a non-empty file at the final path FIRST and returns
//! [`SaveOutcome::AlreadyPresent`] without touching the disk again — a resumed crawl (a match
//! retried after, e.g., a downstream ingest error) never re-compresses/re-writes bytes it already
//! archived. It deliberately does NOT short-circuit the network re-download itself: only a
//! manifest `Error`-status match (a retryable failure that happened AFTER a raw was already saved)
//! can even reach this code path with a pre-existing raw, and that retry still needs
//! `fetch_replay` to re-derive a `ParsedReplay` to attempt the downstream step that actually
//! failed. Skipping the network call there would need a from-disk decode-and-parse path — that is
//! exactly [`crate::reparse`]'s job, not this one's.

use std::fs::File;
use std::io::{self, Write};
use std::path::{Path, PathBuf};

use bytes::Bytes;
use sha2::{Digest, Sha256};

use pipeline_core::MatchId;

/// zstd compression level for the raw archive. See the module doc's "Compression" section for the
/// measurement behind this choice.
const ZSTD_LEVEL: i32 = 19;

/// One [`RawArchive::save`] outcome.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum SaveOutcome {
    /// Written fresh, atomically renamed into place, and read-back-verified against
    /// `sha256` (hex) — see the module doc's "Integrity" section. `bytes_written` is the
    /// COMPRESSED size landed on disk.
    Saved { bytes_written: u64, sha256: String },
    /// A non-empty archive already existed at the target path — left untouched (see
    /// "Idempotency").
    AlreadyPresent,
}

/// The raw `.aoe2record.zst` archive rooted at one directory. See the module doc for layout,
/// compression, integrity, atomicity, and idempotency.
#[derive(Debug)]
pub struct RawArchive {
    root: PathBuf,
}

impl RawArchive {
    pub fn new(root: impl Into<PathBuf>) -> Self {
        RawArchive { root: root.into() }
    }

    /// The final on-disk path for one match's archived replay — `<root>/<match_id %
    /// 1000>/<match_id>.aoe2record.zst`. `rem_euclid` keeps the shard non-negative even for a
    /// (never-expected-in-practice) negative `match_id`.
    pub fn path_for(&self, match_id: MatchId) -> PathBuf {
        let shard = match_id.0.rem_euclid(1000);
        self.root
            .join(shard.to_string())
            .join(format!("{}.aoe2record.zst", match_id.0))
    }

    /// The SHA-256 sidecar path matching [`Self::path_for`] — see the module doc's "Integrity"
    /// section.
    pub fn sha256_sidecar_path(&self, match_id: MatchId) -> PathBuf {
        sidecar_path_for(&self.path_for(match_id))
    }

    /// True if a NON-EMPTY archive already exists for `match_id` (an empty file is treated as not
    /// present — [`Self::save`] never leaves one behind, but a hand-created stray empty file
    /// should not permanently block re-archiving).
    pub fn exists(&self, match_id: MatchId) -> bool {
        std::fs::metadata(self.path_for(match_id))
            .map(|meta| meta.len() > 0)
            .unwrap_or(false)
    }

    /// The SHA-256 (hex) persisted for `match_id` at save time, if a sidecar is present. `None`
    /// covers both "never archived" and "archived before this sidecar existed" — a `reparse` run
    /// treats either the same way (nothing to cross-check against, not an error).
    pub fn read_sha256(&self, match_id: MatchId) -> Option<String> {
        std::fs::read_to_string(self.sha256_sidecar_path(match_id))
            .ok()
            .map(|s| s.trim().to_owned())
    }

    /// Persists `bytes` for `match_id`: compressed, atomically, read-back-verified, skipping
    /// entirely if a non-empty archive is already present. See the module doc's "Integrity" and
    /// "Atomicity" sections for the full guarantee.
    ///
    /// Synchronous, CPU- and IO-bound (zstd -19 compression + a full decompress to verify):
    /// callers on a tokio runtime MUST run this via `spawn_blocking` (the playbook's rule for
    /// "CPU-bound" work) — [`crate::crawl::process_one`] does exactly that.
    pub fn save(&self, match_id: MatchId, bytes: &Bytes) -> io::Result<SaveOutcome> {
        if self.exists(match_id) {
            return Ok(SaveOutcome::AlreadyPresent);
        }

        let final_path = self.path_for(match_id);
        let parent = final_path
            .parent()
            .expect("path_for always nests a file under root/<shard>/");
        std::fs::create_dir_all(parent)?;

        let original_sha256 = sha256_hex(bytes);
        let tmp_path = temp_path_for(parent, &format!("{}.aoe2record.zst", match_id.0));

        let compressed_len = match compress_to(&tmp_path, bytes) {
            Ok(len) => len,
            Err(err) => {
                let _ = std::fs::remove_file(&tmp_path);
                return Err(err);
            }
        };
        std::fs::rename(&tmp_path, &final_path)?;

        // Write-then-verify: never trust a fresh compress+rename until it has been read BACK and
        // confirmed to reproduce the exact original bytes (see the module doc's "Integrity"
        // section) — a corrupt file at the final path is worse than none, so it is deleted rather
        // than left behind on a mismatch.
        if let Err(err) = verify(&final_path, &original_sha256) {
            let _ = std::fs::remove_file(&final_path);
            return Err(err);
        }

        write_sidecar(&sidecar_path_for(&final_path), &original_sha256)?;

        Ok(SaveOutcome::Saved {
            bytes_written: compressed_len,
            sha256: original_sha256,
        })
    }

    /// Enumerates every archived replay currently on disk as `(match_id, path)` pairs — the walk
    /// [`crate::reparse::reparse_dir`] drives. A two-level scan (root -> shard -> file) matching
    /// the layout this module documents, rather than a generic recursive directory walk (the shape
    /// is fixed and known, so no `walkdir`-style dependency is needed — LIBRARY-FIRST cuts both
    /// ways). Returns an empty result (not an error) when `root` doesn't exist yet — an archive
    /// nobody has written to. A file that doesn't parse as `<match_id>.aoe2record.zst` (a stray
    /// temp file left by a crashed process, a `.sha256` sidecar, or unrelated content) is skipped
    /// with a warning rather than aborting the whole walk.
    pub fn iter_saved(&self) -> io::Result<Vec<(MatchId, PathBuf)>> {
        let mut found = Vec::new();
        if !self.root.exists() {
            return Ok(found);
        }
        for shard_entry in std::fs::read_dir(&self.root)? {
            let shard_entry = shard_entry?;
            if !shard_entry.file_type()?.is_dir() {
                continue;
            }
            for file_entry in std::fs::read_dir(shard_entry.path())? {
                let file_entry = file_entry?;
                let path = file_entry.path();
                if !file_entry.file_type()?.is_file() {
                    continue;
                }
                match match_id_from_archive_path(&path) {
                    Some(match_id) => found.push((match_id, path)),
                    None => {
                        tracing::debug!(
                            path = %path.display(),
                            "raw archive: skipping a non-archive entry (sidecar, temp file, or unrelated content)"
                        );
                    }
                }
            }
        }
        Ok(found)
    }
}

/// Recovers a `match_id` from an archived replay's file name (`<match_id>.aoe2record.zst`) —
/// `None` for anything else (`.sha256` sidecars, `.tmp-*` leftovers from a crashed write, or
/// unrelated files someone dropped in the archive directory).
fn match_id_from_archive_path(path: &Path) -> Option<MatchId> {
    let name = path.file_name()?.to_str()?;
    let digits = name.strip_suffix(".aoe2record.zst")?;
    digits.parse::<i64>().ok().map(MatchId)
}

/// The ONE call site that constructs a compressing encoder — see the module doc's "Compression"
/// section for why: a future trained dictionary (built once a corpus of a few thousand archived
/// replays exists) threads through the empty `&[]` below with no other call site to update.
fn new_encoder(file: File) -> io::Result<zstd::stream::write::Encoder<'static, File>> {
    let mut encoder = zstd::stream::write::Encoder::with_dictionary(file, ZSTD_LEVEL, &[])?;
    // Content checksum: a corrupted `.zst` FAILS LOUD on decompress (XXHash64 mismatch) instead
    // of silently returning garbage — see the module doc's "Integrity" section. This archive is
    // meant to eventually be the ONLY copy of these bytes, so trusting an unchecksummed frame is
    // not an option.
    encoder.include_checksum(true)?;
    Ok(encoder)
}

/// Compresses `bytes` into a fresh file at `tmp_path`, fully flushed and fsync'd before
/// returning. Returns the COMPRESSED byte count landed on disk.
fn compress_to(tmp_path: &Path, bytes: &Bytes) -> io::Result<u64> {
    let file = File::create(tmp_path)?;
    let mut encoder = new_encoder(file)?;
    encoder.write_all(bytes)?;
    let mut file = encoder.finish()?;
    file.flush()?;
    file.sync_all()?;
    file.metadata().map(|meta| meta.len())
}

/// Re-opens `path`, fully decompresses it, and confirms its SHA-256 matches
/// `expected_sha256_hex` — see the module doc's "Integrity" section.
fn verify(path: &Path, expected_sha256_hex: &str) -> io::Result<()> {
    let file = File::open(path)?;
    let decompressed = zstd::stream::decode_all(file)?;
    let actual = sha256_hex(&decompressed);
    if actual != expected_sha256_hex {
        return Err(io::Error::new(
            io::ErrorKind::InvalidData,
            format!(
                "raw archive verification failed for {}: sha256 mismatch (expected {expected_sha256_hex}, got {actual})",
                path.display()
            ),
        ));
    }
    Ok(())
}

/// Writes `contents` to `final_path` via the same temp-then-rename discipline as the main
/// archive entry — a torn sidecar write would permanently lose the one recorded hash for a
/// replay whose original bytes are long gone by the time anyone reads it back.
fn write_sidecar(final_path: &Path, contents: &str) -> io::Result<()> {
    let parent = final_path
        .parent()
        .expect("sidecar path always nests under root/<shard>/");
    let file_name = final_path
        .file_name()
        .and_then(|n| n.to_str())
        .unwrap_or("sidecar.sha256");
    let tmp_path = temp_path_for(parent, file_name);
    std::fs::write(&tmp_path, contents.as_bytes())?;
    std::fs::rename(&tmp_path, final_path)
}

fn sidecar_path_for(final_path: &Path) -> PathBuf {
    let mut name = final_path.as_os_str().to_owned();
    name.push(".sha256");
    PathBuf::from(name)
}

/// A collision-avoiding temp-file path for `label` inside `dir` — collision-avoidance only (the
/// `rename` onto the final path is what makes the write atomic; two temp files racing is
/// harmless, and in practice `save`/`write_sidecar` are only ever called once per match per
/// process).
fn temp_path_for(dir: &Path, label: &str) -> PathBuf {
    dir.join(format!(
        "{label}.tmp-{}-{}",
        std::process::id(),
        tmp_nonce()
    ))
}

fn tmp_nonce() -> u64 {
    use std::sync::atomic::{AtomicU64, Ordering};
    static NEXT: AtomicU64 = AtomicU64::new(0);
    NEXT.fetch_add(1, Ordering::Relaxed)
}

fn sha256_hex(bytes: &[u8]) -> String {
    Sha256::digest(bytes)
        .iter()
        .map(|byte| format!("{byte:02x}"))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample_bytes() -> Bytes {
        // Repeated-but-not-trivial content so zstd actually compresses it, mirroring "some
        // structured game data" rather than pure random noise.
        Bytes::from(b"AOE2 replay bytes... ".repeat(2048))
    }

    #[test]
    fn path_for_shards_by_match_id_mod_1000() {
        let archive = RawArchive::new("/tmp/does-not-matter");
        assert_eq!(
            archive.path_for(MatchId(123_456_789)),
            PathBuf::from("/tmp/does-not-matter/789/123456789.aoe2record.zst")
        );
        assert_eq!(
            archive.path_for(MatchId(42)),
            PathBuf::from("/tmp/does-not-matter/42/42.aoe2record.zst")
        );
    }

    #[test]
    fn save_writes_a_file_that_gunzips_back_to_the_original_bytes() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        let bytes = sample_bytes();

        let outcome = archive.save(MatchId(1), &bytes).expect("save must succeed");
        let (bytes_written, sha256) = match outcome {
            SaveOutcome::Saved {
                bytes_written,
                sha256,
            } => (bytes_written, sha256),
            SaveOutcome::AlreadyPresent => panic!("first save must not be AlreadyPresent"),
        };
        assert!(bytes_written > 0, "compressed size must be recorded");
        assert!(
            bytes_written < bytes.len() as u64,
            "repeated content must actually compress"
        );

        let final_path = archive.path_for(MatchId(1));
        assert!(final_path.exists(), "the archived file must exist on disk");

        let restored = zstd::stream::decode_all(std::fs::File::open(&final_path).unwrap())
            .expect("a freshly-saved archive must decompress cleanly");
        assert_eq!(
            restored,
            bytes.to_vec(),
            "decompressing the archive must reproduce the exact original bytes"
        );

        assert_eq!(
            sha256,
            sha256_hex(&bytes),
            "the returned sha256 must be the original bytes' hash"
        );
        assert_eq!(
            archive.read_sha256(MatchId(1)).as_deref(),
            Some(sha256.as_str()),
            "the sidecar's persisted hash must match what save() returned"
        );
    }

    #[test]
    fn save_is_idempotent_and_never_rewrites_an_existing_archive() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        let first = sample_bytes();
        let second = Bytes::from_static(b"different bytes entirely, must never overwrite");

        archive.save(MatchId(2), &first).expect("first save");
        let outcome = archive
            .save(MatchId(2), &second)
            .expect("second save must not error, just skip");
        assert_eq!(outcome, SaveOutcome::AlreadyPresent);

        let restored =
            zstd::stream::decode_all(std::fs::File::open(archive.path_for(MatchId(2))).unwrap())
                .unwrap();
        assert_eq!(
            restored,
            first.to_vec(),
            "the ORIGINAL bytes must survive — a resumed crawl must never clobber an archived replay"
        );
    }

    #[test]
    fn exists_is_false_before_save_and_true_after() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        assert!(!archive.exists(MatchId(3)));
        archive.save(MatchId(3), &sample_bytes()).unwrap();
        assert!(archive.exists(MatchId(3)));
    }

    #[test]
    fn a_corrupted_zst_frame_fails_loud_on_decompress_instead_of_returning_garbage() {
        // Exercises the checksum guarantee directly (requirement 1 of the "Integrity" doc
        // section): compress with the checksum enabled, flip one byte in the compressed stream,
        // and confirm decompression detects it rather than silently returning wrong bytes.
        let mut compressed = Vec::new();
        {
            let mut encoder =
                zstd::stream::write::Encoder::with_dictionary(&mut compressed, ZSTD_LEVEL, &[])
                    .unwrap();
            encoder.include_checksum(true).unwrap();
            encoder.write_all(b"some real replay-shaped bytes").unwrap();
            encoder.finish().unwrap();
        }

        // Flip a byte inside the compressed payload (skip the first few magic-number/header
        // bytes so the corruption lands in the actual frame content, not just a header parse
        // failure — either way it must error, but this targets the checksum specifically).
        let flip_at = compressed.len() - 2;
        compressed[flip_at] ^= 0xFF;

        let result = zstd::stream::decode_all(compressed.as_slice());
        assert!(
            result.is_err(),
            "a corrupted zstd frame with checksums enabled must fail to decompress, never \
             silently return wrong bytes"
        );
    }

    #[test]
    fn verify_rejects_a_final_file_whose_contents_were_corrupted_after_writing() {
        // Exercises `save`'s own write-then-verify call directly (requirement 2): simulates disk
        // rot / a corrupted write landing at the final path, and confirms `verify` (the function
        // `save` relies on before ever returning `Saved`) detects the mismatch.
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        let bytes = sample_bytes();
        archive.save(MatchId(4), &bytes).expect("initial save");

        let final_path = archive.path_for(MatchId(4));
        let mut on_disk = std::fs::read(&final_path).unwrap();
        let flip_at = on_disk.len() - 2;
        on_disk[flip_at] ^= 0xFF;
        std::fs::write(&final_path, &on_disk).unwrap();

        let expected = archive.read_sha256(MatchId(4)).unwrap();
        let err = verify(&final_path, &expected).expect_err("corrupted file must fail verify");
        assert!(
            err.kind() == io::ErrorKind::InvalidData || err.kind() == io::ErrorKind::Other,
            "expected a decompress or hash-mismatch error, got {err:?}"
        );
    }

    #[test]
    fn read_sha256_matches_a_directly_computed_hash_of_the_original_bytes() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        let bytes = sample_bytes();
        archive.save(MatchId(5), &bytes).unwrap();

        let persisted = archive.read_sha256(MatchId(5)).expect("sidecar must exist");
        assert_eq!(persisted, sha256_hex(&bytes));
    }

    #[test]
    fn read_sha256_is_none_when_nothing_was_ever_archived() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        assert_eq!(archive.read_sha256(MatchId(999)), None);
    }

    #[test]
    fn iter_saved_finds_every_archived_match_and_skips_sidecars() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path());
        archive.save(MatchId(10), &sample_bytes()).unwrap();
        archive.save(MatchId(1010), &sample_bytes()).unwrap(); // same shard (10 % 1000 == 1010 % 1000)

        let mut found = archive.iter_saved().unwrap();
        found.sort_by_key(|(id, _)| id.0);
        assert_eq!(
            found.iter().map(|(id, _)| *id).collect::<Vec<_>>(),
            vec![MatchId(10), MatchId(1010)]
        );
        // The `.sha256` sidecars `save` wrote alongside each `.zst` must not show up as their own
        // (bogus) entries.
        assert!(found.iter().all(|(_, path)| path
            .to_str()
            .is_some_and(|s| s.ends_with(".aoe2record.zst"))));
    }

    #[test]
    fn iter_saved_on_a_never_written_root_returns_empty_not_an_error() {
        let dir = tempfile::tempdir().expect("create temp dir");
        let archive = RawArchive::new(dir.path().join("never-created"));
        assert_eq!(archive.iter_saved().unwrap(), Vec::new());
    }
}
