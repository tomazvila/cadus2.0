# Report worker add-on
This override adds the submitted-answer report worker and its isolated verifier to the existing homelab stack. It does not modify the live Compose file or activate either service.
## Build after the feature gates pass
From the repository root:
```sh
docker build --target runtime --build-arg CARGO_BUILD_JOBS=6 -t cadus2:report-dev-20260914 .
docker build -f verification/Dockerfile -t cadus-verifier:report-dev-20260914 verification
```
The runtime image contains `cadus-report-worker`. The verifier image pins its Python base, Lean archive checksum, and Mathlib revision. Build dependencies remain inside Docker.
## Configuration
Use the existing deployment's Compose project name, base files, and protected environment file. Supply `CADUS2_ADMIN_PASSWORD` through that existing mechanism. The worker uses the `cadus_admin` role; no credential is stored in this override. Avoid printing expanded Compose configuration or database URLs.
The worker uses Qwen at `http://10.8.0.3:8080/v1`, with model `qwen-uncensored`. It needs no model API key. Concurrency is fixed at one; the database also permits only one active report lease. The verifier URL is internal service DNS and exposes `/health` and `/verify`.
Set `CADUS_REPORT_WORKER_IMAGE` to an approved runtime image tag if it differs from the build command above.
## Validate and activate deliberately
Set these variables to the existing deployment's actual values. `CADUS_COMPOSE_BASE` must include the existing `cadus2-db`, `cadus2-migrate`, and `cadus2-backend` definitions; preserve any additional base files or flags from its normal invocation.
```sh
: "${CADUS_COMPOSE_PROJECT:?existing project name}"
: "${CADUS_COMPOSE_BASE:?existing base Compose file}"
: "${CADUS_COMPOSE_ENV:?protected deployment environment file}"
compose_report() {
  docker compose --project-name "$CADUS_COMPOSE_PROJECT" \
    --env-file "$CADUS_COMPOSE_ENV" -f "$CADUS_COMPOSE_BASE" \
    -f "$PWD/deploy/report-worker.compose.yaml" "$@"
}
compose_report config --quiet
```
Run approved migrations with the new runtime image before activating workers. Keep the supported migration-first upgrade order in `scripts/deploy.sh`; starting an entire serving stack before migrations finish can disrupt existing services. After migrations succeed and the runtime/verifier gates pass:
```sh
compose_report up -d --no-deps cadus2-verifier
compose_report up -d --no-deps cadus2-report-worker
```
Confirm verifier health before the second command. `--no-deps` deliberately avoids recreating the serving database, web, or migration containers; it also bypasses dependency startup ordering, so that health check is an explicit prerequisite. Validate an owned-account report end to end before claiming the feature works.
## Isolation and stopping
The verifier has no host ports, credentials, host mounts, Docker socket, or non-internal network. It runs as UID/GID 65532 with a read-only root, dropped capabilities, bounded temporary storage, two CPUs, 3 GiB memory, and 64 PIDs. The container physical-memory limit and the verifier child virtual-address-space limit are independent safeguards. The worker has only backend and verification networks and a one-job application limit. Retain these restrictions when tuning resources after measured verifier gates.
Stop this add-on without stopping the serving stack:
```sh
compose_report stop cadus2-report-worker cadus2-verifier
```

The worker requests the advertised `qwen-uncensored` alias, described by the gateway as the uncensored MLX 4-bit model. Compatibility with the legacy raw model path and safe model switching during concurrent gateway requests remain unverified.
