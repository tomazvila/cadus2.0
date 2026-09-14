# syntax=docker/dockerfile:1
#
# Cadus 2.0 self-host images (C3, R4, D9, O3).
#
# TWO images out of one file. `docker-compose.yml` names the stage of each one in
# a `target:` key, so neither depends on the order of the stages below.
#
#   target: runtime  -- the app image. ONE image, FOUR commands. The compose
#                       stack runs it for the web, workers and migrations:
#                         * cadus-web      -- the HTTP tutor, connects as the
#                                             cadus_app role (RLS applies).
#                         * cadus-worker   -- the async layer (R4), connects as
#                                             the cadus_admin role.
#                         * cadus-report-worker -- submitted-answer report workflow.
#                         * cadus-migrate  -- a one-shot that runs migrations/
#                                             and exits 0.
#                       The default CMD runs cadus-web. The orchestrator
#                       overrides it per service.
#   target: spa      -- the edge image: Caddy plus the built SPA in /srv (M6 S14).
#                       Caddy serves those files and proxies /api to web:8080, so
#                       the bundle and the API share ONE origin. The service
#                       refuses a cross-origin cookie write with
#                       `403 cross_origin_rejected`, so a second origin for the
#                       bundle would break every authed POST.
#
# Multi-stage:
#   * builder      -- rust:1.98-bookworm compiles the whole workspace in release
#                     mode.
#   * spa-builder  -- node:22.21.1-bookworm-slim builds web/ into web/dist.
#   * spa          -- caddy:2 plus that dist. It carries NO node: the node lives
#                     in the build stage alone.
#   * runtime      -- debian:bookworm-slim carries the four binaries and the
#                     curriculum tree. It carries no compiler, no source, no SQL
#                     file, and no node. `sqlx` embeds the migrations in
#                     cadus-migrate at compile time.

# --------------------------------------------------------------------------- #
# Stage 1 -- builder: compile the workspace
# --------------------------------------------------------------------------- #
# Pin the exact toolchain version that `rust:1-bookworm` carries today. The tag
# `rust:1` moves, and a moved tag changes the compiler under an unchanged
# commit. `.dockerignore` keeps `rust-toolchain.toml` out of the build context:
# that file names the channel `stable`, rustup treats `stable` and `1.98.0` as
# two different toolchains, and rustup then downloads a second complete
# toolchain from static.rust-lang.org before the first crate compiles. The image
# build needs no rustup egress and no rustfmt and no clippy. The gate runs those
# two on the host.
FROM rust:1.98-bookworm AS builder

WORKDIR /src

# Build the sqlx query macros from the checked-in `.sqlx` cache. The image build
# reaches no database, so an online macro expansion fails the build. The gate
# keeps that cache current with `cargo sqlx prepare --check --workspace`.
ENV SQLX_OFFLINE=true

# Cap the compiler's parallelism. `cargo` defaults to one job per core, and a
# 16-job release build of this workspace pushed a 62 GB box into swap. The image
# is a SELF-HOST image, so it also builds on a small VPS, where 16 jobs is an
# OOM kill and not a slow build. The value is a CAP: a 2-core machine still runs
# 2 jobs. Override it with
# `docker compose build --build-arg CARGO_BUILD_JOBS=16`.
ARG CARGO_BUILD_JOBS=6
ENV CARGO_BUILD_JOBS=${CARGO_BUILD_JOBS}

# COPY the four inputs of the Rust build and NOTHING else. `COPY . .` also copies
# `web/`, so every SPA edit invalidated this layer and rebuilt the whole
# workspace from scratch (M6 S14). The four:
#   * Cargo.toml, Cargo.lock  -- the workspace and the locked versions.
#   * .sqlx                   -- the offline query cache SQLX_OFFLINE reads.
#   * crates                  -- the source.
#   * migrations              -- `crates/store/src/lib.rs` embeds the tree with
#                                `sqlx::migrate!("../../migrations")`, so the
#                                files are a COMPILE-time input of cadus-migrate.
# `.dockerignore` keeps target/ and .git/ out of the context as well.
COPY Cargo.toml Cargo.lock ./
COPY .sqlx ./.sqlx
COPY crates ./crates
COPY migrations ./migrations

RUN cargo build --release --workspace

# --------------------------------------------------------------------------- #
# Stage 2 -- spa-builder: build the SPA bundle with node
# --------------------------------------------------------------------------- #
# Pin the exact node version this project develops against
# (`web/package.json` "engines": ">=22 <23"). A moved tag changes the bundler
# under an unchanged commit. `-slim` carries no build toolchain, and the
# dependency tree of web/ needs none.
FROM node:22.21.1-bookworm-slim AS spa-builder

WORKDIR /src/web

# The manifest and the lock file FIRST, so `npm ci` re-runs only when a
# dependency moves and not when a component changes.
COPY web/package.json web/package-lock.json ./

# `npm ci` and never `npm install`: it installs the locked tree exactly and fails
# on a lock file that does not match the manifest. `--no-audit --no-fund` drops
# two network round trips that report nothing the build reads.
RUN npm ci --no-audit --no-fund

# `.dockerignore` holds `node_modules`, so a host install never lands on top of
# the one above.
COPY web/ ./

# `npm run build` is `tsc -b tsconfig.build.json && vite build`, so the type gate
# runs here too and a type error fails the image build.
#
# `npm run csp` then audits the emitted bundle: no inline script, no eval, no
# `data:` URI. The deployed CSP has no `script-src`, so scripts fall back to
# `default-src 'self'`, and `font-src 'self'` has no `data:`. A bundle that
# breaks either rule is a WHITE PAGE in the browser and a 200 on every asset, so
# the audit runs on the artifact that ships and not on a developer's tree alone.
RUN npm run build && npm run csp

# --------------------------------------------------------------------------- #
# Stage 3 -- spa: Caddy plus the built bundle
# --------------------------------------------------------------------------- #
# The edge image. `deploy/Caddyfile` stays a bind mount in docker-compose.yml, so
# an operator edits the routing without a rebuild; the BUNDLE is baked in, so the
# files Caddy serves always match the commit that built them.
#
# This image carries no node and no source: `spa-builder` keeps both.
FROM caddy:2 AS spa

COPY --from=spa-builder /src/web/dist /srv

# --------------------------------------------------------------------------- #
# Stage 4 -- runtime: slim, non-root, four binaries
# --------------------------------------------------------------------------- #
# This stage is LAST, so a plain `docker build .` with no `--target` still builds
# the app image, as it did before the SPA arrived.
FROM debian:bookworm-slim AS runtime

# ca-certificates: outbound TLS to the model API (R4 authoring and diagnosis).
# The image carries no database client. The one statement that the migrations
# cannot run themselves --
#   ALTER ROLE cadus_admin LOGIN
# -- is the `--admin-login` flag of cadus-migrate. Migration 0001 creates
# cadus_admin as NOLOGIN on purpose, and the worker DSN needs a login. A
# migration must not make that decision for every deployment, so the deployment
# makes it through the flag.
RUN apt-get update \
 && apt-get install -y --no-install-recommends ca-certificates \
 && rm -rf /var/lib/apt/lists/*

# A non-root system user. The binaries need no write access to the image tree.
RUN groupadd --system cadus \
 && useradd --system --gid cadus --home-dir /app --no-create-home cadus

WORKDIR /app

COPY --from=builder /src/target/release/cadus-web /usr/local/bin/cadus-web
COPY --from=builder /src/target/release/cadus-worker /usr/local/bin/cadus-worker
COPY --from=builder /src/target/release/cadus-report-worker /usr/local/bin/cadus-report-worker
COPY --from=builder /src/target/release/cadus-migrate /usr/local/bin/cadus-migrate

# The curriculum tree (D-S1, C5). The worker reads it for the A6 exemplar
# fallback and for the knowledge-point half of the gate re-run, and it REFUSES TO
# START without it (exit 2). The old image carried the three binaries alone, so
# every deployment started in that state: the worker heartbeated forever and
# `serving_pool` stayed empty for every learner (findings #5 and #6).
#
# The tree is reviewed data under git (C5), so it belongs in the image beside the
# code that reads it, and a deployment needs no volume for it.
#
# It comes from the BUILD CONTEXT and no longer from the builder stage: the
# builder now copies the four Rust inputs alone, and the curriculum is not one of
# them. Data that no compiler reads takes the short path into the image.
COPY curriculum /app/curriculum

# The default the worker reads. `scripts/check_ops.sh` asserts that this path is
# a directory inside every image the compose file builds from `target: runtime`.
ENV CADUS_CURRICULUM=/app/curriculum

RUN chown -R cadus:cadus /app

USER cadus

EXPOSE 8080

CMD ["cadus-web"]
