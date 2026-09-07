# Framework 2.0 rollout and rollback
This runbook is the operational gate for the framework branch. It does not authorize production deployment, content import, or content decisions. Those actions require the owner's explicit authorization. A human reviewer decides every digest.
## Release facts
- Production is Compose project `homelab`, rooted at `/home/deploy/homelab`. Its Cadus services are `cadus2-db`, `cadus2-migrate`, `cadus2-web`, `cadus2-worker`, and `cadus2-edge`.
- `scripts/deploy.sh` operates the standalone repository stack. Production uses `scripts/homelab_release.sh`, which derives live container IDs and their shared network from the Compose project before acting.
- Framework 2.0 changes no migration relative to `2ff3c1f`; the latest migration remains `0012`.
- The event log remains the source of truth. `learner_models` is a rebuildable projection cache at `PROJECTOR_VERSION = 7`.
- Pending content never serves. The candidate can deploy and accept pending rows safely. The release remains incomplete until human review and the post-decision readiness audit pass.
## Build the exact content bundle
Create one self-contained import bundle after the exact release head passes its content pipeline:
```sh
release_commit="$(git rev-parse HEAD)"
final_root="/home/deploy/.cache/cadus2_orchestration/final-${release_commit:0:12}"
bundle="$final_root/foundations-release"
python3 scripts/review/foundations_release_bundle.py build \
  --release-root "$PWD" \
  --release-commit "$release_commit" \
  --templates /home/deploy/.cache/cadus2_orchestration/postzero/templates.json \
  --teach docs/content-foundations/whole-course-teach/import-manifest.json \
  --instruction /home/deploy/.cache/cadus2_orchestration/postzero/complete-1/manifest.json \
  --output "$bundle"
```
The builder refuses overlap, drift from 809 templates, 809 Teach pages, and 809 hint ladders, or a mismatch among their canonical knowledge-point sets. `bundle.json` binds the release commit, ordered files, inputs, counts, and hashes. The three import files contain 2,427 unique pending documents.
## Preflight without production writes
The order is bundle, verified backup and recovery rehearsal, deployment, pending import, human per-digest review, then readiness and smoke.
1. Record the merge gate, Rust quality, web quality, and restored-snapshot replay receipts for this exact commit.
2. Take an encrypted production backup and restore it into a disposable database. The receipt must bind this exact commit, confirm no plaintext dump, prove byte-identical event fingerprints before and after replay, and confirm disposable-database removal.
3. Run the topology, bundle, and recovery-receipt preflight:
```sh
recovery_receipt="$final_root/restored-production-replay/receipt.json"
scripts/homelab_release.sh preflight \
  --bundle "$bundle" \
  --recovery-receipt "$recovery_receipt"
```
It confirms the five production services, checks the clean tree and unchanged migrations, derives the live edge mount, compares its Caddyfile with the candidate, and changes no production state.
4. Record read-only production baselines for event count, error rate, p95 state-read latency, projector failures, readiness blockers, and content-serving failures.
## Disposable recovery rehearsal
Run the local mechanism rehearsal before the restored production snapshot check:
```sh
CARGO_BUILD_JOBS=1 scripts/check_recovery.sh
```
The rehearsal uses only the `cadus2-testdb` test container. It validates archive/restore, a killed projection transaction, byte-identical replay, unchanged events, migration identity, and rollback-database restoration. The restored production snapshot receipt remains mandatory because it covers production event shapes and volume.
## Deploy the pending-safe candidate
The live database initially has no content rows, so a review queue cannot exist before the candidate and pending bundle reach production. Pending rows do not serve. After explicit deployment authorization, retain a log and run:
```sh
receipt_dir="$final_root/release-receipts"
set -o pipefail
scripts/homelab_release.sh deploy \
  --bundle "$bundle" \
  --recovery-receipt "$recovery_receipt" \
  --receipt-dir "$receipt_dir" \
  --execute | tee "$final_root/deploy.log"
```
The helper records the current live app and edge image IDs as rollback truth, builds immutable release tags, waits for `cadus2-db`, runs `cadus2-migrate`, and only then recreates `cadus2-web`, `cadus2-worker`, and `cadus2-edge` together. It verifies stable restart counts, public `/api/health`, and internal `/api/ready`. A build or migration failure leaves the old application containers running.
## Import pending content
This is a production database write distinct from deployment. After explicit authorization for the import, run:
```sh
set -o pipefail
scripts/homelab_release.sh import-pending \
  --bundle "$bundle" \
  --recovery-receipt "$recovery_receipt" \
  --execute | tee "$final_root/content-import.log"
```
The helper extracts the worker from the immutable release image, validates the bundle again, dry-runs all three ordered files, then imports through the normal production gates with zero model cost and `--missing-only`. It never approves content. A retry skips rows already pending or approved.
## Human review and approval
1. Sign in as an admin at `https://cadus.<domain>/review`. Save the exact admin Cookie header value in a mode-600 scratch file:
```sh
install -m 600 /dev/null ~/.cache/cadus-review-cookie
```
2. Export the live pending queue and its full bodies, gates, and rendered instances:
```sh
python3 scripts/review/content_review_packet.py export \
  --base-url "https://cadus.<domain>" \
  --cookie-file ~/.cache/cadus-review-cookie \
  --output ~/.cache/cadus-foundations-review.json
```
3. Review every selected digest. Add only explicit `approve` or reasoned `reject` decisions to `~/.cache/cadus-foundations-review.decisions.json`. The `/review` screen is the interactive renderer. Apply templates as their own first batch, then export a fresh packet before deciding Teach pages and hint ladders; each template approval re-gates its pending instruction content.
4. Recheck the exact live bodies without writing:
```sh
python3 scripts/review/content_review_packet.py apply \
  --base-url "https://cadus.<domain>" \
  --cookie-file ~/.cache/cadus-review-cookie \
  --packet ~/.cache/cadus-foundations-review.json \
  --decisions ~/.cache/cadus-foundations-review.decisions.json \
  --receipt ~/.cache/cadus-foundations-review.dry-run.json
```
5. The human reviewer applies that exact decision file by adding `--commit` and using a new receipt path. Re-export after every committed batch because approving a template can re-gate related pending instruction content. Preserve all packet hashes and receipts.
## Readiness and smoke
After all decisions, capture the serving-readiness report:
```sh
scripts/homelab_release.sh readiness \
  --bundle "$bundle" \
  --recovery-receipt "$recovery_receipt" \
  --receipt-dir "$receipt_dir"
```
The release requires 809 ready knowledge points, zero blocked knowledge points, and zero contract failures. Then:
1. With a non-admin test learner, complete one ordinary lesson, one review, and one integrated task.
2. Reload and submit each receipt twice; completion must be awarded once and the stored receipt must remain stable.
3. With an admin test account, verify the review-queue counts. Make no approval during the smoke.
4. Compare live operational metrics with the recorded pre-deploy baseline.
## Roll back
Because the release adds no migration, rollback changes images and preserves the database and event log. After explicit rollback authorization:
```sh
scripts/homelab_release.sh rollback --receipt-dir "$receipt_dir" --execute
```
The helper reads the exact pre-deploy image IDs from `rollback-images.json`, stops the edge, web API, and worker so the outer homelab proxy cannot pass learner writes during rollback, restores those images, recreates all three services, and reruns stability and probe checks. Restore the database backup only for database corruption. Record the first failing request, release and rollback commits, event sequence, projector version, and review-queue counts.
## Release decision record
Record these items in the handover before calling the release complete:
- exact release commit and final bundle digest;
- merge-gate, quality, restored-snapshot replay, and encrypted backup/restore receipts;
- deploy log and exact pre-deploy/current image IDs;
- human content packet, decisions, approved/rejected digests, and apply receipts;
- post-decision readiness report;
- ordinary, review, and integrated browser smoke result;
- deployment or rollback commit, timestamp, and operator.
