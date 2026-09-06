# Framework 2.0 rollout and rollback
This runbook is the operational gate for the framework branch. It does not authorize a production deployment. Run it only after the curriculum review queue has a human decision and the framework checklist records the accepted content scope.

## Release facts
- Framework 2.0 changes no migration relative to the pre-framework commit `2ff3c1f`; the latest migration remains `0012`.
- The event log remains the source of truth. `learner_models` is a rebuildable projection cache.
- The framework fold uses `PROJECTOR_VERSION = 7`. A cache with an older version is rebuilt from its user's event stream on the next state write.
- Pending content never serves. A reviewer must approve each serving document through `/review`.

## Preflight
1. Record the release commit and the rollback commit:
   ```sh
   release_commit="$(git rev-parse HEAD)"
   rollback_commit="2ff3c1f"
   printf 'release=%s\nrollback=%s\n' "$release_commit" "$rollback_commit"
   ```
2. Confirm that the release adds no migration and that the tree is clean:
   ```sh
   git diff --exit-code "$rollback_commit" -- migrations
   git status --short
   ```
3. Run the merge gate against an isolated migrated database:
   ```sh
   CADUS_TEST_DATABASE_URL=postgresql://…/cadus2_gate scripts/gate.sh
   ```
   Continue only after the final line is `GATE OK`.
4. Export and retain the content review queue. Record every approved digest and verify that pending and rejected documents cannot serve.
5. Take a database backup using the deployment's normal encrypted backup procedure. Record its location and restoration check before deployment.

## Disposable recovery rehearsal
Run the local rehearsal before the restored production snapshot check:
```sh
CARGO_BUILD_JOBS=1 scripts/check_recovery.sh
```
The script accepts only the active `cadus2-testdb` container on `127.0.0.1:55434`. It creates three `cadus2_recovery_*` databases and removes them when it stops. The rehearsal does these checks:
1. Apply all migrations and seed five snapshot samples: empty, ordinary, review, integrated, and 258-event history.
2. Write and read a custom-format PostgreSQL archive.
3. Stop a projector process with `SIGKILL` after its version-7 write and before commit.
4. Confirm that PostgreSQL rolled back the write and kept all event bytes.
5. Retry all samples, persist projector version 7, and confirm that the second read resumes with identical model JSON.
6. Confirm that migrations match `2ff3c1f` and restore the retained pre-replay archive into a new rollback database.

This rehearsal proves the local mechanisms. The restored production snapshot check remains necessary for production event shapes, production volume, encrypted backup storage, and retained deployment images.

## Projection replay check
The version mismatch rebuild is the production replay mechanism. Exercise it on a restored production snapshot before deployment:
1. Start the candidate against the restored snapshot with outbound model calls disabled.
2. Select accounts that cover an empty history, an ordinary practice history, a review history, an integrated-task history, and the largest event stream.
3. Read each account through the normal authenticated state or serve endpoint. The read must finish successfully and persist `learner_models.projector_version = 7` through the account's event head.
4. Read the same accounts again. The resulting model JSON, cursor, and framework report values must match the first read byte for byte.
5. Confirm that the event count and maximum sequence for every sampled account are unchanged. Projection replay must append or rewrite no event.

The committed fixture proof is the release-profile projector suite in `scripts/gate.sh`. The restored-snapshot check establishes that production event shapes and volumes are also accepted.

## Deploy
1. Keep the rollback checkout or image available.
2. Run the sole supported upgrade path:
   ```sh
   scripts/deploy.sh
   ```
3. Verify `/api/health` and `/api/ready`, container stability, and the deployed commit.
4. With a non-admin test learner, complete one ordinary lesson, one review, and one integrated task. Confirm reload and duplicate submission preserve the receipt and award completion once.
5. With an admin test account, verify the review queue counts and approve nothing as part of the smoke test.
6. Compare error rate, p95 state-read latency, projector failures, readiness blockers, and content-serving failures with the pre-deploy baseline.

## Roll back
Because this release adds no migration, the code and images can return to `2ff3c1f` without a schema reversal. Events produced by framework 2.0 are schema-versioned and preserve their original bytes; the 1.0 binary may not understand every 2.0 interaction, so stop framework traffic before starting the old services.
1. Stop new learner traffic at the edge while leaving the database running.
2. Check out the recorded rollback commit or select its retained images.
3. Run `scripts/deploy.sh` from that checkout. Do not run `docker compose up -d` directly.
4. Verify `/api/health`, `/api/ready`, container stability, login, and an existing learner state read.
5. Keep the append-only event log. Restore the database backup only for database corruption; a normal application rollback must not discard learner events.
6. Record the first failing request, release and rollback commits, event sequence, projector version, and review-queue counts for the incident review.

## Release decision record
Record these items in the handover before calling the release complete:
- merge-gate result and log digest;
- restored-snapshot replay sample and largest sampled event count;
- human content-review decision and approved digests;
- readiness report produced after that decision;
- browser walk result for ordinary, review, and integrated flows;
- deployment or rollback commit, timestamp, and operator.
