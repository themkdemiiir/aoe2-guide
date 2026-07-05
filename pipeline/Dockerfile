# syntax=docker/dockerfile:1
#
# Reusable multi-stage image for every Rust binary in this workspace, selected via the `BIN`
# build-arg (e.g. `--build-arg BIN=migration`). One Dockerfile serves the whole pipeline/ crate
# set — a future crate (ingest/export/fetch) becomes a ~6-line compose service block, no new
# Dockerfile. Digest-pinned base images throughout; see deploy/README.md for the pin rationale.

# ---- builder --------------------------------------------------------------
# rust:1.96.1-slim-bookworm — current stable Rust (2026-06-26) on Debian bookworm.
FROM rust:1.96.1-slim-bookworm@sha256:e18a79fc84dfcfc3ab5ba72290398a644c135c97eaa881447fddc354ee4701a3 AS builder

WORKDIR /app
COPY . .

ARG BIN
RUN --mount=type=cache,target=/usr/local/cargo/registry \
    --mount=type=cache,target=/app/target \
    cargo build --release --locked --bin ${BIN} \
 && cp /app/target/release/${BIN} /usr/local/bin/app

# ---- runtime ----------------------------------------------------------------
# debian:bookworm-slim — minimal glibc base. No openssl/libpq: every crate here uses rustls
# (with compiled-in webpki-roots), so there's no system TLS/DB-client library to install.
# ca-certificates is added for future crates that call HTTPS APIs (ingest/fetch); the migration
# binary itself doesn't need it (webpki-roots ships its own CA bundle).
FROM debian:bookworm-slim@sha256:60eac759739651111db372c07be67863818726f754804b8707c90979bda511df AS runtime

RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/* \
 && useradd -r -u 10001 -M -s /usr/sbin/nologin app

COPY --from=builder /usr/local/bin/app /usr/local/bin/app

USER app
ENTRYPOINT ["/usr/local/bin/app"]
