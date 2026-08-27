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
#   * builder  -- rust:1.98-bookworm compiles the whole workspace in release mode.
#   * runtime  -- debian:bookworm-slim carries the three binaries and the
#                 curriculum tree. It carries no compiler, no source, and no SQL
#                 file. `sqlx` embeds the migrations in cadus-migrate at compile
#                 time.

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

# .dockerignore keeps target/ and .git/ out of the context, so the copy is small.
COPY . .

RUN cargo build --release --workspace

# --------------------------------------------------------------------------- #
# Stage 2 -- runtime: slim, non-root, three binaries
# --------------------------------------------------------------------------- #
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
COPY --from=builder /src/target/release/cadus-migrate /usr/local/bin/cadus-migrate

# The curriculum tree (D-S1, C5). The worker reads it for the A6 exemplar
# fallback and for the knowledge-point half of the gate re-run, and it REFUSES TO
# START without it (exit 2). The old image carried the three binaries alone, so
# every deployment started in that state: the worker heartbeated forever and
# `serving_pool` stayed empty for every learner (findings #5 and #6).
#
# The tree is reviewed data under git (C5), so it belongs in the image beside the
# code that reads it, and a deployment needs no volume for it.
COPY --from=builder /src/curriculum /app/curriculum

# The default the worker reads. `scripts/check_ops.sh` asserts that this path is
# a directory inside every image the compose file builds.
ENV CADUS_CURRICULUM=/app/curriculum

RUN chown -R cadus:cadus /app

USER cadus

EXPOSE 8080

CMD ["cadus-web"]
