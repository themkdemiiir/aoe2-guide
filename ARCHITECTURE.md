# Architecture

A one-page map of the repo for new contributors. For *how to add content*, see
[`docs/CONTRIBUTING.md`](docs/CONTRIBUTING.md); for the binding stack rules, see
[`CLAUDE.md`](CLAUDE.md).

## The two systems

This repo is really two things that meet only through committed JSON files:

1. **The static site** (`src/`, `public/`) — an **Astro 5** static-output site (EN/TR),
   built by `pnpm build` and deployed to Cloudflare Pages. No server, no SSR, no runtime
   backend. It reads pre-computed data from `src/data/*.json` and `reference-data/`.

2. **The offline data pipeline** (`pipeline/`, `orchestration/`) — a **Rust + PostgreSQL**
   pipeline that runs *on a VM, not in the site build*. It crawls ranked match data, parses
   replays, and **produces the committed JSON** the site reads. The site never talks to it
   live; the pipeline's output is committed to the repo.

## Top-level map

| Folder | What it is | Language / tooling |
|---|---|---|
| `src/` | The Astro site — pages, components, content, i18n. Framework-conventional layout. | Astro / TS |
| `public/` | Static assets served as-is (images, committed data JSON, the analyzer WASM). | — |
| `reference-data/` | Shared reference data baked into the analyzer **and** the Rust pipeline (`maps.tsv`, `civs.tsv`, `relic-civs.tsv`, `costs.json`, `benchmark*.json`). Single source of truth. | TSV / JSON |
| `pipeline/` | The Rust data pipeline: crawl → parse replays → ingest to PostgreSQL → dbt views → export JSON. A Cargo workspace of small crates. | Rust |
| `orchestration/` | Dagster jobs/schedules that run the pipeline containers on the VM. | Python |
| `analyzer/` | The in-browser replay coach: a Rust crate compiled to WASM (`pnpm build:wasm`) that the `/analyzer` page loads. | Rust → WASM |
| `scripts/` | Site **build tooling** (icon maps, civ/unit generation from `aoe2techtree`, translation audits, validation gates) run around `pnpm build`. | JS |
| `guide-sources/` | Raw source guides (pro build-order transcripts) used to *verify* content — not built into the site. | Markdown/text |
| `deploy/` | Pinned Docker Compose stack for the PostgreSQL + pipeline services on the VM. | Compose |
| `docs/` | Contributor + design docs. `docs/superpowers/` holds historical specs/plans (archives). | Markdown |
| `tests/` | Vitest unit tests + Playwright e2e (`tests/e2e/`). | TS |

## How data flows to the site

```
Relic API + replays  ──►  pipeline/ (Rust, on VM)  ──►  PostgreSQL  ──►  dbt views
                                                                             │
                                          export crate  ◄──────────────────┘
                                                │
                            commits JSON ──►  src/data/*.json + reference-data/*.json
                                                │
                                    pnpm build (Astro)  ──►  dist/  ──►  Cloudflare Pages
```

The analyzer is a side path: it bakes `reference-data/` in at compile time (`include_str!`),
compiles to WASM, and runs entirely in the browser on a replay the user uploads.

## Invariants that bite if ignored

- **Static-only.** No SSR adapters, no API routes, no runtime backend. All "dynamic"
  features are client-side over committed static data. (Deploy target is CF Pages.)
- **Two civ-id spaces — never conflate.** `src/data/civ-id-map.json` = game/replay `civ_id`s;
  `src/data/relic-civ-id-map.json` = the Relic API's own ids. Wrong table = every civ label
  silently shuffled (a real past bug). Guard: `tests/relic-civ-map.test.mjs`.
- **English-canonical slugs** across all locales (`/tr/civs/britons/`, not localized slugs).
- **Source-derived data only** — every stat traces to a cited source; generators fail loud on
  missing input rather than defaulting.

## Running things

| | Command |
|---|---|
| Dev server | `pnpm dev` (→ `http://localhost:4321`) |
| Production build | `pnpm build` |
| Type check | `pnpm check` · lint `pnpm check:all` · tests `pnpm test` |
| Rebuild analyzer WASM | `pnpm build:wasm` (needs `wasm-pack` + the `wasm32-unknown-unknown` target) |
| Pipeline (Rust) | `cargo test --all-features` in `pipeline/`; runs on the VM, not in CI |
