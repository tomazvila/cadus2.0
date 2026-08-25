# syntax=docker/dockerfile:1
#
# Cadus 2.0 self-host image (C3, R4, D9).
#
# ONE image, THREE commands. The compose stack runs the same image three times:
#   * cadus-web      -- the HTTP tutor, connects as the cadus_app role (RLS applies).
#   * cadus-worker   -- the async layer (R4), connects as the cadus_admin role.
#   * cadus-migrate  -- a one-shot that runs migrations/ and exits 0.
# The default CMD runs cadus-web. The orchestrator overrides it per service.
#
# Multi-stage:
#   * builder  -- rust:1-bookworm compiles the whole workspace in release mode.
#   * runtime  -- debian:bookworm-slim carries the three binaries and the SQL
#                 migrations only. It carries no compiler and no source.

# --------------------------------------------------------------------------- #
# Stage 1 -- builder: compile the workspace
# --------------------------------------------------------------------------- #
FROM rust:1-bookworm AS builder

WORKDIR /src

# Build the sqlx query macros from the checked-in `.sqlx` cache. The image build
# reaches no database, so an online macro expansion fails the build. The gate
# keeps that cache current with `cargo sqlx prepare --check --workspace`.
ENV SQLX_OFFLINE=true

# .dockerignore keeps target/ and .git/ out of the context, so the copy is small.
COPY . .

RUN cargo build --release --workspace

# --------------------------------------------------------------------------- #
# Stage 2 -- runtime: slim, non-root, three binaries plus the migrations
# --------------------------------------------------------------------------- #
FROM debian:bookworm-slim AS runtime

# ca-certificates: outbound TLS to the model API (R4 authoring and diagnosis).
# postgresql-client: the compose `migrate` service runs one statement that the
# migrations cannot run themselves --
#   ALTER ROLE cadus_admin LOGIN
# Migration 0001 creates cadus_admin as NOLOGIN on purpose, and the worker DSN
# needs a login. A migration must not make that decision for every deployment,
# so the deployment makes it. psql 15 from bookworm speaks to a Postgres 16
# server for plain SQL; only pg_dump needs a version match, and this image runs
# no dump.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates postgresql-client \
 && rm -rf /var/lib/apt/lists/*

# A non-root system user. The binaries need no write access to the image tree.
RUN groupadd --system cadus \
 && useradd --system --gid cadus --home-dir /app --no-create-home cadus

WORKDIR /app

COPY --from=builder /src/target/release/cadus-web /usr/local/bin/cadus-web
COPY --from=builder /src/target/release/cadus-worker /usr/local/bin/cadus-worker
COPY --from=builder /src/target/release/cadus-migrate /usr/local/bin/cadus-migrate

# The SQL migrations ship as data. cadus-migrate reads them from this path.
COPY migrations /app/migrations

RUN chown -R cadus:cadus /app

USER cadus

EXPOSE 8080

CMD ["cadus-web"]
