# Vendored: `aoe2rec`

**Upstream:** https://github.com/aoe2ct/aoe2rec (Rust workspace member `crates/aoe2rec`), MIT.

**Vendored version:** `0.9.0` (matches this directory's `Cargo.toml` `version` field) — upstream
tag `aoe2rec-v0.9.0`, commit `a6b8125c1206aa3b0646fbe3eae436d368640e49`, released 2026-03-08.
Confirmed via `gh api repos/aoe2ct/aoe2rec/tags` + `.../releases` against the pinned Cargo.toml
version — this file itself was added 2026-07-06, ~4 months after that release with no newer
upstream tag published since (`gh api repos/aoe2ct/aoe2rec --jq .pushed_at` was `2026-03-08` at
verification time) — i.e. the vendored copy is genuinely stale relative to upstream's HEAD, not
just old-looking.

**Why vendored (not a git/crates.io dependency):** committed as plain source so the pipeline
workspace builds fully offline/reproducibly (same reasoning as `assets:sync`'s pinned-SHA convention
for the site's icon set) — see `pipeline/Cargo.toml`'s `exclude = ["vendor/aoe2rec"]` comment and
`replay/Cargo.toml`'s dependency comment. **Never hand-edit this directory** — every field the
parser needs is already `pub`; if a fix is needed, vendor a newer upstream release instead of
patching in place (a local patch silently diverges and is invisible to `git diff` against upstream).

**Known risk this pin creates:** `Savegame::from_bytes` (`src/lib.rs`) internally `.unwrap()`s
twice on the header decode (`src/header/mod.rs:14,16` — `yazi::decompress(...).unwrap()` and
`hreader.read_le().unwrap()`) and once more on `ChapterData`'s stream position
(`src/lib.rs:51`). A replay recorded by a NEWER game build than this pinned version understands
(a new header field, a shifted offset, ...) can make the header decode panic instead of returning
`Err`. `pipeline/crates/replay/src/parse.rs::parse_savegame` now catches that panic
(`std::panic::catch_unwind`) and converts it to `Error::Parse` so a stale pin degrades to "this one
replay fails to parse," never a crawler crash — but the UNDERLYING staleness is still real: matches
recorded on a game build released after 2026-03-08 may simply fail to parse at all until this
vendored copy is refreshed.

## Before trusting a new AoE2 game build against this pin

1. Diff upstream's `CHANGELOG.md` (https://github.com/aoe2ct/aoe2rec/blob/main/CHANGELOG.md) between
   `aoe2rec-v0.9.0` and the latest tag — look specifically for header-format / build-id / player-count
   changes (the areas the two `.unwrap()`s above touch).
2. If upstream shipped a fix for the new build, re-vendor: replace this directory's contents with
   the new tagged source (`crates/aoe2rec` in the upstream repo), bump the `version` field in
   `Cargo.toml` to match, and re-run the parity gate (`replay`'s test suite +
   `cargo test -p replay -- --ignored` against a real `.aoe2record` from the new build via
   `TEST_REPLAY=...`).
3. Re-copy the SAME refreshed source into `analyzer/vendor/aoe2rec` too (`pipeline/vendor/aoe2rec`
   and `analyzer/vendor/aoe2rec` are kept byte-identical by convention — see either crate's
   `Cargo.toml` comment).
4. Update the version/commit/date at the top of this file.
