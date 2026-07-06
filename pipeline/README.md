# pipeline

A Rust workspace for the AOE2 guide's data pipeline. Crates are added one milestone at a time;
today it holds `crates/core` (`pipeline-core`), `crates/migration`, `crates/dims`, `crates/ingest`,
`crates/replay`, `crates/fetch`, `crates/aoestats`, and `crates/pipeline`, with `export` coming in a
later milestone. This is a separate workspace from the repo-root `analyzer/` (the in-browser WASM
coach) — the two share no crates or Cargo.lock.

- **`core`** (`pipeline-core`) is the typed, regex-free home for the slug/elo-bucket/map/civ/patch
  lookups that today live duplicated across JS (`scripts/data-pipeline/lib/buckets.mjs`) and Rust
  (`replay-rs/src/analyze/{maps.rs,data.rs}`), plus shared id newtypes, `Age`, `Secret`, and the
  `core::cli` binary bootstrap every pipeline binary uses. Its civ/map/patch lookup tables are
  gated behind the opt-in `refdata` feature (NOT a default — see the comment in
  `crates/core/Cargo.toml`): enable it explicitly with `--features pipeline-core/refdata` (or via
  `--all-features`) where those lookups are actually needed and the repo-root data files they
  `include_str!` are present.
- **`migration`** carries the SeaORM migrations for the gameplay-first PostgreSQL schema.
- **`dims`** idempotently loads the reference dimension tables (`maps`/`civs`/`civs_relic`/
  `patch_index`) from `core`'s `refdata` lookups — a prerequisite for `ingest`, since every match
  row FK-references `maps`/`civs`.
- **`ingest`** is the idempotent binary-COPY bulk loader from typed replay row batches into that
  schema.
- **`aoestats`** idempotently loads the historical aoestats parquet archive (`matches`/
  `match_players`, `matches.source = 'aoestats'`) and, separately, its per-age build summaries
  (`match_ages`, from the `replay_summary_raw` column). The ages path (`import-ages`) requires
  **`python3` on `PATH`** — `replay_summary_raw` is a Python-repr blob (not JSON), so it's parsed
  via `ast.literal_eval` in `pipeline/py/aoestats_summaries.py`, run as a subprocess (`src/py.rs`);
  this is the one place the whole pipeline shells out to a non-Rust tool. `import`/the
  matches+players path has no such dependency.

`core` becomes the shared data core that JS callers cut over to in a later milestone.

## Testing

The full test gate is `cargo test --workspace --all-features` (this is also what CI runs — see
`.github/workflows/pipeline.yml`). A handful of `ingest`/`aoestats` integration tests are
`#[ignore]`d because they need a real Postgres via `testcontainers`/Docker (and, for `aoestats`'s
ages test, a real `python3` on `PATH`); run those locally with `cargo test -p ingest -- --ignored`
/ `cargo test -p aoestats -- --ignored`.
