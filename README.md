# Cadus 2.0

Cadus 2.0 is a Rust rewrite of the Cadus math tutor. `REQUIREMENTS.md` is the
authority; `HANDOVER.md` gives the build process. The workspace holds four crates:
`cadus-core` (pure scheduling and pedagogy), `cadus-store` (Postgres adapter),
`cadus-web` (HTTP API), and `cadus-worker` (background jobs). Adapters depend on
the core; the core depends on no adapter (R3).

Run `scripts/gate.sh` before every commit. It runs fmt, clippy, and the tests. Set
`CADUS_TEST_DATABASE_URL` to a superuser Postgres DSN to add the two SQL steps.
