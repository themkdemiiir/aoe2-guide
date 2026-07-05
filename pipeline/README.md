# pipeline

A Rust workspace for the AOE2 guide's data pipeline. Crates are added one milestone at a time;
today it holds three: `crates/core` (`pipeline-core`), `crates/migration`, and `crates/ingest`,
with `replay`/`fetch`/`export` coming in later milestones. This is a separate workspace from the
repo-root `analyzer/` (the in-browser WASM coach) — the two share no crates or Cargo.lock.

- **`core`** (`pipeline-core`) is the typed, regex-free home for the slug/elo-bucket/map/civ
  lookups that today live duplicated across JS (`scripts/data-pipeline/lib/buckets.mjs`) and Rust
  (`replay-rs/src/analyze/{maps.rs,data.rs}`), plus shared id newtypes, `Age`, `Secret`, and the
  `core::cli` binary bootstrap every pipeline binary uses. Its civ/map lookup tables are gated
  behind the opt-in `refdata` feature (NOT a default — see the comment in
  `crates/core/Cargo.toml`): enable it explicitly with `--features pipeline-core/refdata` (or via
  `--all-features`) where those lookups are actually needed and the repo-root data files they
  `include_str!` are present.
- **`migration`** carries the SeaORM migrations for the gameplay-first PostgreSQL schema.
- **`ingest`** is the idempotent binary-COPY bulk loader from typed replay row batches into that
  schema.

`core` becomes the shared data core that JS callers cut over to in a later milestone.

## Testing

The full test gate is `cargo test --workspace --all-features` (this is also what CI runs — see
`.github/workflows/pipeline.yml`). A handful of `ingest` integration tests are `#[ignore]`d because
they need a real Postgres via `testcontainers`/Docker; run those locally with
`cargo test -p ingest -- --ignored`.
