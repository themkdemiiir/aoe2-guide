# Rust Clean-Architecture Playbook — AOE2 Offline Pipeline

Applied synthesis of external best-practice research + audit of `pipeline/crates/{core,ingest,migration}`. Scope: the whole `pipeline/` workspace, present and coming (`replay`, `fetch`, `export`). Calibrated for a **solo hobby pipeline**: strict/explicit types and clean domain/adapter separation, yes; enterprise ceremony, no.

## Principles we adopt

1. **thiserror at library edges, anyhow at binary edges.** A library's caller needs to `match` on failure kind; a CLI's `main` just displays-and-exits — the two want different error shapes.
2. **A typed error variant must earn its keep.** Add an enum variant only when something downstream will `match` on it (retry vs. skip vs. stop); otherwise `anyhow` + `.context()` is strictly better than an enum nobody destructures.
3. **Newtype every ID space that shares a primitive.** `GameCivId` ≠ `RelicCivId` ≠ `match_id` — the June-2026 civ-mislabel bug was two `i32`s the compiler couldn't tell apart. Make transposition a compile error.
4. **Parse, don't validate, at every port boundary.** Required schema fields are bare (non-`Option`) typed DTO fields; a missing value fails to *compile*, never silently defaults — the "no defaults, fail loud" rule enforced by the type system.
5. **Functional core, imperative shell — Rust's native hexagon.** Pure IO-free logic (`core`, replay parsing) in crates with no tokio/reqwest/pg in `Cargo.toml`; all awaiting/connecting/writing in a thin shell. No interface-per-dependency ceremony.
6. **One "talk to Postgres" idiom, one error vocabulary per crate.** SeaORM for DDL only; `tokio-postgres` for every runtime query; no sqlx third way. One `thiserror` enum per crate in `error.rs`, re-exported with a `Result` alias.
7. **A trait needs ≥2 real impls or a fast test double — else call the concrete function.** One DB → `&mut Client`, no trait. `fetch`'s manifest policy, tested thousands of times → trait worth it.
8. **No `unwrap`/`expect`/`panic` in a `pub` lib fn.** In a 24/7 unattended `fetch` loop, one bad record must not kill the process — keep the panic decision at the binary boundary.

---

## Part 1 — Cheap, high-value cleanups to APPLY now

Ranked by impact ÷ effort.

1. **Add a CI job for `pipeline/`.** *(medium impact, trivial effort)* — `.github/workflows/ci.yml` runs nothing against the workspace; real unit + testcontainers tests exist but never fire. Add: `cargo check --workspace --all-features`, `cargo test --workspace`, `cargo clippy --workspace -- -D warnings`. Directly serves the "keep CI green" value. **Do this first — it guards every cleanup below.**

2. **`GameCivId` / `RelicCivId` newtypes in `core`.** *(high impact, low effort)* — `crates/core/src/civs.rs`: both `slug(id: u32)` key on raw `u32`, the exact two id-spaces the module doc says caused a production incident. Two one-field structs; change the two signatures. Derive `postgres_types::{ToSql,FromSql}` + `#[postgres(transparent)]` so they round-trip through COPY at zero cost.
   ```rust
   #[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, ToSql, FromSql)]
   #[postgres(transparent)] pub struct GameCivId(pub u32);
   ```

3. **Fix the silent f64→f32 truncation.** *(medium impact, trivial effort)* — `crates/ingest/src/dto.rs:79-81` types `feudal_t/castle_t/imperial_t` as `f64`, COPY'd as `Type::FLOAT8`, but the columns are PG `real` (`m20260705_000007:25-27`). Every ingest silently truncates to f32 with no error. Pick one and add a comment: type DTO fields `f32` + `Type::FLOAT4`, **or** widen the migration to `double precision`.

4. **De-duplicate the CLI bootstrap into `core`.** *(medium impact, low effort)* — `init_tracing` + DATABASE_URL-read-or-exit + `redact_secret`-then-log-then-exit is copy-pasted between `migration/src/main.rs:55-88` and `ingest/src/main.rs:34-56,106-116`, diverging only in the log-filter string. Will hit 5 copies with replay/fetch/export. Hoist a tiny `pipeline_core::cli` (`init_tracing(default_filter: &str)` + a DATABASE_URL helper). Not a framework — ~25 already-identical lines moved once.

5. **`[workspace.dependencies]` + `[workspace.package]`.** *(low impact, trivial effort)* — root `pipeline/Cargo.toml` has neither; `tokio/anyhow/clap/tracing/serde/serde_json` are pinned+feature-flagged identically in two crates today, 6 crates soon. Hoist shared deps; crates write `tokio.workspace = true`.

6. **`EloBucket` enum in `core`.** *(low-medium impact, low effort)* — `crates/core/src/elo.rs`: the 9 buckets live twice (the `ELO_BUCKETS` array + the `elo_bucket` match arms) with only a containment test tying them. Model one `enum EloBucket { Under1000, … }` with `as_str()` + `const ALL: [_; 9]`. *(Keep the `_ => "2500+"` open top range — a real open bound, not a lazy catch-all.)*

7. **Read-back assertions + column-order de-duplication in `ingest`.** *(medium impact, medium effort)* — `tests/ingest_postgres.rs:231-322` asserts only row *counts*, so a same-typed adjacent-column swap (e.g. replay_events' `target_id`/`amount`, both `Option<i64>`) passes silently on the "COPY-critical path." Column order is hand-synced 3× per table (SQL string, `[Type;N]`, `[&dyn ToSql;N]`). Add one full-row read-back per table; collapse the 3-way sync into one `&[(&str, Type)]`-per-table.

8. **`#[derive(Iden)]` enums for migrations.** *(low impact, low effort)* — `migration/src/m*.rs` identify columns via ad-hoc `Alias::new("match_id")`; FK matches are only checked at migrate-run time. Define one `Iden` enum per table so a typo'd FK column fails `cargo build`. Retrofit is safe (SeaORM tracks migrations by name). Add a `Migrator::up` → `down` round-trip test — `down()` is currently unexercised.

9. **Update stale docs.** `pipeline/README.md:3` still says "only crates/core".

*Defer (correctly low priority): the `civ_id 0 → "random"` sentinel; fail-loud-on-malformed-source-line in `maps.rs`; replay_events/replay_ages intra-batch dup gap (note in module doc, don't build de-dup pre-partitioning).*

---

## Part 2 — Patterns to BAKE INTO replay / fetch / export

### Newtypes & domain types (in `core`, shared)
- `GameCivId`, `RelicCivId`, and — as they earn it — `MatchId`, `ProfileId` (all `#[postgres(transparent)]`). Apply at struct/fn boundaries where transposition happens, not every local.
- `Age` **enum** (`Dark/Feudal/Castle/Imperial`) with `as_db_str` — replay must emit this, never `String` (today `NewReplayAge.age: String` accepts `"Fuedal"`). Same for the closed part of event `kind`; use `Other(String)` catch-all for the open-ended tail so unknown-but-real data round-trips.
- `NonZeroU8` for `n_players` and replay `player_number` (AOE2 players are 1..=8; 0 = corrupt record). Same size as `u8` — free.
- `Elo` smart constructor (private field, `new` validates `0..=3500`, `Option<Elo>` keeps `None` = unknown, no sentinel).
- `Secret` newtype wrapping `DATABASE_URL` and (in fetch) the Relic token: `Debug`/`Display` print `<redacted>`, no `Deref`/`Serialize`, real value only via `.expose()`. Compiler-enforced second layer behind `redact_secret`.
- `ReplayBatch::new` validating constructor: every child row's `match_id` present in `matches` before `Ok(...)`.

### Error types
- **`replay`, `fetch`, `export`:** one `thiserror` enum in `error.rs`, `pub use error::{Error, Result}` from `lib.rs`. Enumerate the *closed, known* dependency surface via `#[from]` — no `Box<dyn Error>` catch-all.
  ```rust
  #[derive(Debug, Error)] pub enum FetchError {
      #[error("relic API rate-limited profile {profile_id}")] RateLimited { profile_id: i64 },
      #[error("http request failed")] Http(#[from] reqwest::Error),
      #[error("manifest db error")]   Manifest(#[from] rusqlite::Error),
  }
  ```
  `fetch`'s loop mechanically branches: `RateLimited`/`Http` → sleep+retry; `Manifest` → stop. That distinction is *why* the enum exists. Messages lowercase, no trailing period. Payloads `Send + Sync + 'static`.
- **Binaries** (`main.rs`): `anyhow::Result<()>`, `.context()` per `?`, and copy `ingest/src/main.rs`'s terminal-error handler **verbatim** — `redact_secret(&format!("{err:#}"), &url)` before `tracing::error!`, then `exit(1)`.

### Crate boundaries (functional core / imperative shell)
- **`replay`** — pure end-to-end, zero IO (bytes already in memory). Corpus/golden-file tests for the top-level parser (real captured replays with known map/civs/winner); proptest only for pure sub-routines (LEB128 reader, string-table decoder, tick→ms) via round-trip.
- **`fetch`** — adapter-heavy. The manifest *policy* (pending/retry/backoff) is domain logic → put it behind a `ReplayManifest` trait **owned by fetch**; the rusqlite-backed struct implements it. The one place a trait earns its keep: thousands of cheap scheduling unit tests against a `Vec`-backed fake, generic `fn run<S: ReplayManifest>`. Real Relic vocab (`RelicMatchType`) → `#[non_exhaustive]` enum with a mandatory fail-loud wildcard.
- **`export`** — read-side mirror of ingest. Typed JSON DTOs mirroring column nullability 1:1. Stream row-by-row: `COPY (SELECT …) TO STDOUT BINARY` via `copy_out`, or `query_raw` `RowStream` — never `client.query` (buffers a full `Vec<Row>`; `match_ages` is ~60M rows → OOM).

### Async / pooling
- `fetch`: `JoinSet` (never spawn-and-forget); `Arc<Semaphore>` for **concurrency** + `governor::RateLimiter` (GCRA) for **rate** — separate constraints; one root `CancellationToken` → `.child_token()` per subsystem, raced via `tokio::select!`; bounded `mpsc::channel(N)` between discover→download→ingest stages (capacity *is* backpressure); `spawn_blocking` for SQLite writes + CPU-bound replay parse; one dedicated actor task owns the SQLite connection (WAL + `busy_timeout`), workers send `ManifestEvent`s. `#[tracing::instrument]` every task with its `match_id`.
- `ingest`: **keep the single `&mut Client`** — one batch = one `BEGIN…COMMIT`. Race `ingest_batch(...).await` against cancellation; mid-batch cancel → `Drop` rolls back → retry.
- Pooling: **only when there's genuine concurrent DB access** (a future parallel `export`). Then `deadpool-postgres` — never hand-rolled reconnect. TEMP staging tables are connection-scoped, so thread the *same* held connection through a whole batch.

### Postgres data-layer (replicate ingest's proven shape)
Binary COPY → `TEMP … ON COMMIT DROP` staging → single set-based `INSERT`; enum/custom columns stay `TEXT` in staging (`Type::TEXT`), cast once (`col::enum_type`); idempotency via captured `new_ids` TEMP table + JOIN-gated children. DTO nullability = column nullability, cross-checked against the migration `ColumnDef`. Typed binary params (`with-chrono-0_4`, add `with-serde_json-1`/`with-uuid-1` as needed) — never `to_string()` + SQL-text cast.

### Testing
Pin the test Postgres image by the **same digest** `deploy/docker-compose.yml` uses, in one shared constant. `#[tokio::test] #[ignore]` every Docker-dependent test. Fixtures = plain typed builder fns (`fn sample_batch() -> ReplayBatch`). Add `proptest` to `core` now: `elo_bucket ∈ ELO_BUCKETS ∀ i32`; `slug` idempotent + lowercase-alnum; `redact_secret` never panics/leaks (incl. empty-needle). Commit `proptest-regressions/`.

---

## Deliberately SKIP (keep-it-light)
- `#[non_exhaustive]` on our *own* enums (we want the compiler to interrupt every match site when vocab changes; use it only on enums mirroring Relic vocab we don't control).
- Sealed-trait scaffolding; `Box<dyn Error>` catch-all variant; **sqlx** (a third query idiom); generic typestate (ownership-consuming `self` gives it free); DI container/framework; a connection pool for `ingest`; smart constructors for primitives without real invariants.

## Domain vs. adapter map
**Domain (pure, no IO):** `core` (slug/elo/maps/civs, `redact_secret`, new newtypes/`Age`/`EloBucket`/`Secret`/`cli` helper); `replay` (decodes in-memory bytes, no socket/file/clock); `fetch`'s scheduling *policy* (behind the `ReplayManifest` trait it owns). **Adapter/shell (all IO):** `migration` (SeaORM DDL); `ingest`'s `ingest.rs`/`main.rs` (COPY/tx/connect); `fetch`'s rusqlite manifest, reqwest client, semaphore/rate-limiter/JoinSet; `export`'s streaming `copy_out`. Rule: **a crate that awaits a socket, opens a file, or reads the clock is a shell; the domain never depends on the adapter.**
