# Owner decisions

Decisions that REQUIREMENTS.md §8 left open. Each line records the answer and the date.

| ID | Decision | Date | Effect |
|---|---|---|---|
| O1 | Start learners fresh. Do not migrate 1.0 `events`. | 2026-08-26 | No migrator. The `events` row shape stays as in 1.0 because it is the proven shape, not for migration. M3 parity fixtures come from recorded 2.0 streams and from the curriculum answer corpus. |
| O2 | Not yet answered. The build uses the defaults: 10 diagnosis calls per session, 600 output tokens per call, model id and provider from configuration. | — | Needed by M5. |
| O3 | Rewrite the frontend as a React + TypeScript SPA. Do not keep the 1.0 SPA. | 2026-08-26 | M6 builds the new SPA against the HTTP API. The 1.0 `static/` tree is a design reference only (Tokyo Night). |
| — | Review loop: stop after four rounds; fix the accepted-not-fixed items; continue with M1. | 2026-08-26 | Recorded in PROGRESS.md. |
