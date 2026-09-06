# Unit07 pending templates

`templates.json` contains 74 local pending template drafts in the normal `kp_id`, `kind`, `arguments` import format. Each passed the production worker gate and produced 12 distinct valid instances. These files have not been imported into a database or approved.

Generate and validate from the repository root:

```sh
nix-shell -p python3Packages.pyyaml python3Packages.sympy --run 'python3 scripts/authoring/unit07/templates.py'
CARGO_BUILD_JOBS=1 SQLX_OFFLINE=true cargo run -p cadus-worker --example unit07_verify --locked --offline
```

The verifier reads the actual curriculum, preserves its response contracts, checks every tuple, and writes only passing drafts here. It records the 28 blocked KPs separately. See [the correction report](../../reports/unit07-correction-2026-09-07.md), [instance evidence](../../reports/unit07-template-evidence.json), and [exact blockers](../../reports/unit07-schema-blockers.json).
