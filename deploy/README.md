# Self-hosted data stack (pinned Docker Compose)

The offline pipeline's infrastructure, **version-pinned** and reproducible. Runs in the privileged
Docker LXC **`aoe2-stack` (192.168.111.8)** on the Proxmox host `botanera`, data on the NVMe
(`/opt/pgdata`). Not part of the Cloudflare Pages build — it's the VM-side data workload.

## Services (pinned)
| Service | Image / version | Notes |
|---|---|---|
| `postgres` | `postgres:18.4` | PG 18 (data-checksums, async I/O). Data on NVMe bind `/opt/pgdata`. LAN `:5432`. |
| `migration` | `aoe2-migration:0.1.0` (built from `../pipeline/Dockerfile`, `BIN=migration`) | One-shot SeaORM migrator. `profiles: ["tools"]` — not started by a plain `up -d`. |
| `dagster` | (added M5/M6) `1.13.12` | Orchestrator: partitioned now→old backfills, dbt-asset lineage, Pipes→Rust. |
| `dbt` | (added M5) `dbt-core 1.11.12` + `dbt-postgres` | The Postgres aggregation model DAG. |

Rust pipeline tools all build from the same `pipeline/Dockerfile`, parametrized by the `BIN`
build-arg — each future crate (`ingest`/`export`/`fetch`) becomes a small service block like
`migration`'s, reusing the same image recipe.

## Run (on the CT)
```
cd /opt/aoe2-stack
cp .env.example .env && edit POSTGRES_PASSWORD   # (generated on first bring-up)
docker compose up -d postgres
docker compose ps
```
`deploy/.env` holds the password and is git-ignored. The pipeline's `DATABASE_URL` is stored
root-only at `/root/aoe2-db-url` on the CT.

### Running the migrator
```
docker compose run --rm migration status   # list applied/pending migrations (read-only)
docker compose run --rm migration up       # apply all pending migrations
```
`docker compose build migration` (re)builds the image after a `pipeline/crates/migration`
change; `up -d` alone never starts it (see `profiles: ["tools"]` above).

## Host layout
- Docker LXC: privileged, `nesting=1,keyctl=1,fuse=1`, apparmor unconfined (Docker-in-LXC needs it).
- PG data: 400 GB NVMe volume mounted at `/opt/pgdata` → container `/var/lib/postgresql`.
