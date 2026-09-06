# Framework production preflight evidence

## Scope

This record covers read-only production inspection, the historical-v1 recovery scan, and an encrypted backup restore on 2026-09-06. It does not authorize or record a deployment, content approval, event correction, or production write.

## Content and history inventory

- Production `content_store`: 0 rows.
- Production event tenants: 1.
- Complete tenant snapshot: 36 events through sequence 36; all stored event versions are v1.
- Historical scan: 6 attempts, 6 v1 attempts, 0 v1 incorrect attempts, 0 attempts with explicit uncertain outcomes, and 0 human-review candidates.
- Historical scanner source snapshot SHA-256: `dcc86aad64598114db86726d3a9116168f317cfb2e36d26b5852fb43dca6676b`.
- The private raw export and report are stored mode 0600 under `/home/deploy/.cache/cadus2_orchestration/`.

The export ran in a repeatable-read, read-only transaction with an explicit tenant setting and filter. The separate deployment-wide tenant inventory found the same single tenant. The scan changed no event, projection, approval, or recovery state.

## Encrypted backup and restore

- `pg_dump -Fc` read the production database and streamed directly into `age`; no plaintext dump was written to disk.
- `age` encrypted to the deployment user's existing Ed25519 SSH recipient.
- Encrypted archive: `/home/deploy/.cache/cadus2_orchestration/prod-backup-20260906.dump.age`, 73,682 bytes, mode 0600.
- Decryption streamed directly into `pg_restore` in disposable `cadus2-testdb` database `cadus2_prodrestore_20260906_2142` with `--no-owner --no-privileges`.
- Production event fingerprint: 36 events, maximum sequence 36, MD5 `8442802e0407e6d1d9d00d074e44d0b5` over ordered tenant/sequence/version/payload tuples.
- Restored event fingerprint: exactly the same values and digest.
- Production and restored `content_store` counts are both 0.

An initial disposable restore reproduced three default-privilege errors because the test cluster has no `postgres` role. The second restore excluded owner and privilege metadata, completed with exit status 0, and preserved all application schema/data checked above. The failed disposable database was removed.

## Remaining release evidence

- Start the final candidate against the retained restored snapshot with outbound model calls disabled.
- Read the available production account twice, prove projector version 7 and byte-identical second-read model/report state, and prove the event fingerprint is unchanged.
- Run the final integrated gate on the final content commit.
- Record the human content-review decision and post-decision readiness report.
- Obtain explicit deployment authorization, deploy through `scripts/deploy.sh`, and collect live health, readiness, latency, smoke, and rollback-image evidence.
